use axum::{
    extract::{Path, State},
    http::StatusCode,
    routing::get,
    Json, Router,
};
use clawkson_core::{ScheduledTask, TaskExecution, TaskOutputFile};
use serde::Deserialize;
use uuid::Uuid;

use crate::auth::AuthUser;
use crate::scheduler::compute_next_run;
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", get(list_tasks).post(create_task))
        .route("/{id}", get(get_task).patch(update_task).delete(delete_task))
        .route("/{id}/run", axum::routing::post(run_task))
        .route("/{id}/history", get(list_history))
}

// ── Request types ────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct CreateTaskRequest {
    pub name: String,
    pub agent_id: Uuid,
    pub prompt: String,
    pub cron_expression: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct PatchTaskRequest {
    pub name: Option<String>,
    pub prompt: Option<String>,
    pub cron_expression: Option<Option<String>>,
    pub enabled: Option<bool>,
}

// ── Mapping helpers ──────────────────────────────────────────────────

fn row_to_api(row: clawkson_db::scheduled_task::ScheduledTaskRow) -> ScheduledTask {
    ScheduledTask {
        id: row.id,
        owner_id: row.owner_id,
        agent_id: row.agent_id,
        name: row.name,
        prompt: row.prompt,
        cron_expression: row.cron_expression,
        enabled: row.enabled,
        last_run_at: row.last_run_at,
        next_run_at: row.next_run_at,
        created_at: row.created_at,
        updated_at: row.updated_at,
    }
}

fn exec_to_api(row: clawkson_db::scheduled_task::TaskExecutionRow) -> TaskExecution {
    TaskExecution {
        id: row.id,
        task_id: row.task_id,
        conversation_id: row.conversation_id,
        status: row.status,
        result_summary: row.result_summary,
        error_message: row.error_message,
        started_at: row.started_at,
        completed_at: row.completed_at,
        duration_ms: row.duration_ms,
        output_files: vec![],
    }
}

fn exec_to_api_with_files(
    row: clawkson_db::scheduled_task::TaskExecutionRow,
    files: Vec<TaskOutputFile>,
) -> TaskExecution {
    TaskExecution {
        id: row.id,
        task_id: row.task_id,
        conversation_id: row.conversation_id,
        status: row.status,
        result_summary: row.result_summary,
        error_message: row.error_message,
        started_at: row.started_at,
        completed_at: row.completed_at,
        duration_ms: row.duration_ms,
        output_files: files,
    }
}

// ── Handlers ─────────────────────────────────────────────────────────

async fn list_tasks(
    auth: AuthUser,
    State(state): State<AppState>,
) -> Result<Json<Vec<ScheduledTask>>, StatusCode> {
    let rows = clawkson_db::scheduled_task::list_for_user(&state.db, auth.id())
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(rows.into_iter().map(row_to_api).collect()))
}

async fn create_task(
    auth: AuthUser,
    State(state): State<AppState>,
    Json(req): Json<CreateTaskRequest>,
) -> Result<Json<ScheduledTask>, StatusCode> {
    // Validate cron expression if provided
    let next_run = if let Some(ref expr) = req.cron_expression {
        let next = compute_next_run(Some(expr));
        if next.is_none() {
            return Err(StatusCode::BAD_REQUEST);
        }
        next
    } else {
        None
    };

    let row = clawkson_db::scheduled_task::create(
        &state.db,
        auth.id(),
        req.agent_id,
        &req.name,
        &req.prompt,
        req.cron_expression.as_deref(),
        next_run,
    )
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(row_to_api(row)))
}

