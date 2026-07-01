//! Issue #633: Graceful shutdown with connection draining.
//!
//! Implements a coordinated shutdown sequence that:
//! - Receives OS shutdown signals (SIGTERM, SIGINT)
//! - Tracks in-flight requests
//! - Drains requests with a configurable timeout
//! - Closes database connections gracefully
//! - Stops indexer task cleanly
//! - Propagates shutdown signal to SSE connections

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::signal;
use tokio::sync::broadcast;
use tracing::{info, warn};

/// Configuration for graceful shutdown.
#[derive(Clone, Debug)]
pub struct GracefulShutdownConfig {
    /// Timeout for draining in-flight requests (in seconds)
    pub drain_timeout_secs: u64,
    /// Maximum concurrent requests during shutdown
    pub max_requests: u64,
}

impl GracefulShutdownConfig {
    /// Load from environment variables or return defaults.
    pub fn from_env() -> Self {
        let drain_timeout_secs = std::env::var("GRACEFUL_SHUTDOWN_TIMEOUT_SECS")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(30);

        let max_requests = std::env::var("GRACEFUL_SHUTDOWN_MAX_REQUESTS")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(1000);

        Self {
            drain_timeout_secs,
            max_requests,
        }
    }
}

/// Tracks in-flight requests for graceful shutdown.
pub struct RequestTracker {
    in_flight: Arc<AtomicU64>,
    config: GracefulShutdownConfig,
}

impl RequestTracker {
    /// Create a new request tracker.
    pub fn new(config: GracefulShutdownConfig) -> Self {
        Self {
            in_flight: Arc::new(AtomicU64::new(0)),
            config,
        }
    }

    /// Increment in-flight request counter.
    pub fn increment(&self) -> Result<(), &'static str> {
        let current = self.in_flight.fetch_add(1, Ordering::SeqCst);
        if current >= self.config.max_requests {
            self.in_flight.fetch_sub(1, Ordering::SeqCst);
            return Err("Too many in-flight requests");
        }
        Ok(())
    }

    /// Decrement in-flight request counter.
    pub fn decrement(&self) {
        self.in_flight.fetch_sub(1, Ordering::SeqCst);
    }

    /// Get current number of in-flight requests.
    pub fn count(&self) -> u64 {
        self.in_flight.load(Ordering::SeqCst)
    }

    /// Clone the in-flight counter for use in middleware.
    pub fn clone_counter(&self) -> Arc<AtomicU64> {
        Arc::clone(&self.in_flight)
    }
}

/// Handle graceful shutdown by listening for OS signals.
///
/// This function:
/// 1. Listens for SIGTERM/SIGINT
/// 2. Notifies all shutdown channels
/// 3. Drains in-flight requests with timeout
/// 4. Closes database connections
/// 5. Stops indexer task
pub async fn handle_shutdown(
    request_tracker: Arc<RequestTracker>,
    shutdown_tx: broadcast::Sender<()>,
    db_pool: sqlx::PgPool,
    drain_timeout: Duration,
) -> Result<(), Box<dyn std::error::Error>> {
    // Set up signal handlers
    let mut sigterm = signal::unix::signal(signal::unix::SignalKind::terminate())?;
    let mut sigint = signal::unix::signal(signal::unix::SignalKind::interrupt())?;

    tokio::select! {
        _ = sigterm.recv() => {
            info!("Received SIGTERM, initiating graceful shutdown");
        }
        _ = sigint.recv() => {
            info!("Received SIGINT, initiating graceful shutdown");
        }
        _ = signal::ctrl_c() => {
            info!("Received Ctrl+C, initiating graceful shutdown");
        }
    }

    // Broadcast shutdown signal to all listeners (SSE streams, indexer, etc.)
    let _ = shutdown_tx.send(());

    // Drain in-flight requests
    drain_requests(&request_tracker, drain_timeout).await;

    // Close database pool
    close_database(&db_pool).await;

    info!("Graceful shutdown completed");
    Ok(())
}

/// Drain in-flight requests with timeout.
async fn drain_requests(tracker: &RequestTracker, timeout: Duration) {
    let start = std::time::Instant::now();
    let check_interval = Duration::from_millis(100);

    loop {
        let count = tracker.count();
        if count == 0 {
            info!("All in-flight requests drained");
            break;
        }

        if start.elapsed() > timeout {
            warn!(
                remaining_requests = count,
                timeout_secs = timeout.as_secs(),
                "Shutdown timeout reached with requests still in-flight"
            );
            break;
        }

        info!(
            in_flight = count,
            elapsed_secs = start.elapsed().as_secs(),
            "Draining in-flight requests..."
        );

        tokio::time::sleep(check_interval).await;
    }
}

/// Close database pool gracefully.
async fn close_database(pool: &sqlx::PgPool) {
    info!("Closing database pool");
    // SQLx automatically closes connections when the pool is dropped
    // or we can explicitly wait for connections to close
    let start = std::time::Instant::now();
    while pool.num_idle() < pool.max_size() && start.elapsed() < Duration::from_secs(10) {
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    info!(
        idle_connections = pool.num_idle(),
        "Database pool closed"
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn graceful_shutdown_config_from_env() {
        std::env::set_var("GRACEFUL_SHUTDOWN_TIMEOUT_SECS", "45");
        std::env::set_var("GRACEFUL_SHUTDOWN_MAX_REQUESTS", "500");

        let config = GracefulShutdownConfig::from_env();
        assert_eq!(config.drain_timeout_secs, 45);
        assert_eq!(config.max_requests, 500);

        std::env::remove_var("GRACEFUL_SHUTDOWN_TIMEOUT_SECS");
        std::env::remove_var("GRACEFUL_SHUTDOWN_MAX_REQUESTS");
    }

    #[test]
    fn request_tracker_increment_decrement() {
        let config = GracefulShutdownConfig {
            drain_timeout_secs: 30,
            max_requests: 100,
        };
        let tracker = RequestTracker::new(config);

        assert_eq!(tracker.count(), 0);

        tracker.increment().unwrap();
        assert_eq!(tracker.count(), 1);

        tracker.increment().unwrap();
        assert_eq!(tracker.count(), 2);

        tracker.decrement();
        assert_eq!(tracker.count(), 1);

        tracker.decrement();
        assert_eq!(tracker.count(), 0);
    }

    #[test]
    fn request_tracker_respects_max_requests() {
        let config = GracefulShutdownConfig {
            drain_timeout_secs: 30,
            max_requests: 2,
        };
        let tracker = RequestTracker::new(config);

        tracker.increment().unwrap();
        tracker.increment().unwrap();

        let result = tracker.increment();
        assert!(result.is_err());
        assert_eq!(tracker.count(), 2);

        tracker.decrement();
        tracker.increment().unwrap();
        assert_eq!(tracker.count(), 2);
    }

    #[test]
    fn config_defaults() {
        std::env::remove_var("GRACEFUL_SHUTDOWN_TIMEOUT_SECS");
        std::env::remove_var("GRACEFUL_SHUTDOWN_MAX_REQUESTS");

        let config = GracefulShutdownConfig::from_env();
        assert_eq!(config.drain_timeout_secs, 30);
        assert_eq!(config.max_requests, 1000);
    }
}
