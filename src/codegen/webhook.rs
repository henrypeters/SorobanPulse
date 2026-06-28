//! Generates webhook handler scaffolding with HMAC signing, retry policy,
//! priority evaluation, failover, and DLQ integration.

use super::{apply, ChannelType, GeneratedFile, ScaffoldConfig};

const WEBHOOK_TEMPLATE: &str = r#"//! Webhook delivery for {{PASCAL}} subscriptions.
//!
//! Integrates HMAC-SHA256 payload signing, configurable retry policy,
//! JSONPath-based priority evaluation, optional failover URL, and DLQ
//! insertion on final failure. Wire `deliver_{{SNAKE}}_webhook` into the
//! delivery worker once the event passes content filters.

use crate::{
    metrics,
    models::SorobanEvent,
    retry_policy::RetryPolicy,
    webhook::{evaluate_priority, sign_payload},
};
use reqwest::Client;
use serde_json::json;
use tracing::{error, info, warn};
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Delivery
// ---------------------------------------------------------------------------

/// Deliver `event` to `url` for a {{PASCAL}} subscription.
///
/// Signs the payload with `secret` when provided and attaches the signature
/// as `X-Signature-256`. On final failure the event is inserted into the
/// webhook DLQ for manual inspection or reprocessing.
pub async fn deliver_{{SNAKE}}_webhook(
    client: Client,
    url: String,
    secret: Option<String>,
    event: SorobanEvent,
    pool: Option<&sqlx::PgPool>,
) {
    let policy = RetryPolicy::webhook_default();
    deliver_{{SNAKE}}_with_policy(client, url, secret, event, pool, &policy, "medium".into()).await;
}

/// Deliver with an explicit retry policy and priority label.
pub async fn deliver_{{SNAKE}}_with_policy(
    client: Client,
    url: String,
    secret: Option<String>,
    event: SorobanEvent,
    pool: Option<&sqlx::PgPool>,
    policy: &RetryPolicy,
    priority: String,
) {
    // Suppress delivery to blocked URLs.
    if let Some(pool) = pool {
        match sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM suppression_lists \
             WHERE target = $1 AND target_type = 'webhook' \
             AND (expires_at IS NULL OR expires_at > NOW())",
        )
        .bind(&url)
        .fetch_one(pool)
        .await
        {
            Ok(count) if count > 0 => {
                info!(url = %url, "{{PASCAL}} webhook suppressed, skipping delivery");
                metrics::record_notification_suppressed();
                return;
            }
            _ => {}
        }
    }

    let body = match serde_json::to_vec(&event) {
        Ok(b) => b,
        Err(e) => {
            error!(error = %e, "Failed to serialize event for {{PASCAL}} webhook");
            return;
        }
    };

    let signature = secret.as_deref().map(|s| sign_payload(s, &body));
    let priority_owned = priority.clone();

    let result = policy
        .execute_with_retry(|attempt| {
            let client = client.clone();
            let url = url.clone();
            let body = body.clone();
            let sig = signature.clone();
            let pri = priority_owned.clone();
            async move {
                let mut req = client
                    .post(&url)
                    .header("Content-Type", "application/json")
                    .header("X-Notification-Priority", pri)
                    .body(body);
                if let Some(ref s) = sig {
                    req = req.header("X-Signature-256", format!("sha256={s}"));
                }
                match req.send().await {
                    Ok(resp) if resp.status().is_success() => {
                        info!(url = %url, attempt = attempt, "{{PASCAL}} webhook delivered");
                        Ok(())
                    }
                    Ok(resp) => Err(format!("HTTP {}", resp.status())),
                    Err(e) => Err(format!("Request error: {e}")),
                }
            }
        })
        .await;

    if let Err(error_msg) = result {
        error!(
            url = %url,
            contract = %event.contract_id,
            error = %error_msg,
            "{{PASCAL}} webhook failed after all retries"
        );
        insert_{{SNAKE}}_dlq(&url, &event, policy.max_attempts, &error_msg, pool).await;
        metrics::record_webhook_failure();
    }
}

// ---------------------------------------------------------------------------
// Failover delivery
// ---------------------------------------------------------------------------

