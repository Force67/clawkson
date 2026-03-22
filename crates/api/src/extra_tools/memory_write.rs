/// Memory write tool: allows agents to persist explicit long-term notes.
use std::sync::Arc;

use denkwerk::functions::{FunctionDefinition, FunctionParameter, KernelFunction};
use denkwerk::DynKernelFunction;
use serde_json::{json, Value};
use uuid::Uuid;

use clawkson_db::Db;

pub struct MemoryWriteTool {
    db: Db,
    agent_id: Uuid,
}

impl MemoryWriteTool {
    pub fn new(db: Db, agent_id: Uuid) -> Self { Self { db, agent_id } }
    pub fn into_dyn(self) -> DynKernelFunction { Arc::new(self) }
}

#[async_trait::async_trait]
impl KernelFunction for MemoryWriteTool {
    fn definition(&self) -> FunctionDefinition {
        let mut def = FunctionDefinition::new("memory_write")
            .with_description("Write an explicit long-term memory note. Use this to remember important facts, user preferences, decisions, or context that persists across conversations.");
        def.add_parameter(FunctionParameter::new("title", json!({"type": "string"})).with_description("Short title for the memory"));
        def.add_parameter(FunctionParameter::new("content", json!({"type": "string"})).with_description("The memory content to persist"));
        def
    }

    async fn invoke(&self, arguments: &Value) -> Result<Value, denkwerk::LLMError> {
        let title = arguments.get("title").and_then(|v| v.as_str())
            .ok_or_else(|| denkwerk::LLMError::InvalidFunctionArguments("title required".into()))?;
        let content = arguments.get("content").and_then(|v| v.as_str())
            .ok_or_else(|| denkwerk::LLMError::InvalidFunctionArguments("content required".into()))?;

        let pool = self.db.pool();
        let kb = match clawkson_db::knowledge_base::get_or_create_agent_memory_kb(
            pool, self.agent_id, Uuid::nil(), "default",
        ).await {
            Ok(kb) => kb,
            Err(e) => return Ok(json!({"error": format!("memory KB: {e}")})),
        };

        match clawkson_db::knowledge_entry::create(pool, kb.id, title, content, None).await {
            Ok(_) => Ok(json!({"status": "ok", "title": title, "message": format!("Memory '{}' saved.", title)})),
            Err(e) => Ok(json!({"error": format!("create entry: {e}")})),
        }
    }
}
