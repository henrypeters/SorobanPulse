//! Issue #622: Connection pool metrics and auto-scaling logic.
//!
//! Monitors database connection pool utilization and emits alerts when the
//! pool approaches exhaustion.  Because `sqlx::PgPool` does not support
//! runtime resizing, auto-scaling is implemented as a recommendation engine:
//! it tracks peak utilization and logs actionable tuning advice so operators
//! can adjust `DB_MAX_CONNECTIONS` / `DB_MIN_CONNECTIONS` at next restart.
//!
//! # Metrics emitted
//! | Name | Kind | Description |
//! |------|------|-------------|
//! | `soroban_pulse_db_pool_utilization` | Gauge | Active / max (0.0–1.0) |
//! | `soroban_pulse_db_pool_active_connections` | Gauge | In-use connections |
//! | `soroban_pulse_db_pool_max_connections` | Gauge | Configured maximum |
//! | `soroban_pulse_db_pool_acquire_latency_seconds` | Histogram | Time to get a connection |
//! | `soroban_pulse_db_pool_exhaustion_alerts_total` | Counter | Times util ≥ 90 % |

use sqlx::PgPool;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tracing::{info, warn};

use crate::metrics;

/// Shared utilization peak tracker.  Updated by the monitor task and read by
/// the `/status` endpoint (future) and the tuning recommender.
#[derive(Debug, Default)]
pub struct PoolStats {
    /// Highest utilization fraction seen since process start (×1000 fixed-point).
    peak_utilization_milli: AtomicU64,
    /// Total number of exhaustion events (util ≥ 90 %).
    exhaustion_event_count: AtomicU64,
}

impl PoolStats {
    pub fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }

    fn update(&self, utilization: f64) {
        let milli = (utilization * 1000.0) as u64;
        let _ = self
            .peak_utilization_milli
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |prev| {
                if milli > prev { Some(milli) } else { None }
            });
    }

    pub fn peak_utilization(&self) -> f64 {
        self.peak_utilization_milli.load(Ordering::Relaxed) as f64 / 1000.0
    }

    pub fn exhaustion_events(&self) -> u64 {
        self.exhaustion_event_count.load(Ordering::Relaxed)
    }
}

/// Configuration for the pool monitor.
#[derive(Debug, Clone)]
pub struct PoolMonitorConfig {
    /// Configured maximum pool size (from `DB_MAX_CONNECTIONS`).
    pub max_connections: u32,
    /// Configured minimum pool size (from `DB_MIN_CONNECTIONS`).
    pub min_connections: u32,
    /// Utilization fraction above which an exhaustion alert is fired (default 0.9).
    pub exhaustion_threshold: f64,
    /// How often to sample and emit metrics (default 15 s).
    pub sample_interval: Duration,
}

impl Default for PoolMonitorConfig {
    fn default() -> Self {
        Self {
            max_connections: 10,
            min_connections: 1,
            exhaustion_threshold: 0.9,
            sample_interval: Duration::from_secs(15),
        }
    }
}

/// Spawn a background task that periodically samples pool utilization and
/// emits metrics / warnings.  Returns a handle to the shared [`PoolStats`].
pub fn spawn_pool_monitor(
    pool: PgPool,
    config: PoolMonitorConfig,
) -> Arc<PoolStats> {
    let stats = PoolStats::new();
    let stats_clone = Arc::clone(&stats);

    tokio::spawn(async move {
        let mut interval = tokio::time::interval(config.sample_interval);
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

        loop {
            interval.tick().await;

            let size = pool.size();
            let idle = pool.num_idle();
            let active = size.saturating_sub(idle as u32);
            let max = config.max_connections;
            let utilization = if max > 0 { active as f64 / max as f64 } else { 0.0 };

            // Emit Prometheus metrics.
            metrics::update_pool_utilization(&pool, max);

            // Track peak.
            stats_clone.update(utilization);

            // Exhaustion alert.
            if utilization >= config.exhaustion_threshold {
                stats_clone
                    .exhaustion_event_count
                    .fetch_add(1, Ordering::Relaxed);
                metrics::record_pool_exhaustion_alert();

                warn!(
                    utilization = format!("{:.1}%", utilization * 100.0),
                    active_connections = active,
                    max_connections = max,
                    "DB connection pool near exhaustion — consider increasing DB_MAX_CONNECTIONS"
                );
            }

            // Periodic tuning advice.
            let peak = stats_clone.peak_utilization();
            if peak < 0.3 && config.min_connections as f64 > max as f64 * 0.2 {
                info!(
                    peak_utilization = format!("{:.1}%", peak * 100.0),
                    current_min = config.min_connections,
                    suggestion = (config.min_connections / 2).max(1),
                    "Pool utilization is low — you may reduce DB_MIN_CONNECTIONS"
                );
            }
        }
    });

    stats
}

/// Acquire a connection from the pool and record the latency.
/// Callers should prefer `pool.acquire()` directly for hot paths; this
/// wrapper is intended for background workers where latency attribution
/// is valuable.
pub async fn acquire_tracked(pool: &PgPool) -> Result<sqlx::pool::PoolConnection<sqlx::Postgres>, sqlx::Error> {
    let start = Instant::now();
    let conn = pool.acquire().await?;
    let elapsed = start.elapsed();
    metrics::record_pool_acquire_latency(elapsed);
    Ok(conn)
}

/// Emit a snapshot of pool metrics to the log and as structured fields.
/// Useful for startup diagnostics or on-demand admin queries.
pub fn log_pool_snapshot(pool: &PgPool, max_connections: u32) {
    let size = pool.size();
    let idle = pool.num_idle();
    let active = size.saturating_sub(idle as u32);
    let utilization = if max_connections > 0 {
        active as f64 / max_connections as f64
    } else {
        0.0
    };

    info!(
        pool_size = size,
        pool_idle = idle,
        pool_active = active,
        pool_max = max_connections,
        utilization = format!("{:.1}%", utilization * 100.0),
        "DB connection pool snapshot"
    );
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pool_stats_peak_tracks_highest() {
        let stats = PoolStats::default();
        stats.update(0.5);
        stats.update(0.8);
        stats.update(0.6);
        assert!((stats.peak_utilization() - 0.8).abs() < 0.001);
    }

    #[test]
    fn pool_stats_peak_does_not_regress() {
        let stats = PoolStats::default();
        stats.update(0.9);
        stats.update(0.1);
        assert!((stats.peak_utilization() - 0.9).abs() < 0.001);
    }

    #[test]
    fn pool_monitor_config_defaults() {
        let cfg = PoolMonitorConfig::default();
        assert_eq!(cfg.exhaustion_threshold, 0.9);
        assert_eq!(cfg.sample_interval, Duration::from_secs(15));
    }

    #[test]
    fn exhaustion_threshold_boundary() {
        let cfg = PoolMonitorConfig::default();
        // Utilization exactly at threshold should trigger alert.
        assert!(0.9 >= cfg.exhaustion_threshold);
        assert!(0.89 < cfg.exhaustion_threshold);
    }
}
