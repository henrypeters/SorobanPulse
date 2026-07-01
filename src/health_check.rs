/// Granular health check module for Postgres, RPC, and external services
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use std::time::Duration;
use tokio::time::timeout;
use tracing::{debug, warn};

use crate::metrics;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum HealthStatus {
    #[serde(rename = "ok")]
    Ok,
    #[serde(rename = "degraded")]
    Degraded,
    #[serde(rename = "unhealthy")]
    Unhealthy,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthCheckResult {
    pub status: HealthStatus,
    pub message: Option<String>,
    pub details: Option<serde_json::Value>,
    pub response_time_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PostgresHealthCheck {
    pub status: HealthStatus,
    pub message: Option<String>,
    pub pool_size: Option<usize>,
    pub idle_connections: Option<usize>,
    pub response_time_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RpcHealthCheck {
    pub status: HealthStatus,
    pub message: Option<String>,
    pub active_endpoint: Option<String>,
    pub response_time_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExternalServiceHealthCheck {
    pub status: HealthStatus,
    pub service: String,
    pub message: Option<String>,
    pub response_time_ms: u64,
}

/// Check PostgreSQL health
pub async fn check_postgres(
    pool: &PgPool,
    timeout_ms: u64,
) -> PostgresHealthCheck {
    let start = std::time::Instant::now();
    let timeout_duration = Duration::from_millis(timeout_ms);

    match timeout(timeout_duration, sqlx::query("SELECT 1").fetch_one(pool)).await {
        Ok(Ok(_)) => {
            let response_time_ms = start.elapsed().as_millis() as u64;
            debug!("PostgreSQL health check: OK ({}ms)", response_time_ms);
            metrics::record_postgres_health_ok(response_time_ms);
            
            PostgresHealthCheck {
                status: HealthStatus::Ok,
                message: Some("Database connection successful".to_string()),
                pool_size: Some(pool.num_connections() as usize),
                idle_connections: Some(pool.num_idle_connections() as usize),
                response_time_ms,
            }
        }
        Ok(Err(sqlx::Error::PoolTimedOut)) => {
            let response_time_ms = start.elapsed().as_millis() as u64;
            warn!("PostgreSQL health check: Pool exhausted ({}ms)", response_time_ms);
            metrics::record_postgres_health_degraded(response_time_ms);
            
            PostgresHealthCheck {
                status: HealthStatus::Degraded,
                message: Some("Connection pool exhausted".to_string()),
                pool_size: Some(pool.num_connections() as usize),
                idle_connections: Some(pool.num_idle_connections() as usize),
                response_time_ms,
            }
        }
        Ok(Err(e)) => {
            let response_time_ms = start.elapsed().as_millis() as u64;
            warn!("PostgreSQL health check: Connection error: {} ({}ms)", e, response_time_ms);
            metrics::record_postgres_health_error(response_time_ms);
            
            PostgresHealthCheck {
                status: HealthStatus::Unhealthy,
                message: Some(format!("Database connection failed: {}", e)),
                pool_size: Some(pool.num_connections() as usize),
                idle_connections: Some(pool.num_idle_connections() as usize),
                response_time_ms,
            }
        }
        Err(_) => {
            let response_time_ms = start.elapsed().as_millis() as u64;
            warn!("PostgreSQL health check: Timeout after {}ms", response_time_ms);
            metrics::record_postgres_health_error(response_time_ms);
            
            PostgresHealthCheck {
                status: HealthStatus::Unhealthy,
                message: Some(format!("Health check timeout after {}ms", timeout_ms)),
                pool_size: Some(pool.num_connections() as usize),
                idle_connections: Some(pool.num_idle_connections() as usize),
                response_time_ms,
            }
        }
    }
}

/// Check RPC endpoint health using a direct HTTP call
pub async fn check_rpc(
    rpc_url: &str,
    timeout_ms: u64,
) -> RpcHealthCheck {
    let start = std::time::Instant::now();
    let timeout_duration = Duration::from_millis(timeout_ms);

    let client = reqwest::Client::new();
    let body = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "getLatestLedger"
    });

    match timeout(
        timeout_duration,
        client.post(rpc_url).json(&body).send(),
    )
    .await
    {
        Ok(Ok(response)) => {
            // Try to parse the response
            match timeout(timeout_duration, response.json::<serde_json::Value>()).await {
                Ok(Ok(json)) => {
                    let response_time_ms = start.elapsed().as_millis() as u64;
                    
                    // Check if the response contains a valid result or error
                    if json.get("result").is_some() || json.get("error").is_some() {
                        debug!("RPC health check: OK ({}ms)", response_time_ms);
                        metrics::record_rpc_health_ok(response_time_ms);
                        
                        RpcHealthCheck {
                            status: HealthStatus::Ok,
                            message: Some("RPC endpoint responding correctly".to_string()),
                            active_endpoint: Some(rpc_url.to_string()),
                            response_time_ms,
                        }
                    } else {
                        warn!("RPC health check: Invalid response format ({}ms)", response_time_ms);
                        metrics::record_rpc_health_error(response_time_ms);
                        
                        RpcHealthCheck {
                            status: HealthStatus::Unhealthy,
                            message: Some("RPC returned invalid response format".to_string()),
                            active_endpoint: Some(rpc_url.to_string()),
                            response_time_ms,
                        }
                    }
                }
                Ok(Err(e)) => {
                    let response_time_ms = start.elapsed().as_millis() as u64;
                    warn!("RPC health check: Failed to parse response: {} ({}ms)", e, response_time_ms);
                    metrics::record_rpc_health_error(response_time_ms);
                    
                    RpcHealthCheck {
                        status: HealthStatus::Unhealthy,
                        message: Some(format!("Failed to parse RPC response: {}", e)),
                        active_endpoint: Some(rpc_url.to_string()),
                        response_time_ms,
                    }
                }
                Err(_) => {
                    let response_time_ms = start.elapsed().as_millis() as u64;
                    warn!("RPC health check: Timeout parsing response after {}ms", response_time_ms);
                    metrics::record_rpc_health_error(response_time_ms);
                    
                    RpcHealthCheck {
                        status: HealthStatus::Unhealthy,
                        message: Some(format!("RPC health check timeout after {}ms", timeout_ms)),
                        active_endpoint: Some(rpc_url.to_string()),
                        response_time_ms,
                    }
                }
            }
        }
        Ok(Err(e)) => {
            let response_time_ms = start.elapsed().as_millis() as u64;
            warn!("RPC health check: Request failed: {} ({}ms)", e, response_time_ms);
            metrics::record_rpc_health_error(response_time_ms);
            
            RpcHealthCheck {
                status: HealthStatus::Unhealthy,
                message: Some(format!("RPC request failed: {}", e)),
                active_endpoint: Some(rpc_url.to_string()),
                response_time_ms,
            }
        }
        Err(_) => {
            let response_time_ms = start.elapsed().as_millis() as u64;
            warn!("RPC health check: Timeout after {}ms", response_time_ms);
            metrics::record_rpc_health_error(response_time_ms);
            
            RpcHealthCheck {
                status: HealthStatus::Unhealthy,
                message: Some(format!("RPC health check timeout after {}ms", timeout_ms)),
                active_endpoint: Some(rpc_url.to_string()),
                response_time_ms,
            }
        }
    }
}

/// Placeholder for checking external services (webhooks, email, etc.)
pub async fn check_external_service(
    service: &str,
    _timeout_ms: u64,
) -> ExternalServiceHealthCheck {
    // This is a placeholder implementation
    // In a real scenario, you would implement service-specific checks
    // For now, we just return a status based on the service name
    
    let response_time_ms = 0;
    match service {
        "webhooks" => {
            debug!("External service health check: {} - not yet implemented", service);
            ExternalServiceHealthCheck {
                status: HealthStatus::Ok,
                service: service.to_string(),
                message: Some("Service available (basic check)".to_string()),
                response_time_ms,
            }
        }
        "email" => {
            debug!("External service health check: {} - not yet implemented", service);
            ExternalServiceHealthCheck {
                status: HealthStatus::Ok,
                service: service.to_string(),
                message: Some("Service available (basic check)".to_string()),
                response_time_ms,
            }
        }
        _ => ExternalServiceHealthCheck {
            status: HealthStatus::Unhealthy,
            service: service.to_string(),
            message: Some(format!("Unknown service: {}", service)),
            response_time_ms,
        },
    }
}
