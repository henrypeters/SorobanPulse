//! Generates subscription handler module and SQL migration.

use super::{apply, GeneratedFile, ScaffoldConfig};

const HANDLER_TEMPLATE: &str = r#"use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::PgPool;
use std::time::Duration;
use tokio::time::sleep;
use uuid::Uuid;

use crate::{error::AppError, routes::AppState};
use crate::subscriptions::validate_callback_url;

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct {{PASCAL}}Subscription {
    pub id: Uuid,
    pub callback_url: String,
    pub from_ledger: i64,
    pub acked_ledger: i64,
    pub status: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Deserialize)]
pub struct Create{{PASCAL}}SubscriptionRequest {
    pub callback_url: String,
    pub from_ledger: i64,
}

#[derive(Debug, Deserialize)]
pub struct {{PASCAL}}AckRequest {
    pub ledger: i64,
}

// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

pub async fn create_{{SNAKE}}_subscription(
    State(state): State<AppState>,
    Json(body): Json<Create{{PASCAL}}SubscriptionRequest>,
) -> Result<(StatusCode, Json<Value>), AppError> {
    if body.callback_url.is_empty() {
        return Err(AppError::Validation("callback_url is required".into()));
    }
    validate_callback_url(&body.callback_url, &state.config.environment)?;
    if body.from_ledger < 0 {
        return Err(AppError::Validation(
            "from_ledger must be non-negative".into(),
        ));
    }

    let sub: {{PASCAL}}Subscription = sqlx::query_as(
        "INSERT INTO {{SNAKE}}_subscriptions (callback_url, from_ledger)
         VALUES ($1, $2)
         RETURNING id, callback_url, from_ledger, acked_ledger, status, created_at",
    )
    .bind(&body.callback_url)
    .bind(body.from_ledger)
    .fetch_one(&state.pool)
    .await?;

    sqlx::query(
        "INSERT INTO {{SNAKE}}_delivery_queue (subscription_id, event_id, ledger)
         SELECT $1, id, ledger FROM events WHERE ledger >= $2
         ORDER BY ledger ASC",
    )
    .bind(sub.id)
    .bind(body.from_ledger)
    .execute(&state.pool)
    .await?;

    Ok((StatusCode::CREATED, Json(json!(sub))))
}

pub async fn get_{{SNAKE}}_subscription(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, AppError> {
    let sub: {{PASCAL}}Subscription = sqlx::query_as(
        "SELECT id, callback_url, from_ledger, acked_ledger, status, created_at
         FROM {{SNAKE}}_subscriptions WHERE id = $1",
    )
    .bind(id)
    .fetch_optional(&state.pool)
    .await?
    .ok_or(AppError::NotFound)?;

    let pending: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM {{SNAKE}}_delivery_queue
         WHERE subscription_id = $1 AND status = 'pending'",
    )
    .bind(id)
    .fetch_one(&state.pool)
    .await?;

    Ok(Json(json!({
        "subscription": sub,
        "pending_deliveries": pending
    })))
}

pub async fn cancel_{{SNAKE}}_subscription(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, AppError> {
    let rows = sqlx::query(
        "UPDATE {{SNAKE}}_subscriptions SET status = 'cancelled'
         WHERE id = $1 AND status = 'active'",
    )
    .bind(id)
    .execute(&state.pool)
    .await?
    .rows_affected();

    if rows == 0 {
        return Err(AppError::NotFound);
    }
    Ok(StatusCode::NO_CONTENT)
}

pub async fn ack_{{SNAKE}}_subscription(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(body): Json<{{PASCAL}}AckRequest>,
) -> Result<Json<Value>, AppError> {
    // Advance acked_ledger only forward; ignore if already at or past this ledger.
    let rows = sqlx::query(
        "UPDATE {{SNAKE}}_subscriptions SET acked_ledger = $1
         WHERE id = $2 AND status = 'active' AND acked_ledger < $1",
    )
    .bind(body.ledger)
    .bind(id)
    .execute(&state.pool)
    .await?
    .rows_affected();

    if rows == 0 {
        let exists: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM {{SNAKE}}_subscriptions WHERE id = $1)",
        )
        .bind(id)
        .fetch_one(&state.pool)
        .await?;
        if !exists {
            return Err(AppError::NotFound);
        }
    }

    sqlx::query(
        "UPDATE {{SNAKE}}_delivery_queue SET status = 'delivered'
         WHERE subscription_id = $1 AND ledger <= $2 AND status = 'pending'",
    )
    .bind(id)
    .bind(body.ledger)
    .execute(&state.pool)
    .await?;

    Ok(Json(json!({ "acked_ledger": body.ledger })))
}

