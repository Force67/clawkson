use chrono::{DateTime, Utc};
use sqlx::PgPool;
use uuid::Uuid;

#[derive(Debug, Clone, sqlx::Type, PartialEq, Eq)]
#[sqlx(type_name = "share_permission", rename_all = "snake_case")]
pub enum SharePermission {
    Read,
    Write,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct ShareRow {
    pub id: Uuid,
    pub conversation_id: Uuid,
    pub shared_by: Uuid,
    pub shared_with: Uuid,
    pub permission: SharePermission,
    pub created_at: DateTime<Utc>,
}

pub async fn create(
    pool: &PgPool,
    conversation_id: Uuid,
    shared_by: Uuid,
    shared_with: Uuid,
    permission: SharePermission,
) -> Result<ShareRow, sqlx::Error> {
    sqlx::query_as::<_, ShareRow>(
        r#"
        INSERT INTO conversation_shares (conversation_id, shared_by, shared_with, permission)
        VALUES ($1, $2, $3, $4)
        ON CONFLICT (conversation_id, shared_with) DO UPDATE SET permission = $4
        RETURNING *
        "#,
    )
    .bind(conversation_id)
    .bind(shared_by)
    .bind(shared_with)
    .bind(permission)
    .fetch_one(pool)
    .await
}

/// List all shares for a conversation.
pub async fn list_for_conversation(
    pool: &PgPool,
    conversation_id: Uuid,
) -> Result<Vec<ShareRow>, sqlx::Error> {
    sqlx::query_as::<_, ShareRow>(
        "SELECT * FROM conversation_shares WHERE conversation_id = $1 ORDER BY created_at",
    )
    .bind(conversation_id)
    .fetch_all(pool)
    .await
}

/// List conversations shared with a specific user.
pub async fn list_shared_with_user(
    pool: &PgPool,
    user_id: Uuid,
) -> Result<Vec<ShareRow>, sqlx::Error> {
    sqlx::query_as::<_, ShareRow>(
        "SELECT * FROM conversation_shares WHERE shared_with = $1",
    )
    .bind(user_id)
    .fetch_all(pool)
    .await
}

/// Check if a user has access to a conversation (as share recipient).
pub async fn get_user_share(
    pool: &PgPool,
    conversation_id: Uuid,
    user_id: Uuid,
) -> Result<Option<ShareRow>, sqlx::Error> {
    sqlx::query_as::<_, ShareRow>(
        "SELECT * FROM conversation_shares WHERE conversation_id = $1 AND shared_with = $2",
    )
    .bind(conversation_id)
    .bind(user_id)
    .fetch_optional(pool)
    .await
}

pub async fn delete(pool: &PgPool, id: Uuid) -> Result<bool, sqlx::Error> {
    let result = sqlx::query("DELETE FROM conversation_shares WHERE id = $1")
        .bind(id)
        .execute(pool)
        .await?;
    Ok(result.rows_affected() > 0)
}

pub async fn delete_by_conversation_and_user(
    pool: &PgPool,
    conversation_id: Uuid,
    shared_with: Uuid,
) -> Result<bool, sqlx::Error> {
    let result = sqlx::query(
        "DELETE FROM conversation_shares WHERE conversation_id = $1 AND shared_with = $2",
    )
    .bind(conversation_id)
    .bind(shared_with)
    .execute(pool)
    .await?;
    Ok(result.rows_affected() > 0)
}
