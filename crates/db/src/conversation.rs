use chrono::{DateTime, Utc};
use sqlx::FromRow;
use uuid::Uuid;

use crate::{Db, DbError};

#[derive(Debug, Clone, FromRow, serde::Serialize, serde::Deserialize)]
pub struct Conversation {
    pub id: Uuid,
    pub agent_id: Option<Uuid>,
    pub title: String,
    pub summary: Option<String>,
    pub pinned: bool,
    pub archived: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Create a new conversation, optionally linked to an agent.
pub async fn create(
    db: &Db,
    agent_id: Option<Uuid>,
    title: &str,
) -> Result<Conversation, DbError> {
    let row = sqlx::query_as::<_, Conversation>(
        "INSERT INTO conversations (agent_id, title)
         VALUES ($1, $2)
         RETURNING *",
    )
    .bind(agent_id)
    .bind(title)
    .fetch_one(db.pool())
    .await?;

    Ok(row)
}

/// Fetch a conversation by id.
pub async fn get_by_id(db: &Db, id: Uuid) -> Result<Option<Conversation>, DbError> {
    let row = sqlx::query_as::<_, Conversation>(
        "SELECT * FROM conversations WHERE id = $1",
    )
    .bind(id)
    .fetch_optional(db.pool())
    .await?;

    Ok(row)
}

/// List recent non-archived conversations, newest first.
/// Optionally filter by agent_id.
pub async fn list_recent(
    db: &Db,
    agent_id: Option<Uuid>,
    limit: i64,
    offset: i64,
) -> Result<Vec<Conversation>, DbError> {
    let rows = if let Some(agent_id) = agent_id {
        sqlx::query_as::<_, Conversation>(
            "SELECT * FROM conversations
             WHERE agent_id = $1 AND archived = FALSE
             ORDER BY updated_at DESC
             LIMIT $2 OFFSET $3",
        )
        .bind(agent_id)
        .bind(limit)
        .bind(offset)
        .fetch_all(db.pool())
        .await?
    } else {
        sqlx::query_as::<_, Conversation>(
            "SELECT * FROM conversations
             WHERE archived = FALSE
             ORDER BY updated_at DESC
             LIMIT $1 OFFSET $2",
        )
        .bind(limit)
        .bind(offset)
        .fetch_all(db.pool())
        .await?
    };

    Ok(rows)
}

/// List pinned conversations for an agent (or all if agent_id is None).
pub async fn list_pinned(
    db: &Db,
    agent_id: Option<Uuid>,
) -> Result<Vec<Conversation>, DbError> {
    let rows = if let Some(agent_id) = agent_id {
        sqlx::query_as::<_, Conversation>(
            "SELECT * FROM conversations
             WHERE agent_id = $1 AND pinned = TRUE
             ORDER BY updated_at DESC",
        )
        .bind(agent_id)
        .fetch_all(db.pool())
        .await?
    } else {
        sqlx::query_as::<_, Conversation>(
            "SELECT * FROM conversations
             WHERE pinned = TRUE
             ORDER BY updated_at DESC",
        )
        .fetch_all(db.pool())
        .await?
    };

    Ok(rows)
}

/// Update the title of a conversation.
pub async fn update_title(
    db: &Db,
    id: Uuid,
    title: &str,
) -> Result<Option<Conversation>, DbError> {
    let row = sqlx::query_as::<_, Conversation>(
        "UPDATE conversations
         SET title = $2, updated_at = now()
         WHERE id = $1
         RETURNING *",
    )
    .bind(id)
    .bind(title)
    .fetch_optional(db.pool())
    .await?;

    Ok(row)
}

/// Update the auto-generated summary.
pub async fn update_summary(
    db: &Db,
    id: Uuid,
    summary: &str,
) -> Result<Option<Conversation>, DbError> {
    let row = sqlx::query_as::<_, Conversation>(
        "UPDATE conversations
         SET summary = $2, updated_at = now()
         WHERE id = $1
         RETURNING *",
    )
    .bind(id)
    .bind(summary)
    .fetch_optional(db.pool())
    .await?;

    Ok(row)
}

/// Pin or unpin a conversation.
pub async fn set_pinned(db: &Db, id: Uuid, pinned: bool) -> Result<bool, DbError> {
    let result = sqlx::query(
        "UPDATE conversations SET pinned = $2, updated_at = now() WHERE id = $1",
    )
    .bind(id)
    .bind(pinned)
    .execute(db.pool())
    .await?;

    Ok(result.rows_affected() > 0)
}

/// Archive or unarchive a conversation.
pub async fn set_archived(db: &Db, id: Uuid, archived: bool) -> Result<bool, DbError> {
    let result = sqlx::query(
        "UPDATE conversations SET archived = $2, updated_at = now() WHERE id = $1",
    )
    .bind(id)
    .bind(archived)
    .execute(db.pool())
    .await?;

    Ok(result.rows_affected() > 0)
}

/// Bump updated_at to now (called when a new message is added).
pub async fn touch(db: &Db, id: Uuid) -> Result<(), DbError> {
    sqlx::query("UPDATE conversations SET updated_at = now() WHERE id = $1")
        .bind(id)
        .execute(db.pool())
        .await?;

    Ok(())
}

/// Delete a conversation and all its messages (CASCADE).
pub async fn delete(db: &Db, id: Uuid) -> Result<bool, DbError> {
    let result = sqlx::query("DELETE FROM conversations WHERE id = $1")
        .bind(id)
        .execute(db.pool())
        .await?;

    Ok(result.rows_affected() > 0)
}
