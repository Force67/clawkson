use chrono::{DateTime, Utc};
use sqlx::FromRow;
use uuid::Uuid;

use crate::{Db, DbError};

/// Database row for the tool_audit_log table.
#[derive(Debug, Clone, FromRow, serde::Serialize, serde::Deserialize)]
pub struct ToolAuditRow {
    pub id: Uuid,
    pub conversation_id: Uuid,
    pub agent_id: Uuid,
    pub user_id: Uuid,
    pub tool_name: String,
    pub http_method: Option<String>,
    pub target_path: Option<String>,
    pub connector_id: Option<Uuid>,
    pub decision: String,
    pub denial_reason: Option<String>,
    pub duration_ms: Option<i64>,
    pub created_at: DateTime<Utc>,
}

/// Insert a new audit log entry.
pub async fn insert(
    db: &Db,
    conversation_id: Uuid,
    agent_id: Uuid,
    user_id: Uuid,
    tool_name: &str,
    http_method: Option<&str>,
    target_path: Option<&str>,
    connector_id: Option<Uuid>,
    decision: &str,
    denial_reason: Option<&str>,
    duration_ms: Option<i64>,
) -> Result<ToolAuditRow, DbError> {
    let row = sqlx::query_as::<_, ToolAuditRow>(
        "INSERT INTO tool_audit_log
            (conversation_id, agent_id, user_id, tool_name, http_method, target_path,
             connector_id, decision, denial_reason, duration_ms)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
         RETURNING *",
    )
    .bind(conversation_id)
    .bind(agent_id)
    .bind(user_id)
    .bind(tool_name)
    .bind(http_method)
    .bind(target_path)
    .bind(connector_id)
    .bind(decision)
    .bind(denial_reason)
    .bind(duration_ms)
    .fetch_one(db.pool())
    .await?;

    Ok(row)
}

/// List audit entries for a conversation, ordered by most recent first.
pub async fn list_by_conversation(
    db: &Db,
    conversation_id: Uuid,
    limit: i64,
    offset: i64,
) -> Result<Vec<ToolAuditRow>, DbError> {
    let rows = sqlx::query_as::<_, ToolAuditRow>(
        "SELECT * FROM tool_audit_log
         WHERE conversation_id = $1
         ORDER BY created_at DESC
         LIMIT $2 OFFSET $3",
    )
    .bind(conversation_id)
    .bind(limit)
    .bind(offset)
    .fetch_all(db.pool())
    .await?;

    Ok(rows)
}

/// List audit entries for an agent, ordered by most recent first.
pub async fn list_by_agent(
    db: &Db,
    agent_id: Uuid,
    limit: i64,
    offset: i64,
) -> Result<Vec<ToolAuditRow>, DbError> {
    let rows = sqlx::query_as::<_, ToolAuditRow>(
        "SELECT * FROM tool_audit_log
         WHERE agent_id = $1
         ORDER BY created_at DESC
         LIMIT $2 OFFSET $3",
    )
    .bind(agent_id)
    .bind(limit)
    .bind(offset)
    .fetch_all(db.pool())
    .await?;

    Ok(rows)
}

/// List only denied entries for a user (useful for the dashboard / security review).
pub async fn list_denied_for_user(
    db: &Db,
    user_id: Uuid,
    limit: i64,
) -> Result<Vec<ToolAuditRow>, DbError> {
    let rows = sqlx::query_as::<_, ToolAuditRow>(
        "SELECT * FROM tool_audit_log
         WHERE user_id = $1 AND decision = 'denied'
         ORDER BY created_at DESC
         LIMIT $2",
    )
    .bind(user_id)
    .bind(limit)
    .fetch_all(db.pool())
    .await?;

    Ok(rows)
}

/// Count entries grouped by decision for a conversation (for quick stats).
#[derive(Debug, Clone, FromRow, serde::Serialize)]
pub struct AuditStats {
    pub decision: String,
    pub count: i64,
}

pub async fn stats_by_conversation(
    db: &Db,
    conversation_id: Uuid,
) -> Result<Vec<AuditStats>, DbError> {
    let rows = sqlx::query_as::<_, AuditStats>(
        "SELECT decision, COUNT(*) as count
         FROM tool_audit_log
         WHERE conversation_id = $1
         GROUP BY decision",
    )
    .bind(conversation_id)
    .fetch_all(db.pool())
    .await?;

    Ok(rows)
}