async fn get_task(
    auth: AuthUser,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<ScheduledTask>, StatusCode> {
    let row = clawkson_db::scheduled_task::get_by_id(&state.db, id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;

    if row.owner_id != auth.id() && !auth.is_admin() {
        return Err(StatusCode::FORBIDDEN);
    }

    Ok(Json(row_to_api(row)))
}

async fn update_task(
    auth: AuthUser,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(req): Json<PatchTaskRequest>,
) -> Result<Json<ScheduledTask>, StatusCode> {
    // Ownership check
    let existing = clawkson_db::scheduled_task::get_by_id(&state.db, id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;

    if existing.owner_id != auth.id() && !auth.is_admin() {
        return Err(StatusCode::FORBIDDEN);
    }

    // Recompute next_run if cron changed or task is being enabled/disabled
    let new_cron = req
        .cron_expression
        .as_ref()
        .map(|opt| opt.as_deref())
        .unwrap_or(None);
    let cron_changed = req.cron_expression.is_some();
    let enabled_changed = req.enabled.is_some();

    let next_run_update = if cron_changed || enabled_changed {
        let effective_cron = if cron_changed {
            new_cron
        } else {
            existing.cron_expression.as_deref()
        };
        let effective_enabled = req.enabled.unwrap_or(existing.enabled);

        if effective_enabled {
            // Validate and compute
            if let Some(expr) = effective_cron {
                let next = compute_next_run(Some(expr));
                if next.is_none() && cron_changed {
                    return Err(StatusCode::BAD_REQUEST);
                }
                Some(next)
            } else {
                Some(None)
            }
        } else {
            Some(None)
        }
    } else {
        None
    };

    let row = clawkson_db::scheduled_task::update(
        &state.db,
        id,
        req.name.as_deref(),
        req.prompt.as_deref(),
        req.cron_expression
            .as_ref()
            .map(|opt| opt.as_deref()),
        req.enabled,
        next_run_update,
    )
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
    .ok_or(StatusCode::NOT_FOUND)?;

    Ok(Json(row_to_api(row)))
}

async fn delete_task(
    auth: AuthUser,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, StatusCode> {
    let existing = clawkson_db::scheduled_task::get_by_id(&state.db, id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;

    if existing.owner_id != auth.id() && !auth.is_admin() {
        return Err(StatusCode::FORBIDDEN);
    }

    clawkson_db::scheduled_task::delete(&state.db, id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(StatusCode::NO_CONTENT)
}

/// Trigger an immediate manual run. Returns 202 + the execution record.
async fn run_task(
    auth: AuthUser,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<(StatusCode, Json<TaskExecution>), StatusCode> {
    let task = clawkson_db::scheduled_task::get_by_id(&state.db, id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;

    if task.owner_id != auth.id() && !auth.is_admin() {
        return Err(StatusCode::FORBIDDEN);
    }

    // Create execution record upfront so we can return it immediately
    let exec = clawkson_db::scheduled_task::create_execution(&state.db, task.id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let exec_api = exec_to_api(exec.clone());

    // Spawn the actual execution in the background, reusing the shared execute_task
    let state_clone = state.clone();
    let exec_id = exec.id;
    tokio::spawn(async move {
        if let Err(e) = crate::scheduler::execute_task(&state_clone, &task, Some(exec_id)).await {
            tracing::error!(task_id = %task.id, "manual task execution failed: {e}");
        }
    });

    Ok((StatusCode::ACCEPTED, Json(exec_api)))
}

async fn list_history(
    auth: AuthUser,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<Vec<TaskExecution>>, StatusCode> {
    let task = clawkson_db::scheduled_task::get_by_id(&state.db, id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;

    if task.owner_id != auth.id() && !auth.is_admin() {
        return Err(StatusCode::FORBIDDEN);
    }

    let rows = clawkson_db::scheduled_task::list_executions(&state.db, id, 50)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let pool = state.db.pool();
    let mut results = Vec::with_capacity(rows.len());
    for row in rows {
        let files = if let Some(conv_id) = row.conversation_id {
            clawkson_db::chat_attachment::list_for_conversation(pool, conv_id)
                .await
                .unwrap_or_default()
                .into_iter()
                .map(|a| TaskOutputFile {
                    id: a.id,
                    filename: a.filename,
                    content_type: a.content_type,
                    size_bytes: a.size_bytes,
                })
                .collect()
        } else {
            vec![]
        };
        results.push(exec_to_api_with_files(row, files));
    }

    Ok(Json(results))
}
