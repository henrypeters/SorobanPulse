use jsonschema::{Draft, JSONSchema, error::ValidationError};
use serde_json::Value;
use sqlx::PgPool;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, warn};
use crate::error::ValidationErrorDetail;

/// Schema validator that caches compiled JSON schemas per contract
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

    /// Load all schemas from the database into the cache
    pub async fn load_schemas(&self) -> Result<(), sqlx::Error> {
        let schemas: Vec<(String, Value)> = sqlx::query_as(
            "SELECT contract_id, schema FROM contract_schemas"
        )
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

    /// Register a new schema for a contract
    pub async fn register_schema(
        &self,
        contract_id: &str,
        schema: &Value,
    ) -> Result<(), anyhow::Error> {
        // Validate that the schema itself is valid
        let compiled = JSONSchema::options()
            .with_draft(Draft::Draft7)
            .compile(schema)
            .map_err(|e| anyhow::anyhow!("Invalid JSON Schema: {}", e))?;

        // Store in database
        sqlx::query(
            r#"
            INSERT INTO contract_schemas (contract_id, schema, updated_at)
            VALUES ($1, $2, NOW())
            ON CONFLICT (contract_id)
            DO UPDATE SET schema = EXCLUDED.schema, updated_at = NOW()
            "#,
        )
        .bind(contract_id)
        .bind(schema)
        .execute(&self.pool)
        .await?;

        // Update cache
        let mut cache = self.cache.write().await;
        cache.insert(contract_id.to_string(), Arc::new(compiled));

        debug!(contract_id = %contract_id, "Registered schema for contract");
        Ok(())
    }

    /// Validate event data against the registered schema for a contract
    /// Returns:
    /// - None if no schema is registered for this contract
    /// - Some((true, vec![])) if validation passes
    /// - Some((false, errors)) if validation fails with error details
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
                
                return Some((false, error_details));
            }
        }

        Some((is_valid, vec![]))
    }

    /// Get the schema for a contract
    pub async fn get_schema(&self, contract_id: &str) -> Option<Value> {
        sqlx::query_scalar::<_, Value>(
            "SELECT schema FROM contract_schemas WHERE contract_id = $1"
        )
        .bind(contract_id)
        .fetch_optional(&self.pool)
        .await
        .ok()
        .flatten()
    }

    /// Delete a schema for a contract
    pub async fn delete_schema(&self, contract_id: &str) -> Result<bool, sqlx::Error> {
        let result = sqlx::query(
            "DELETE FROM contract_schemas WHERE contract_id = $1"
        )
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
}
