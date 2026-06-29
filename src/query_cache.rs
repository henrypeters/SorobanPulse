use moka::future::Cache;
use serde_json::Value;
use std::sync::Arc;
use std::time::Duration;

pub const MIN_TTL_SECS: u64 = 300;   // 5 min
pub const MAX_TTL_SECS: u64 = 3600;  // 60 min
pub const DEFAULT_TTL_SECS: u64 = 300;
pub const DEFAULT_MAX_CAPACITY: u64 = 1_000;

/// Clamp a caller-supplied TTL to the allowed [MIN_TTL_SECS, MAX_TTL_SECS] range.
pub fn clamp_ttl(secs: u64) -> Duration {
    Duration::from_secs(secs.clamp(MIN_TTL_SECS, MAX_TTL_SECS))
}

/// Build the shared query-result cache.
pub fn build(ttl_secs: u64, max_capacity: u64) -> Arc<Cache<String, Value>> {
    Arc::new(
        Cache::builder()
            .max_capacity(max_capacity)
            .time_to_live(clamp_ttl(ttl_secs))
            .build(),
    )
}

/// Extract the low-cardinality query type label from a cache key.
/// Keys are formatted as "type:specifics" (e.g. "contract_event_counts:CABC…").
fn query_type_label(key: &str) -> &str {
    key.split(':').next().unwrap_or(key)
}

/// Check whether a cached entry is present and record a cache hit/miss metric.
/// Returns the cached value if found, otherwise returns `None`.
pub async fn get(cache: &Cache<String, Value>, key: &str) -> Option<Value> {
    let label = query_type_label(key);
    match cache.get(key).await {
        Some(v) => {
            crate::metrics::record_query_cache_hit(label);
            Some(v)
        }
        None => {
            crate::metrics::record_query_cache_miss(label);
            None
        }
    }
}

/// Insert a value and record the store metric.
pub async fn set(cache: &Cache<String, Value>, key: String, value: Value) {
    cache.insert(key, value).await;
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn clamp_ttl_min() {
        assert_eq!(clamp_ttl(0), Duration::from_secs(MIN_TTL_SECS));
        assert_eq!(clamp_ttl(MIN_TTL_SECS - 1), Duration::from_secs(MIN_TTL_SECS));
    }

    #[test]
    fn clamp_ttl_max() {
        assert_eq!(clamp_ttl(u64::MAX), Duration::from_secs(MAX_TTL_SECS));
        assert_eq!(clamp_ttl(MAX_TTL_SECS + 1), Duration::from_secs(MAX_TTL_SECS));
    }

    #[test]
    fn clamp_ttl_in_range() {
        assert_eq!(clamp_ttl(600), Duration::from_secs(600));
    }

    #[tokio::test]
    async fn build_and_retrieve() {
        let cache = build(DEFAULT_TTL_SECS, 10);
        cache.insert("k".to_string(), json!({"ok": true})).await;
        let v = cache.get("k").await.unwrap();
        assert_eq!(v["ok"], json!(true));
    }
}
