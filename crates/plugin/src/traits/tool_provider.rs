use denkwerk::DynKernelFunction;
use uuid::Uuid;

/// Context passed to tool providers when assembling tools for a conversation.
#[derive(Debug, Clone)]
pub struct ToolContext {
    /// The agent that owns the conversation.
    pub agent_id: Uuid,
    /// The current conversation.
    pub conversation_id: Uuid,
    /// The user making the request.
    pub user_id: Uuid,
    /// Whether this is a scheduled task execution (not interactive).
    pub is_task_execution: bool,
}

/// Extension trait for plugins that provide tools to agents.
#[async_trait::async_trait]
pub trait ToolProvider: Send + Sync {
    /// Return the tools this plugin provides for the given context.
    async fn tools(&self, ctx: &ToolContext) -> Vec<DynKernelFunction>;
}
