use chrono::{DateTime, Utc};
use serde_json::Value as JsonValue;
use sqlx::FromRow;
use uuid::Uuid;

use crate::{Db, DbError};

/// The role of a message author.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize, sqlx::Type)]
#[sqlx(type_name = "message_role", rename_all = "snake_case")]
#[serde(rename_all = "snake_case")]
pub enum MessageRole {
    User,
    Assistant,
    System,
    Tool,
}

/// Lifecycle status of a message.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize, sqlx::Type)]
#[sqlx(type_name = "message_status", rename_all = "snake_case")]
#[serde(rename_all = "snake_case")]
pub enum MessageStatus {
    Sending,
    Sent,
    Streaming,
    Failed,
    Cancelled,
}

/// A single chat message.
#[derive(Debug, Clone, FromRow, serde::Serialize, serde::Deserialize)]
pub struct Message {
    pub id: Uuid,
    pub conversation_id: Uuid,
    pub parent_id: Option<Uuid>,
    pub role: MessageRole,
    pub content: String,
    pub model: Option<String>,
    pub metadata: Option<JsonValue>,
    pub token_count: Option<i32>,
    pub status: MessageStatus,
    pub edited: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Insert a new message.
pub async fn create(
    db: &Db,
    conversation_id: Uuid,
    parent_id: Option<Uuid>,
    role: MessageRole,
    content: &str,
    model: Option<&str>,
    metadata: Option<JsonValue>,
) -> Result<Message, DbError> {
    let row = sqlx::query_as::<_, Message>(
        "INSERT INTO messages (conversation_id, parent_id, role, content, model, metadata)
         VALUES ($1, $2, $3, $4, $5, $6)
         RETURNING *",
    )
    .bind(conversation_id)
    .bind(parent_id)
    .bind(role)
    .bind(content)
    .bind(model)
    .bind(metadata)
    .fetch_one(db.pool())
    .await?;

    Ok(row)
}

/// Fetch a single message by id.
pub async fn get_by_id(db: &Db, id: Uuid) -> Result<Option<Message>, DbError> {
    let row = sqlx::query_as::<_, Message>(
        "SELECT * FROM messages WHERE id = $1",
    )
    .bind(id)
    .fetch_optional(db.pool())
    .await?;

    Ok(row)
}

/// All messages in a conversation, ordered chronologically.
pub async fn list_for_conversation(
    db: &Db,
    conversation_id: Uuid,
) -> Result<Vec<Message>, DbError> {
    let rows = sqlx::query_as::<_, Message>(
        "SELECT * FROM messages
         WHERE conversation_id = $1
         ORDER BY created_at ASC",
    )
    .bind(conversation_id)
    .fetch_all(db.pool())
    .await?;

    Ok(rows)
}

/// Top-level messages only (no thread replies).
pub async fn list_root_messages(
    db: &Db,
    conversation_id: Uuid,
) -> Result<Vec<Message>, DbError> {
    let rows = sqlx::query_as::<_, Message>(
        "SELECT * FROM messages
         WHERE conversation_id = $1 AND parent_id IS NULL
         ORDER BY created_at ASC",
    )
    .bind(conversation_id)
    .fetch_all(db.pool())
    .await?;

    Ok(rows)
}

/// Replies within a thread (children of a parent message).
pub async fn list_thread(
    db: &Db,
    parent_id: Uuid,
) -> Result<Vec<Message>, DbError> {
    let rows = sqlx::query_as::<_, Message>(
        "SELECT * FROM messages
         WHERE parent_id = $1
         ORDER BY created_at ASC",
    )
    .bind(parent_id)
    .fetch_all(db.pool())
    .await?;

    Ok(rows)
}

/// Count replies under a given parent message.
pub async fn count_thread_replies(db: &Db, parent_id: Uuid) -> Result<i64, DbError> {
    let (count,): (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM messages WHERE parent_id = $1")
            .bind(parent_id)
            .fetch_one(db.pool())
            .await?;

    Ok(count)
}

/// Count all messages in a conversation.
pub async fn count_for_conversation(db: &Db, conversation_id: Uuid) -> Result<i64, DbError> {
    let (count,): (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM messages WHERE conversation_id = $1")
            .bind(conversation_id)
            .fetch_one(db.pool())
            .await?;

    Ok(count)
}

/// Edit a message's content. Sets `edited = true` and bumps `updated_at`.
pub async fn update_content(
    db: &Db,
    id: Uuid,
    content: &str,
) -> Result<Option<Message>, DbError> {
    let row = sqlx::query_as::<_, Message>(
        "UPDATE messages
         SET content = $2, edited = TRUE, updated_at = now()
         WHERE id = $1
         RETURNING *",
    )
    .bind(id)
    .bind(content)
    .fetch_optional(db.pool())
    .await?;

    Ok(row)
}

/// Update the status of a message (e.g. sending -> sent, streaming -> sent).
pub async fn set_status(
    db: &Db,
    id: Uuid,
    status: MessageStatus,
) -> Result<bool, DbError> {
    let result = sqlx::query(
        "UPDATE messages SET status = $2, updated_at = now() WHERE id = $1",
    )
    .bind(id)
    .bind(status)
    .execute(db.pool())
    .await?;

    Ok(result.rows_affected() > 0)
}

/// Record token usage after a completion finishes.
pub async fn set_token_count(
    db: &Db,
    id: Uuid,
    token_count: i32,
) -> Result<bool, DbError> {
    let result = sqlx::query(
        "UPDATE messages SET token_count = $2, updated_at = now() WHERE id = $1",
    )
    .bind(id)
    .bind(token_count)
    .execute(db.pool())
    .await?;

    Ok(result.rows_affected() > 0)
}

/// Delete a single message. Thread replies get parent_id set to NULL (ON DELETE SET NULL).
pub async fn delete(db: &Db, id: Uuid) -> Result<bool, DbError> {
    let result = sqlx::query("DELETE FROM messages WHERE id = $1")
        .bind(id)
        .execute(db.pool())
        .await?;

    Ok(result.rows_affected() > 0)
}

// ── Branching queries ────────────────────────────────────────────

/// Walk from a leaf message to the root via parent_id, returned in chronological order.
pub async fn get_branch_path(db: &Db, leaf_id: Uuid) -> Result<Vec<Message>, DbError> {
    let rows = sqlx::query_as::<_, Message>(
        "WITH RECURSIVE branch AS (
            SELECT * FROM messages WHERE id = $1
            UNION ALL
            SELECT m.* FROM messages m JOIN branch b ON m.id = b.parent_id
         )
         SELECT * FROM branch ORDER BY created_at ASC",
    )
    .bind(leaf_id)
    .fetch_all(db.pool())
    .await?;
    Ok(rows)
}

/// Messages with more than one child (branch points).
/// Returns (parent_message_id, child_count).
pub async fn list_branch_points(
    db: &Db,
    conversation_id: Uuid,
) -> Result<Vec<(Uuid, i64)>, DbError> {
    let rows: Vec<(Uuid, i64)> = sqlx::query_as(
        "SELECT parent_id, COUNT(*) AS child_count
         FROM messages
         WHERE conversation_id = $1 AND parent_id IS NOT NULL
         GROUP BY parent_id
         HAVING COUNT(*) > 1",
    )
    .bind(conversation_id)
    .fetch_all(db.pool())
    .await?;
    Ok(rows)
}

/// Messages with no children (each defines a unique branch).
pub async fn get_leaf_messages(
    db: &Db,
    conversation_id: Uuid,
) -> Result<Vec<Message>, DbError> {
    let rows = sqlx::query_as::<_, Message>(
        "SELECT m.* FROM messages m
         WHERE m.conversation_id = $1
           AND NOT EXISTS (
               SELECT 1 FROM messages c WHERE c.parent_id = m.id
           )
         ORDER BY m.created_at DESC",
    )
    .bind(conversation_id)
    .fetch_all(db.pool())
    .await?;
    Ok(rows)
}

/// Delete all messages in a conversation without deleting the conversation itself.
/// Returns the number of messages deleted.
pub async fn clear_for_conversation(db: &Db, conversation_id: Uuid) -> Result<u64, DbError> {
    let result = sqlx::query("DELETE FROM messages WHERE conversation_id = $1")
        .bind(conversation_id)
        .execute(db.pool())
        .await?;

    Ok(result.rows_affected())
}

// ── Full-text search ────────────────────────────────────────────

/// A conversation-level search hit with a matching message snippet.
#[derive(Debug, Clone, FromRow, serde::Serialize)]
pub struct MessageSearchHit {
    pub conversation_id: Uuid,
    pub conversation_title: String,
    pub agent_id: Option<Uuid>,
    pub agent_name: Option<String>,
    pub message_snippet: String,
    pub message_role: MessageRole,
    pub message_created_at: DateTime<Utc>,
    pub conversation_updated_at: DateTime<Utc>,
}

/// Search message content across all conversations visible to a user.
/// Returns one hit per conversation (the most recent matching message).
pub async fn search_content(
    db: &Db,
    user_id: Uuid,
    is_admin: bool,
    query: &str,
    limit: i64,
) -> Result<Vec<MessageSearchHit>, DbError> {
    let pattern = format!("%{}%", query.replace('%', "\\%").replace('_', "\\_"));

    let rows = if is_admin {
        sqlx::query_as::<_, MessageSearchHit>(
            "SELECT DISTINCT ON (c.id)
                c.id AS conversation_id,
                c.title AS conversation_title,
                c.agent_id,
                a.name AS agent_name,
                LEFT(m.content, 200) AS message_snippet,
                m.role AS message_role,
                m.created_at AS message_created_at,
                c.updated_at AS conversation_updated_at
             FROM messages m
             JOIN conversations c ON c.id = m.conversation_id
             LEFT JOIN agents a ON a.id = c.agent_id
             WHERE m.role IN ('user', 'assistant')
               AND m.content ILIKE $1
             ORDER BY c.id, m.created_at DESC",
        )
        .bind(&pattern)
        .fetch_all(db.pool())
        .await?
    } else {
        sqlx::query_as::<_, MessageSearchHit>(
            "SELECT DISTINCT ON (c.id)
                c.id AS conversation_id,
                c.title AS conversation_title,
                c.agent_id,
                a.name AS agent_name,
                LEFT(m.content, 200) AS message_snippet,
                m.role AS message_role,
                m.created_at AS message_created_at,
                c.updated_at AS conversation_updated_at
             FROM messages m
             JOIN conversations c ON c.id = m.conversation_id
             LEFT JOIN agents a ON a.id = c.agent_id
             WHERE m.role IN ('user', 'assistant')
               AND m.content ILIKE $1
               AND (c.owner_id = $2 OR c.id IN (
                   SELECT conversation_id FROM conversation_shares WHERE shared_with = $2
               ))
             ORDER BY c.id, m.created_at DESC",
        )
        .bind(&pattern)
        .bind(user_id)
        .fetch_all(db.pool())
        .await?
    };

    // Sort by most recently updated conversation, then take limit
    let mut rows = rows;
    rows.sort_by(|a, b| b.conversation_updated_at.cmp(&a.conversation_updated_at));
    rows.truncate(limit as usize);
    Ok(rows)
}
