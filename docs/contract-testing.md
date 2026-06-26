# API Contract Testing Guide

## Overview

**Contract testing** verifies that APIs fulfill their contracts — ensuring client and server implementations maintain compatibility. Unlike integration tests (which test a full system), contract tests focus on the agreement between specific components:

```
Client Code           API Server
    ↓                     ↓
  [Request]  ←→  [Contract]  ←→  [Response]
    ↑                     ↑
 Expects:             Provides:
 - Valid response     - Correct schema
 - Known fields       - Expected fields
 - Typed values       - Consistent behavior
```

## Why Contract Testing?

### Problem: Coupling Between Client & Server

```rust
// Server changes response schema
pub struct Event {
    pub id: String,
    pub contract_id: String,
    // pub ledger: i64,  ← REMOVED
    pub timestamp: String,
}

// Client breaks silently
let event = response.data[0];
println!("{}", event.ledger);  // panic! field missing
```

### Solution: Contractual Agreement

Define a **contract** that both client and server respect:

```
Contract: GET /v1/events response includes:
  - data: Event[]
  - page: i64
  - limit: i64
  - total: i64
  - has_more: bool
  - Each Event has: id, contract_id, event_type, ledger, timestamp, event_data
```

If either side violates the contract, tests fail **immediately**.

## Contract Types in This Project

### 1. Request/Response Schemas

Define the shape of requests and responses:

```rust
pub struct GetEventsRequest {
    pub page: Option<i64>,
    pub limit: Option<i64>,
    pub event_type: Option<String>,
}

pub struct GetEventsResponse {
    pub data: Vec<Event>,
    pub page: i64,
    pub limit: i64,
    pub total: i64,
    pub has_more: bool,
}
```

### 2. Schema Validation

Verify that values conform to the contract:

```rust
impl GetEventsRequest {
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
        Ok(())
    }
}
```

### 3. Provider States

Test-friendly environment states (data setup):

```rust
pub enum ProviderState {
    EventsExist { count: usize },
    EventsEmpty,
    DatabaseConnected,
}
```

Before running contract tests against a state, set up the provider:
- `EventsExist { count: 100 }` → Insert 100 test events
- `EventsEmpty` → Clear all events
- `DatabaseConnected` → Verify DB connectivity

### 4. Error Response Contracts

Standardize error handling:

```rust
pub struct ErrorResponse {
    pub error: String,      // "Invalid page parameter"
    pub code: String,       // "VALIDATION_ERROR"
    pub status_code: u16,   // 400
}
```

**Standard error codes** (all clients should understand):
- `VALIDATION_ERROR` (400)
- `NOT_FOUND` (404)
- `UNAUTHORIZED` (401)
- `FORBIDDEN` (403)
- `INTERNAL_ERROR` (500)
- `SERVICE_UNAVAILABLE` (503)

## Running Contract Tests

### Local Tests

```bash
# Run all contract tests
cargo test --test contract_tests

# Run specific contract
cargo test --test contract_tests events_contract::

# Verbose output
cargo test --test contract_tests -- --nocapture
```

### Example Output

```
running 8 tests
test events_contract::tests::contract_valid_request_passes_schema ... ok
test events_contract::tests::contract_invalid_page_fails_schema ... ok
test error_response_contract::tests::contract_valid_error_response ... ok
...
test result: ok. 8 passed; 0 failed; 0 ignored
```

## Schema Contracts

### Request Contract: GET /v1/events

```json
{
  "page": number (optional, >= 1, default: 1),
  "limit": number (optional, 1-100, default: 20),
  "event_type": string (optional, one of: "contract", "diagnostic", "system"),
  "from_ledger": number (optional, >= 0),
  "to_ledger": number (optional, >= 0)
}
```

**Validation rules:**
- `page`: Must be >= 1 if provided
- `limit`: Must be between 1 and 100 (inclusive)
- `event_type`: Must match one of the allowed values
- `from_ledger` ≤ `to_ledger`: If both provided, from must not exceed to

