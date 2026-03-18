use chrono::{DateTime, Utc};
use sqlx::FromRow;
use uuid::Uuid;

use crate::{Db, DbError};

/// Enriched audit row that joins agent name for display purposes.
#[derive(Debug, Clone, FromRow, serde::Serialize, serde::Deserialize)]
pub struct ToolAuditEnrichedRow {
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
    pub agent_name: String,
    pub conversation_title: Option<String>,
}

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

/// List enriched audit entries for a user, with optional filters.
/// Joins agent name and conversation title for display.
pub async fn list_for_user(
    db: &Db,
    user_id: Uuid,
    agent_id: Option<Uuid>,
    tool_name: Option<&str>,
    decision: Option<&str>,
    since: Option<DateTime<Utc>>,
    limit: i64,
    offset: i64,
) -> Result<Vec<ToolAuditEnrichedRow>, DbError> {
    let rows = sqlx::query_as::<_, ToolAuditEnrichedRow>(
        "SELECT t.id, t.conversation_id, t.agent_id, t.user_id,
                t.tool_name, t.http_method, t.target_path, t.connector_id,
                t.decision, t.denial_reason, t.duration_ms, t.created_at,
                a.name AS agent_name,
                c.title AS conversation_title
         FROM tool_audit_log t
         JOIN agents a ON a.id = t.agent_id
         LEFT JOIN conversations c ON c.id = t.conversation_id
         WHERE t.user_id = $1
           AND ($2::UUID IS NULL OR t.agent_id = $2)
           AND ($3::TEXT IS NULL OR t.tool_name = $3)
           AND ($4::TEXT IS NULL OR t.decision = $4)
           AND ($5::TIMESTAMPTZ IS NULL OR t.created_at >= $5)
         ORDER BY t.created_at DESC
         LIMIT $6 OFFSET $7",
    )
    .bind(user_id)
    .bind(agent_id)
    .bind(tool_name)
    .bind(decision)
    .bind(since)
    .bind(limit)
    .bind(offset)
    .fetch_all(db.pool())
    .await?;

    Ok(rows)
}

/// Aggregate stats for a user: totals by decision and by tool name.
#[derive(Debug, Clone, FromRow, serde::Serialize)]
pub struct ToolAuditBreakdown {
    pub key: String,
    pub count: i64,
}

pub async fn stats_for_user(
    db: &Db,
    user_id: Uuid,
    since: Option<DateTime<Utc>>,
) -> Result<UserAuditStats, DbError> {
    // Total + by-decision counts
    let decision_rows = sqlx::query_as::<_, AuditStats>(
        "SELECT decision, COUNT(*) as count
         FROM tool_audit_log
         WHERE user_id = $1
           AND ($2::TIMESTAMPTZ IS NULL OR created_at >= $2)
         GROUP BY decision",
    )
    .bind(user_id)
    .bind(since)
    .fetch_all(db.pool())
    .await?;

    let mut allowed: i64 = 0;
    let mut denied: i64 = 0;
    for row in &decision_rows {
        match row.decision.as_str() {
            "allowed" => allowed = row.count,
            "denied" => denied = row.count,
            _ => {}
        }
    }

    // By tool name (top 10)
    let by_tool = sqlx::query_as::<_, ToolAuditBreakdown>(
        "SELECT tool_name AS key, COUNT(*) AS count
         FROM tool_audit_log
         WHERE user_id = $1
           AND ($2::TIMESTAMPTZ IS NULL OR created_at >= $2)
         GROUP BY tool_name
         ORDER BY count DESC
         LIMIT 10",
    )
    .bind(user_id)
    .bind(since)
    .fetch_all(db.pool())
    .await?;

    // By agent (top 10), join for name
    let by_agent = sqlx::query_as::<_, ToolAuditBreakdown>(
        "SELECT a.name AS key, COUNT(*) AS count
         FROM tool_audit_log t
         JOIN agents a ON a.id = t.agent_id
         WHERE t.user_id = $1
           AND ($2::TIMESTAMPTZ IS NULL OR t.created_at >= $2)
         GROUP BY a.name
         ORDER BY count DESC
         LIMIT 10",
    )
    .bind(user_id)
    .bind(since)
    .fetch_all(db.pool())
    .await?;

    Ok(UserAuditStats {
        total: allowed + denied,
        allowed,
        denied,
        by_tool,
        by_agent,
    })
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct UserAuditStats {
    pub total: i64,
    pub allowed: i64,
    pub denied: i64,
    pub by_tool: Vec<ToolAuditBreakdown>,
    pub by_agent: Vec<ToolAuditBreakdown>,
}

/// Delete audit entries older than the given cutoff.
/// Returns the number of rows deleted.
pub async fn cleanup_older_than(
    db: &Db,
    cutoff: DateTime<Utc>,
) -> Result<u64, DbError> {
    let result = sqlx::query("DELETE FROM tool_audit_log WHERE created_at < $1")
        .bind(cutoff)
        .execute(db.pool())
        .await?;

    Ok(result.rows_affected())
}
