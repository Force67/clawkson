use chrono::{DateTime, Utc};
use sqlx::PgPool;
use uuid::Uuid;

#[derive(Debug, Clone, sqlx::FromRow, serde::Serialize)]
pub struct ChatAttachmentRow {
    pub id: Uuid,
    pub owner_id: Option<Uuid>,
    pub conversation_id: Option<Uuid>,
    pub message_id: Option<Uuid>,
    pub filename: String,
    pub content_type: String,
    pub s3_key: String,
    pub size_bytes: i64,
    pub metadata: Option<serde_json::Value>,
    pub created_at: DateTime<Utc>,
}

/// Insert a new chat attachment record.
pub async fn create(
    pool: &PgPool,
    id: Uuid,
    owner_id: Uuid,
    conversation_id: Option<Uuid>,
    filename: &str,
    content_type: &str,
    s3_key: &str,
    size_bytes: i64,
) -> Result<ChatAttachmentRow, sqlx::Error> {
    create_with_metadata(pool, id, owner_id, conversation_id, filename, content_type, s3_key, size_bytes, None).await
}

/// Insert a new chat attachment record with optional metadata.
pub async fn create_with_metadata(
    pool: &PgPool,
    id: Uuid,
    owner_id: Uuid,
    conversation_id: Option<Uuid>,
    filename: &str,
    content_type: &str,
    s3_key: &str,
    size_bytes: i64,
    metadata: Option<serde_json::Value>,
) -> Result<ChatAttachmentRow, sqlx::Error> {
    sqlx::query_as::<_, ChatAttachmentRow>(
        r#"
        INSERT INTO chat_attachments (id, owner_id, conversation_id, filename, content_type, s3_key, size_bytes, metadata)
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
        RETURNING *
        "#,
    )
    .bind(id)
    .bind(owner_id)
    .bind(conversation_id)
    .bind(filename)
    .bind(content_type)
    .bind(s3_key)
    .bind(size_bytes)
    .bind(metadata)
    .fetch_one(pool)
    .await
}

/// Fetch a single attachment by ID.
pub async fn get_by_id(pool: &PgPool, id: Uuid) -> Result<Option<ChatAttachmentRow>, sqlx::Error> {
    sqlx::query_as::<_, ChatAttachmentRow>("SELECT * FROM chat_attachments WHERE id = $1")
        .bind(id)
        .fetch_optional(pool)
        .await
}

/// List attachments for a conversation.
pub async fn list_for_conversation(pool: &PgPool, conversation_id: Uuid) -> Result<Vec<ChatAttachmentRow>, sqlx::Error> {
    sqlx::query_as::<_, ChatAttachmentRow>(
        "SELECT * FROM chat_attachments WHERE conversation_id = $1 ORDER BY created_at ASC",
    )
    .bind(conversation_id)
    .fetch_all(pool)
    .await
}

/// List attachments for a specific message.
pub async fn list_for_message(pool: &PgPool, message_id: Uuid) -> Result<Vec<ChatAttachmentRow>, sqlx::Error> {
    sqlx::query_as::<_, ChatAttachmentRow>(
        "SELECT * FROM chat_attachments WHERE message_id = $1 ORDER BY created_at ASC",
    )
    .bind(message_id)
    .fetch_all(pool)
    .await
}

/// Link an attachment to a message (after the message is created).
pub async fn link_to_message(pool: &PgPool, id: Uuid, message_id: Uuid) -> Result<bool, sqlx::Error> {
    let result = sqlx::query("UPDATE chat_attachments SET message_id = $2 WHERE id = $1")
        .bind(id)
        .bind(message_id)
        .execute(pool)
        .await?;
    Ok(result.rows_affected() > 0)
}

/// Delete an attachment record.
pub async fn delete(pool: &PgPool, id: Uuid) -> Result<bool, sqlx::Error> {
    let result = sqlx::query("DELETE FROM chat_attachments WHERE id = $1")
        .bind(id)
        .execute(pool)
        .await?;
    Ok(result.rows_affected() > 0)
}
