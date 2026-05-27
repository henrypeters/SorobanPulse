use hmac::{Hmac, Mac};
use reqwest::Client;
use sha2::Sha256;
use std::time::Duration;
use tokio::time::sleep;
use tracing::{error, info, warn};
use uuid::Uuid;

use crate::{metrics, models::SorobanEvent};

type HmacSha256 = Hmac<Sha256>;

/// Sign a payload with HMAC-SHA256 and return the hex digest.
pub fn sign_payload(secret: &str, body: &[u8]) -> String {
    let mut mac =
        HmacSha256::new_from_slice(secret.as_bytes()).expect("HMAC accepts any key length");
    mac.update(body);
    let result = mac.finalize();
    hex::encode(result.into_bytes())
}

/// Deliver a single event to the webhook URL with up to 3 retries and
/// exponential backoff (1s, 2s, 4s). On final failure, insert into DLQ.
pub async fn deliver(
    client: Client,
    url: String,
    secret: Option<String>,
    event: SorobanEvent,
    pool: Option<&sqlx::PgPool>,
) {
    let body = match serde_json::to_vec(&event) {
        Ok(b) => b,
        Err(e) => {
            error!(error = %e, "Failed to serialize event for webhook delivery");
            return;
        }
    };

    let signature = secret.as_deref().map(|s| sign_payload(s, &body));

    let mut backoff_ms = 1000u64;
    for attempt in 1..=3u32 {
        let mut req = client
            .post(&url)
            .header("Content-Type", "application/json")
            .body(body.clone());

        if let Some(ref sig) = signature {
            req = req.header("X-Signature-256", format!("sha256={sig}"));
        }

        match req.send().await {
            Ok(resp) if resp.status().is_success() => {
                info!(
                    url = %url,
                    contract_id = %event.contract_id,
                    attempt = attempt,
                    "Webhook delivered successfully"
                );
                return;
            }
            Ok(resp) => {
                warn!(
                    url = %url,
                    status = %resp.status(),
                    attempt = attempt,
                    "Webhook delivery failed with non-2xx status"
                );
            }
            Err(e) => {
                warn!(
                    url = %url,
                    error = %e,
                    attempt = attempt,
                    "Webhook delivery request error"
                );
            }
        }

        if attempt < 3 {
            sleep(Duration::from_millis(backoff_ms)).await;
            backoff_ms *= 2;
        }
    }

    error!(url = %url, contract_id = %event.contract_id, "Webhook delivery failed after 3 attempts");
    
    // Insert into DLQ if pool is available
    if let Some(pool) = pool {
        let payload = serde_json::to_value(&event).unwrap_or(serde_json::json!({}));
        let next_retry = chrono::Utc::now() + chrono::Duration::seconds(60);
        
        if let Err(e) = sqlx::query(
            "INSERT INTO webhook_failures (url, payload, attempts, last_error, next_retry_at) 
             VALUES ($1, $2, $3, $4, $5)"
        )
        .bind(&url)
        .bind(payload)
        .bind(3i32)
        .bind("Failed after 3 retries")
        .bind(next_retry)
        .execute(pool)
        .await
        {
            error!(error = %e, "Failed to insert webhook failure into DLQ");
        }
    }
    
    metrics::record_webhook_failure();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sign_payload_produces_consistent_hex() {
        let sig1 = sign_payload("mysecret", b"hello world");
        let sig2 = sign_payload("mysecret", b"hello world");
        assert_eq!(sig1, sig2);
        assert_eq!(sig1.len(), 64); // 32 bytes = 64 hex chars
    }

    #[test]
    fn test_sign_payload_different_secrets_differ() {
        let sig1 = sign_payload("secret1", b"payload");
        let sig2 = sign_payload("secret2", b"payload");
        assert_ne!(sig1, sig2);
    }

    #[test]
    fn test_sign_payload_different_bodies_differ() {
        let sig1 = sign_payload("secret", b"payload1");
        let sig2 = sign_payload("secret", b"payload2");
        assert_ne!(sig1, sig2);
    }

    #[test]
    fn test_sign_payload_known_value() {
        // Verified with: echo -n "test" | openssl dgst -sha256 -hmac "key"
        let sig = sign_payload("key", b"test");
        assert_eq!(
            sig,
            "02afb56304902c656fcb737cdd03de6205bb6d401da2812efd9b2d36a08af159"
        );
    }
}
