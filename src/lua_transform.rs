//! Lua scripting hook for event transformation
//!
//! This module provides optional Lua scripting support for transforming events
//! before they are stored in the database. Scripts can modify event data or
//! return nil to skip events entirely.

use crate::models::SorobanEvent;
use crate::metrics;
use anyhow::{Context, Result};
use mlua::{Lua, Table, Value as LuaValue};
use serde_json::Value;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;
use tracing::{debug, warn};

/// Lua script executor for event transformation
pub struct LuaTransformer {
    lua: Arc<Lua>,
    timeout: Duration,
    script_path: String,
}

const LUA_INSTRUCTION_LIMIT: u32 = 1_000_000;
const LUA_MEMORY_LIMIT_MB: usize = 64;

impl LuaTransformer {
    /// Create a new LuaTransformer by loading a script from the given path
    pub fn new(script_path: &Path, timeout_ms: u64) -> Result<Self> {
        let lua = Lua::new();
        
        // Set memory limit (in bytes)
        lua.set_memory_limit(LUA_MEMORY_LIMIT_MB * 1024 * 1024)?;
        
        // Load the script
        let script_content = std::fs::read_to_string(script_path)
            .with_context(|| format!("Failed to read Lua script: {}", script_path.display()))?;
        
        lua.load(&script_content)
            .exec()
            .with_context(|| format!("Failed to execute Lua script: {}", script_path.display()))?;
        
        debug!(
            script_path = %script_path.display(),
            timeout_ms = timeout_ms,
            instruction_limit = LUA_INSTRUCTION_LIMIT,
            memory_limit_mb = LUA_MEMORY_LIMIT_MB,
            "Lua transformer initialized"
        );
        
        Ok(Self {
            lua: Arc::new(lua),
            timeout: Duration::from_millis(timeout_ms),
            script_path: script_path.display().to_string(),
        })
    }
    
    /// Transform an event using the loaded Lua script
    /// 
    /// Returns:
    /// - Ok(Some(event)) if the event was transformed or passed through
    /// - Ok(None) if the script returned nil (skip the event)
    /// - Err if the transformation failed
    pub async fn transform(&self, event: SorobanEvent) -> Result<Option<SorobanEvent>> {
        let lua = self.lua.clone();
        let timeout = self.timeout;
        let script_path = self.script_path.clone();
        
        // Run the Lua transformation in a blocking task with timeout
        let result = tokio::time::timeout(
            timeout,
            tokio::task::spawn_blocking(move || Self::transform_sync(&lua, event))
        ).await;
        
        match result {
            Ok(Ok(transformed)) => Ok(transformed),
            Ok(Err(e)) => {
                warn!(error = %e, script_path = %script_path, "Lua transformation failed");
                Err(e)
            }
            Err(_) => {
                warn!(timeout_ms = ?timeout.as_millis(), script_path = %script_path, "Lua script timeout");
                metrics::record_lua_timeout();
                Err(anyhow::anyhow!("Lua script execution timeout"))
            }
        }
    }
    
    /// Synchronous transformation logic
    fn transform_sync(lua: &Lua, event: SorobanEvent) -> Result<Option<SorobanEvent>> {
        // Set instruction hook to limit CPU usage
        lua.set_hook(mlua::HookTriggers::new().with_every_nth_instruction(1000), |_lua, _debug| {
            Err(mlua::Error::external("Instruction limit exceeded"))
        })?;
        
        // Get the transform_event function from the Lua script
        let transform_fn: mlua::Function = lua
            .globals()
            .get("transform_event")
            .context("Lua script must define a 'transform_event' function")?;
        
        // Convert the event to a Lua table
        let event_table = Self::event_to_lua_table(lua, &event)?;
        
        // Call the transform function
        let result: LuaValue = transform_fn
            .call(event_table)
            .context("Error calling transform_event function")?;
        
        // Handle the result
        match result {
            LuaValue::Nil => {
                debug!(
                    tx_hash = %event.tx_hash,
                    contract_id = %event.contract_id,
                    "Event skipped by Lua script"
                );
                Ok(None)
            }
            LuaValue::Table(table) => {
                let transformed = Self::lua_table_to_event(lua, table, event)?;
                Ok(Some(transformed))
            }
            _ => Err(anyhow::anyhow!(
                "transform_event must return a table or nil, got {:?}",
                result.type_name()
            )),
        }
    }
    
