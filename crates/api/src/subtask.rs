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
const MAX_SUBTASK_ROUNDS: usize = 8;
/// Maximum characters returned per sub-task result to the parent context.
/// Results longer than this are truncated with a note. Keeps parent context lean.
const MAX_RESULT_CHARS: usize = 3000;

/// Lean system prompt for sub-agents. Much shorter than the full SOUL.md
/// to save tokens in the sub-agent context. Focused on execution, not orchestration.
const SUBTASK_SYSTEM_PROMPT: &str = "\
You are a focused worker agent executing a specific task. You have tools available: \
code execution (Python/Bash in a Docker sandbox), browser automation, HTTP requests, \
knowledge base search, and web search.\n\n\
Rules:\n\
- Act immediately. Never ask questions — make reasonable assumptions and proceed.\n\
- Use tools proactively. Install packages without asking (pip install, apt-get).\n\
- If a tool call fails, try an alternative approach.\n\
- Be CONCISE. Return only the key findings, data, and conclusions.\n\
- Prefer structured output: bullet points, numbered lists, or short tables.\n\
- Do NOT include lengthy preambles, caveats, or meta-commentary.\n\
- Do NOT repeat the task description back. Jump straight to results.\n\
- Aim for under 500 words unless the task explicitly requires more detail.\n\
- Include specific data, numbers, URLs, and sources — not vague summaries.";

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
                "YOUR PRIMARY TOOL. Spawn parallel sub-agents to do the heavy lifting. \
                 Each sub-agent runs independently with its own context window and full \
                 tool access (code execution, browser, HTTP, knowledge search, web search). \
                 \
                 DEFAULT TO THIS for any work involving: fetching data, browsing websites, \
                 running code, researching topics, analyzing documents, or any task \
                 requiring 2+ tool calls. This keeps YOUR context lean and fast. \
                 \
                 Sub-agents return concise results that you synthesize into a final answer. \
                 \
                 Each sub-agent has NO conversation memory — include ALL context, URLs, \
                 data, and expected output format in each task description. Tell each \
                 sub-agent to keep output concise (bullet points, key facts, data). \
                 \
                 Only skip delegation for: simple knowledge answers, single tool \
                 calls, conversational responses, or tasks that don't split into \
                 independent parts. NEVER delegate a single task to a single sub-agent — \
                 that adds overhead with no benefit. Minimum 2 sub-tasks per call. \
                 Max 5 parallel sub-tasks.",
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
                                "description": "Self-contained instructions: what to do, where to look (URLs, APIs), \
                                    and what format to return results in. The sub-agent has NO conversation memory, \
                                    so include everything it needs."
                            },
                            "context": {
                                "type": "string",
                                "description": "Additional data the sub-agent needs: user requirements, specs, \
                                    reference data, or prior results from other steps."
                            }
                        },
                        "required": ["id", "description"]
                    },
                    "minItems": 2,
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

        // Reject single-task delegation — it's wasteful overhead
        if args.tasks.len() == 1 {
            return Ok(serde_json::json!({
                "error": "Delegation requires at least 2 parallel sub-tasks. \
                          For a single task, use your tools directly instead of delegating. \
                          Only delegate when work can be parallelized.",
            }));
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

            // Truncate result to keep parent context lean
            let truncated = if content.len() > MAX_RESULT_CHARS {
                let cut = &content[..MAX_RESULT_CHARS];
                // Try to cut at a word boundary
                let cut = match cut.rfind('\n') {
                    Some(pos) if pos > MAX_RESULT_CHARS / 2 => &cut[..pos],
                    _ => match cut.rfind(' ') {
                        Some(pos) if pos > MAX_RESULT_CHARS / 2 => &cut[..pos],
                        _ => cut,
                    },
                };
                format!("{cut}\n\n[... truncated from {} chars — key findings above]", content.len())
            } else {
                content.clone()
            };

            results.push(serde_json::json!({
                "task_id": id,
                "success": ok,
                "result": truncated,
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

    // Build the sub-task user message — lean and focused, no fluff
    let mut user_message = format!("## Task\n{}\n", task.description);
    if let Some(ctx) = &task.context {
        user_message.push_str(&format!("\n## Context\n{ctx}\n"));
    }

    let history = vec![(
        clawkson_core::MessageRole::User,
        user_message,
        vec![], // no images
    )];

    // Use lean sub-agent system prompt instead of the full parent system prompt.
    // This saves tokens and prevents the sub-agent from trying to orchestrate/delegate.
    let system_prompt = SUBTASK_SYSTEM_PROMPT;

    if !registry.definitions().is_empty() {
        crate::llm::complete_with_tools(
            connector,
            Some(system_prompt),
            &history,
            agent_cfg.subtask_temperature.or(agent_cfg.temperature),
            agent_cfg.subtask_max_tokens.or(agent_cfg.max_tokens),
            &registry,
            MAX_SUBTASK_ROUNDS,
            None, // no extended reasoning for sub-tasks
            timeout_secs,
        )
        .await
        .map(|cr| cr.text)
        .map_err(|e| format!("Sub-task LLM error: {e}"))
    } else {
        crate::llm::complete(
            connector,
            Some(system_prompt),
            &history,
            agent_cfg.subtask_temperature.or(agent_cfg.temperature),
            agent_cfg.subtask_max_tokens.or(agent_cfg.max_tokens),
            None,
            timeout_secs,
        )
        .await
        .map(|cr| cr.text)
        .map_err(|e| format!("Sub-task LLM error: {e}"))
    }
}
