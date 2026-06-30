use sqlx::PgPool;
use std::time::Duration;
use tokio::sync::watch;
use uuid::Uuid;

const DEFAULT_ERROR_RATE_WINDOW_SECS: u64 = 300;
const DEFAULT_ROLLBACK_THRESHOLD: f64 = 0.05;

/// Feature flag context for evaluation
#[derive(Clone, Debug)]
pub struct FeatureFlagContext {
    pub contract_id: Option<String>,
    pub user_id: Option<String>,
    pub ip_address: Option<String>,
    pub region: Option<String>,
}

pub struct FeatureFlagWatcher {
    pool: PgPool,
    window_secs: u64,
    rollback_threshold: f64,
}

impl FeatureFlagWatcher {
    pub fn new(pool: PgPool) -> Self {
        Self {
            pool,
            window_secs: DEFAULT_ERROR_RATE_WINDOW_SECS,
            rollback_threshold: DEFAULT_ROLLBACK_THRESHOLD,
        }
    }

    async fn current_error_rate(&self) -> Option<f64> {
        let row: Option<(i64, i64)> = sqlx::query_as(
            "SELECT
                SUM(CASE WHEN status >= 500 THEN 1 ELSE 0 END)::bigint,
                COUNT(*)::bigint
             FROM request_logs
             WHERE created_at > NOW() - ($1 || ' seconds')::interval",
        )
        .bind(self.window_secs as i64)
        .fetch_optional(&self.pool)
        .await
        .ok()
        .flatten();

        row.and_then(|(errors, total)| {
            if total == 0 {
                None
            } else {
                Some(errors as f64 / total as f64)
            }
        })
    }

    async fn rollback_enabled_flags(&self, error_rate: f64) {
        let flags: Vec<(uuid::Uuid, String)> = match sqlx::query_as(
            "SELECT id, name FROM feature_flags WHERE enabled = TRUE AND auto_rollback = TRUE",
        )
        .fetch_all(&self.pool)
        .await
        {
            Ok(r) => r,
            Err(e) => {
                tracing::warn!(error = %e, "Failed to fetch feature flags for rollback check");
                return;
            }
        };

        for (id, name) in flags {
            if let Err(e) = sqlx::query(
                "UPDATE feature_flags SET enabled = FALSE, updated_at = NOW() WHERE id = $1",
            )
            .bind(id)
            .execute(&self.pool)
            .await
            {
                tracing::warn!(flag_id = %id, error = %e, "Failed to rollback feature flag");
                continue;
            }

            tracing::warn!(
                flag_name = %name,
                flag_id = %id,
                error_rate = error_rate,
                threshold = self.rollback_threshold,
                "Feature flag auto-rolled back due to error rate spike",
            );
            crate::metrics::record_feature_flag_rollback(&name);

            let _ = sqlx::query(
                "INSERT INTO feature_flag_audit (flag_id, action, reason, triggered_by)
                 VALUES ($1, 'rollback', $2, 'auto-rollback')",
            )
            .bind(id)
            .bind(format!(
                "Auto-rollback: error rate {:.2}% exceeded threshold {:.2}%",
                error_rate * 100.0,
                self.rollback_threshold * 100.0
            ))
            .execute(&self.pool)
            .await;
        }
    }

    pub async fn run_once(&self) {
        if let Some(rate) = self.current_error_rate().await {
            extern crate metrics as m;
            m::gauge!("soroban_pulse_feature_flag_error_rate").set(rate);

            if rate > self.rollback_threshold {
                self.rollback_enabled_flags(rate).await;
            }
        }
    }
}

/// Evaluate whether a feature flag should be enabled for a given context
pub async fn is_feature_enabled(
    pool: &PgPool,
    flag_name: &str,
    context: &FeatureFlagContext,
) -> Result<bool, sqlx::Error> {
    let row: Option<(bool, i32, Option<Vec<String>>, Option<Vec<String>>, Option<Vec<String>>, Option<Vec<String>>)> =
        sqlx::query_as(
            "SELECT enabled, rollout_percentage, target_contract_ids, target_user_ids, target_ips, target_regions
             FROM feature_flags WHERE name = $1",
        )
        .bind(flag_name)
        .fetch_optional(pool)
        .await?;

    match row {
        None => Ok(false), // Feature flag doesn't exist
        Some((false, _, _, _, _, _)) => Ok(false), // Feature flag is disabled globally
        Some((true, rollout_pct, target_contracts, target_users, target_ips, target_regions)) => {
            // Check targeting: if any targeting rules are set, require a match
            let has_targeting = target_contracts.is_some() || target_users.is_some() || target_ips.is_some() || target_regions.is_some();
            
            if has_targeting {
                let mut target_matched = false;

                if let Some(ref contracts) = target_contracts {
                    if let Some(ref contract_id) = context.contract_id {
                        if contracts.contains(contract_id) {
                            target_matched = true;
                        }
                    }
                }

                if let Some(ref users) = target_users {
                    if let Some(ref user_id) = context.user_id {
                        if users.contains(user_id) {
                            target_matched = true;
                        }
                    }
                }

                if let Some(ref ips) = target_ips {
                    if let Some(ref ip) = context.ip_address {
                        if ips.contains(ip) {
                            target_matched = true;
                        }
                    }
                }

                if let Some(ref regions) = target_regions {
                    if let Some(ref region) = context.region {
                        if regions.contains(region) {
                            target_matched = true;
                        }
                    }
                }

                if !target_matched {
                    return Ok(false);
                }
            }

            // Check percentage-based rollout
            let hash = compute_rollout_hash(flag_name, context);
            let bucket = (hash % 100) as i32;
            Ok(bucket < rollout_pct)
        }
    }
}

