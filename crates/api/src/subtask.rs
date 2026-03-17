//! Sub-agent coordination: a tool that lets the LLM decompose complex work into
//! parallel sub-tasks, each running its own LLM completion loop.

use std::sync::Arc;

use denkwerk::{
    functions::{FunctionParameter, KernelFunction},
    DynKernelFunction, FunctionDefinition,
};
use futures::stream::FuturesUnordered;
use futures::StreamExt;
use serde::Deserialize;
use serde_json::Value;
use uuid::Uuid;

use crate::routes::conversations::AgentConfig;
use crate::state::AppState;
use clawkson_core::LlmConnector;

/// Maximum number of sub-tasks that can run in parallel.
const MAX_SUBTASKS: usize = 5;
/// Maximum tool-calling rounds per sub-task.
const MAX_SUBTASK_ROUNDS: usize = 5;

/// A tool that lets the LLM break complex work into parallel sub-tasks.
/// Each sub-task spawns its own LLM completion with the same tools (minus delegation).
pub struct DelegateTasksTool {
    state: AppState,
    agent_cfg: AgentConfig,
    connector: LlmConnector,
    conversation_id: Uuid,
    user_id: Uuid,
    search_enabled: bool,
    timeout_secs: u64,
    /// When present, subtask progress events are emitted to the parent stream.
    tx: Option<tokio::sync::mpsc::Sender<String>>,
}

#[derive(Debug, Deserialize)]
struct DelegateArgs {
    tasks: Vec<SubtaskDef>,
}

#[derive(Debug, Clone, Deserialize)]
struct SubtaskDef {
    /// Short identifier for correlating results (e.g. "research", "analysis").
    id: String,
    /// What the sub-agent should do.
    description: String,
    /// Optional extra context to provide to the sub-agent.
    #[serde(default)]
    context: Option<String>,
}

impl DelegateTasksTool {
    pub(crate) fn new(
        state: AppState,
        agent_cfg: AgentConfig,
        connector: LlmConnector,
        conversation_id: Uuid,
        user_id: Uuid,
        search_enabled: bool,
        timeout_secs: u64,
        tx: Option<tokio::sync::mpsc::Sender<String>>,
    ) -> Self {
        Self {
            state,
            agent_cfg,
            connector,
            conversation_id,
            user_id,
            search_enabled,
            timeout_secs,
            tx,
        }
    }

    pub(crate) fn into_dyn(self) -> DynKernelFunction {
        Arc::new(self)
    }

    /// Emit a subtask event to the parent stream.
    fn emit_event(&self, event: &Value) {
        if let Some(tx) = &self.tx {
            let _ = tx.try_send(format!("\x02{event}"));
        }
    }
}

#[async_trait::async_trait]
impl KernelFunction for DelegateTasksTool {
    fn definition(&self) -> FunctionDefinition {
        let mut def = FunctionDefinition::new("delegate_tasks")
            .with_description(
                "Break complex work into parallel sub-tasks that run simultaneously. \
                 Each sub-task gets its own independent AI agent with access to the same tools \
                 (code execution, knowledge search, HTTP requests, etc.). \
                 Use this when a task naturally decomposes into independent parts that can be \
                 worked on in parallel — for example, researching multiple topics, analyzing \
                 different data sources, or running several computations at once. \
                 Do NOT use this for simple sequential tasks or when sub-tasks depend on each other's results. \
                 Maximum 5 sub-tasks per call.",
            );

        def.add_parameter(
            FunctionParameter::new(
                "tasks",
                serde_json::json!({
                    "type": "array",
                    "items": {
                        "type": "object",
                        "properties": {
                            "id": {
                                "type": "string",
                                "description": "Short identifier for this sub-task (e.g. 'research_competitors', 'analyze_data')"
                            },
                            "description": {
                                "type": "string",
                                "description": "Clear, complete instructions for what this sub-task should accomplish"
                            },
                            "context": {
                                "type": "string",
                                "description": "Optional additional context or data to provide to this sub-task"
                            }
                        },
                        "required": ["id", "description"]
                    },
                    "minItems": 1,
                    "maxItems": 5
                }),
            )
            .with_description("Array of sub-tasks to execute in parallel"),
        );

        def
    }