// ---------------------------------------------------------------------------
// Delivery worker
// ---------------------------------------------------------------------------

pub async fn enqueue_{{SNAKE}}_event(pool: &PgPool, event_id: Uuid, ledger: i64) {
    let result = sqlx::query(
        "INSERT INTO {{SNAKE}}_delivery_queue (subscription_id, event_id, ledger)
         SELECT id, $1, $2 FROM {{SNAKE}}_subscriptions
         WHERE status = 'active' AND from_ledger <= $2
         ON CONFLICT DO NOTHING",
    )
    .bind(event_id)
    .bind(ledger)
    .execute(pool)
    .await;

    if let Err(e) = result {
        tracing::warn!(error = %e, "Failed to enqueue event for {{PASCAL}} subscriptions");
    }
}

pub async fn run_{{SNAKE}}_delivery_worker(pool: PgPool, http: reqwest::Client) {
    loop {
        match deliver_{{SNAKE}}_pending(&pool, &http).await {
            Ok(n) if n > 0 => tracing::debug!(delivered = n, "{{PASCAL}} delivery worker cycle"),
            Ok(_) => {}
            Err(e) => tracing::warn!(error = %e, "{{PASCAL}} delivery worker error"),
        }
        sleep(Duration::from_secs(5)).await;
    }
}

async fn deliver_{{SNAKE}}_pending(
    pool: &PgPool,
    http: &reqwest::Client,
) -> Result<usize, sqlx::Error> {
    let rows: Vec<(Uuid, Uuid, String, Value, i64)> = sqlx::query_as(
        "SELECT dq.id, dq.event_id, s.callback_url, e.event_data, dq.ledger
         FROM {{SNAKE}}_delivery_queue dq
         JOIN {{SNAKE}}_subscriptions s ON s.id = dq.subscription_id
         JOIN events e ON e.id = dq.event_id
         WHERE dq.status = 'pending'
           AND dq.next_attempt_at <= NOW()
           AND s.status = 'active'
         ORDER BY dq.ledger ASC
         LIMIT 50",
    )
    .fetch_all(pool)
    .await?;

    let count = rows.len();
    for (queue_id, event_id, callback_url, event_data, ledger) in rows {
        let payload = json!({
            "event_id": event_id,
            "ledger": ledger,
            "event_data": event_data,
        });
        match http
            .post(&callback_url)
            .json(&payload)
            .timeout(Duration::from_secs(10))
            .send()
            .await
        {
            Ok(resp) if resp.status().is_success() => {
                sqlx::query(
                    "UPDATE {{SNAKE}}_delivery_queue SET status = 'delivered' WHERE id = $1",
                )
                .bind(queue_id)
                .execute(pool)
                .await?;
            }
            Ok(resp) => {
                schedule_{{SNAKE}}_retry(pool, queue_id, &format!("HTTP {}", resp.status())).await?;
            }
            Err(e) => {
                schedule_{{SNAKE}}_retry(pool, queue_id, &e.to_string()).await?;
            }
        }
    }
    Ok(count)
}

// Exponential backoff: 2^attempts seconds, capped at 1 hour.
async fn schedule_{{SNAKE}}_retry(
    pool: &PgPool,
    queue_id: Uuid,
    error: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "UPDATE {{SNAKE}}_delivery_queue
         SET attempts = attempts + 1,
             last_error = $2,
             next_attempt_at = NOW() + (LEAST(POWER(2, attempts + 1), 3600) || ' seconds')::interval
         WHERE id = $1",
    )
    .bind(queue_id)
    .bind(error)
    .execute(pool)
    .await?;
    Ok(())
}
"#;