### Response Contract: GET /v1/events

```json
{
  "data": [
    {
      "id": string,
      "contract_id": string (56-char hex),
      "event_type": string (one of: "contract", "diagnostic", "system"),
      "ledger": number (>= 0),
      "timestamp": string (ISO 8601),
      "event_data": object
    }
  ],
  "page": number (>= 1),
  "limit": number (1-100),
  "total": number (>= 0),
  "has_more": boolean
}
```

**Validation rules:**
- `contract_id`: Must be exactly 56 hexadecimal characters
- `timestamp`: Must be valid RFC 3339 / ISO 8601
- `has_more`: Must equal `(page * limit) < total`
- `data` array length: Must not exceed `limit`
- All required fields must be present

### Error Response Contract

```json
{
  "error": string,    // Human-readable message
  "code": string,     // SCREAMING_SNAKE_CASE error code
  "status_code": number (400-599)
}
```

**Common errors:**
```json
{
  "error": "Invalid page parameter",
  "code": "VALIDATION_ERROR",
  "status_code": 400
}
```

## Contract Testing Patterns

### 1. Test Valid Requests/Responses

```rust
#[test]
fn contract_valid_request_passes_schema() {
    let request = GetEventsRequest {
        page: Some(1),
        limit: Some(20),
        event_type: Some("contract".to_string()),
        from_ledger: None,
        to_ledger: None,
    };
    assert!(request.verify_schema().is_ok());
}
```

### 2. Test Invalid Values

```rust
#[test]
fn contract_invalid_limit_fails_schema() {
    let request = GetEventsRequest {
        page: Some(1),
        limit: Some(200),  // Exceeds max of 100
        event_type: None,
        from_ledger: None,
        to_ledger: None,
    };
    assert!(request.verify_schema().is_err());
}
```

### 3. Test Interdependencies

```rust
#[test]
fn contract_has_more_flag_correctness() {
    let response = GetEventsResponse {
        data: vec![],
        page: 5,
        limit: 20,
        total: 100,
        has_more: false,  // (5 * 20) = 100, which is NOT < 100
    };
    assert!(response.verify_schema().is_ok());

    let response_invalid = GetEventsResponse {
        data: vec![],
        page: 5,
        limit: 20,
        total: 100,
        has_more: true,  // WRONG
    };
    assert!(response_invalid.verify_schema().is_err());
}
```

### 4. Test Backward Compatibility

```rust
fn verify_backward_compatible_schema(response: &Value) -> Result<(), String> {
    let required_fields = vec!["data", "page", "limit", "total", "has_more"];
    
    for field in required_fields {
        if !response.as_object()?.contains_key(field) {
            return Err(format!("missing required field: {}", field));
        }
    }
    Ok(())
}
```

New fields can be added to responses without breaking old clients:
```json
{
  "data": [],
  "page": 1,
  "limit": 20,
  "total": 0,
  "has_more": false,
  "new_feature_v2": "clients ignore unknown fields"  // ✓ OK
}
```

## Provider States

Setup reproducible test environments:

```rust
pub enum ProviderState {
    EventsExist { count: usize },
    EventsEmpty,
    DatabaseConnected,
    IndexerRunning,
}

impl ProviderState {
    pub async fn setup(&self) -> Result<(), String> {
        match self {
            ProviderState::EventsExist { count } => {
                // Insert test events
                insert_test_events(*count).await?;
            }
            ProviderState::EventsEmpty => {
                // Clear all events
                truncate_events_table().await?;
            }
            // ...
        }
        Ok(())
    }
}
```

Use provider states in tests:

```rust
#[tokio::test]
async fn contract_response_when_events_exist() {
    ProviderState::EventsExist { count: 50 }.setup().await.unwrap();
    
    let response = fetch_events(1, 20).await;
    assert!(response.total >= 50);
    assert_eq!(response.data.len(), 20);
}
```

