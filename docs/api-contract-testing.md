# API Contract Testing Guide

API contract testing ensures that client and server agree on the API interface. This document describes SorobanPulse's API contract testing strategy using Pact.

## What is Contract Testing?

Contract testing is a technique that verifies API compatibility by testing both sides independently:

- **Consumer tests**: Verify that clients send correctly formatted requests and handle responses properly
- **Provider tests**: Verify that the server responds with the expected schema and status codes
- **Pact Broker**: Centralizes contracts to verify compatibility

### vs. Integration Tests

| Aspect | Integration Tests | Contract Tests |
|--------|-------------------|----------------|
| Scope | Full end-to-end flow | Request/response interface |
| Database | Required | Not required |
| Speed | Slow (seconds) | Fast (milliseconds) |
| Isolation | Tests multiple components | Tests API contract only |
| Purpose | Verify system works | Verify API compatibility |

Contract tests complement integration tests — they're faster and catch API incompatibilities early.

## Running Contract Tests

```bash
# Run all contract tests
cargo test --test pact_contract_tests

# Run specific contract group
cargo test --test pact_contract_tests events_api_contracts

# Run with output
cargo test --test pact_contract_tests -- --nocapture
```

## Test Organization

Contract tests are organized by API domain in `tests/pact_contract_tests.rs`:

### 1. Events API Contracts (`events_api_contracts`)

**Tests:**
- `contract_get_events_success_response` - Events list format
- `contract_get_events_with_filters` - Filtering and pagination
- `contract_get_contract_events` - Single contract query
- `contract_get_tx_events` - Transaction events query
- `contract_get_events_stream` - Server-Sent Events format

**Verified properties:**
- Response contains `data`, `total`, `page`, `limit`, `approximate`
- Pagination parameters are integers
- Event objects include required fields: `id`, `contract_id`, `event_type`, `ledger`, `timestamp`, `created_at`
- Filters work independently and combine correctly
- SSE endpoint returns `Content-Type: text/event-stream`

### 2. Error Response Contracts (`error_response_contracts`)

**Tests:**
- `contract_bad_request_format` - 400 errors
- `contract_unauthorized_format` - 401 errors
- `contract_forbidden_format` - 403 errors
- `contract_not_found_format` - 404 errors
- `contract_rate_limit_exceeded_format` - 429 errors
- `contract_internal_error_format` - 500 errors
- `contract_service_unavailable_format` - 503 errors

**Verified properties:**
- All error responses follow the format: `{ "error": "description", "code": "error_code" }`
- Status codes match HTTP standards
- Rate-limit errors include `Retry-After` header
- Error messages don't expose sensitive information

### 3. Health Check Contracts (`health_check_contracts`)

**Tests:**
- `contract_liveness_probe` - `/healthz/live` endpoint
- `contract_readiness_probe_ready` - `/healthz/ready` when healthy
- `contract_readiness_probe_db_down` - `/healthz/ready` when DB unavailable

**Verified properties:**
- Liveness probe always returns 200 if process is running
- Readiness probe includes dependency status
- Readiness probe returns 503 when database is unavailable

### 4. Subscription API Contracts (`subscription_api_contracts`)

**Tests:**
- `contract_create_subscription` - POST /v1/subscriptions
- `contract_list_subscriptions` - GET /v1/subscriptions
- `contract_delete_subscription_success` - DELETE /v1/subscriptions/{id}

**Verified properties:**
- Create returns subscription with ID and signing secret
- List returns array with pagination metadata
- Delete returns 204 No Content
- Delete returns 404 if subscription doesn't exist

### 5. API Versioning Contracts (`api_versioning_contracts`)

**Tests:**
- `contract_versioned_endpoints` - All endpoints under /v1/
- `contract_deprecation_headers` - Deprecation headers on old endpoints

**Verified properties:**
- All new endpoints use `/v1/` prefix
- Unversioned endpoints include `Deprecation` and `Link` headers

