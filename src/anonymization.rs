//! Issue #618: Event data anonymization pipeline for privacy-sensitive deployments.
//!
//! ## Overview
//!
//! This module provides:
//! - **Anonymization rules** — regex-based patterns that identify PII fields.
//! - **PII detection** — scans event_data JSON trees for values matching any rule.
//! - **Anonymization** — replaces matched values with a redacted placeholder.
//! - **Anonymization worker** — background task that processes un-anonymized events.
//! - **Audit trail** — every anonymization action is recorded in `audit_logs`.
//! - **Metrics** — Prometheus counters for detections and applied redactions.
//!
//! ## Anonymization patterns
//!
//! | Pattern name | Regex                                         | Example match         |
//! |-------------|-----------------------------------------------|-----------------------|
//! | `email`     | `[a-zA-Z0-9._%+-]+@[a-zA-Z0-9.-]+\.[a-zA-Z]{2,}` | `user@example.com`   |
//! | `phone`     | `\+?[0-9]{7,15}`                              | `+14155552671`        |
//! | `ssn`       | `\b\d{3}-\d{2}-\d{4}\b`                       | `123-45-6789`         |
//! | `ipv4`      | `\b(?:\d{1,3}\.){3}\d{1,3}\b`                 | `192.168.1.1`         |
//! | `credit_card` | `\b(?:\d[ -]?){13,16}\b`                   | `4111 1111 1111 1111` |

use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::PgPool;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, info, warn};
use uuid::Uuid;

use crate::metrics;

// ── Anonymization rule model ─────────────────────────────────────────────────

/// A single anonymization rule: a named regex pattern applied to string values.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnonymizationRule {
    pub id: Option<i32>,
    pub name: String,
    pub pattern: String,
    pub replacement: String,
    pub enabled: bool,
    pub description: Option<String>,
}

impl AnonymizationRule {
    /// Compile the rule's pattern into a `Regex`.
    pub fn compile(&self) -> Result<Regex, regex::Error> {
        Regex::new(&self.pattern)
    }
}

/// Request body for creating/updating a rule via the API.
#[derive(Debug, Deserialize, Serialize)]
pub struct AnonymizationRuleRequest {
    pub name: String,
    pub pattern: String,
    pub replacement: Option<String>,
    pub enabled: Option<bool>,
    pub description: Option<String>,
}

// ── Built-in default patterns ────────────────────────────────────────────────

/// Return the built-in PII detection rules used when no custom rules are configured.
pub fn default_rules() -> Vec<AnonymizationRule> {
    vec![
        AnonymizationRule {
            id: None,
            name: "email".into(),
            pattern: r"[a-zA-Z0-9._%+\-]+@[a-zA-Z0-9.\-]+\.[a-zA-Z]{2,}".into(),
            replacement: "[REDACTED:email]".into(),
            enabled: true,
            description: Some("Email addresses".into()),
        },
        AnonymizationRule {
            id: None,
            name: "phone".into(),
            pattern: r"\+?[0-9]{7,15}".into(),
            replacement: "[REDACTED:phone]".into(),
            enabled: true,
            description: Some("Phone numbers (7–15 digits, optional leading +)".into()),
        },
        AnonymizationRule {
            id: None,
            name: "ssn".into(),
            pattern: r"\b\d{3}-\d{2}-\d{4}\b".into(),
            replacement: "[REDACTED:ssn]".into(),
            enabled: true,
            description: Some("US Social Security Numbers".into()),
        },
        AnonymizationRule {
            id: None,
            name: "ipv4".into(),
            pattern: r"\b(?:\d{1,3}\.){3}\d{1,3}\b".into(),
            replacement: "[REDACTED:ipv4]".into(),
            enabled: true,
            description: Some("IPv4 addresses".into()),
        },
        AnonymizationRule {
            id: None,
            name: "credit_card".into(),
            pattern: r"\b(?:\d[ \-]?){13,16}\b".into(),
            replacement: "[REDACTED:credit_card]".into(),
            enabled: true,
            description: Some("Credit / debit card numbers".into()),
        },
    ]
}

// ── PII scanner ──────────────────────────────────────────────────────────────

/// Result of scanning a single JSON value for PII.
#[derive(Debug)]
pub struct PiiScanResult {
    /// Field path(s) where PII was detected (dot-separated JSON path).
    pub detections: Vec<PiiDetection>,
}

#[derive(Debug)]
pub struct PiiDetection {
    pub field_path: String,
    pub rule_name: String,
}