/// Attempt delivery to `primary_url`; fall back to `failover_url` on failure.
/// Returns `true` if at least one URL succeeded.
pub async fn deliver_{{SNAKE}}_with_failover(
    client: Client,
    primary_url: String,
    primary_secret: Option<String>,
    failover_url: Option<String>,
    failover_secret: Option<String>,
    event: SorobanEvent,
    pool: Option<&sqlx::PgPool>,
    policy: &RetryPolicy,
) -> bool {
    let body = match serde_json::to_vec(&event) {
        Ok(b) => b,
        Err(e) => {
            error!(error = %e, "Failed to serialize {{PASCAL}} event");
            return false;
        }
    };

    let primary_sig = primary_secret.as_deref().map(|s| sign_payload(s, &body));

    let primary_ok = policy
        .execute_with_retry(|attempt| {
            let client = client.clone();
            let url = primary_url.clone();
            let body = body.clone();
            let sig = primary_sig.clone();
            async move {
                let mut req = client
                    .post(&url)
                    .header("Content-Type", "application/json")
                    .body(body);
                if let Some(ref s) = sig {
                    req = req.header("X-Signature-256", format!("sha256={s}"));
                }
                match req.send().await {
                    Ok(r) if r.status().is_success() => {
                        info!(url = %url, attempt = attempt, "{{PASCAL}} delivered (primary)");
                        Ok(())
                    }
                    Ok(r) => Err(format!("HTTP {}", r.status())),
                    Err(e) => Err(e.to_string()),
                }
            }
        })
        .await
        .is_ok();

    if primary_ok {
        return true;
    }

    warn!(primary = %primary_url, "{{PASCAL}} primary delivery failed, trying failover");

    if let Some(f_url) = failover_url {
        metrics::record_notification_failover("webhook");
        let f_sig = failover_secret.as_deref().map(|s| sign_payload(s, &body));

        let failover_ok = policy
            .execute_with_retry(|attempt| {
                let client = client.clone();
                let url = f_url.clone();
                let body = body.clone();
                let sig = f_sig.clone();
                async move {
                    let mut req = client
                        .post(&url)
                        .header("Content-Type", "application/json")
                        .body(body);
                    if let Some(ref s) = sig {
                        req = req.header("X-Signature-256", format!("sha256={s}"));
                    }
                    match req.send().await {
                        Ok(r) if r.status().is_success() => {
                            info!(url = %url, attempt = attempt, "{{PASCAL}} delivered (failover)");
                            Ok(())
                        }
                        Ok(r) => Err(format!("HTTP {}", r.status())),
                        Err(e) => Err(e.to_string()),
                    }
                }
            })
            .await
            .is_ok();

        if failover_ok {
            return true;
        }

        error!(failover = %f_url, "{{PASCAL}} failover delivery also failed");
    }

    insert_{{SNAKE}}_dlq(
        &primary_url,
        &event,
        policy.max_attempts,
        "Primary and failover both failed",
        pool,
    )
    .await;
    metrics::record_webhook_failure();
    false
}

// ---------------------------------------------------------------------------
// Priority evaluation
// ---------------------------------------------------------------------------

/// Determine the delivery priority for a {{PASCAL}} event.
///
/// Override the default `"medium"` priority by supplying a JSONPath rule.
/// Example: `rule_path = Some("$.amount")`, `rule_value = Some("5000000")`,
/// `rule_priority = Some("critical")`.
pub fn evaluate_{{SNAKE}}_priority<'a>(
    event: &crate::models::SorobanEvent,
    rule_path: Option<&str>,
    rule_value: Option<&str>,
    rule_priority: Option<&'a str>,
) -> &'a str {
    evaluate_priority(event, rule_path, rule_value, rule_priority, "medium")
}

// ---------------------------------------------------------------------------
// DLQ insertion
// ---------------------------------------------------------------------------

async fn insert_{{SNAKE}}_dlq(
    url: &str,
    event: &SorobanEvent,
    attempts: usize,
    error: &str,
    pool: Option<&sqlx::PgPool>,
) {
    let Some(pool) = pool else { return };
    let payload = serde_json::to_value(event).unwrap_or(json!({}));
    let next_retry = chrono::Utc::now() + chrono::Duration::seconds(60);
    if let Err(e) = sqlx::query(
        "INSERT INTO webhook_failures (url, payload, attempts, last_error, next_retry_at)
         VALUES ($1, $2, $3, $4, $5)",
    )
    .bind(url)
    .bind(payload)
    .bind(attempts as i32)
    .bind(error)
    .bind(next_retry)
    .execute(pool)
    .await
    {
        error!(error = %e, "Failed to insert {{PASCAL}} webhook failure into DLQ");
    }
}
"#;