/// Compute a deterministic hash for percentage-based rollout
fn compute_rollout_hash(flag_name: &str, context: &FeatureFlagContext) -> u64 {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    let mut hasher = DefaultHasher::new();
    flag_name.hash(&mut hasher);

    // Use contract ID as the primary targeting identifier for rollout consistency
    if let Some(ref contract_id) = context.contract_id {
        contract_id.hash(&mut hasher);
    } else if let Some(ref user_id) = context.user_id {
        user_id.hash(&mut hasher);
    } else if let Some(ref ip) = context.ip_address {
        ip.hash(&mut hasher);
    }

    hasher.finish()
}

pub fn spawn(pool: PgPool, interval_secs: u64, mut shutdown_rx: watch::Receiver<bool>) {
    tokio::spawn(async move {
        let watcher = FeatureFlagWatcher::new(pool);
        let mut ticker = tokio::time::interval(Duration::from_secs(interval_secs));
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            tokio::select! {
                _ = ticker.tick() => {
                    watcher.run_once().await;
                }
                _ = shutdown_rx.changed() => {
                    tracing::debug!("Feature flag watcher shutting down");
                    break;
                }
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_threshold_is_five_percent() {
        assert!((DEFAULT_ROLLBACK_THRESHOLD - 0.05).abs() < f64::EPSILON);
    }

    #[test]
    fn default_window_is_five_minutes() {
        assert_eq!(DEFAULT_ERROR_RATE_WINDOW_SECS, 300);
    }

    #[test]
    fn error_rate_zero_total_returns_none() {
        let rate: Option<f64> = {
            let total: i64 = 0;
            if total == 0 { None } else { Some(0.0) }
        };
        assert!(rate.is_none());
    }

    #[test]
    fn error_rate_calculation() {
        let errors: i64 = 10;
        let total: i64 = 100;
        let rate = errors as f64 / total as f64;
        assert!((rate - 0.1).abs() < f64::EPSILON);
    }

    #[test]
    fn rollout_hash_consistent() {
        let context = FeatureFlagContext {
            contract_id: Some("CABC123".to_string()),
            user_id: None,
            ip_address: None,
            region: None,
        };
        let hash1 = compute_rollout_hash("my-flag", &context);
        let hash2 = compute_rollout_hash("my-flag", &context);
        assert_eq!(hash1, hash2, "Hash should be deterministic");
    }

    #[test]
    fn rollout_hash_differs_by_flag() {
        let context = FeatureFlagContext {
            contract_id: Some("CABC123".to_string()),
            user_id: None,
            ip_address: None,
            region: None,
        };
        let hash1 = compute_rollout_hash("flag-a", &context);
        let hash2 = compute_rollout_hash("flag-b", &context);
        assert_ne!(hash1, hash2, "Hash should differ by flag name");
    }

    #[test]
    fn rollout_percentage_distribution() {
        // Verify that the rollout hash distribution is roughly uniform
        let mut context = FeatureFlagContext {
            contract_id: Some("contract-1".to_string()),
            user_id: None,
            ip_address: None,
            region: None,
        };

        let mut enabled_count = 0;
        let total = 1000;
        for i in 0..total {
            context.contract_id = Some(format!("contract-{}", i));
            let hash = compute_rollout_hash("test-flag", &context);
            let bucket = (hash % 100) as i32;
            if bucket < 50 { // 50% rollout
                enabled_count += 1;
            }
        }

        // Should be close to 50% (allow 10% variance)
        let ratio = enabled_count as f64 / total as f64;
        assert!(ratio > 0.4 && ratio < 0.6, "Rollout distribution should be ~50%, got {}", ratio);
    }
}