/// Recursively walk a JSON value and detect PII string matches.
pub fn scan_for_pii(value: &Value, rules: &[(String, Regex)]) -> PiiScanResult {
    let mut detections = Vec::new();
    scan_recursive(value, "", rules, &mut detections);
    PiiScanResult { detections }
}

fn scan_recursive(
    value: &Value,
    path: &str,
    rules: &[(String, Regex)],
    out: &mut Vec<PiiDetection>,
) {
    match value {
        Value::String(s) => {
            for (name, re) in rules {
                if re.is_match(s) {
                    metrics::record_pii_detected(name);
                    out.push(PiiDetection {
                        field_path: path.to_string(),
                        rule_name: name.clone(),
                    });
                }
            }
        }
        Value::Object(map) => {
            for (k, v) in map {
                let child_path = if path.is_empty() {
                    k.clone()
                } else {
                    format!("{path}.{k}")
                };
                scan_recursive(v, &child_path, rules, out);
            }
        }
        Value::Array(arr) => {
            for (i, v) in arr.iter().enumerate() {
                let child_path = format!("{path}[{i}]");
                scan_recursive(v, &child_path, rules, out);
            }
        }
        _ => {}
    }
}

// ── Anonymizer ────────────────────────────────────────────────────────────────

/// Apply anonymization rules to a JSON value, replacing PII strings in-place.
///
/// Returns the mutated value and the list of rules that were applied.
pub fn anonymize_value(value: Value, rules: &[(String, Regex, String)]) -> (Value, Vec<String>) {
    let mut applied = Vec::new();
    let result = anonymize_recursive(value, rules, &mut applied);
    (result, applied)
}

fn anonymize_recursive(
    value: Value,
    rules: &[(String, Regex, String)],
    applied: &mut Vec<String>,
) -> Value {
    match value {
        Value::String(s) => {
            let mut current = s;
            for (name, re, replacement) in rules {
                if re.is_match(&current) {
                    let replaced = re.replace_all(&current, replacement.as_str()).to_string();
                    if replaced != current {
                        if !applied.contains(name) {
                            applied.push(name.clone());
                        }
                        current = replaced;
                    }
                }
            }
            Value::String(current)
        }
        Value::Object(map) => {
            let new_map = map
                .into_iter()
                .map(|(k, v)| (k, anonymize_recursive(v, rules, applied)))
                .collect();
            Value::Object(new_map)
        }
        Value::Array(arr) => {
            Value::Array(
                arr.into_iter()
                    .map(|v| anonymize_recursive(v, rules, applied))
                    .collect(),
            )
        }
        other => other,
    }
}

// ── Configuration manager ─────────────────────────────────────────────────────

/// Manages anonymization rules loaded from the database with an in-memory cache.
#[derive(Clone)]
pub struct AnonymizationConfig {
    pool: PgPool,
    rules: Arc<RwLock<Vec<AnonymizationRule>>>,
}

impl AnonymizationConfig {
    pub fn new(pool: PgPool) -> Self {
        Self {
            pool,
            rules: Arc::new(RwLock::new(default_rules())),
        }
    }

    /// Load rules from the `anonymization_rules` table, falling back to defaults if empty.
    pub async fn load(&self) -> Result<(), sqlx::Error> {
        let rows = sqlx::query_as::<_, (i32, String, String, String, bool, Option<String>)>(
            "SELECT id, name, pattern, replacement, enabled, description \
             FROM anonymization_rules ORDER BY id",
        )
        .fetch_all(&self.pool)
        .await?;

        if rows.is_empty() {
            debug!("No anonymization rules found in DB, using built-in defaults");
            return Ok(());
        }

        let rules: Vec<AnonymizationRule> = rows
            .into_iter()
            .map(|(id, name, pattern, replacement, enabled, description)| AnonymizationRule {
                id: Some(id),
                name,
                pattern,
                replacement,
                enabled,
                description,
            })
            .collect();

        *self.rules.write().await = rules;
        Ok(())
    }

