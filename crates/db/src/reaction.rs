use chrono::{DateTime, Utc};
use sqlx::FromRow;
use uuid::Uuid;

use crate::{Db, DbError};

#[derive(Debug, Clone, FromRow)]
pub struct ReactionRow {
    pub id: Uuid,
    pub message_id: Uuid,
    pub user_id: Uuid,
    pub emoji: String,
    pub created_at: DateTime<Utc>,
}

/// List all reactions for a message.
pub async fn list_for_message(db: &Db, message_id: Uuid) -> Result<Vec<ReactionRow>, DbError> {
    let rows = sqlx::query_as::<_, ReactionRow>(
        "SELECT id, message_id, user_id, emoji, created_at
         FROM message_reactions
         WHERE message_id = $1
         ORDER BY created_at",
    )
    .bind(message_id)
    .fetch_all(db.pool())
    .await?;
    Ok(rows)
}

/// Add a reaction. Returns the new row or the existing one if already present.
pub async fn add(db: &Db, message_id: Uuid, user_id: Uuid, emoji: &str) -> Result<ReactionRow, DbError> {
    let row = sqlx::query_as::<_, ReactionRow>(
        "INSERT INTO message_reactions (message_id, user_id, emoji)
         VALUES ($1, $2, $3)
         ON CONFLICT (message_id, user_id, emoji) DO UPDATE SET created_at = message_reactions.created_at
         RETURNING id, message_id, user_id, emoji, created_at",
    )
    .bind(message_id)
    .bind(user_id)
    .bind(emoji)
    .fetch_one(db.pool())
    .await?;
    Ok(row)
}

/// Remove a reaction.
pub async fn remove(db: &Db, message_id: Uuid, user_id: Uuid, emoji: &str) -> Result<bool, DbError> {
    let result = sqlx::query(
        "DELETE FROM message_reactions WHERE message_id = $1 AND user_id = $2 AND emoji = $3",
    )
    .bind(message_id)
    .bind(user_id)
    .bind(emoji)
    .execute(db.pool())
    .await?;
    Ok(result.rows_affected() > 0)
}

/// Count reactions grouped by emoji for a message.
pub async fn counts_for_message(db: &Db, message_id: Uuid) -> Result<Vec<(String, i64)>, DbError> {
    let rows: Vec<(String, i64)> = sqlx::query_as(
        "SELECT emoji, COUNT(*) as count
         FROM message_reactions
         WHERE message_id = $1
         GROUP BY emoji
         ORDER BY count DESC",
    )
    .bind(message_id)
    .fetch_all(db.pool())
    .await?;
    Ok(rows)
}
