//! Issue #628: Distributed tracing spans for each API request.
//!
//! Adds OpenTelemetry tracing with:
//! - Trace ID header parsing (traceparent, X-Trace-ID)
//! - Request-scoped spans with custom attributes
//! - Database query and RPC call spans
//! - Span hierarchy (request → handler → db/rpc)
//! - Sampling strategy configuration

use tracing::{Instrument, Span};
use std::sync::Arc;

/// Configuration for distributed tracing.
#[derive(Clone, Debug)]
pub struct TracingConfig {
    /// Whether distributed tracing is enabled (when `otel` feature is compiled)
    pub enabled: bool,
    /// Sampling rate: 0.0 to 1.0 (0.0 = no tracing, 1.0 = trace everything)
    pub sample_rate: f64,
    /// Default namespace/service name for spans
    pub service_name: String,
}

impl TracingConfig {
    /// Create a new tracing configuration from environment.
    pub fn from_env() -> Self {
        #[cfg(feature = "otel")]
        let enabled = true;
        #[cfg(not(feature = "otel"))]
        let enabled = false;

        let sample_rate = std::env::var("TRACE_SAMPLE_RATE")
            .ok()
            .and_then(|s| s.parse::<f64>().ok())
            .unwrap_or(1.0)
            .clamp(0.0, 1.0);

        let service_name = std::env::var("TRACE_SERVICE_NAME")
            .unwrap_or_else(|_| "soroban-pulse".to_string());

        Self {
            enabled,
            sample_rate,
            service_name,
        }
    }

    /// Should this trace be sampled?
    pub fn should_sample(&self) -> bool {
        if !self.enabled {
            return false;
        }
        if self.sample_rate >= 1.0 {
            return true;
        }
        if self.sample_rate <= 0.0 {
            return false;
        }
        // Simple deterministic sampling based on current time
        (std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos() as f64 / u128::MAX as f64)
            < self.sample_rate
    }
}

/// Parse trace context from HTTP headers (traceparent or X-Trace-ID).
///
/// W3C Trace Context format: `traceparent: version-trace_id-parent_id-trace_flags`
/// Example: `00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01`
pub fn extract_trace_context(headers: &http::HeaderMap) -> Option<TraceContext> {
    // Try W3C traceparent first
    if let Some(traceparent) = headers.get("traceparent") {
        if let Ok(s) = traceparent.to_str() {
            return parse_traceparent(s);
        }
    }

    // Fallback to X-Trace-ID
    if let Some(trace_id) = headers.get("X-Trace-ID") {
        if let Ok(s) = trace_id.to_str() {
            return Some(TraceContext {
                trace_id: s.to_string(),
                parent_id: None,
                trace_flags: "01".to_string(), // sampled
            });
        }
    }

    None
}

/// Parsed W3C trace context.
#[derive(Clone, Debug)]
pub struct TraceContext {
    pub trace_id: String,
    pub parent_id: Option<String>,
    pub trace_flags: String,
}

fn parse_traceparent(s: &str) -> Option<TraceContext> {
    let parts: Vec<&str> = s.split('-').collect();
    if parts.len() != 4 {
        return None;
    }

    // version, trace_id, parent_id, trace_flags
    Some(TraceContext {
        trace_id: parts[1].to_string(),
        parent_id: Some(parts[2].to_string()),
        trace_flags: parts[3].to_string(),
    })
}

/// Record a custom span attribute.
pub fn set_span_attribute(key: &str, value: impl std::fmt::Display) {
    #[cfg(feature = "otel")]
    {
        tracing::Span::current().record(key, value.to_string());
    }
    #[cfg(not(feature = "otel"))]
    {
        let _ = (key, value);
    }
}

/// Create a new span for a database query.
pub fn create_db_span(query: &str, table: &str) -> Span {
    #[cfg(feature = "otel")]
    {
        tracing::info_span!(
            "db.query",
            db.system = "postgresql",
            db.operation = "query",
            db.statement = query,
            "db.table" = table,
        )
    }
    #[cfg(not(feature = "otel"))]
    {
        let _ = (query, table);
        tracing::info_span!("db.query")
    }
}

/// Create a new span for an RPC call.
pub fn create_rpc_span(method: &str, url: &str) -> Span {
    #[cfg(feature = "otel")]
    {
        tracing::info_span!(
            "rpc.call",
            rpc.method = method,
            rpc.url = url,
            rpc.system = "soroban-rpc",
        )
    }
    #[cfg(not(feature = "otel"))]
    {
        let _ = (method, url);
        tracing::info_span!("rpc.call")
    }
}

/// Create a new span for API request handling with custom attributes.
pub fn create_api_span(method: &str, path: &str, contract_id: Option<&str>) -> Span {
    #[cfg(feature = "otel")]
    {
        let span = tracing::info_span!(
            "http.request",
            http.method = method,
            http.url = path,
            http.target = path,
        );

        if let Some(cid) = contract_id {
            span.record("contract_id", cid);
        }

        span
    }
    #[cfg(not(feature = "otel"))]
    {
        let _ = (method, path, contract_id);
        tracing::info_span!("http.request")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_traceparent_valid() {
        let s = "00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01";
        let ctx = parse_traceparent(s).unwrap();
        assert_eq!(ctx.trace_id, "4bf92f3577b34da6a3ce929d0e0e4736");
        assert_eq!(ctx.parent_id, Some("00f067aa0ba902b7".to_string()));
        assert_eq!(ctx.trace_flags, "01");
    }

    #[test]
    fn parse_traceparent_invalid() {
        let s = "invalid-format";
        let ctx = parse_traceparent(s);
        assert!(ctx.is_none());
    }

    #[test]
    fn tracing_config_from_env() {
        let config = TracingConfig::from_env();
        assert!(config.sample_rate >= 0.0 && config.sample_rate <= 1.0);
        assert!(!config.service_name.is_empty());
    }

    #[test]
    fn tracing_config_sample_rate_clamped() {
        std::env::set_var("TRACE_SAMPLE_RATE", "2.0");
        let config = TracingConfig::from_env();
        assert_eq!(config.sample_rate, 1.0);

        std::env::set_var("TRACE_SAMPLE_RATE", "-0.5");
        let config = TracingConfig::from_env();
        assert_eq!(config.sample_rate, 0.0);

        std::env::remove_var("TRACE_SAMPLE_RATE");
    }
}