    /// Upsert a rule in the database and refresh the cache.
    pub async fn upsert_rule(&self, req: &AnonymizationRuleRequest) -> Result<AnonymizationRule, anyhow::Error> {
        // Validate the regex compiles
        Regex::new(&req.pattern)
            .map_err(|e| anyhow::anyhow!("Invalid regex pattern: {}", e))?;

        let replacement = req.replacement.clone().unwrap_or_else(|| "[REDACTED]".into());
        let enabled = req.enabled.unwrap_or(true);

        let row = sqlx::query_as::<_, (i32, String, String, String, bool, Option<String>)>(
            r#"
            INSERT INTO anonymization_rules (name, pattern, replacement, enabled, description)
            VALUES ($1, $2, $3, $4, $5)
            ON CONFLICT (name) DO UPDATE
                SET pattern = EXCLUDED.pattern,
                    replacement = EXCLUDED.replacement,
                    enabled = EXCLUDED.enabled,
                    description = COALESCE(EXCLUDED.description, anonymization_rules.description),
                    updated_at = NOW()
            RETURNING id, name, pattern, replacement, enabled, description
            "#,
        )
        .bind(&req.name)
        .bind(&req.pattern)
        .bind(&replacement)
        .bind(enabled)
        .bind(&req.description)
        .fetch_one(&self.pool)
        .await?;

        let rule = AnonymizationRule {
            id: Some(row.0),
            name: row.1,
            pattern: row.2,
            replacement: row.3,
            enabled: row.4,
            description: row.5,
        };

        // Refresh cache
        self.load().await.map_err(|e| anyhow::anyhow!("{}", e))?;
        Ok(rule)
    }

    /// Delete a rule by name.
    pub async fn delete_rule(&self, name: &str) -> Result<bool, sqlx::Error> {
        let result = sqlx::query("DELETE FROM anonymization_rules WHERE name = $1")
            .bind(name)
            .execute(&self.pool)
            .await?;
        if result.rows_affected() > 0 {
            self.load().await?;
            Ok(true)
        } else {
            Ok(false)
        }
    }

    /// Get current rules (cloned snapshot).
    pub async fn get_rules(&self) -> Vec<AnonymizationRule> {
        self.rules.read().await.clone()
    }

    /// Compile all enabled rules into (name, regex, replacement) triples.
    pub async fn compile_rules(&self) -> Vec<(String, Regex, String)> {
        self.rules
            .read()
            .await
            .iter()
            .filter(|r| r.enabled)
            .filter_map(|r| {
                Regex::new(&r.pattern)
                    .ok()
                    .map(|re| (r.name.clone(), re, r.replacement.clone()))
            })
            .collect()
    }

    /// Compile all enabled rules into (name, regex) pairs for detection only.
    pub async fn compile_detection_rules(&self) -> Vec<(String, Regex)> {
        self.rules
            .read()
            .await
            .iter()
            .filter(|r| r.enabled)
            .filter_map(|r| Regex::new(&r.pattern).ok().map(|re| (r.name.clone(), re)))
            .collect()
    }
}

// ── Audit logging ─────────────────────────────────────────────────────────────

/// Record an anonymization action in the audit_logs table.
pub async fn audit_anonymization(
    pool: &PgPool,
    event_id: &str,
    applied_rules: &[String],
    actor: &str,
) -> Result<(), sqlx::Error> {
    let changes = serde_json::json!({
        "applied_rules": applied_rules,
    });

    sqlx::query(
        "INSERT INTO audit_logs \
             (event_type, action, resource_type, resource_id, created_by, changes, severity) \
         VALUES ('ANONYMIZE', 'anonymize_event', 'event', $1, $2, $3, 'INFO')",
    )
    .bind(event_id)
    .bind(actor)
    .bind(&changes)
    .execute(pool)
    .await?;

    Ok(())
}

// ── Background worker ─────────────────────────────────────────────────────────

/// Background worker that scans un-anonymized events and redacts PII.
///
/// Runs in a loop, processing up to `batch_size` events per tick.
/// Stops when the shutdown signal fires.
pub async fn run_anonymization_worker(
    pool: PgPool,
    config: AnonymizationConfig,
    batch_size: i64,
    interval: std::time::Duration,
    mut shutdown_rx: tokio::sync::watch::Receiver<bool>,
) {
    info!("Anonymization worker started (batch_size={batch_size})");
    let mut ticker = tokio::time::interval(interval);

    loop {
        tokio::select! {
            _ = ticker.tick() => {
                match process_batch(&pool, &config, batch_size).await {
                    Ok(processed) if processed > 0 => {
                        info!(processed = processed, "Anonymization worker: batch complete");
                    }
                    Ok(_) => {}
                    Err(e) => {
                        warn!(error = %e, "Anonymization worker: batch error");
                    }
                }
            }
            _ = shutdown_rx.changed() => {
                if *shutdown_rx.borrow() {
                    info!("Anonymization worker shutting down");
                    break;
                }
            }
        }
    }
}

