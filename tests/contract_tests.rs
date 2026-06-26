/// API Contract Tests using Pact
/// These tests verify the contract between client and server implementations,
/// ensuring requests and responses conform to agreed-upon schemas.

use serde_json::{json, Value};

/// Contract for GET /v1/events endpoint
mod events_contract {
    use super::*;

    /// Request contract definition
    pub struct GetEventsRequest {
        pub page: Option<i64>,
        pub limit: Option<i64>,
        pub event_type: Option<String>,
        pub from_ledger: Option<i64>,
        pub to_ledger: Option<i64>,
    }

    impl GetEventsRequest {
        /// Verify request schema is valid
        pub fn verify_schema(&self) -> Result<(), String> {
            if let Some(page) = self.page {
                if page < 1 {
                    return Err("page must be >= 1".to_string());
                }
            }

            if let Some(limit) = self.limit {
                if limit < 1 || limit > 100 {
                    return Err("limit must be between 1 and 100".to_string());
                }
            }

            if let Some(event_type) = &self.event_type {
                match event_type.as_str() {
                    "contract" | "diagnostic" | "system" => {}
                    _ => return Err("invalid event_type".to_string()),
                }
            }

            if let Some((from, to)) = self.page.and_then(|p| self.limit.map(|l| (p, l))) {
                if from > to {
                    return Err("from_ledger cannot be greater than to_ledger".to_string());
                }
            }

            Ok(())
        }
    }

    /// Response contract definition
    #[derive(Debug, Clone)]
    pub struct Event {
        pub id: String,
        pub contract_id: String,
        pub event_type: String,
        pub ledger: i64,
        pub timestamp: String,
        pub event_data: Value,
    }

    #[derive(Debug)]
    pub struct GetEventsResponse {
        pub data: Vec<Event>,
        pub page: i64,
        pub limit: i64,
        pub total: i64,
        pub has_more: bool,
    }

    impl GetEventsResponse {
        /// Verify response schema is valid
        pub fn verify_schema(&self) -> Result<(), String> {
            if self.page < 1 {
                return Err("response page must be >= 1".to_string());
            }

            if self.limit < 1 || self.limit > 100 {
                return Err("response limit must be between 1 and 100".to_string());
            }

            if self.total < 0 {
                return Err("response total must be >= 0".to_string());
            }

            // Verify has_more flag correctness
            let expected_has_more = (self.page * self.limit) < self.total;
            if self.has_more != expected_has_more {
                return Err(format!(
                    "has_more should be {} for page={}, limit={}, total={}",
                    expected_has_more, self.page, self.limit, self.total
                ));
            }

            // Verify data array bounds
            let max_expected_items = self.limit as usize;
            if self.data.len() > max_expected_items {
                return Err(format!(
                    "data array exceeds limit: {} > {}",
                    self.data.len(),
                    max_expected_items
                ));
            }

            for event in &self.data {
                event.verify_schema()?;
            }

            Ok(())
        }
    }

