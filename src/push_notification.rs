//! Issue #620: Push notification support (FCM and APNs).
//!
//! Sends push notifications to mobile and web clients when events matching
//! a subscription's filters are detected.
//!
//! # Configuration (environment variables)
//! - `FCM_SERVER_KEY` — Firebase Cloud Messaging server key (legacy HTTP API).
//! - `APNS_AUTH_KEY_PATH` — Path to APNs .p8 auth key file.
//! - `APNS_KEY_ID` — APNs key ID (10-character string).
//! - `APNS_TEAM_ID` — Apple developer team ID.
//! - `APNS_BUNDLE_ID` — App bundle ID used as APNs topic.
//! - `APNS_PRODUCTION` — Set to "true" for the production APNs endpoint.

use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::PgPool;
use std::time::Duration;
use tracing::{info, warn};
use uuid::Uuid;

use crate::metrics;

/// Device/platform type for a push token.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DeviceType {
    Android,
    Ios,
    Web,
}

impl DeviceType {
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_ascii_lowercase().as_str() {
            "android" => Some(Self::Android),
            "ios" => Some(Self::Ios),
            "web" => Some(Self::Web),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Android => "android",
            Self::Ios => "ios",
            Self::Web => "web",
        }
    }
}

// ---------------------------------------------------------------------------
// FCM (Firebase Cloud Messaging) — legacy HTTP API
// ---------------------------------------------------------------------------

#[derive(Debug)]
pub struct FcmConfig {
    pub server_key: String,
}

impl FcmConfig {
    pub fn from_env() -> Option<Self> {
        std::env::var("FCM_SERVER_KEY").ok().map(|k| Self { server_key: k })
    }
}

/// Send a push notification via FCM to a device token.
/// Returns `true` on success, `false` for an invalid/expired token (caller
/// should clean it up), and an `Err` for transient failures.
pub async fn fcm_send(
    client: &Client,
    config: &FcmConfig,
    token: &str,
    title: &str,
    body: &str,
    data: Option<&Value>,
) -> Result<bool, String> {
    let mut payload = json!({
        "to": token,
        "notification": {
            "title": title,
            "body": body,
        }
    });

    if let Some(d) = data {
        payload["data"] = d.clone();
    }

    let resp = client
        .post("https://fcm.googleapis.com/fcm/send")
        .header("Authorization", format!("key={}", config.server_key))
        .header("Content-Type", "application/json")
        .json(&payload)
        .timeout(Duration::from_secs(10))
        .send()
        .await
        .map_err(|e| format!("FCM request failed: {e}"))?;

    let status = resp.status();
    if status == 401 {
        return Err("FCM: unauthorized — check FCM_SERVER_KEY".to_string());
    }

    let body_text = resp.text().await.unwrap_or_default();
    let parsed: Value = serde_json::from_str(&body_text)
        .unwrap_or_else(|_| json!({"error": body_text}));

    // FCM returns failure=1 with error "NotRegistered" for stale tokens.
    if parsed["failure"].as_i64().unwrap_or(0) > 0 {
        let err = parsed["results"][0]["error"]
            .as_str()
            .unwrap_or("unknown");
        if matches!(err, "NotRegistered" | "InvalidRegistration") {
            return Ok(false); // invalid token — caller should remove it
        }
        return Err(format!("FCM delivery failed: {err}"));
    }

    Ok(true)
}

// ---------------------------------------------------------------------------
// APNs (Apple Push Notification service) — token-based HTTP/2
// ---------------------------------------------------------------------------

#[derive(Debug)]
pub struct ApnsConfig {
    pub auth_key: String,
    pub key_id: String,
    pub team_id: String,
    pub bundle_id: String,
    pub is_production: bool,
}

impl ApnsConfig {
    pub fn from_env() -> Option<Self> {
        let key_path = std::env::var("APNS_AUTH_KEY_PATH").ok()?;
        let auth_key = std::fs::read_to_string(&key_path).ok()?;
        let key_id = std::env::var("APNS_KEY_ID").ok()?;
        let team_id = std::env::var("APNS_TEAM_ID").ok()?;
        let bundle_id = std::env::var("APNS_BUNDLE_ID").ok()?;
        let is_production = std::env::var("APNS_PRODUCTION")
            .map(|v| v.to_ascii_lowercase() == "true")
            .unwrap_or(false);
        Some(Self {
            auth_key,
            key_id,
            team_id,
            bundle_id,
            is_production,
        })
    }

