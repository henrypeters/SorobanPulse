//! Generates content filter configuration scaffolding.
//!
//! The output module shows how to wire the existing `ContentFilter` type
//! into a named subscription, with ready-made predicate examples.

use super::{apply, GeneratedFile, ScaffoldConfig};

const FILTER_TEMPLATE: &str = r#"//! Content filter configuration for {{PASCAL}} subscriptions.
//!
//! Filters are evaluated before each notification is delivered. Only events
//! whose data satisfies every predicate in the active filter set are sent to
//! the callback URL, reducing noise for high-volume contracts.
//!
//! Add this module to your subscription route and pass a `Vec<ContentFilter>`
//! when creating or updating a subscription.

use crate::content_filter::{ContentFilter, FilterOp};

// ---------------------------------------------------------------------------
// Preset filter bundles
// ---------------------------------------------------------------------------

/// Returns filters that pass only high-value transfer events (amount > 1_000_000).
pub fn high_value_only() -> Vec<ContentFilter> {
    vec![ContentFilter {
        path: "$.amount".into(),
        op: FilterOp::Gt,
        value: "1000000".into(),
    }]
}

/// Returns filters that limit events to a specific Stellar account address.
pub fn for_account(account: impl Into<String>) -> Vec<ContentFilter> {
    vec![ContentFilter {
        path: "$.transfer.to".into(),
        op: FilterOp::Eq,
        value: account.into(),
    }]
}

/// Returns filters that match a specific event action (e.g. "transfer", "mint").
pub fn for_action(action: impl Into<String>) -> Vec<ContentFilter> {
    vec![ContentFilter {
        path: "$.action".into(),
        op: FilterOp::Eq,
        value: action.into(),
    }]
}

/// Returns filters scoped to accounts matching a Stellar address regex pattern.
pub fn for_account_pattern(pattern: impl Into<String>) -> Vec<ContentFilter> {
    vec![ContentFilter {
        path: "$.account".into(),
        op: FilterOp::Matches,
        value: pattern.into(),
    }]
}

// ---------------------------------------------------------------------------
// Config struct
// ---------------------------------------------------------------------------

/// Declarative filter configuration for {{PASCAL}} subscriptions.
///
/// Deserializable from JSON so subscribers can supply filters at creation time
/// alongside `callback_url` and `from_ledger`.
#[derive(Debug, serde::Deserialize, serde::Serialize)]
pub struct {{PASCAL}}FilterConfig {
    /// Minimum amount threshold. Events with `$.amount <= min_amount` are dropped.
    pub min_amount: Option<String>,
    /// Restrict to a specific recipient account.
    pub to_account: Option<String>,
    /// Restrict to a specific event action type.
    pub action: Option<String>,
    /// Additional freeform predicates (JSONPath path/op/value triples).
    #[serde(default)]
    pub predicates: Vec<ContentFilter>,
}

impl {{PASCAL}}FilterConfig {
    /// Build a validated `Vec<ContentFilter>` from this config.
    pub fn build(&self) -> Result<Vec<ContentFilter>, String> {
        let mut filters: Vec<ContentFilter> = Vec::new();

        if let Some(ref min) = self.min_amount {
            filters.push(ContentFilter {
                path: "$.amount".into(),
                op: FilterOp::Gt,
                value: min.clone(),
            });
        }
        if let Some(ref account) = self.to_account {
            filters.push(ContentFilter {
                path: "$.transfer.to".into(),
                op: FilterOp::Eq,
                value: account.clone(),
            });
        }
        if let Some(ref action) = self.action {
            filters.push(ContentFilter {
                path: "$.action".into(),
                op: FilterOp::Eq,
                value: action.clone(),
            });
        }

        for predicate in &self.predicates {
            predicate.validate().map_err(|e| e.to_string())?;
            filters.push(predicate.clone());
        }

        Ok(filters)
    }
}

// ---------------------------------------------------------------------------
// Evaluation helper
// ---------------------------------------------------------------------------

/// Returns `true` if `data` passes all filters in `filters`.
/// An empty filter list passes every event (no filtering).
pub fn evaluate_all(filters: &[ContentFilter], data: &serde_json::Value) -> bool {
    filters.iter().all(|f| f.evaluate(data))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn high_value_only_passes_large_amounts() {
        let filters = high_value_only();
        assert!(evaluate_all(&filters, &json!({ "amount": "5000000" })));
        assert!(!evaluate_all(&filters, &json!({ "amount": "100" })));
    }

    #[test]
    fn for_account_matches_exact_address() {
        let account = "GABC1234567890";
        let filters = for_account(account);
        assert!(evaluate_all(
            &filters,
            &json!({ "transfer": { "to": account } })
        ));
        assert!(!evaluate_all(
            &filters,
            &json!({ "transfer": { "to": "GOTHER" } })
        ));
    }

    #[test]
    fn for_action_filters_by_event_type() {
        let filters = for_action("transfer");
        assert!(evaluate_all(&filters, &json!({ "action": "transfer" })));
        assert!(!evaluate_all(&filters, &json!({ "action": "mint" })));
    }

    #[test]
    fn empty_filters_pass_everything() {
        let filters: Vec<ContentFilter> = vec![];
        assert!(evaluate_all(&filters, &json!({ "any": "value" })));
    }

    #[test]
    fn config_build_validates_predicates() {
        let cfg = {{PASCAL}}FilterConfig {
            min_amount: Some("500000".into()),
            to_account: None,
            action: Some("transfer".into()),
            predicates: vec![],
        };
        let filters = cfg.build().expect("valid config");
        assert_eq!(filters.len(), 2);
    }

    #[test]
    fn config_build_rejects_bad_predicate_path() {
        let cfg = {{PASCAL}}FilterConfig {
            min_amount: None,
            to_account: None,
            action: None,
            predicates: vec![ContentFilter {
                path: "bad_path".into(), // must start with $
                op: FilterOp::Eq,
                value: "x".into(),
            }],
        };
        assert!(cfg.build().is_err());
    }
}
"#;

pub fn generate(config: &ScaffoldConfig) -> GeneratedFile {
    GeneratedFile {
        relative_path: format!("src/{}_filter_config.rs", config.snake_name),
        content: apply(FILTER_TEMPLATE, config),
    }
}

#[cfg(test)]
mod meta_tests {
    use super::*;
    use crate::codegen::{ChannelType, ScaffoldConfig};

    #[test]
    fn generated_file_contains_struct_name() {
        let cfg = ScaffoldConfig::new("payment", ChannelType::Webhook, false, false);
        let f = generate(&cfg);
        assert!(f.content.contains("PaymentFilterConfig"));
    }

    #[test]
    fn generated_file_path_uses_snake_name() {
        let cfg = ScaffoldConfig::new("nft-sale", ChannelType::Webhook, false, false);
        let f = generate(&cfg);
        assert_eq!(f.relative_path, "src/nft_sale_filter_config.rs");
    }
}