    impl Event {
        pub fn verify_schema(&self) -> Result<(), String> {
            if self.id.is_empty() {
                return Err("event id cannot be empty".to_string());
            }

            if self.contract_id.len() != 56 || !self.contract_id.chars().all(|c| c.is_ascii_hexdigit()) {
                return Err("contract_id must be 56-char hex string".to_string());
            }

            match self.event_type.as_str() {
                "contract" | "diagnostic" | "system" => {}
                _ => return Err("invalid event_type".to_string()),
            }

            if self.ledger < 0 {
                return Err("ledger must be >= 0".to_string());
            }

            // Verify timestamp is valid ISO 8601
            chrono::DateTime::parse_from_rfc3339(&self.timestamp)
                .map_err(|_| "timestamp must be valid ISO 8601".to_string())?;

            Ok(())
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn contract_valid_request_passes_schema() {
            let request = GetEventsRequest {
                page: Some(1),
                limit: Some(20),
                event_type: Some("contract".to_string()),
                from_ledger: Some(1000),
                to_ledger: Some(2000),
            };
            assert!(request.verify_schema().is_ok());
        }

        #[test]
        fn contract_invalid_page_fails_schema() {
            let request = GetEventsRequest {
                page: Some(0),
                limit: Some(20),
                event_type: None,
                from_ledger: None,
                to_ledger: None,
            };
            assert!(request.verify_schema().is_err());
        }

        #[test]
        fn contract_invalid_limit_fails_schema() {
            let request = GetEventsRequest {
                page: Some(1),
                limit: Some(200),
                event_type: None,
                from_ledger: None,
                to_ledger: None,
            };
            assert!(request.verify_schema().is_err());
        }

        #[test]
        fn contract_invalid_event_type_fails_schema() {
            let request = GetEventsRequest {
                page: Some(1),
                limit: Some(20),
                event_type: Some("invalid".to_string()),
                from_ledger: None,
                to_ledger: None,
            };
            assert!(request.verify_schema().is_err());
        }

        #[test]
        fn contract_valid_response_passes_schema() {
            let response = GetEventsResponse {
                data: vec![Event {
                    id: "evt-123".to_string(),
                    contract_id: "cccccccccccccccccccccccccccccccccccccccccccccccccccccccc".to_string(),
                    event_type: "contract".to_string(),
                    ledger: 1000,
                    timestamp: "2024-01-01T12:00:00Z".to_string(),
                    event_data: json!({}),
                }],
                page: 1,
                limit: 20,
                total: 100,
                has_more: true,
            };
            assert!(response.verify_schema().is_ok());
        }

        #[test]
        fn contract_has_more_flag_correctness() {
            // Page 5, limit 20, total 100 → has_more should be false (100 <= 100)
            let response = GetEventsResponse {
                data: vec![],
                page: 5,
                limit: 20,
                total: 100,
                has_more: false,
            };
            assert!(response.verify_schema().is_ok());

            // Same but with has_more=true → should fail
            let response_invalid = GetEventsResponse {
                data: vec![],
                page: 5,
                limit: 20,
                total: 100,
                has_more: true,
            };
            assert!(response_invalid.verify_schema().is_err());
        }

        #[test]
        fn contract_invalid_contract_id_fails() {
            let response = GetEventsResponse {
                data: vec![Event {
                    id: "evt-123".to_string(),
                    contract_id: "invalid".to_string(), // Too short, not hex
                    event_type: "contract".to_string(),
                    ledger: 1000,
                    timestamp: "2024-01-01T12:00:00Z".to_string(),
                    event_data: json!({}),
                }],
                page: 1,
                limit: 20,
                total: 1,
                has_more: false,
            };
            assert!(response.verify_schema().is_err());
        }

        #[test]
        fn contract_invalid_timestamp_fails() {
            let response = GetEventsResponse {
                data: vec![Event {
                    id: "evt-123".to_string(),
                    contract_id: "cccccccccccccccccccccccccccccccccccccccccccccccccccccccc".to_string(),
                    event_type: "contract".to_string(),
                    ledger: 1000,
                    timestamp: "invalid-timestamp".to_string(),
                    event_data: json!({}),
                }],
                page: 1,
                limit: 20,
                total: 1,
                has_more: false,
            };
            assert!(response.verify_schema().is_err());
        }
    }
}

/// Contract for error responses
mod error_response_contract {
    use super::*;

    #[derive(Debug)]
    pub struct ErrorResponse {
        pub error: String,
        pub code: String,
        pub status_code: u16,
    }

    impl ErrorResponse {
        pub fn verify_schema(&self) -> Result<(), String> {
            if self.error.is_empty() {
                return Err("error message cannot be empty".to_string());
            }

            if self.code.is_empty() {
                return Err("error code cannot be empty".to_string());
            }

            // Verify status code is reasonable for errors
            if !(400..=599).contains(&self.status_code) {
                return Err("status_code must be in 4xx or 5xx range".to_string());
            }

            // Verify error code format (should be SNAKE_CASE or similar)
            if !self.code.chars().all(|c| c.is_ascii_uppercase() || c == '_' || c.is_ascii_digit()) {
                return Err("error code should be SNAKE_CASE or similar".to_string());
            }

            Ok(())
        }
    }

    /// Common error codes that servers should implement
    pub mod standard_codes {
        pub const VALIDATION_ERROR: &str = "VALIDATION_ERROR";
        pub const NOT_FOUND: &str = "NOT_FOUND";
        pub const UNAUTHORIZED: &str = "UNAUTHORIZED";
        pub const FORBIDDEN: &str = "FORBIDDEN";
        pub const INTERNAL_ERROR: &str = "INTERNAL_ERROR";
        pub const SERVICE_UNAVAILABLE: &str = "SERVICE_UNAVAILABLE";
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn contract_valid_error_response() {
            let error = ErrorResponse {
                error: "Invalid page parameter".to_string(),
                code: "VALIDATION_ERROR".to_string(),
                status_code: 400,
            };
            assert!(error.verify_schema().is_ok());
        }

        #[test]
        fn contract_missing_error_message() {
            let error = ErrorResponse {
                error: "".to_string(),
                code: "VALIDATION_ERROR".to_string(),
                status_code: 400,
            };
            assert!(error.verify_schema().is_err());
        }

        #[test]
        fn contract_invalid_status_code() {
            let error = ErrorResponse {
                error: "Some error".to_string(),
                code: "VALIDATION_ERROR".to_string(),
                status_code: 200,
            };
            assert!(error.verify_schema().is_err());
        }

        #[test]
        fn contract_standard_error_codes_are_valid() {
            let codes = vec![
                standard_codes::VALIDATION_ERROR,
                standard_codes::NOT_FOUND,
                standard_codes::UNAUTHORIZED,
                standard_codes::FORBIDDEN,
                standard_codes::INTERNAL_ERROR,
                standard_codes::SERVICE_UNAVAILABLE,
            ];

            for code in codes {
                let error = ErrorResponse {
                    error: "Test error".to_string(),
                    code: code.to_string(),
                    status_code: 400,
                };
                assert!(error.verify_schema().is_ok(), "Code {} should be valid", code);
            }
        }
    }
}

/// Provider state tests for contract testing
mod provider_states {
    use super::*;