async fn process_batch(
    pool: &PgPool,
    config: &AnonymizationConfig,
    batch_size: i64,
) -> Result<u64, anyhow::Error> {
    let detection_rules = config.compile_detection_rules().await;
    if detection_rules.is_empty() {
        return Ok(0);
    }

    // Fetch a batch of non-anonymized events
    let rows: Vec<(Uuid, serde_json::Value)> = sqlx::query_as(
        "SELECT id, event_data FROM events \
         WHERE anonymized = FALSE \
         ORDER BY created_at ASC \
         LIMIT $1",
    )
    .bind(batch_size)
    .fetch_all(pool)
    .await?;

    if rows.is_empty() {
        return Ok(0);
    }

    let compile_rules = config.compile_rules().await;
    let mut total_anonymized: u64 = 0;

    for (id, event_data) in rows {
        let scan = scan_for_pii(&event_data, &detection_rules);
        if scan.detections.is_empty() {
            continue;
        }

        let (redacted, applied) = anonymize_value(event_data, &compile_rules);

        if !applied.is_empty() {
            sqlx::query(
                "UPDATE events SET event_data = $1, anonymized = TRUE WHERE id = $2",
            )
            .bind(&redacted)
            .bind(id)
            .execute(pool)
            .await?;

            // Audit trail
            if let Err(e) = audit_anonymization(pool, &id.to_string(), &applied, "worker").await {
                warn!(event_id = %id, error = %e, "Failed to write anonymization audit log");
            }

            for rule_name in &applied {
                metrics::record_anonymization_applied(rule_name);
            }

            total_anonymized += 1;
        }
    }

    Ok(total_anonymized)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn compile(rules: Vec<AnonymizationRule>) -> Vec<(String, Regex, String)> {
        rules
            .iter()
            .filter(|r| r.enabled)
            .filter_map(|r| {
                Regex::new(&r.pattern)
                    .ok()
                    .map(|re| (r.name.clone(), re, r.replacement.clone()))
            })
            .collect()
    }

    fn detect(rules: &[(String, Regex, String)]) -> Vec<(String, Regex)> {
        rules
            .iter()
            .map(|(n, re, _)| (n.clone(), re.clone()))
            .collect()
    }

    #[test]
    fn detects_email_in_string_value() {
        let rules = compile(default_rules());
        let detect_rules = detect(&rules);
        let v = json!({"contact": "user@example.com"});
        let result = scan_for_pii(&v, &detect_rules);
        assert!(!result.detections.is_empty());
        assert_eq!(result.detections[0].rule_name, "email");
        assert_eq!(result.detections[0].field_path, "contact");
    }

    #[test]
    fn no_pii_returns_empty_detections() {
        let rules = compile(default_rules());
        let detect_rules = detect(&rules);
        let v = json!({"amount": 42, "token": "USDC"});
        let result = scan_for_pii(&v, &detect_rules);
        assert!(result.detections.is_empty());
    }

    #[test]
    fn anonymize_replaces_email() {
        let rules = compile(default_rules());
        let v = json!({"email": "alice@example.com"});
        let (out, applied) = anonymize_value(v, &rules);
        assert_eq!(out["email"], "[REDACTED:email]");
        assert!(applied.contains(&"email".to_string()));
    }

    #[test]
    fn anonymize_nested_value() {
        let rules = compile(default_rules());
        let v = json!({"user": {"contact": "bob@test.org", "amount": 100}});
        let (out, applied) = anonymize_value(v, &rules);
        assert_eq!(out["user"]["contact"], "[REDACTED:email]");
        assert_eq!(out["user"]["amount"], 100);
        assert!(applied.contains(&"email".to_string()));
    }

    #[test]
    fn anonymize_array_value() {
        let rules = compile(default_rules());
        let v = json!({"contacts": ["alice@a.com", "not-an-email"]});
        let (out, applied) = anonymize_value(v, &rules);
        assert_eq!(out["contacts"][0], "[REDACTED:email]");
        assert_eq!(out["contacts"][1], "not-an-email");
        assert!(applied.contains(&"email".to_string()));
    }

    #[test]
    fn non_pii_value_unchanged() {
        let rules = compile(default_rules());
        let v = json!({"tx": "abc123", "amount": 999});
        let (out, applied) = anonymize_value(v, &rules);
        assert_eq!(out["tx"], "abc123");
        assert!(applied.is_empty());
    }

    #[test]
    fn default_rules_all_enabled() {
        let rules = default_rules();
        assert!(rules.iter().all(|r| r.enabled));
    }

    #[test]
    fn default_rules_compile_ok() {
        for rule in default_rules() {
            rule.compile().expect(&format!("Rule '{}' pattern should compile", rule.name));
        }
    }

    #[test]
    fn scan_detects_ssn() {
        let rules = compile(default_rules());
        let detect_rules = detect(&rules);
        let v = json!({"id": "123-45-6789"});
        let result = scan_for_pii(&v, &detect_rules);
        assert!(result.detections.iter().any(|d| d.rule_name == "ssn"));
    }
}
