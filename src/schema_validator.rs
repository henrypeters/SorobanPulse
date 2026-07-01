//! Issue #617: JSON Schema validation for Soroban event data.
//!
//! Compiles and caches JSON Draft-7 schemas per contract. Schemas are stored in the
//! `contract_schemas` table and loaded at startup. Each schema update increments a
//! `version` counter (via DB trigger) so consumers can detect stale cached copies.
//!
//! ## Integration points
//! - `register_schema` / `get_schema` / `delete_schema` / `list_schemas` — CRUD.
//! - `validate_event_data` — called by the indexer to gate event storage.
//! - `record_validation_metrics` — persists pass/fail counts to `schema_validation_metrics`.

use jsonschema::{Draft, JSONSchema};
use serde::Serialize;
use serde_json::Value;
use sqlx::PgPool;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, warn};

use crate::error::ValidationErrorDetail;
use crate::metrics;

/// Summary returned when listing all registered schemas.
#[derive(Debug, Serialize)]
pub struct SchemaInfo {
    pub contract_id: String,
    pub version: i32,
    pub description: Option<String>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

/// Schema validator that caches compiled JSON schemas per contract.
#[derive(Clone)]
pub struct SchemaValidator {
    pool: PgPool,
    cache: Arc<RwLock<HashMap<String, Arc<JSONSchema>>>>,
}

impl SchemaValidator {
    pub fn new(pool: PgPool) -> Self {
        Self {
            pool,
            cache: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Load all schemas from the database into the in-memory cache.
    pub async fn load_schemas(&self) -> Result<(), sqlx::Error> {
        let schemas: Vec<(String, Value)> =
            sqlx::query_as("SELECT contract_id, schema FROM contract_schemas")
                .fetch_all(&self.pool)
                .await?;

        let mut cache = self.cache.write().await;
        for (contract_id, schema_value) in schemas {
            match JSONSchema::options()
                .with_draft(Draft::Draft7)
                .compile(&schema_value)
            {
                Ok(compiled) => {
                    cache.insert(contract_id.clone(), Arc::new(compiled));
                    debug!(contract_id = %contract_id, "Loaded schema for contract");
                }
                Err(e) => {
                    warn!(contract_id = %contract_id, error = %e, "Failed to compile schema");
                }
            }
        }
        Ok(())
    }

    /// Register (or replace) a JSON Schema for a contract.
    ///
    /// The `version` column is incremented automatically by the DB trigger on UPDATE.
    pub async fn register_schema(
        &self,
        contract_id: &str,
        schema: &Value,
    ) -> Result<(), anyhow::Error> {
        register_schema_with_desc(self, contract_id, schema, None).await
    }

    /// Register a schema with an optional human-readable description.
    pub async fn register_schema_described(
        &self,
        contract_id: &str,
        schema: &Value,
        description: Option<&str>,
    ) -> Result<(), anyhow::Error> {
        register_schema_with_desc(self, contract_id, schema, description).await
    }

    /// Validate event data against the registered schema for a contract.
    ///
    /// Returns:
    /// - `None` — no schema is registered for this contract (event is accepted).
    /// - `Some((true, []))` — validation passed.
    /// - `Some((false, errors))` — validation failed with structured error details.
    ///
    /// Increments Prometheus counters (`schema_validation_pass/fail_total`) on every call.
    pub async fn validate_event_data(
        &self,
        contract_id: &str,
        event_data: &Value,
    ) -> Option<(bool, Vec<ValidationErrorDetail>)> {
        let cache = self.cache.read().await;
        let schema = cache.get(contract_id)?;

        let is_valid = schema.is_valid(event_data);

        if !is_valid {
            if let Err(errors) = schema.validate(event_data) {
                let error_details: Vec<ValidationErrorDetail> = errors
                    .map(|e| ValidationErrorDetail {
                        instance_path: e.instance_path.to_string(),
                        schema_path: e.schema_path.to_string(),
                        message: e.to_string(),
                    })
                    .collect();

                let error_messages: Vec<String> = error_details
                    .iter()
                    .map(|e| format!("{} at {}", e.message, e.instance_path))
                    .collect();

                warn!(
                    contract_id = %contract_id,
                    errors = ?error_messages,
                    "Event data failed schema validation"
                );

                metrics::record_schema_validation_fail(contract_id);
                return Some((false, error_details));
            }
        }

        metrics::record_schema_validation_pass(contract_id);
        Some((is_valid, vec![]))
    }

    /// Retrieve the full schema JSON and its current version for a contract.
    pub async fn get_schema(&self, contract_id: &str) -> Option<Value> {
        sqlx::query_scalar::<_, Value>(
            "SELECT schema FROM contract_schemas WHERE contract_id = $1",
        )
        .bind(contract_id)
        .fetch_optional(&self.pool)
        .await
        .ok()
        .flatten()
    }

    /// Retrieve schema metadata (without the full schema body) for a contract.
    pub async fn get_schema_info(&self, contract_id: &str) -> Option<SchemaInfo> {
        sqlx::query_as::<_, (String, i32, Option<String>, chrono::DateTime<chrono::Utc>)>(
            "SELECT contract_id, version, description, updated_at \
             FROM contract_schemas WHERE contract_id = $1",
        )
        .bind(contract_id)
        .fetch_optional(&self.pool)
        .await
        .ok()
        .flatten()
        .map(|(cid, version, desc, updated_at)| SchemaInfo {
            contract_id: cid,
            version,
            description: desc,
            updated_at,
        })
    }

    /// List all registered schemas (metadata only, no schema body).
    pub async fn list_schemas(&self) -> Result<Vec<SchemaInfo>, sqlx::Error> {
        let rows = sqlx::query_as::<_, (String, i32, Option<String>, chrono::DateTime<chrono::Utc>)>(
            "SELECT contract_id, version, description, updated_at \
             FROM contract_schemas ORDER BY contract_id",
        )
        .fetch_all(&self.pool)
        .await?;

        Ok(rows
            .into_iter()
            .map(|(contract_id, version, description, updated_at)| SchemaInfo {
                contract_id,
                version,
                description,
                updated_at,
            })
            .collect())
    }

    /// Delete the schema for a contract and evict it from the cache.
    pub async fn delete_schema(&self, contract_id: &str) -> Result<bool, sqlx::Error> {
        let result = sqlx::query("DELETE FROM contract_schemas WHERE contract_id = $1")
            .bind(contract_id)
            .execute(&self.pool)
            .await?;

        if result.rows_affected() > 0 {
            let mut cache = self.cache.write().await;
            cache.remove(contract_id);
            debug!(contract_id = %contract_id, "Deleted schema for contract");
            Ok(true)
        } else {
            Ok(false)
        }
    }

    /// Persist cumulative pass/fail counters to `schema_validation_metrics`.
    ///
    /// Called periodically to keep the DB table in sync with the in-process counters.
    /// Uses `ON CONFLICT DO UPDATE` so the row is upserted atomically.
    pub async fn record_validation_metrics(
        pool: &PgPool,
        contract_id: &str,
        pass_delta: i64,
        fail_delta: i64,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            r#"
            INSERT INTO schema_validation_metrics (contract_id, pass_count, fail_count, last_checked)
            VALUES ($1, $2, $3, NOW())
            ON CONFLICT (contract_id) DO UPDATE
                SET pass_count   = schema_validation_metrics.pass_count + EXCLUDED.pass_count,
                    fail_count   = schema_validation_metrics.fail_count + EXCLUDED.fail_count,
                    last_checked = NOW()
            "#,
        )
        .bind(contract_id)
        .bind(pass_delta)
        .bind(fail_delta)
        .execute(pool)
        .await?;
        Ok(())
    }
}

/// Internal helper shared by `register_schema` and `register_schema_described`.
async fn register_schema_with_desc(
    sv: &SchemaValidator,
    contract_id: &str,
    schema: &Value,
    description: Option<&str>,
) -> Result<(), anyhow::Error> {
    let compiled = JSONSchema::options()
        .with_draft(Draft::Draft7)
        .compile(schema)
        .map_err(|e| anyhow::anyhow!("Invalid JSON Schema: {}", e))?;

    sqlx::query(
        r#"
        INSERT INTO contract_schemas (contract_id, schema, description, updated_at)
        VALUES ($1, $2, $3, NOW())
        ON CONFLICT (contract_id)
        DO UPDATE SET schema = EXCLUDED.schema,
                      description = COALESCE(EXCLUDED.description, contract_schemas.description),
                      updated_at = NOW()
        "#,
    )
    .bind(contract_id)
    .bind(schema)
    .bind(description)
    .execute(&sv.pool)
    .await?;

    let mut cache = sv.cache.write().await;
    cache.insert(contract_id.to_string(), Arc::new(compiled));

    debug!(contract_id = %contract_id, "Registered schema for contract");
    Ok(())
}
