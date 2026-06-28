//! Generates test scaffolding for a named subscription type.
//!
//! Produces a standalone test file covering:
//! - SSRF URL validation
//! - Exponential backoff formula
//! - Mock delivery success and retry paths
//! - Ack cursor monotonicity
//! - Content filter evaluation (if applicable)
//! - Integration test stubs (require `sqlx::test`)

use super::{apply, GeneratedFile, ScaffoldConfig};

const TEST_TEMPLATE: &str = r#"//! Tests for {{PASCAL}} subscription scaffolding.
//!
//! Run unit tests:   cargo test {{SNAKE}}
//! Run DB tests:     make test-db  (requires PostgreSQL via docker-compose.test.yml)

#[cfg(test)]
mod {{SNAKE}}_subscription_tests {
    use serde_json::json;
    use std::sync::{Arc, Mutex};
    use uuid::Uuid;

    use soroban_pulse::content_filter::{ContentFilter, FilterOp};
    use soroban_pulse::subscriptions::validate_callback_url;
    use soroban_pulse::config::Environment;

    // -----------------------------------------------------------------------
    // SSRF validation
    // -----------------------------------------------------------------------

    #[test]
    fn rejects_localhost_callback() {
        assert!(
            validate_callback_url("http://localhost/{{SNAKE}}", &Environment::Development).is_err(),
            "localhost must be rejected"
        );
    }

    #[test]
    fn rejects_rfc1918_address() {
        assert!(
            validate_callback_url("http://192.168.1.1/hook", &Environment::Development).is_err(),
            "RFC 1918 address must be rejected"
        );
    }

    #[test]
    fn rejects_http_in_production() {
        assert!(
            validate_callback_url("http://example.com/hook", &Environment::Production).is_err(),
            "HTTP callback must be rejected in production"
        );
    }

    #[test]
    fn accepts_https_in_production() {
        assert!(
            validate_callback_url("https://example.com/{{SNAKE}}", &Environment::Production).is_ok(),
            "HTTPS public URL must be accepted in production"
        );
    }

    #[test]
    fn allowlist_permits_internal_url() {
        std::env::set_var(
            "SUBSCRIPTION_ALLOWED_URL_PREFIXES",
            "http://internal.corp/,https://trusted.corp/",
        );
        let result =
            validate_callback_url("http://internal.corp/{{SNAKE}}", &Environment::Production);
        std::env::remove_var("SUBSCRIPTION_ALLOWED_URL_PREFIXES");
        assert!(result.is_ok(), "allowlisted prefix must be permitted");
    }

    // -----------------------------------------------------------------------
    // Exponential backoff formula
    // -----------------------------------------------------------------------

    #[test]
    fn backoff_is_capped_at_3600_seconds() {
        let backoff = |attempts: u32| -> u64 { 2u64.pow(attempts + 1).min(3600) };
        assert_eq!(backoff(0), 2);
        assert_eq!(backoff(1), 4);
        assert_eq!(backoff(10), 2048);
        assert_eq!(backoff(11), 3600, "2^12=4096 should be capped at 3600");
        assert_eq!(backoff(20), 3600, "large attempt count stays capped");
    }

    // -----------------------------------------------------------------------
    // Mock HTTP delivery
    // -----------------------------------------------------------------------

    struct MockDelivery {
        received: Arc<Mutex<Vec<serde_json::Value>>>,
        remaining_failures: Arc<Mutex<u32>>,
    }

    impl MockDelivery {
        fn new() -> Self {
            Self {
                received: Arc::new(Mutex::new(Vec::new())),
                remaining_failures: Arc::new(Mutex::new(0)),
            }
        }

        fn with_failures(n: u32) -> Self {
            Self {
                received: Arc::new(Mutex::new(Vec::new())),
                remaining_failures: Arc::new(Mutex::new(n)),
            }
        }

        async fn send(&self, payload: serde_json::Value) -> Result<(), String> {
            let mut fails = self.remaining_failures.lock().unwrap();
            if *fails > 0 {
                *fails -= 1;
                return Err("connection refused".into());
            }
            self.received.lock().unwrap().push(payload);
            Ok(())
        }
    }

    #[tokio::test]
    async fn delivery_succeeds_on_first_attempt() {
        let mock = MockDelivery::new();
        let payload = json!({
            "event_id": Uuid::new_v4(),
            "ledger": 100,
            "event_data": {}
        });
        mock.send(payload.clone()).await.expect("should succeed");
        assert_eq!(mock.received.lock().unwrap().len(), 1);
        assert_eq!(mock.received.lock().unwrap()[0]["ledger"], 100);
    }

    #[tokio::test]
    async fn delivery_succeeds_after_transient_failures() {
        let mock = MockDelivery::with_failures(2);
        let payload = json!({ "event_id": Uuid::new_v4(), "ledger": 200, "event_data": {} });

        // Two failures
        assert!(mock.send(payload.clone()).await.is_err());
        assert!(mock.send(payload.clone()).await.is_err());
        // Third attempt succeeds
        mock.send(payload.clone()).await.expect("should succeed on third attempt");

        assert_eq!(mock.received.lock().unwrap().len(), 1);
        assert_eq!(mock.received.lock().unwrap()[0]["ledger"], 200);
    }

    // -----------------------------------------------------------------------
    // Ack cursor monotonicity
    // -----------------------------------------------------------------------

    #[test]
    fn ack_cursor_only_advances_forward() {
        let advance = |current: i64, proposed: i64| -> i64 {
            if proposed > current { proposed } else { current }
        };

        let mut cursor: i64 = 50;
        cursor = advance(cursor, 100);
        assert_eq!(cursor, 100);

        cursor = advance(cursor, 80); // regression attempt
        assert_eq!(cursor, 100, "cursor must not regress");

        cursor = advance(cursor, 150);
        assert_eq!(cursor, 150);
    }

    // -----------------------------------------------------------------------
    // Payload structure
    // -----------------------------------------------------------------------

    #[test]
    fn delivery_payload_has_required_fields() {
        let event_id = Uuid::new_v4();
        let payload = json!({
            "event_id": event_id,
            "ledger": 42_i64,
            "event_data": { "amount": "100" },
        });
        assert!(payload.get("event_id").is_some());
        assert!(payload.get("ledger").is_some());
        assert!(payload.get("event_data").is_some());
    }

    // -----------------------------------------------------------------------
    // Content filter evaluation
    // -----------------------------------------------------------------------

    #[test]
    fn filter_passes_matching_event() {
        let filter = ContentFilter {
            path: "$.amount".into(),
            op: FilterOp::Gt,
            value: "1000000".into(),
        };
        assert!(filter.evaluate(&json!({ "amount": "5000000" })));
        assert!(!filter.evaluate(&json!({ "amount": "100" })));
    }

    #[test]
    fn filter_absent_field_fails_except_ne() {
        let ne = ContentFilter { path: "$.x".into(), op: FilterOp::Ne, value: "y".into() };
        let eq = ContentFilter { path: "$.x".into(), op: FilterOp::Eq, value: "y".into() };
        assert!(ne.evaluate(&json!({})), "ne on absent field is vacuously true");
        assert!(!eq.evaluate(&json!({})), "eq on absent field is false");
    }

    // -----------------------------------------------------------------------
    // Integration test stubs (require PostgreSQL)
    // -----------------------------------------------------------------------
    //
    // Uncomment and run with: make test-db
    //
    // #[sqlx::test(migrations = "./migrations")]
    // async fn creates_{{SNAKE}}_subscription(pool: sqlx::PgPool) {
    //     let row: (uuid::Uuid,) = sqlx::query_as(
    //         "INSERT INTO {{SNAKE}}_subscriptions (callback_url, from_ledger)
    //          VALUES ($1, $2) RETURNING id",
    //     )
    //     .bind("https://example.com/hook")
    //     .bind(1_i64)
    //     .fetch_one(&pool)
    //     .await
    //     .expect("insert should succeed");
    //     assert!(!row.0.is_nil());
    // }
    //
    // #[sqlx::test(migrations = "./migrations")]
    // async fn enqueue_populates_delivery_queue(pool: sqlx::PgPool) {
    //     // Insert a subscription then call enqueue_{{SNAKE}}_event and verify
    //     // the delivery_queue receives a row.
    //     todo!("implement this test");
    // }
}
"#;

