use chrono::{DateTime, Utc};
use serde_json::Value as JsonValue;
use sqlx::FromRow;
use uuid::Uuid;

use crate::{Db, DbError};

#[derive(Debug, Clone, FromRow)]
pub struct WebhookRow {
    pub id: Uuid,
    pub owner_id: Uuid,
    pub agent_id: Uuid,
    pub name: String,
    pub description: String,
    pub secret: String,
    pub enabled: bool,
    pub payload_template: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, FromRow)]
pub struct WebhookExecutionRow {
    pub id: Uuid,
    pub webhook_id: Uuid,
    pub conversation_id: Option<Uuid>,
    pub status: String,
    pub result_summary: Option<String>,
    pub error_message: Option<String>,
    pub payload: Option<JsonValue>,
    pub started_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
    pub duration_ms: Option<i64>,
}

// ── Webhook CRUD ────────────────────────────────────────────────

pub async fn create(
    db: &Db,
    owner_id: Uuid,
    agent_id: Uuid,
    name: &str,
    description: &str,
    secret: &str,
    payload_template: Option<&str>,
) -> Result<WebhookRow, DbError> {
    let row = sqlx::query_as::<_, WebhookRow>(
        "INSERT INTO webhooks (owner_id, agent_id, name, description, secret, payload_template)
         VALUES ($1, $2, $3, $4, $5, $6)
         RETURNING *",
    )
    .bind(owner_id)
    .bind(agent_id)
    .bind(name)
    .bind(description)
    .bind(secret)
    .bind(payload_template)
    .fetch_one(db.pool())
    .await?;
    Ok(row)
}

pub async fn get_by_id(db: &Db, id: Uuid) -> Result<Option<WebhookRow>, DbError> {
    let row = sqlx::query_as::<_, WebhookRow>(
        "SELECT * FROM webhooks WHERE id = $1",
    )
    .bind(id)
    .fetch_optional(db.pool())
    .await?;
    Ok(row)
}

pub async fn list_for_user(db: &Db, owner_id: Uuid) -> Result<Vec<WebhookRow>, DbError> {
    let rows = sqlx::query_as::<_, WebhookRow>(
        "SELECT * FROM webhooks WHERE owner_id = $1 ORDER BY created_at DESC",
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
    description: Option<&str>,
    enabled: Option<bool>,
    payload_template: Option<Option<&str>>,
) -> Result<Option<WebhookRow>, DbError> {
    let existing = match get_by_id(db, id).await? {
        Some(r) => r,
        None => return Ok(None),
    };

    let name = name.unwrap_or(&existing.name);
    let description = description.unwrap_or(&existing.description);
    let enabled = enabled.unwrap_or(existing.enabled);
    let payload_template = match payload_template {
        Some(pt) => pt.map(|s| s.to_string()),
        None => existing.payload_template.clone(),
    };

    let row = sqlx::query_as::<_, WebhookRow>(
        "UPDATE webhooks
         SET name = $2, description = $3, enabled = $4, payload_template = $5, updated_at = now()
         WHERE id = $1
         RETURNING *",
    )
    .bind(id)
    .bind(name)
    .bind(description)
    .bind(enabled)
    .bind(payload_template)
    .fetch_optional(db.pool())
    .await?;
    Ok(row)
}

pub async fn delete(db: &Db, id: Uuid) -> Result<bool, DbError> {
    let result = sqlx::query("DELETE FROM webhooks WHERE id = $1")
        .bind(id)
        .execute(db.pool())
        .await?;
    Ok(result.rows_affected() > 0)
}

pub async fn set_enabled(db: &Db, id: Uuid, enabled: bool) -> Result<bool, DbError> {
    let result = sqlx::query(
        "UPDATE webhooks SET enabled = $2, updated_at = now() WHERE id = $1",
    )
    .bind(id)
    .bind(enabled)
    .execute(db.pool())
    .await?;
    Ok(result.rows_affected() > 0)
}

// ── Execution CRUD ──────────────────────────────────────────────

pub async fn create_execution(
    db: &Db,
    webhook_id: Uuid,
    payload: Option<&JsonValue>,
) -> Result<WebhookExecutionRow, DbError> {
    let row = sqlx::query_as::<_, WebhookExecutionRow>(
        "INSERT INTO webhook_executions (webhook_id, payload)
         VALUES ($1, $2)
         RETURNING *",
    )
    .bind(webhook_id)
    .bind(payload)
    .fetch_one(db.pool())
    .await?;
    Ok(row)
}

pub async fn set_execution_conversation(
    db: &Db,
    id: Uuid,
    conversation_id: Uuid,
) -> Result<(), DbError> {
    sqlx::query("UPDATE webhook_executions SET conversation_id = $2 WHERE id = $1")
        .bind(id)
        .bind(conversation_id)
        .execute(db.pool())
        .await?;
    Ok(())
}

pub async fn complete_execution(
    db: &Db,
    id: Uuid,
    status: &str,
    result_summary: Option<&str>,
    error_message: Option<&str>,
    duration_ms: i64,
) -> Result<(), DbError> {
    sqlx::query(
        "UPDATE webhook_executions
         SET status = $2, result_summary = $3, error_message = $4,
             duration_ms = $5, completed_at = now()
         WHERE id = $1",
    )
    .bind(id)
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
    webhook_id: Uuid,
    limit: i64,
) -> Result<Vec<WebhookExecutionRow>, DbError> {
    let rows = sqlx::query_as::<_, WebhookExecutionRow>(
        "SELECT * FROM webhook_executions
         WHERE webhook_id = $1
         ORDER BY started_at DESC
         LIMIT $2",
    )
    .bind(webhook_id)
    .bind(limit)
    .fetch_all(db.pool())
    .await?;
    Ok(rows)
}