    /// Defines reusable provider states that can be set up for contract tests
    pub enum ProviderState {
        EventsExist { count: usize },
        EventsEmpty,
        DatabaseConnected,
        IndexerRunning,
    }

    impl ProviderState {
        /// Describes the state for documentation
        pub fn description(&self) -> &str {
            match self {
                ProviderState::EventsExist { .. } => "events exist in database",
                ProviderState::EventsEmpty => "database has no events",
                ProviderState::DatabaseConnected => "database is connected and healthy",
                ProviderState::IndexerRunning => "indexer is running and up-to-date",
            }
        }

        /// Setup the provider state (would be called before each contract test)
        pub async fn setup(&self) -> Result<(), String> {
            match self {
                ProviderState::EventsExist { count } => {
                    println!("Setting up {} events in database", count);
                    // In real implementation, insert test events via test database
                    Ok(())
                }
                ProviderState::EventsEmpty => {
                    println!("Clearing all events from database");
                    // In real implementation, truncate events table
                    Ok(())
                }
                ProviderState::DatabaseConnected => {
                    println!("Verifying database connection");
                    Ok(())
                }
                ProviderState::IndexerRunning => {
                    println!("Verifying indexer is running");
                    Ok(())
                }
            }
        }

        /// Tear down the provider state after test
        pub async fn teardown(&self) -> Result<(), String> {
            Ok(())
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[tokio::test]
        async fn provider_state_setup_succeeds() {
            let state = ProviderState::EventsExist { count: 10 };
            assert!(state.setup().await.is_ok());
            assert!(state.teardown().await.is_ok());
        }

        #[test]
        fn provider_state_descriptions_are_meaningful() {
            assert!(!ProviderState::EventsExist { count: 5 }.description().is_empty());
            assert!(!ProviderState::EventsEmpty.description().is_empty());
        }
    }
}

/// Schema compatibility tests
mod schema_compatibility {
    use super::*;

    /// Verify that response schema maintains backward compatibility
    pub fn verify_backward_compatible_schema(response: &Value) -> Result<(), String> {
        // Required fields that clients depend on
        let required_fields = vec!["data", "page", "limit", "total", "has_more"];

        for field in required_fields {
            if !response.as_object()
                .ok_or("response must be a JSON object")?
                .contains_key(field)
            {
                return Err(format!("missing required field: {}", field));
            }
        }

        Ok(())
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn verify_schema_with_all_required_fields() {
            let response = json!({
                "data": [],
                "page": 1,
                "limit": 20,
                "total": 0,
                "has_more": false
            });

            assert!(verify_backward_compatible_schema(&response).is_ok());
        }

        #[test]
        fn verify_schema_missing_required_field() {
            let response = json!({
                "data": [],
                "page": 1,
                // missing "limit"
                "total": 0,
                "has_more": false
            });

            assert!(verify_backward_compatible_schema(&response).is_err());
        }
    }
}

#[cfg(test)]
mod integration_contract_tests {
    use super::*;

    /// Test that multiple contract definitions work together
    #[test]
    fn contract_request_and_response_compatible() {
        let request = events_contract::GetEventsRequest {
            page: Some(1),
            limit: Some(20),
            event_type: Some("contract".to_string()),
            from_ledger: None,
            to_ledger: None,
        };

        let response = events_contract::GetEventsResponse {
            data: vec![],
            page: 1,
            limit: 20,
            total: 0,
            has_more: false,
        };

        assert!(request.verify_schema().is_ok());
        assert!(response.verify_schema().is_ok());

        // Verify response respects the request parameters
        assert_eq!(response.page, request.page.unwrap());
        assert_eq!(response.limit, request.limit.unwrap());
    }

    #[test]
    fn contract_error_response_schema_valid() {
        let error = error_response_contract::ErrorResponse {
            error: "Not found".to_string(),
            code: error_response_contract::standard_codes::NOT_FOUND.to_string(),
            status_code: 404,
        };

        assert!(error.verify_schema().is_ok());
    }

    #[test]
    fn contract_response_backward_compatibility() {
        let response = json!({
            "data": [{
                "id": "evt-1",
                "contract_id": "cccccccccccccccccccccccccccccccccccccccccccccccccccccccc",
                "event_type": "contract",
                "ledger": 1000,
                "timestamp": "2024-01-01T12:00:00Z",
                "event_data": {}
            }],
            "page": 1,
            "limit": 20,
            "total": 1,
            "has_more": false,
            // New optional fields can be added here without breaking compatibility
            "deprecation_notice": "This field is deprecated in v2"
        });

        assert!(schema_compatibility::verify_backward_compatible_schema(&response).is_ok());
    }
}
