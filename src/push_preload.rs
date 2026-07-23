//! Push-preload support for Soroban Pulse.
//!
//! This module provides two related features that let the server proactively
//! deliver contract schema and ABI data to clients, reducing round trips:
//!
//! ## 1. `/v1/push/{contract_id}/schema` and `/v1/push/{contract_id}/abi`
//!
//! Lightweight GET endpoints that return the schema / ABI for a contract in
//! the same JSON shape as their admin counterparts.  They are **read-only**
//! and require no admin key, making them safe to call from browsers and SDKs.
//!
//! ## 2. Link-header middleware (`push_link_header_middleware`)
//!
//! Applied as a `route_layer` on the v1 router.  When `enable_push_preload`
//! is `true` it appends [`Link`](https://www.rfc-editor.org/rfc/rfc8288)
//! preload headers to responses for `/v1/events/contract/{contract_id}` so
//! that HTTP/2 clients and reverse proxies can push the schema and ABI
//! resources before the client explicitly requests them:
//!
//! ```text
//! Link: </v1/push/CABC.../schema>; rel="preload"; as="fetch"
//! Link: </v1/push/CABC.../abi>; rel="preload"; as="fetch"
//! ```
//!
//! Both features are guarded by the `enable_push_preload` config flag
//! (`ENABLE_PUSH_PRELOAD=true`).  When the flag is off the middleware is a
//! no-op and the push routes return **501 Not Implemented**.

use std::sync::Arc;

use axum::{
    body::Body,
    extract::{Path, Request, State},
    http::{HeaderValue, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
    Json,
};
use serde_json::json;

use crate::{error::AppError, handlers::validate_contract_id, routes::AppState};

// ─────────────────────────────────────────────────────────────────────────────
// Lightweight state for the Link-header middleware
// ─────────────────────────────────────────────────────────────────────────────

/// Minimal state passed to `push_link_header_middleware` via
/// `from_fn_with_state`.  Holds only the single flag so it can be constructed
/// before `AppState` and passed to `route_layer`.
#[derive(Clone, Debug)]
pub struct PushPreloadState {
    pub enabled: bool,
}

impl PushPreloadState {
    pub fn new(enabled: bool) -> Arc<Self> {
        Arc::new(Self { enabled })
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Push endpoint handlers
// ─────────────────────────────────────────────────────────────────────────────

/// `GET /v1/push/{contract_id}/schema`
///
/// Returns the registered JSON Schema for the contract.  Responds with the
/// same body shape as the admin schema endpoint so clients can reuse the
/// same deserialization code.
///
/// Returns **501 Not Implemented** when `ENABLE_PUSH_PRELOAD` is `false`.  
/// Returns **404 Not Found** when no schema has been registered.
#[utoipa::path(
    get,
    path = "/v1/push/{contract_id}/schema",
    tag = "push",
    params(
        ("contract_id" = String, Path, description = "Stellar contract ID"),
    ),
    responses(
        (status = 200, description = "Contract JSON Schema", body = serde_json::Value),
        (status = 404, description = "Schema not found", body = crate::error::ErrorResponse),
        (status = 400, description = "Invalid contract_id", body = crate::error::ErrorResponse),
        (status = 501, description = "Push preload feature is disabled"),
    )
)]
pub async fn get_push_schema(
    State(state): State<AppState>,
    Path(contract_id): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    if !state.config.enable_push_preload {
        return Ok((
            StatusCode::NOT_IMPLEMENTED,
            Json(json!({
                "error": "push preload feature is disabled",
                "hint": "Set ENABLE_PUSH_PRELOAD=true to enable /v1/push/* endpoints"
            })),
        )
            .into_response());
    }

    validate_contract_id(&contract_id)?;

    let validator = state
        .schema_validator
        .as_ref()
        .ok_or_else(|| AppError::Internal("Schema validator not initialized".to_string()))?;

    let schema = validator
        .get_schema(&contract_id)
        .await
        .ok_or(AppError::NotFound)?;

    Ok((
        StatusCode::OK,
        Json(json!({
            "contract_id": contract_id,
            "schema": schema,
        })),
    )
        .into_response())
}

/// `GET /v1/push/{contract_id}/abi`
///
/// Returns the registered ABI for the contract.  Responds with the same body
/// shape as the admin ABI endpoint.
///
/// Returns **501 Not Implemented** when `ENABLE_PUSH_PRELOAD` is `false`.  
/// Returns **404 Not Found** when no ABI has been registered.
#[utoipa::path(
    get,
    path = "/v1/push/{contract_id}/abi",
    tag = "push",
    params(
        ("contract_id" = String, Path, description = "Stellar contract ID"),
    ),
    responses(
        (status = 200, description = "Contract ABI", body = serde_json::Value),
        (status = 404, description = "ABI not found", body = crate::error::ErrorResponse),
        (status = 400, description = "Invalid contract_id", body = crate::error::ErrorResponse),
        (status = 501, description = "Push preload feature is disabled"),
    )
)]
pub async fn get_push_abi(
    State(state): State<AppState>,
    Path(contract_id): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    if !state.config.enable_push_preload {
        return Ok((
            StatusCode::NOT_IMPLEMENTED,
            Json(json!({
                "error": "push preload feature is disabled",
                "hint": "Set ENABLE_PUSH_PRELOAD=true to enable /v1/push/* endpoints"
            })),
        )
            .into_response());
    }

    validate_contract_id(&contract_id)?;

    // Use the in-process ABI cache for lower latency on repeated lookups.
    let abi = crate::abi::fetch_contract_abi(&state.pool, &state.abi_cache, &contract_id)
        .await
        .ok_or(AppError::NotFound)?;

    Ok((
        StatusCode::OK,
        Json(json!({
            "contract_id": contract_id,
            "abi": abi,
        })),
    )
        .into_response())
}