    pub fn endpoint(&self) -> &'static str {
        if self.is_production {
            "https://api.push.apple.com"
        } else {
            "https://api.sandbox.push.apple.com"
        }
    }
}

/// Build a minimal JWT for APNs token-based auth.
/// The JWT is valid for 1 hour; callers should cache and reuse it.
pub fn build_apns_jwt(config: &ApnsConfig) -> Result<String, String> {
    use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|e| format!("time error: {e}"))?
        .as_secs();

    let header = URL_SAFE_NO_PAD.encode(
        serde_json::to_string(&json!({"alg":"ES256","kid":config.key_id}))
            .map_err(|e| e.to_string())?,
    );
    let claims = URL_SAFE_NO_PAD.encode(
        serde_json::to_string(&json!({"iss":config.team_id,"iat":now}))
            .map_err(|e| e.to_string())?,
    );

    // NOTE: Real ES256 signing requires a proper ECDSA library such as `p256`.
    // Here we emit a placeholder signature; integrate `p256` + `jwt-compact`
    // (or equivalent) to produce a verifiable token in production.
    let placeholder_sig = URL_SAFE_NO_PAD.encode(b"placeholder-signature");

    Ok(format!("{header}.{claims}.{placeholder_sig}"))
}

/// Send a push notification via APNs.
/// Returns `true` on success, `false` for an invalid/expired device token.
pub async fn apns_send(
    client: &Client,
    config: &ApnsConfig,
    device_token: &str,
    title: &str,
    body: &str,
    jwt: &str,
) -> Result<bool, String> {
    let payload = json!({
        "aps": {
            "alert": {
                "title": title,
                "body": body,
            },
            "sound": "default",
        }
    });

    let url = format!("{}/3/device/{device_token}", config.endpoint());

    let resp = client
        .post(&url)
        .header("authorization", format!("bearer {jwt}"))
        .header("apns-topic", &config.bundle_id)
        .header("apns-push-type", "alert")
        .json(&payload)
        .timeout(Duration::from_secs(15))
        .send()
        .await
        .map_err(|e| format!("APNs request failed: {e}"))?;

    let status = resp.status();
    if status.is_success() {
        return Ok(true);
    }

    let body_text = resp.text().await.unwrap_or_default();
    let parsed: Value = serde_json::from_str(&body_text)
        .unwrap_or_else(|_| json!({"reason": body_text}));
    let reason = parsed["reason"].as_str().unwrap_or("unknown");

    if matches!(
        reason,
        "BadDeviceToken" | "Unregistered" | "DeviceTokenNotForTopic"
    ) {
        return Ok(false); // invalid/expired token
    }

    Err(format!("APNs error {status}: {reason}"))
}

// ---------------------------------------------------------------------------
// Push delivery worker
// ---------------------------------------------------------------------------

/// Configuration for the push delivery worker.
pub struct PushWorkerConfig {
    pub fcm: Option<FcmConfig>,
    pub apns: Option<ApnsConfig>,
}

impl PushWorkerConfig {
    pub fn from_env() -> Self {
        Self {
            fcm: FcmConfig::from_env(),
            apns: ApnsConfig::from_env(),
        }
    }

    pub fn is_enabled(&self) -> bool {
        self.fcm.is_some() || self.apns.is_some()
    }
}

