use chrono::{DateTime, Utc};
use sqlx::FromRow;
use uuid::Uuid;

use crate::{Db, DbError};

// ── Row types ───────────────────────────────────────────────────────

#[derive(Debug, Clone, FromRow, serde::Serialize, serde::Deserialize)]
pub struct ScheduledTaskRow {
    pub id: Uuid,
    pub owner_id: Uuid,
    pub agent_id: Uuid,
    pub name: String,
    pub prompt: String,
    pub cron_expression: Option<String>,
    pub enabled: bool,
    pub last_run_at: Option<DateTime<Utc>>,
    pub next_run_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub created_by_agent_id: Option<Uuid>,
    pub created_by_conversation_id: Option<Uuid>,
}

#[derive(Debug, Clone, FromRow, serde::Serialize, serde::Deserialize)]
pub struct TaskExecutionRow {
    pub id: Uuid,
    pub task_id: Uuid,
    pub conversation_id: Option<Uuid>,
    pub status: String,
    pub result_summary: Option<String>,
    pub error_message: Option<String>,
    pub started_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
    pub duration_ms: Option<i64>,
}

// ── Task CRUD ───────────────────────────────────────────────────────

pub async fn create(
    db: &Db,
    owner_id: Uuid,
    agent_id: Uuid,
    name: &str,
    prompt: &str,
    cron_expression: Option<&str>,
    next_run_at: Option<DateTime<Utc>>,
) -> Result<ScheduledTaskRow, DbError> {
    create_with_provenance(db, owner_id, agent_id, name, prompt, cron_expression, next_run_at, None, None).await
}

pub async fn create_with_provenance(
    db: &Db,
    owner_id: Uuid,
    agent_id: Uuid,
    name: &str,
    prompt: &str,
    cron_expression: Option<&str>,
    next_run_at: Option<DateTime<Utc>>,
    created_by_agent_id: Option<Uuid>,
    created_by_conversation_id: Option<Uuid>,
) -> Result<ScheduledTaskRow, DbError> {
    let row = sqlx::query_as::<_, ScheduledTaskRow>(
        "INSERT INTO scheduled_tasks (owner_id, agent_id, name, prompt, cron_expression, next_run_at, created_by_agent_id, created_by_conversation_id)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
         RETURNING *",
    )
    .bind(owner_id)
    .bind(agent_id)
    .bind(name)
    .bind(prompt)
    .bind(cron_expression)
    .bind(next_run_at)
    .bind(created_by_agent_id)
    .bind(created_by_conversation_id)
    .fetch_one(db.pool())
    .await?;

    Ok(row)
}

pub async fn get_by_id(db: &Db, id: Uuid) -> Result<Option<ScheduledTaskRow>, DbError> {
    let row = sqlx::query_as::<_, ScheduledTaskRow>(
        "SELECT * FROM scheduled_tasks WHERE id = $1",
    )
    .bind(id)
    .fetch_optional(db.pool())
    .await?;

    Ok(row)
}

pub async fn list_for_user(db: &Db, owner_id: Uuid) -> Result<Vec<ScheduledTaskRow>, DbError> {
    let rows = sqlx::query_as::<_, ScheduledTaskRow>(
        "SELECT * FROM scheduled_tasks WHERE owner_id = $1 ORDER BY created_at DESC",
    )
    .bind(owner_id)
    .fetch_all(db.pool())
    .await?;

    Ok(rows)
}

pub async fn update(
    db: &Db,
    id: Uuid,
    name: Option<&str>,
    prompt: Option<&str>,
    cron_expression: Option<Option<&str>>,
    enabled: Option<bool>,
    next_run_at: Option<Option<DateTime<Utc>>>,
) -> Result<Option<ScheduledTaskRow>, DbError> {
    let Some(mut task) = get_by_id(db, id).await? else {
        return Ok(None);
    };

    if let Some(v) = name {
        task.name = v.to_string();
    }
    if let Some(v) = prompt {
        task.prompt = v.to_string();
    }
    if let Some(v) = cron_expression {
        task.cron_expression = v.map(|s| s.to_string());
    }
    if let Some(v) = enabled {
        task.enabled = v;
    }
    if let Some(v) = next_run_at {
        task.next_run_at = v;
    }

    let row = sqlx::query_as::<_, ScheduledTaskRow>(
        "UPDATE scheduled_tasks
         SET name = $2, prompt = $3, cron_expression = $4, enabled = $5,
             next_run_at = $6, updated_at = now()
         WHERE id = $1
         RETURNING *",
    )
    .bind(id)
    .bind(&task.name)
    .bind(&task.prompt)
    .bind(&task.cron_expression)
    .bind(task.enabled)
    .bind(task.next_run_at)
    .fetch_optional(db.pool())
    .await?;

    Ok(row)
}