## CI Integration

Contract tests run as part of the standard test suite:

```bash
# In CI
cargo test --test contract_tests

# These also verify response compatibility in integration tests
cargo test --test integration_tests
```

The tests are **fast** (no network) and **deterministic** (no external dependencies).

## Example: Adding a New Contract

### 1. Define Request & Response Structures

```rust
pub struct CreateEventRequest {
    pub contract_id: String,
    pub event_type: String,
    pub event_data: Value,
}

pub struct CreateEventResponse {
    pub id: String,
    pub created_at: String,
}
```

### 2. Add Schema Validation

```rust
impl CreateEventRequest {
    pub fn verify_schema(&self) -> Result<(), String> {
        if self.contract_id.len() != 56 {
            return Err("contract_id must be 56 chars".to_string());
        }
        match self.event_type.as_str() {
            "contract" | "diagnostic" | "system" => {}
            _ => return Err("invalid event_type".to_string()),
        }
        Ok(())
    }
}

impl CreateEventResponse {
    pub fn verify_schema(&self) -> Result<(), String> {
        if self.id.is_empty() {
            return Err("id cannot be empty".to_string());
        }
        // Verify timestamp
        chrono::DateTime::parse_from_rfc3339(&self.created_at)?;
        Ok(())
    }
}
```

### 3. Add Tests

```rust
#[test]
fn contract_create_event_valid_request() {
    let request = CreateEventRequest { /* ... */ };
    assert!(request.verify_schema().is_ok());
}

#[test]
fn contract_create_event_response_has_id() {
    let response = CreateEventResponse { /* ... */ };
    assert!(response.verify_schema().is_ok());
}
```

## Versioning Contracts

When changing API contracts:

### Breaking Changes (Avoid if Possible)
- Removing required fields
- Changing field types
- Renaming fields

### Non-Breaking Changes (Always OK)
- Adding new optional fields
- Making optional fields required (if old clients won't hit them)
- Adding new error codes
- Expanding allowed values in enums

```rust
// Breaking: removing 'ledger'
pub struct Event {
    pub id: String,
    // pub ledger: i64,  ← BREAKING
    pub contract_id: String,
}

// Non-Breaking: adding new field
pub struct Event {
    pub id: String,
    pub contract_id: String,
    pub ledger: i64,
    pub metadata: Option<Value>,  // ← Non-breaking (optional)
}

// Versioning strategy
pub struct EventV1 { /* old schema */ }
pub struct EventV2 { /* new schema */ }

// Clients choose which version to use
impl EventV1 {
    pub fn to_v2(self) -> EventV2 { /* convert */ }
}
```

## Pact Broker Integration

For advanced contract testing with independent client/server teams:

### Install pact

```bash
npm install -g @pact-foundation/pact
```

### Publish contract

```bash
pact-broker publish tests/pacts \
  --consumer-app-version 1.0.0 \
  --broker-base-url https://broker.example.com
```

### Verify compatibility

```bash
pact-broker can-i-deploy \
  --pacticipant soroban-pulse-api \
  --version 1.0.0 \
  --broker-base-url https://broker.example.com
```

## Troubleshooting

### "Contract validation failed for request"
- Check that all required fields are present
- Verify field types match the contract
- Ensure values are within allowed ranges

### "Response missing required field"
- Ensure server implements the full contract
- Check that field serialization is correct
- Verify JSON schema matches contract definition

### "has_more flag is incorrect"
- Formula: `has_more = (page * limit) < total`
- Check pagination parameters match response
- Verify total count is accurate

## Resources

- [Pact Documentation](https://docs.pact.foundation/)
- [Consumer-Driven Contracts](https://martinfowler.com/articles/consumerDrivenContracts.html)
- [API Versioning Best Practices](https://semver.org/)
- [JSON Schema Validation](https://json-schema.org/)