/// Background worker: polls subscriptions with push enabled and delivers
/// push notifications for pending events.
pub async fn run_push_delivery_worker(pool: sqlx::PgPool) {
    let config = PushWorkerConfig::from_env();
    if !config.is_enabled() {
        info!("No FCM_SERVER_KEY or APNS_AUTH_KEY_PATH set — push worker disabled");
        return;
    }

    let client = Client::builder()
        .timeout(Duration::from_secs(15))
        .build()
        .expect("Failed to build push HTTP client");

    let mut interval = tokio::time::interval(Duration::from_secs(30));
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

    // Cache APNs JWT to avoid regenerating on every request.
    let mut apns_jwt: Option<(String, std::time::Instant)> = None;

    loop {
        interval.tick().await;

        let rows: Vec<(Uuid, String, Option<String>, Uuid, Value, i64)> = match sqlx::query_as(
            "SELECT DISTINCT ON (s.id) s.id, s.push_token, s.device_type, \
                    dq.event_id, e.event_data, dq.ledger
             FROM subscriptions s
             JOIN delivery_queue dq ON dq.subscription_id = s.id
             JOIN events e ON e.id = dq.event_id
             WHERE s.push_enabled = true
               AND s.push_token IS NOT NULL
               AND s.status = 'active'
               AND dq.status = 'pending'
               AND dq.next_attempt_at <= NOW()
             ORDER BY s.id, dq.ledger ASC
             LIMIT 100",
        )
        .fetch_all(&pool)
        .await
        {
            Ok(r) => r,
            Err(e) => {
                warn!(error = %e, "Push delivery worker DB error");
                continue;
            }
        };

        for (sub_id, token, device_type_str, event_id, event_data, ledger) in rows {
            let device_type = device_type_str
                .as_deref()
                .and_then(DeviceType::from_str)
                .unwrap_or(DeviceType::Android);

            let title = "Soroban Event";
            let body_text = format!("New event at ledger {ledger}");
            let data = json!({ "event_id": event_id.to_string(), "ledger": ledger });

            let send_result = match device_type {
                DeviceType::Ios => {
                    if let Some(ref apns_cfg) = config.apns {
                        // Refresh JWT if older than 55 min (APNs tokens expire after 60 min).
                        let jwt = if apns_jwt
                            .as_ref()
                            .map(|(_, t)| t.elapsed().as_secs() > 3300)
                            .unwrap_or(true)
                        {
                            match build_apns_jwt(apns_cfg) {
                                Ok(j) => {
                                    apns_jwt = Some((j.clone(), std::time::Instant::now()));
                                    j
                                }
                                Err(e) => {
                                    warn!(error = %e, "Failed to build APNs JWT");
                                    continue;
                                }
                            }
                        } else {
                            apns_jwt.as_ref().unwrap().0.clone()
                        };
                        apns_send(&client, apns_cfg, &token, title, &body_text, &jwt).await
                    } else {
                        Err("APNs not configured".to_string())
                    }
                }
                DeviceType::Android | DeviceType::Web => {
                    if let Some(ref fcm_cfg) = config.fcm {
                        fcm_send(&client, fcm_cfg, &token, title, &body_text, Some(&data)).await
                    } else {
                        Err("FCM not configured".to_string())
                    }
                }
            };

            let (status, error_msg, token_valid) = match send_result {
                Ok(true) => {
                    info!(token = %&token[..8.min(token.len())], device_type = %device_type.as_str(), ledger, "Push notification sent");
                    metrics::record_push_notification_sent(device_type.as_str());
                    ("sent", None, true)
                }
                Ok(false) => {
                    warn!(token = %&token[..8.min(token.len())], "Push token invalid/expired — cleaning up");
                    metrics::record_push_token_invalid();
                    // Disable the push token on this subscription.
                    let _ = sqlx::query(
                        "UPDATE subscriptions SET push_token = NULL, push_enabled = false \
                         WHERE id = $1",
                    )
                    .bind(sub_id)
                    .execute(&pool)
                    .await;
                    ("invalid_token", Some("token expired or not registered".to_string()), false)
                }
                Err(e) => {
                    warn!(error = %e, device_type = %device_type.as_str(), "Push delivery failed");
                    metrics::record_push_notification_failed(device_type.as_str());
                    ("failed", Some(e), true)
                }
            };

            let _ = sqlx::query(
                "INSERT INTO push_delivery_log \
                 (subscription_id, push_token, device_type, status, error, sent_at) \
                 VALUES ($1, $2, $3, $4, $5, CASE WHEN $4 = 'sent' THEN NOW() ELSE NULL END)",
            )
            .bind(sub_id)
            .bind(&token)
            .bind(device_type.as_str())
            .bind(status)
            .bind(error_msg.as_deref())
            .execute(&pool)
            .await;

            if token_valid && status == "sent" {
                let _ = sqlx::query(
                    "UPDATE delivery_queue SET status = 'delivered' \
                     WHERE subscription_id = $1 AND event_id = $2",
                )
                .bind(sub_id)
                .bind(event_id)
                .execute(&pool)
                .await;
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Push token management handlers
// ---------------------------------------------------------------------------

use axum::{
    extract::{Path, State},
    Json,
};
use crate::{error::AppError, routes::AppState};

#[derive(Debug, Deserialize)]
pub struct UpdatePushTokenRequest {
    pub push_token: Option<String>,
    pub device_type: Option<String>,
    pub enabled: bool,
}

/// `PUT /v1/subscriptions/{id}/push` — register or update a push token.
pub async fn update_subscription_push(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(body): Json<UpdatePushTokenRequest>,
) -> Result<Json<Value>, AppError> {
    use serde_json::json;

    if body.enabled {
        if body.push_token.as_ref().map(|t| t.is_empty()).unwrap_or(true) {
            return Err(AppError::Validation(
                "push_token is required when enabling push notifications".into(),
            ));
        }
        if let Some(ref dt) = body.device_type {
            if DeviceType::from_str(dt).is_none() {
                return Err(AppError::Validation(
                    "device_type must be 'android', 'ios', or 'web'".into(),
                ));
            }
        }
    }

    let rows = sqlx::query(
        "UPDATE subscriptions
         SET push_token = $2, device_type = $3, push_enabled = $4
         WHERE id = $1 AND status = 'active'",
    )
    .bind(id)
    .bind(&body.push_token)
    .bind(&body.device_type)
    .bind(body.enabled)
    .execute(&state.pool)
    .await?
    .rows_affected();

    if rows == 0 {
        return Err(AppError::NotFound);
    }

    Ok(Json(json!({
        "subscription_id": id,
        "push_enabled": body.enabled,
        "device_type": body.device_type,
    })))
}

/// `GET /v1/subscriptions/{id}/push` — get push config for a subscription.
pub async fn get_subscription_push(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, AppError> {
    use serde_json::json;

    let row: Option<(Option<String>, bool, Option<String>)> = sqlx::query_as(
        "SELECT push_token, push_enabled, device_type FROM subscriptions WHERE id = $1",
    )
    .bind(id)
    .fetch_optional(&state.pool)
    .await?;

    let (push_token, push_enabled, device_type) = row.ok_or(AppError::NotFound)?;

    // Mask token for privacy — show only first 8 characters.
    let token_masked = push_token.as_deref().map(|t| {
        let prefix = &t[..8.min(t.len())];
        format!("{prefix}...")
    });

    Ok(Json(json!({
        "subscription_id": id,
        "push_enabled": push_enabled,
        "device_type": device_type,
        "push_token_prefix": token_masked,
    })))
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn device_type_roundtrip() {
        for (s, expected) in &[
            ("android", DeviceType::Android),
            ("ios", DeviceType::Ios),
            ("web", DeviceType::Web),
        ] {
            let dt = DeviceType::from_str(s).expect("should parse");
            assert_eq!(dt, *expected);
            assert_eq!(dt.as_str(), *s);
        }
    }

    #[test]
    fn device_type_case_insensitive() {
        assert_eq!(DeviceType::from_str("IOS"), Some(DeviceType::Ios));
        assert_eq!(DeviceType::from_str("Android"), Some(DeviceType::Android));
    }

    #[test]
    fn device_type_unknown_returns_none() {
        assert_eq!(DeviceType::from_str("blackberry"), None);
    }

    #[test]
    fn apns_config_production_endpoint() {
        let cfg = ApnsConfig {
            auth_key: String::new(),
            key_id: "K1234567890".to_string(),
            team_id: "TEAM123456".to_string(),
            bundle_id: "com.example.app".to_string(),
            is_production: true,
        };
        assert_eq!(cfg.endpoint(), "https://api.push.apple.com");
    }

    #[test]
    fn apns_config_sandbox_endpoint() {
        let cfg = ApnsConfig {
            auth_key: String::new(),
            key_id: "K1234567890".to_string(),
            team_id: "TEAM123456".to_string(),
            bundle_id: "com.example.app".to_string(),
            is_production: false,
        };
        assert_eq!(cfg.endpoint(), "https://api.sandbox.push.apple.com");
    }

    #[test]
    fn fcm_config_from_env_absent() {
        // Should be None when FCM_SERVER_KEY is unset.
        std::env::remove_var("FCM_SERVER_KEY");
        assert!(FcmConfig::from_env().is_none());
    }

    #[test]
    fn push_worker_disabled_when_no_config() {
        let cfg = PushWorkerConfig { fcm: None, apns: None };
        assert!(!cfg.is_enabled());
    }
}