const MIGRATION_TEMPLATE: &str = r#"-- Migration: {{SNAKE}} subscriptions + delivery queue
-- Generated by gen_subscription_scaffold
-- Run: sqlx migrate run

CREATE TABLE IF NOT EXISTS {{SNAKE}}_subscriptions (
    id            UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    callback_url  TEXT        NOT NULL,
    from_ledger   BIGINT      NOT NULL,
    acked_ledger  BIGINT      NOT NULL DEFAULT 0,
    status        TEXT        NOT NULL DEFAULT 'active'
                              CHECK (status IN ('active', 'cancelled')),
    channel_type  TEXT        NOT NULL DEFAULT '{{CHANNEL}}',
    created_at    TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE IF NOT EXISTS {{SNAKE}}_delivery_queue (
    id               UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    subscription_id  UUID        NOT NULL
                                 REFERENCES {{SNAKE}}_subscriptions(id) ON DELETE CASCADE,
    event_id         UUID        NOT NULL
                                 REFERENCES events(id) ON DELETE CASCADE,
    ledger           BIGINT      NOT NULL,
    status           TEXT        NOT NULL DEFAULT 'pending'
                                 CHECK (status IN ('pending', 'delivered', 'failed')),
    attempts         INT         NOT NULL DEFAULT 0,
    next_attempt_at  TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    last_error       TEXT,
    UNIQUE (subscription_id, event_id)
);

CREATE INDEX idx_{{SNAKE}}_delivery_queue_pending
    ON {{SNAKE}}_delivery_queue (next_attempt_at)
    WHERE status = 'pending';
"#;

pub fn generate_handler(config: &ScaffoldConfig) -> GeneratedFile {
    GeneratedFile {
        relative_path: format!("src/{}_subscriptions.rs", config.snake_name),
        content: apply(HANDLER_TEMPLATE, config),
    }
}

pub fn generate_migration(config: &ScaffoldConfig) -> GeneratedFile {
    let timestamp = chrono::Utc::now().format("%Y%m%d%H%M%S");
    GeneratedFile {
        relative_path: format!(
            "migrations/{}_add_{}_subscriptions.sql",
            timestamp, config.snake_name
        ),
        content: apply(MIGRATION_TEMPLATE, config),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen::{ChannelType, ScaffoldConfig};

    fn cfg() -> ScaffoldConfig {
        ScaffoldConfig::new("token-transfer", ChannelType::Webhook, false, false)
    }

    #[test]
    fn handler_contains_pascal_name() {
        let f = generate_handler(&cfg());
        assert!(f.content.contains("TokenTransferSubscription"));
        assert!(f.content.contains("CreateTokenTransferSubscriptionRequest"));
    }

    #[test]
    fn handler_contains_snake_name() {
        let f = generate_handler(&cfg());
        assert!(f.content.contains("create_token_transfer_subscription"));
        assert!(f.content.contains("token_transfer_subscriptions"));
    }

    #[test]
    fn migration_contains_table_names() {
        let f = generate_migration(&cfg());
        assert!(f.content.contains("token_transfer_subscriptions"));
        assert!(f.content.contains("token_transfer_delivery_queue"));
    }

    #[test]
    fn migration_contains_channel_type() {
        let f = generate_migration(&cfg());
        assert!(f.content.contains("'webhook'"));
    }

    #[test]
    fn handler_path_uses_snake_name() {
        let f = generate_handler(&cfg());
        assert_eq!(f.relative_path, "src/token_transfer_subscriptions.rs");
    }

    #[test]
    fn migration_path_ends_with_sql() {
        let f = generate_migration(&cfg());
        assert!(f.relative_path.ends_with("_add_token_transfer_subscriptions.sql"));
    }
}
