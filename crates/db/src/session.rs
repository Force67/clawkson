use chrono::{DateTime, Utc};
use sqlx::PgPool;
use uuid::Uuid;

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct SessionRow {
    pub token: String,
    pub user_id: Uuid,
    pub expires_at: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
}

pub async fn create(
    pool: &PgPool,
    token: &str,
    user_id: Uuid,
    expires_at: DateTime<Utc>,
) -> Result<SessionRow, sqlx::Error> {
    sqlx::query_as::<_, SessionRow>(
        r#"
        INSERT INTO sessions (token, user_id, expires_at)
        VALUES ($1, $2, $3)
        RETURNING *
        "#,
    )
    .bind(token)
    .bind(user_id)
    .bind(expires_at)
    .fetch_one(pool)
    .await
}

/// Get a valid (non-expired) session by token.
pub async fn get_valid(pool: &PgPool, token: &str) -> Result<Option<SessionRow>, sqlx::Error> {
    sqlx::query_as::<_, SessionRow>(
        "SELECT * FROM sessions WHERE token = $1 AND expires_at > now()",
    )
    .bind(token)
    .fetch_optional(pool)
    .await
}

pub async fn delete(pool: &PgPool, token: &str) -> Result<bool, sqlx::Error> {
    let result = sqlx::query("DELETE FROM sessions WHERE token = $1")
        .bind(token)
        .execute(pool)
        .await?;
    Ok(result.rows_affected() > 0)
}

pub async fn delete_all_for_user(pool: &PgPool, user_id: Uuid) -> Result<u64, sqlx::Error> {
    let result = sqlx::query("DELETE FROM sessions WHERE user_id = $1")
        .bind(user_id)
        .execute(pool)
        .await?;
    Ok(result.rows_affected())
}

/// Purge expired sessions (run periodically or on login).
pub async fn purge_expired(pool: &PgPool) -> Result<u64, sqlx::Error> {
    let result = sqlx::query("DELETE FROM sessions WHERE expires_at <= now()")
        .execute(pool)
        .await?;
    Ok(result.rows_affected())
}