pub async fn delete(db: &Db, id: Uuid) -> Result<bool, DbError> {
    let result = sqlx::query("DELETE FROM scheduled_tasks WHERE id = $1")
        .bind(id)
        .execute(db.pool())
        .await?;

    Ok(result.rows_affected() > 0)
}

/// Return all tasks whose next_run_at has passed (due for execution).
pub async fn list_due(db: &Db) -> Result<Vec<ScheduledTaskRow>, DbError> {
    let rows = sqlx::query_as::<_, ScheduledTaskRow>(
        "SELECT * FROM scheduled_tasks
         WHERE enabled AND next_run_at IS NOT NULL AND next_run_at <= now()",
    )
    .fetch_all(db.pool())
    .await?;

    Ok(rows)
}

/// Update schedule after an execution: record last_run_at and set next_run_at.
pub async fn update_schedule(
    db: &Db,
    id: Uuid,
    last_run_at: DateTime<Utc>,
    next_run_at: Option<DateTime<Utc>>,
) -> Result<(), DbError> {
    sqlx::query(
        "UPDATE scheduled_tasks
         SET last_run_at = $2, next_run_at = $3, updated_at = now()
         WHERE id = $1",
    )
    .bind(id)
    .bind(last_run_at)
    .bind(next_run_at)
    .execute(db.pool())
    .await?;

    Ok(())
}

// ── Execution CRUD ──────────────────────────────────────────────────

pub async fn create_execution(db: &Db, task_id: Uuid) -> Result<TaskExecutionRow, DbError> {
    let row = sqlx::query_as::<_, TaskExecutionRow>(
        "INSERT INTO task_executions (task_id) VALUES ($1) RETURNING *",
    )
    .bind(task_id)
    .fetch_one(db.pool())
    .await?;

    Ok(row)
}

pub async fn set_execution_conversation(
    db: &Db,
    execution_id: Uuid,
    conversation_id: Uuid,
) -> Result<(), DbError> {
    sqlx::query(
        "UPDATE task_executions SET conversation_id = $2 WHERE id = $1",
    )
    .bind(execution_id)
    .bind(conversation_id)
    .execute(db.pool())
    .await?;

    Ok(())
}

pub async fn complete_execution(
    db: &Db,
    execution_id: Uuid,
    status: &str,
    result_summary: Option<&str>,
    error_message: Option<&str>,
    duration_ms: i64,
) -> Result<(), DbError> {
    sqlx::query(
        "UPDATE task_executions
         SET status = $2, result_summary = $3, error_message = $4,
             duration_ms = $5, completed_at = now()
         WHERE id = $1",
    )
    .bind(execution_id)
    .bind(status)
    .bind(result_summary)
    .bind(error_message)
    .bind(duration_ms)
    .execute(db.pool())
    .await?;

    Ok(())
}

pub async fn list_executions(
    db: &Db,
    task_id: Uuid,
    limit: i64,
) -> Result<Vec<TaskExecutionRow>, DbError> {
    let rows = sqlx::query_as::<_, TaskExecutionRow>(
        "SELECT * FROM task_executions
         WHERE task_id = $1
         ORDER BY started_at DESC
         LIMIT $2",
    )
    .bind(task_id)
    .bind(limit)
    .fetch_all(db.pool())
    .await?;

    Ok(rows)
}

/// Mark orphaned running executions as errored (for server restart cleanup).
pub async fn cleanup_orphaned_executions(db: &Db) -> Result<u64, DbError> {
    let result = sqlx::query(
        "UPDATE task_executions
         SET status = 'error', error_message = 'Server restarted', completed_at = now()
         WHERE status = 'running'",
    )
    .execute(db.pool())
    .await?;

    Ok(result.rows_affected())
}