// ─────────────────────────────────────────────────────────────────────────────
// Link-header middleware
// ─────────────────────────────────────────────────────────────────────────────

/// Middleware that appends `Link` preload headers to any response when
/// `PushPreloadState::enabled` is `true` and the request path contains a
/// recognisable `{contract_id}` segment.
///
/// Intended to be attached as a `route_layer` on the v1 router via
/// `from_fn_with_state(push_preload_state, push_link_header_middleware)`.
///
/// For `/v1/events/contract/CABC123` the response will carry:
///
/// ```text
/// Link: </v1/push/CABC123/schema>; rel="preload"; as="fetch"
/// Link: </v1/push/CABC123/abi>; rel="preload"; as="fetch"
/// ```
///
/// When the flag is off this function calls `next` and returns immediately
/// without inspecting or modifying the response — zero allocation on the hot
/// path.
pub async fn push_link_header_middleware(
    State(ps): State<Arc<PushPreloadState>>,
    req: Request<Body>,
    next: Next,
) -> Response {
    // Extract contract_id before running the inner handler so the reference
    // into the URI string is no longer borrowed when we append headers.
    let contract_id = extract_contract_id_from_path(req.uri().path());

    let mut response = next.run(req).await;

    if ps.enabled {
        if let Some(cid) = contract_id {
            let schema_link =
                format!("</v1/push/{cid}/schema>; rel=\"preload\"; as=\"fetch\"");
            let abi_link =
                format!("</v1/push/{cid}/abi>; rel=\"preload\"; as=\"fetch\"");

            if let Ok(v) = HeaderValue::from_str(&schema_link) {
                response.headers_mut().append("Link", v);
            }
            if let Ok(v) = HeaderValue::from_str(&abi_link) {
                response.headers_mut().append("Link", v);
            }
        }
    }

    response
}

// ─────────────────────────────────────────────────────────────────────────────
// Internal helpers
// ─────────────────────────────────────────────────────────────────────────────

/// Extract the `{contract_id}` path segment from URLs of the form
/// `/…/events/contract/{contract_id}[/…]`.
///
/// Returns `None` for any path that does not match the pattern, including
/// paths where the segment after the marker is empty.
pub(crate) fn extract_contract_id_from_path(path: &str) -> Option<String> {
    let marker = "/events/contract/";
    let idx = path.find(marker)?;
    let rest = &path[idx + marker.len()..];
    let end = rest.find('/').unwrap_or(rest.len());
    let contract_id = &rest[..end];
    if contract_id.is_empty() {
        None
    } else {
        Some(contract_id.to_string())
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── extract_contract_id_from_path ────────────────────────────────────────

    #[test]
    fn extracts_contract_id_from_events_contract_path() {
        assert_eq!(
            extract_contract_id_from_path("/v1/events/contract/CABC123"),
            Some("CABC123".to_string())
        );
    }

    #[test]
    fn extracts_contract_id_with_trailing_stream_segment() {
        assert_eq!(
            extract_contract_id_from_path("/v1/events/contract/CABC123/stream"),
            Some("CABC123".to_string())
        );
    }

    #[test]
    fn returns_none_for_unrelated_path() {
        assert_eq!(extract_contract_id_from_path("/v1/events"), None);
    }

    #[test]
    fn returns_none_for_empty_contract_id_segment() {
        assert_eq!(
            extract_contract_id_from_path("/v1/events/contract/"),
            None
        );
    }

    #[test]
    fn extracts_full_length_stellar_contract_id() {
        let id = "CAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAABSC4";
        let path = format!("/v1/events/contract/{id}");
        assert_eq!(
            extract_contract_id_from_path(&path),
            Some(id.to_string())
        );
    }

    #[test]
    fn extracts_contract_id_without_v1_prefix() {
        // Deprecated unversioned path — still contains the marker.
        assert_eq!(
            extract_contract_id_from_path("/events/contract/CXYZ"),
            Some("CXYZ".to_string())
        );
    }

    // ── Link header value format ─────────────────────────────────────────────

    #[test]
    fn schema_link_value_is_valid_header_value() {
        let cid = "CABC123";
        let link = format!("</v1/push/{cid}/schema>; rel=\"preload\"; as=\"fetch\"");
        assert!(
            HeaderValue::from_str(&link).is_ok(),
            "link header should be a valid HeaderValue"
        );
    }

    #[test]
    fn abi_link_value_is_valid_header_value() {
        let cid = "CABC123";
        let link = format!("</v1/push/{cid}/abi>; rel=\"preload\"; as=\"fetch\"");
        assert!(
            HeaderValue::from_str(&link).is_ok(),
            "link header should be a valid HeaderValue"
        );
    }

    // ── PushPreloadState ─────────────────────────────────────────────────────

    #[test]
    fn push_preload_state_default_disabled() {
        let state = PushPreloadState::new(false);
        assert!(!state.enabled);
    }

    #[test]
    fn push_preload_state_enabled() {
        let state = PushPreloadState::new(true);
        assert!(state.enabled);
    }
}
