use chrono::{DateTime, Utc};
use sqlx::FromRow;
use uuid::Uuid;

use crate::{Db, DbError};

#[derive(Debug, Clone, FromRow)]
pub struct TelegramChatRow {
    pub connector_id: Uuid,
    pub telegram_chat_id: i64,
    pub conversation_id: Uuid,
    pub telegram_username: Option<String>,
    pub telegram_first_name: Option<String>,
    pub created_at: DateTime<Utc>,
}

/// Look up the conversation for a given (connector, telegram_chat_id) pair.
pub async fn get(
    db: &Db,
    connector_id: Uuid,
    telegram_chat_id: i64,
) -> Result<Option<TelegramChatRow>, DbError> {
    let row = sqlx::query_as::<_, TelegramChatRow>(
        "SELECT * FROM telegram_chats WHERE connector_id = $1 AND telegram_chat_id = $2",
    )
    .bind(connector_id)
    .bind(telegram_chat_id)
    .fetch_optional(db.pool())
    .await?;
    Ok(row)
}

/// Create a mapping from a Telegram chat to a Clawkson conversation.
pub async fn create(
    db: &Db,
    connector_id: Uuid,
    telegram_chat_id: i64,
    conversation_id: Uuid,
    username: Option<&str>,
    first_name: Option<&str>,
) -> Result<TelegramChatRow, DbError> {
    let row = sqlx::query_as::<_, TelegramChatRow>(
        "INSERT INTO telegram_chats (connector_id, telegram_chat_id, conversation_id, telegram_username, telegram_first_name)
         VALUES ($1, $2, $3, $4, $5)
         RETURNING *",
    )
    .bind(connector_id)
    .bind(telegram_chat_id)
    .bind(conversation_id)
    .bind(username)
    .bind(first_name)
    .fetch_one(db.pool())
    .await?;
    Ok(row)
}

/// Delete all chat mappings for a connector (used when the connector is deleted).
pub async fn delete_for_connector(db: &Db, connector_id: Uuid) -> Result<u64, DbError> {
    let result = sqlx::query("DELETE FROM telegram_chats WHERE connector_id = $1")
        .bind(connector_id)
        .execute(db.pool())
        .await?;
    Ok(result.rows_affected())
}