    /// Convert a SorobanEvent to a Lua table
    fn event_to_lua_table<'lua>(lua: &'lua Lua, event: &SorobanEvent) -> Result<Table<'lua>> {
        let table = lua.create_table()?;
        
        table.set("contract_id", event.contract_id.clone())?;
        table.set("event_type", event.event_type.clone())?;
        table.set("tx_hash", event.tx_hash.clone())?;
        table.set("ledger", event.ledger)?;
        table.set("ledger_closed_at", event.ledger_closed_at.clone())?;
        
        if let Some(ref hash) = event.ledger_hash {
            table.set("ledger_hash", hash.clone())?;
        }
        
        table.set("in_successful_call", event.in_successful_call)?;
        
        // Convert JSON values to Lua values
        let value_lua = Self::json_to_lua(lua, &event.value)?;
        table.set("value", value_lua)?;
        
        if let Some(ref topic) = event.topic {
            let topic_table = lua.create_table()?;
            for (i, item) in topic.iter().enumerate() {
                let item_lua = Self::json_to_lua(lua, item)?;
                topic_table.set(i + 1, item_lua)?; // Lua arrays are 1-indexed
            }
            table.set("topic", topic_table)?;
        }
        
        Ok(table)
    }
    
    /// Convert a Lua table back to a SorobanEvent
    fn lua_table_to_event(lua: &Lua, table: Table, original: SorobanEvent) -> Result<SorobanEvent> {
        Ok(SorobanEvent {
            contract_id: table.get("contract_id").unwrap_or(original.contract_id),
            event_type: table.get("event_type").unwrap_or(original.event_type),
            tx_hash: table.get("tx_hash").unwrap_or(original.tx_hash),
            ledger: table.get("ledger").unwrap_or(original.ledger),
            ledger_closed_at: table.get("ledger_closed_at").unwrap_or(original.ledger_closed_at),
            ledger_hash: table.get("ledger_hash").ok().or(original.ledger_hash),
            in_successful_call: table.get("in_successful_call").unwrap_or(original.in_successful_call),
            value: Self::lua_to_json(lua, table.get("value")?)?,
            topic: match table.get::<_, LuaValue>("topic")? {
                LuaValue::Nil => original.topic,
                LuaValue::Table(topic_table) => {
                    let mut topic_vec = Vec::new();
                    for pair in topic_table.pairs::<usize, LuaValue>() {
                        let (_, value) = pair?;
                        topic_vec.push(Self::lua_to_json(lua, value)?);
                    }
                    Some(topic_vec)
                }
                _ => original.topic,
            },
        })
    }
    
    /// Convert a JSON value to a Lua value
    fn json_to_lua<'lua>(lua: &'lua Lua, value: &Value) -> Result<LuaValue<'lua>> {
        match value {
            Value::Null => Ok(LuaValue::Nil),
            Value::Bool(b) => Ok(LuaValue::Boolean(*b)),
            Value::Number(n) => {
                if let Some(i) = n.as_i64() {
                    Ok(LuaValue::Integer(i))
                } else if let Some(f) = n.as_f64() {
                    Ok(LuaValue::Number(f))
                } else {
                    Ok(LuaValue::Nil)
                }
            }
            Value::String(s) => Ok(LuaValue::String(lua.create_string(s)?)),
            Value::Array(arr) => {
                let table = lua.create_table()?;
                for (i, item) in arr.iter().enumerate() {
                    table.set(i + 1, Self::json_to_lua(lua, item)?)?;
                }
                Ok(LuaValue::Table(table))
            }
            Value::Object(obj) => {
                let table = lua.create_table()?;
                for (key, val) in obj.iter() {
                    table.set(key.as_str(), Self::json_to_lua(lua, val)?)?;
                }
                Ok(LuaValue::Table(table))
            }
        }
    }
    
    /// Convert a Lua value to a JSON value
    fn lua_to_json(_lua: &Lua, value: LuaValue) -> Result<Value> {
        match value {
            LuaValue::Nil => Ok(Value::Null),
            LuaValue::Boolean(b) => Ok(Value::Bool(b)),
            LuaValue::Integer(i) => Ok(Value::Number(i.into())),
            LuaValue::Number(n) => {
                Ok(serde_json::Number::from_f64(n)
                    .map(Value::Number)
                    .unwrap_or(Value::Null))
            }
            LuaValue::String(s) => Ok(Value::String(s.to_str()?.to_string())),
            LuaValue::Table(table) => {
                // Check if it's an array or object
                let mut is_array = true;
                let mut max_index = 0;
                
                for pair in table.clone().pairs::<LuaValue, LuaValue>() {
                    let (key, _) = pair?;
                    if let LuaValue::Integer(i) = key {
                        if i > 0 {
                            max_index = max_index.max(i as usize);
                        } else {
                            is_array = false;
                            break;
                        }
                    } else {
                        is_array = false;
                        break;
                    }
                }
                
                if is_array && max_index > 0 {
                    let mut arr = Vec::new();
                    for i in 1..=max_index {
                        let val: LuaValue = table.get(i)?;
                        arr.push(Self::lua_to_json(_lua, val)?);
                    }
                    Ok(Value::Array(arr))
                } else {
                    let mut obj = serde_json::Map::new();
                    for pair in table.pairs::<LuaValue, LuaValue>() {
                        let (key, val) = pair?;
                        let key_str = match key {
                            LuaValue::String(s) => s.to_str()?.to_string(),
                            LuaValue::Integer(i) => i.to_string(),
                            LuaValue::Number(n) => n.to_string(),
                            _ => continue,
                        };
                        obj.insert(key_str, Self::lua_to_json(_lua, val)?);
                    }
                    Ok(Value::Object(obj))
                }
            }
            _ => Err(anyhow::anyhow!("Unsupported Lua type: {}", value.type_name())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::io::Write;
    use tempfile::NamedTempFile;

    fn create_test_event() -> SorobanEvent {
        SorobanEvent {
            contract_id: "CAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAD2KM".to_string(),
            event_type: "contract".to_string(),
            tx_hash: "abc123".to_string(),
            ledger: 100,
            ledger_closed_at: "2024-01-01T00:00:00Z".to_string(),
            ledger_hash: Some("hash123".to_string()),
            in_successful_call: true,
            value: json!({"amount": 1000}),
            topic: Some(vec![json!("transfer")]),
        }
    }

    #[tokio::test]
    async fn test_passthrough_script() {
        let mut script_file = NamedTempFile::new().unwrap();
        writeln!(
            script_file,
            r#"
            function transform_event(event)
                return event
            end
            "#
        )
        .unwrap();

        let transformer = LuaTransformer::new(script_file.path(), 100).unwrap();
        let event = create_test_event();
        let result = transformer.transform(event.clone()).await.unwrap();

        assert!(result.is_some());
        let transformed = result.unwrap();
        assert_eq!(transformed.contract_id, event.contract_id);
        assert_eq!(transformed.tx_hash, event.tx_hash);
    }

    #[tokio::test]
    async fn test_skip_event() {
        let mut script_file = NamedTempFile::new().unwrap();
        writeln!(
            script_file,
            r#"
            function transform_event(event)
                return nil
            end
            "#
        )
        .unwrap();

        let transformer = LuaTransformer::new(script_file.path(), 100).unwrap();
        let event = create_test_event();
        let result = transformer.transform(event).await.unwrap();

        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_modify_event() {
        let mut script_file = NamedTempFile::new().unwrap();
        writeln!(
            script_file,
            r#"
            function transform_event(event)
                event.value.amount = event.value.amount * 2
                event.value.transformed = true
                return event
            end
            "#
        )
        .unwrap();

        let transformer = LuaTransformer::new(script_file.path(), 100).unwrap();
        let event = create_test_event();
        let result = transformer.transform(event).await.unwrap();

        assert!(result.is_some());
        let transformed = result.unwrap();
        assert_eq!(transformed.value["amount"], 2000);
        assert_eq!(transformed.value["transformed"], true);
    }

    #[tokio::test]
    async fn test_filter_by_contract() {
        let mut script_file = NamedTempFile::new().unwrap();
        writeln!(
            script_file,
            r#"
            function transform_event(event)
                if event.contract_id == "CAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAD2KM" then
                    return event
                else
                    return nil
                end
            end
            "#
        )
        .unwrap();

        let transformer = LuaTransformer::new(script_file.path(), 100).unwrap();
        let event = create_test_event();
        let result = transformer.transform(event).await.unwrap();

        assert!(result.is_some());
    }

    #[tokio::test]
    async fn test_timeout() {
        let mut script_file = NamedTempFile::new().unwrap();
        writeln!(
            script_file,
            r#"
            function transform_event(event)
                while true do end
                return event
            end
            "#
        )
        .unwrap();

        let transformer = LuaTransformer::new(script_file.path(), 50).unwrap();
        let event = create_test_event();
        let result = transformer.transform(event).await;

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_memory_limit() {
        let mut script_file = NamedTempFile::new().unwrap();
        writeln!(
            script_file,
            r#"
            function transform_event(event)
                local t = {}
                for i = 1, 10000000 do
                    t[i] = string.rep("x", 1000)
                end
                return event
            end
            "#
        )
        .unwrap();

        let transformer = LuaTransformer::new(script_file.path(), 5000).unwrap();
        let event = create_test_event();
        let result = transformer.transform(event).await;

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_instruction_limit() {
        let mut script_file = NamedTempFile::new().unwrap();
        writeln!(
            script_file,
            r#"
            function transform_event(event)
                local sum = 0
                for i = 1, 100000000 do
                    sum = sum + i
                end
                return event
            end
            "#
        )
        .unwrap();

        let transformer = LuaTransformer::new(script_file.path(), 5000).unwrap();
        let event = create_test_event();
        let result = transformer.transform(event).await;

        assert!(result.is_err());
    }
}
