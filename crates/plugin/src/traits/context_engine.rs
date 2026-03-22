use serde_json::Value;
use uuid::Uuid;

/// Context passed through the context engine pipeline.
#[derive(Debug, Clone)]
pub struct ContextPipelineState {
    pub agent_id: Uuid,
    pub conversation_id: Uuid,
    pub user_id: Uuid,
    /// The history as (role, content, images) tuples.
    pub history: Vec<(String, String, Vec<String>)>,
    /// Metadata bag for plugins to pass data between stages.
    pub metadata: Value,
}

/// Extension trait for plugins that hook into the context engine pipeline.
///
/// The pipeline has 4 stages:
/// 1. **Ingest** — before saving user message (entity extraction, intent detection)
/// 2. **Assemble** — after loading history (inject memories, daily logs, RAG context)
/// 3. **Compact** — when tokens exceed budget (summarize old messages)
/// 4. **AfterTurn** — after assistant response (memory updates, daily log append)
#[async_trait::async_trait]
pub trait ContextEnginePlugin: Send + Sync {
    /// Called before saving the user message. Can extract entities, detect intent, etc.
    async fn on_ingest(&self, _state: &mut ContextPipelineState) -> anyhow::Result<()> {
        Ok(())
    }

    /// Called after loading history. Can inject additional context (memories, RAG, etc).
    async fn on_assemble(&self, _state: &mut ContextPipelineState) -> anyhow::Result<()> {
        Ok(())
    }

    /// Called when token count exceeds budget. Can summarize/compress old messages.
    async fn on_compact(&self, _state: &mut ContextPipelineState) -> anyhow::Result<()> {
        Ok(())
    }

    /// Called after the assistant's response. Can update memories, logs, etc.
    async fn after_turn(
        &self,
        _state: &mut ContextPipelineState,
        _assistant_response: &str,
    ) -> anyhow::Result<()> {
        Ok(())
    }
}
