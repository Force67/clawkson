use std::str::FromStr;

use chrono::Utc;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use crate::routes::conversations::{
    attach_workspace_outputs, enrich_history, expand_skill_references, load_agent_config,
    load_history, load_llm_connector, resolve_connector_id, run_completion, AgentConfig,
};
use crate::state::AppState;

/// Manages the background scheduler loop.
#[derive(Clone)]
pub struct SchedulerManager {
    cancel: CancellationToken,
}

impl SchedulerManager {
    pub fn new() -> Self {
        Self {
            cancel: CancellationToken::new(),
        }
    }

    /// Spawn the background scheduler loop.
    pub fn start(&self, state: AppState) {
        let cancel = self.cancel.clone();
        tokio::spawn(scheduler_loop(state, cancel));
    }

    /// Signal the scheduler to stop.
    pub async fn shutdown(&self) {
        self.cancel.cancel();
    }
}

/// Main scheduler loop: wakes every 30 seconds and runs due tasks.
async fn scheduler_loop(state: AppState, cancel: CancellationToken) {
    // On boot: clean up orphaned running executions from previous server lifetime.
    match clawkson_db::scheduled_task::cleanup_orphaned_executions(&state.db).await {
        Ok(n) if n > 0 => tracing::info!(count = n, "cleaned up orphaned task executions"),
        Ok(_) => {}
        Err(e) => tracing::warn!("failed to clean up orphaned executions: {e}"),
    }

    let mut interval = tokio::time::interval(std::time::Duration::from_secs(30));

    loop {
        tokio::select! {
            _ = cancel.cancelled() => {
                tracing::info!("scheduler shutting down");
                break;
            }
            _ = interval.tick() => {
                if let Err(e) = check_and_run_due_tasks(&state).await {
                    tracing::error!("scheduler tick error: {e}");
                }
            }
        }
    }
}

/// Query for due tasks and spawn each as an independent tokio task.
async fn check_and_run_due_tasks(state: &AppState) -> anyhow::Result<()> {
    let due_tasks = clawkson_db::scheduled_task::list_due(&state.db).await?;

    for task in due_tasks {
        // Immediately advance next_run_at to prevent double-fire on the next tick.
        let next = compute_next_run(task.cron_expression.as_deref());
        let now = Utc::now();
        if let Err(e) =
            clawkson_db::scheduled_task::update_schedule(&state.db, task.id, now, next).await
        {
            tracing::error!(task_id = %task.id, "failed to advance next_run_at: {e}");
            continue;
        }

        let state = state.clone();
        tokio::spawn(async move {
            if let Err(e) = execute_task(&state, &task, None).await {
                tracing::error!(task_id = %task.id, task_name = %task.name, "task execution failed: {e}");
            }
        });
    }

    Ok(())
}