const EMAIL_TEMPLATE: &str = r#"//! Email notification handler for {{PASCAL}} subscriptions.
//!
//! Renders a Handlebars template and sends via SMTP. Wire `send_{{SNAKE}}_email`
//! into the delivery worker once the event passes content filters.

use crate::{
    email::{render_template, send_email},
    error::AppError,
    models::SorobanEvent,
};
use serde_json::json;

/// Send an email notification for a {{PASCAL}} event to `recipient`.
pub async fn send_{{SNAKE}}_email(
    event: &SorobanEvent,
    recipient: &str,
    smtp_host: &str,
    smtp_user: &str,
    smtp_pass: &str,
) -> Result<(), AppError> {
    let context = json!({
        "contract_id": event.contract_id,
        "ledger": event.ledger,
        "event_type": event.event_type,
        "value": event.value,
        "ledger_closed_at": event.ledger_closed_at,
    });

    let subject = format!("{{PASCAL}} Event — {}", event.event_type);
    let html = render_template("{{SNAKE}}_notification", &context)?;

    send_email(smtp_host, smtp_user, smtp_pass, recipient, &subject, &html).await
}
"#;

const SMS_TEMPLATE: &str = r#"//! SMS notification handler for {{PASCAL}} subscriptions.
//!
//! Formats a short message and dispatches via the configured SMS provider.
//! Wire `send_{{SNAKE}}_sms` into the delivery worker once the event passes
//! content filters.

use crate::{error::AppError, models::SorobanEvent, sms::send_sms};

/// Send an SMS notification for a {{PASCAL}} event to `phone_number`.
pub async fn send_{{SNAKE}}_sms(
    event: &SorobanEvent,
    phone_number: &str,
    provider_api_key: &str,
) -> Result<(), AppError> {
    let message = format!(
        "SorobanPulse: {{PASCAL}} event on contract {} at ledger {}. Type: {}.",
        event.contract_id, event.ledger, event.event_type,
    );
    send_sms(phone_number, &message, provider_api_key).await
}
"#;

pub fn generate(config: &ScaffoldConfig) -> GeneratedFile {
    let (filename, template) = match config.channel_type {
        ChannelType::Webhook => (
            format!("src/{}_webhook.rs", config.snake_name),
            WEBHOOK_TEMPLATE,
        ),
        ChannelType::Email => (
            format!("src/{}_email.rs", config.snake_name),
            EMAIL_TEMPLATE,
        ),
        ChannelType::Sms => (
            format!("src/{}_sms.rs", config.snake_name),
            SMS_TEMPLATE,
        ),
    };
    GeneratedFile {
        relative_path: filename,
        content: apply(template, config),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen::{ChannelType, ScaffoldConfig};

    #[test]
    fn webhook_file_contains_signing_call() {
        let cfg = ScaffoldConfig::new("transfer", ChannelType::Webhook, false, false);
        let f = generate(&cfg);
        assert!(f.content.contains("sign_payload"));
        assert!(f.content.contains("X-Signature-256"));
    }

    #[test]
    fn webhook_file_contains_dlq_insert() {
        let cfg = ScaffoldConfig::new("transfer", ChannelType::Webhook, false, false);
        let f = generate(&cfg);
        assert!(f.content.contains("webhook_failures"));
    }

    #[test]
    fn webhook_file_contains_failover() {
        let cfg = ScaffoldConfig::new("transfer", ChannelType::Webhook, false, false);
        let f = generate(&cfg);
        assert!(f.content.contains("deliver_transfer_with_failover"));
    }

    #[test]
    fn email_file_has_correct_path() {
        let cfg = ScaffoldConfig::new("payment", ChannelType::Email, false, false);
        let f = generate(&cfg);
        assert_eq!(f.relative_path, "src/payment_email.rs");
        assert!(f.content.contains("send_payment_email"));
    }

    #[test]
    fn sms_file_has_correct_path() {
        let cfg = ScaffoldConfig::new("alert", ChannelType::Sms, false, false);
        let f = generate(&cfg);
        assert_eq!(f.relative_path, "src/alert_sms.rs");
        assert!(f.content.contains("send_alert_sms"));
    }

    #[test]
    fn priority_function_name_uses_snake() {
        let cfg = ScaffoldConfig::new("token-swap", ChannelType::Webhook, false, false);
        let f = generate(&cfg);
        assert!(f.content.contains("evaluate_token_swap_priority"));
    }
}