    async fn invoke(&self, arguments: &Value) -> Result<Value, denkwerk::LLMError> {
        let args: DelegateArgs = serde_json::from_value(arguments.clone()).map_err(|e| {
            denkwerk::LLMError::InvalidFunctionArguments(format!(
                "Invalid arguments for delegate_tasks: {e}"
            ))
        })?;

        if args.tasks.is_empty() {
            return Ok(serde_json::json!({ "error": "No tasks provided" }));
        }

        let tasks = if args.tasks.len() > MAX_SUBTASKS {
            tracing::warn!(
                "delegate_tasks called with {} tasks, capping at {MAX_SUBTASKS}",
                args.tasks.len()
            );
            args.tasks[..MAX_SUBTASKS].to_vec()
        } else {
            args.tasks
        };

        let task_count = tasks.len();
        tracing::info!("delegate_tasks: spawning {task_count} parallel sub-tasks");

        // Resolve the LLM connector for sub-tasks (may differ from parent)
        let subtask_connector = resolve_subtask_connector(
            &self.state,
            &self.agent_cfg,
            &self.connector,
        )
        .await;

        if subtask_connector.id != self.connector.id {
            tracing::info!(
                "sub-tasks using dedicated connector: {} ({})",
                subtask_connector.name,
                subtask_connector.model
            );
        }

        // Emit subtask_start events
        for task in &tasks {
            self.emit_event(&serde_json::json!({
                "type": "subtask_start",
                "id": task.id,
                "description": task.description,
                "total": task_count,
            }));
        }

        // Spawn all sub-tasks in parallel
        let mut futures: FuturesUnordered<_> = tasks
            .iter()
            .map(|task| {
                let state = self.state.clone();
                let agent_cfg = self.agent_cfg.clone();
                let connector = subtask_connector.clone();
                let conv_id = self.conversation_id;
                let user_id = self.user_id;
                let search_enabled = self.search_enabled;
                let timeout = self.timeout_secs;
                let task = task.clone();

                async move {
                    let start = std::time::Instant::now();
                    let result = run_subtask(
                        &state,
                        &agent_cfg,
                        &connector,
                        conv_id,
                        user_id,
                        search_enabled,
                        timeout,
                        &task,
                    )
                    .await;
                    let duration_ms = start.elapsed().as_millis() as u64;
                    (task.id.clone(), result, duration_ms)
                }
            })
            .collect();

        // Collect results as they complete
        let mut results = Vec::with_capacity(task_count);
        let mut completed = 0usize;

        while let Some((id, result, duration_ms)) = futures.next().await {
            completed += 1;
            let (ok, content) = match &result {
                Ok(text) => (true, text.clone()),
                Err(err) => (false, err.clone()),
            };

            tracing::info!(
                "subtask '{id}' completed ({completed}/{task_count}): ok={ok}, len={}",
                content.len()
            );

            // Emit subtask_end event
            let summary = if content.len() > 200 {
                format!("{}...", &content[..200])
            } else {
                content.clone()
            };
            self.emit_event(&serde_json::json!({
                "type": "subtask_end",
                "id": id,
                "ok": ok,
                "result": summary,
                "duration_ms": duration_ms,
                "completed": completed,
                "total": task_count,
            }));

            results.push(serde_json::json!({
                "task_id": id,
                "success": ok,
                "result": content,
            }));
        }

        let all_ok = results.iter().all(|r| r["success"].as_bool().unwrap_or(false));
        Ok(serde_json::json!({
            "completed": completed,
            "total": task_count,
            "all_succeeded": all_ok,
            "results": results,
        }))
    }
}

/// Resolve the LLM connector to use for sub-tasks.
/// If the agent has a dedicated subtask connector configured, load that;
/// otherwise fall back to the parent connector.
async fn resolve_subtask_connector(
    state: &AppState,
    agent_cfg: &AgentConfig,
    parent_connector: &LlmConnector,
) -> LlmConnector {
    if let Some(subtask_id) = agent_cfg.subtask_llm_connector_id {
        if let Some(connector) = crate::routes::conversations::load_llm_connector(state, subtask_id).await {
            return connector;
        }
        tracing::warn!(
            "subtask_llm_connector_id {} not found, falling back to parent connector",
            subtask_id
        );
    }
    parent_connector.clone()
}

/// Execute a single sub-task: build a focused prompt, run LLM completion with tools.
async fn run_subtask(
    state: &AppState,
    agent_cfg: &AgentConfig,
    connector: &LlmConnector,
    conversation_id: Uuid,
    user_id: Uuid,
    search_enabled: bool,
    timeout_secs: u64,
    task: &SubtaskDef,
) -> Result<String, String> {
    // Build the tool registry WITHOUT the delegation tool (prevents recursion)
    let registry = crate::routes::conversations::build_tool_registry(
        state,
        agent_cfg,
        conversation_id,
        user_id,
        search_enabled,
    )
    .await;

    // Build the sub-task user message
    let mut user_message = format!(
        "You are a focused sub-agent working on a specific task. \
         Complete the following task thoroughly and return your findings concisely.\n\n\
         ## Task\n{}\n",
        task.description
    );
    if let Some(ctx) = &task.context {
        user_message.push_str(&format!("\n## Additional Context\n{ctx}\n"));
    }
    user_message.push_str(
        "\n## Instructions\n\
         - Focus exclusively on the task described above.\n\
         - Use available tools as needed to complete the task.\n\
         - Return a clear, concise summary of your findings or results.\n\
         - Do not ask questions — make reasonable assumptions and proceed.",
    );

    let history = vec![(
        clawkson_core::MessageRole::User,
        user_message,
        vec![], // no images
    )];

    if !registry.definitions().is_empty() {
        crate::llm::complete_with_tools(
            connector,
            agent_cfg.system_prompt.as_deref(),
            &history,
            agent_cfg.temperature,
            agent_cfg.max_tokens,
            &registry,
            MAX_SUBTASK_ROUNDS,
            None, // no extended reasoning for sub-tasks
            timeout_secs,
        )
        .await
        .map_err(|e| format!("Sub-task LLM error: {e}"))
    } else {
        crate::llm::complete(
            connector,
            agent_cfg.system_prompt.as_deref(),
            &history,
            agent_cfg.temperature,
            agent_cfg.max_tokens,
            None,
            timeout_secs,
        )
        .await
        .map_err(|e| format!("Sub-task LLM error: {e}"))
    }
}