### 6. NDJSON Response Contracts (`ndjson_response_contracts`)

**Tests:**
- `contract_ndjson_format` - NDJSON response format

**Verified properties:**
- `Accept: application/x-ndjson` returns proper content type
- Responses are one JSON object per line

## Common Contracts

### Pagination Schema

```json
{
  "data": [],
  "total": 100,
  "page": 1,
  "limit": 20,
  "approximate": true
}
```

**Contract assertions:**
- `total` is always >= 0
- `page` is always >= 1
- `limit` is always between 1 and 100 (inclusive)
- `approximate` is a boolean
- `data` is an array (may be empty)

### Event Object Schema

```json
{
  "id": "uuid",
  "contract_id": "CABC...",
  "event_type": "contract|diagnostic|system",
  "tx_hash": "64-char hex",
  "ledger": 1234567,
  "timestamp": "2026-03-14T00:00:00Z",
  "event_data": {},
  "created_at": "2026-03-14T00:00:01Z"
}
```

**Contract assertions:**
- `id` is a valid UUID
- `contract_id` is a valid Stellar contract ID (C + 55 base32 chars)
- `event_type` is one of: contract, diagnostic, system
- `tx_hash` is 64 lowercase hex characters
- `ledger` is a non-negative integer
- `timestamp` and `created_at` are ISO 8601 formatted
- `event_data` is a valid JSON object

### Error Response Schema

```json
{
  "error": "Human-readable error description",
  "code": "machine_readable_code"
}
```

**Contract assertions:**
- `error` is a non-empty string (< 500 chars, no sensitive info)
- `code` is a machine-readable identifier (lowercase_with_underscores)
- Response status code matches HTTP standards

## Pact Broker Integration

### Publishing Contracts

To publish contracts to a Pact Broker:

```bash
# Set broker details
export PACT_BROKER_URL=https://pact-broker.example.com
export PACT_BROKER_TOKEN=your-token

# Publish pacts from this build
cargo test --test pact_contract_tests
# Pacts are automatically generated in target/pacts/

# Publish to broker
pact-broker publish target/pacts \
  --consumer-app-version=$GIT_SHA \
  --branch=$GIT_BRANCH
```

### Verifying Contracts

Consumers use pacts to verify the server meets their expectations:

```bash
# Verify server against published consumer contracts
pact_verifier \
  --provider SorobanPulse \
  --provider-base-url http://localhost:3000 \
  --broker-url https://pact-broker.example.com \
  --broker-token=$PACT_BROKER_TOKEN
```

## CI Integration

Add to `.github/workflows/test.yml`:

```yaml
- name: Contract Tests
  run: cargo test --test pact_contract_tests

- name: Publish Contracts
  if: github.ref == 'refs/heads/main'
  run: |
    cargo install pact-broker
    pact-broker publish target/pacts \
      --consumer-app-version=${{ github.sha }} \
      --branch=${{ github.ref_name }}
  env:
    PACT_BROKER_URL: ${{ secrets.PACT_BROKER_URL }}
    PACT_BROKER_TOKEN: ${{ secrets.PACT_BROKER_TOKEN }}
```

## Writing New Contract Tests

### 1. Define the Request

```rust
let request = json!({
    "method": "GET",
    "path": "/v1/events",
    "query": {
        "page": 1,
        "limit": 20
    }
});
```

### 2. Define the Expected Response

```rust
let response = json!({
    "status": 200,
    "headers": {
        "content-type": "application/json"
    },
    "body": {
        "data": [],
        "total": 0,
        "page": 1,
        "limit": 20,
        "approximate": true
    }
});
```

### 3. Document the Contract

```rust
/// Contract: GET /v1/events returns paginated list
///
/// When a client requests events with valid pagination,
/// the API must return JSON with data, total, page, limit, approximate
#[test]
fn contract_get_events() {
    // ...
}
```

### 4. Add Assertions