/// Execute a single scheduled task, mirroring the telegram.rs:handle_message pattern.
///
/// If `existing_exec_id` is Some, reuses that execution record (for manual runs where the
/// record was pre-created to return to the caller). Otherwise creates a new one.
pub(crate) async fn execute_task(
    state: &AppState,
    task: &clawkson_db::scheduled_task::ScheduledTaskRow,
    existing_exec_id: Option<Uuid>,
) -> anyhow::Result<()> {
    let start = std::time::Instant::now();

    // 1. Create or reuse execution record
    let exec_id = match existing_exec_id {
        Some(id) => id,
        None => {
            clawkson_db::scheduled_task::create_execution(&state.db, task.id)
                .await?
                .id
        }
    };

    // 2. Create a new conversation for this execution
    let title = format!("Task: {}", task.name);
    let conv = match clawkson_db::conversation::create(
        &state.db,
        Some(task.agent_id),
        Some(task.owner_id),
        &title,
    )
    .await
    {
        Ok(c) => c,
        Err(e) => {
            finish_execution(
                state,
                exec_id,
                start,
                "error",
                None,
                Some(&format!("Failed to create conversation: {e}")),
            )
            .await;
            return Err(e.into());
        }
    };

    // 3. Link execution to conversation
    let _ =
        clawkson_db::scheduled_task::set_execution_conversation(&state.db, exec_id, conv.id).await;

    // 4. Expand skill references and save user message
    let expanded_prompt = expand_skill_references(state, task.agent_id, &task.prompt).await;
    clawkson_db::message::create(
        &state.db,
        conv.id,
        None,
        clawkson_db::message::MessageRole::User,
        &expanded_prompt,
        None,
        None,
    )
    .await?;

    // 5. Resolve LLM connector
    let connector_id = resolve_connector_id(state, task.agent_id).await;
    let Some(connector_id) = connector_id else {
        finish_execution(
            state,
            exec_id,
            start,
            "error",
            None,
            Some("No LLM connector configured"),
        )
        .await;
        return Ok(());
    };
    let connector = load_llm_connector(state, connector_id).await;
    let Some(connector) = connector else {
        finish_execution(
            state,
            exec_id,
            start,
            "error",
            None,
            Some("LLM connector not found"),
        )
        .await;
        return Ok(());
    };

    // 6. Load agent config
    let agent_cfg = load_agent_config(state, task.agent_id).await;
    let default_cfg = AgentConfig {
        agent_id: task.agent_id,
        system_prompt: None,
        temperature: None,
        max_tokens: None,
        container_enabled: false,
        container_config: None,
        connector_policies: vec![],
        subtask_llm_connector_id: None,
    };
    let cfg = agent_cfg.as_ref().unwrap_or(&default_cfg);

    // 7. Load & enrich history
    let raw_history = load_history(state, conv.id)
        .await
        .map_err(|_| anyhow::anyhow!("failed to load history"))?;
    let supports_vision = crate::llm::provider_supports_vision(&connector);
    let agent_has_container = agent_cfg
        .as_ref()
        .map(|c| c.container_enabled)
        .unwrap_or(false);
    let history = enrich_history(state, raw_history, supports_vision, agent_has_container).await;

    // 8. Run completion
    let timeout_secs = clawkson_db::settings::get(&state.db)
        .await
        .map(|s| s.llm_request_timeout_secs as u64)
        .unwrap_or(120);

    let assistant_content = match run_completion(
        state,
        &connector,
        cfg,
        &history,
        None,
        conv.id,
        task.owner_id,
        true,
        timeout_secs,
    )
    .await
    {
        Ok(text) => text,
        Err(e) => {
            tracing::error!(task_id = %task.id, "LLM completion failed: {e}");
            finish_execution(
                state,
                exec_id,
                start,
                "error",
                None,
                Some(&format!("LLM error: {e}")),
            )
            .await;
            return Ok(());
        }
    };

    // 9. Save assistant message
    let assistant_msg = clawkson_db::message::create(
        &state.db,
        conv.id,
        None,
        clawkson_db::message::MessageRole::Assistant,
        &assistant_content,
        None,
        None,
    )
    .await?;
    let _ = clawkson_db::conversation::touch(&state.db, conv.id).await;

    // 10. Attach workspace output files (container agents only)
    if cfg.container_enabled {
        attach_workspace_outputs(state, task.agent_id, assistant_msg.id, task.owner_id, conv.id)
            .await;
    }

    // 12. Embed chat turn for memory (background, non-blocking)
    {
        let mem = state.memory.clone();
        let title = format!("Scheduled: {}", task.name);
        let user_content = task.prompt.clone();
        let asst_content = assistant_content.clone();
        let owner_id = task.owner_id;
        tokio::spawn(async move {
            mem.push_turn(conv.id, owner_id, title, user_content, asst_content)
                .await;
        });
    }

    // 13. Generate a brief summary (first 200 chars, UTF-8 safe)
    let summary = truncate_utf8(&assistant_content, 200);

    finish_execution(state, exec_id, start, "success", Some(&summary), None).await;

    tracing::info!(
        task_id = %task.id,
        task_name = %task.name,
        conv_id = %conv.id,
        duration_ms = %start.elapsed().as_millis(),
        "scheduled task completed"
    );

    Ok(())
}

/// Helper to finalize an execution record.
async fn finish_execution(
    state: &AppState,
    exec_id: Uuid,
    start: std::time::Instant,
    status: &str,
    summary: Option<&str>,
    error: Option<&str>,
) {
    let _ = clawkson_db::scheduled_task::complete_execution(
        &state.db,
        exec_id,
        status,
        summary,
        error,
        start.elapsed().as_millis() as i64,
    )
    .await;
}

/// Truncate a string to at most `max_chars` characters, appending "..." if truncated.
fn truncate_utf8(s: &str, max_chars: usize) -> String {
    let mut chars = s.chars();
    let truncated: String = chars.by_ref().take(max_chars).collect();
    if chars.next().is_some() {
        format!("{truncated}...")
    } else {
        truncated
    }
}

/// Compute the next run time from a cron expression.
/// Returns None if the expression is invalid or absent.
pub(crate) fn compute_next_run(cron_expression: Option<&str>) -> Option<chrono::DateTime<Utc>> {
    let expr = cron_expression?;
    let schedule = cron::Schedule::from_str(expr).ok()?;
    schedule.upcoming(Utc).next()
}