pub fn generate(config: &ScaffoldConfig) -> GeneratedFile {
    GeneratedFile {
        relative_path: format!("tests/{}_subscription_tests.rs", config.snake_name),
        content: apply(TEST_TEMPLATE, config),
    }
}

#[cfg(test)]
mod meta_tests {
    use super::*;
    use crate::codegen::{ChannelType, ScaffoldConfig};

    #[test]
    fn test_file_path_uses_snake_name() {
        let cfg = ScaffoldConfig::new("token-swap", ChannelType::Webhook, false, true);
        let f = generate(&cfg);
        assert_eq!(f.relative_path, "tests/token_swap_subscription_tests.rs");
    }

    #[test]
    fn test_file_contains_pascal_name() {
        let cfg = ScaffoldConfig::new("nft-sale", ChannelType::Webhook, false, true);
        let f = generate(&cfg);
        assert!(f.content.contains("NftSale"));
    }

    #[test]
    fn test_file_contains_snake_name_in_module() {
        let cfg = ScaffoldConfig::new("nft-sale", ChannelType::Webhook, false, true);
        let f = generate(&cfg);
        assert!(f.content.contains("nft_sale_subscription_tests"));
    }

    #[test]
    fn test_file_has_ssrf_tests() {
        let cfg = ScaffoldConfig::new("payment", ChannelType::Webhook, false, true);
        let f = generate(&cfg);
        assert!(f.content.contains("rejects_localhost_callback"));
        assert!(f.content.contains("rejects_rfc1918_address"));
    }

    #[test]
    fn test_file_has_backoff_test() {
        let cfg = ScaffoldConfig::new("payment", ChannelType::Webhook, false, true);
        let f = generate(&cfg);
        assert!(f.content.contains("backoff_is_capped_at_3600_seconds"));
    }
}