```rust
#[test]
fn contract_get_events() {
    // ... request and response definitions ...
    
    // Verify response schema
    assert_eq!(response.get("status"), Some(&json!(200)));
    assert!(response.get("body").and_then(|b| b.get("data")).is_some());
    
    // Verify data types
    assert!(response.get("body")
        .and_then(|b| b.get("total"))
        .and_then(|t| t.as_i64())
        .is_some());
}
```

## Best Practices

### 1. **Test the interface, not the implementation**

```rust
// Good: tests the API contract
#[test]
fn contract_get_events_returns_json() {
    let response = json!({
        "status": 200,
        "headers": { "content-type": "application/json" }
    });
    assert_eq!(response.get("status"), Some(&json!(200)));
}

// Avoid: tests implementation details
#[test]
fn contract_uses_sqlx_driver() {
    // Tests know too much about internals
}
```

### 2. **Include both happy path and error cases**

```rust
#[test]
fn contract_get_events_valid_pagination() { }

#[test]
fn contract_get_events_invalid_page_returns_400() { }

#[test]
fn contract_get_events_missing_db_returns_503() { }
```

### 3. **Document why each contract exists**

```rust
/// Contract: Pagination limit is between 1 and 100
///
/// Clients rely on this to safely show pagination controls.
/// Limits > 100 cause performance issues.
/// Limits < 1 are invalid.
#[test]
fn contract_pagination_limit_bounds() { }
```

### 4. **Keep contracts concise**

Test one contract per function. Don't combine multiple concerns:

```rust
// Good: single responsibility
#[test]
fn contract_response_includes_total_count() { }

// Avoid: testing multiple things
#[test]
fn contract_response_is_valid_json_with_correct_fields_and_types() { }
```

### 5. **Use realistic test data**

```rust
// Good: realistic data
let contract_id = "CABC123def456ABC123def456ABC123def456ABC123def456ABC";

// Avoid: fake data that violates contract
let contract_id = "NOT_A_VALID_CONTRACT_ID";
```

## Troubleshooting

### Test passes locally but fails in CI

Contract tests might have assumed state or timing issues:

1. Check for hardcoded UUIDs or timestamps
2. Verify test isolation (tests shouldn't depend on order)
3. Check for time-dependent assertions
4. Ensure all external dependencies are mocked

### Contract conflict between client and server

When a client test expects different behavior than server provides:

1. **Coordinate with team** - Discuss the actual requirement
2. **Update contracts** - Make expectations consistent
3. **Version the API** - If changes are breaking, use /v2/
4. **Gradual migration** - Support both old and new formats temporarily

### Pact verification fails

When provider doesn't implement what consumers expect:

1. **Read the verification report** - It shows which contract failed
2. **Check the Pact file** - See exactly what consumer expects
3. **Implement the provider** - Add the missing endpoint or fix the response
4. **Re-verify** - Run verification again

## Advanced Topics

### Provider States

Provider states allow tests to set up preconditions:

```rust
// Define a provider state
#[test]
#[provider_state("an event exists for contract CABC123")]
fn contract_get_contract_events() {
    // Test assumes this event exists in the database
}
```

### Matching Rules

Pact supports flexible matching (useful for timestamps, UUIDs):

```rust
let event = json!({
    "id": { "pact:matcher:type": "uuid" },
    "timestamp": { "pact:matcher:type": "iso8601-datetime" },
    "contract_id": { "pact:matcher:type": "regex", "regex": "^C[A-Z2-7]{55}$" }
});
```

## Resources

- [Pact documentation](https://docs.pact.foundation/)
- [Pact Rust crate](https://docs.rs/pact/)
- [Consumer-Driven Contract Testing](https://martinfowler.com/articles/consumerDrivenContracts.html)
- [API Versioning best practices](../api-versioning.md)

## Related Files

- `tests/pact_contract_tests.rs` - Contract test implementations
- `docs/api-versioning.md` - API versioning strategy
- `.github/workflows/ci.yml` - CI pipeline with contract tests
