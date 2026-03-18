use chrono::{DateTime, Utc};
use sqlx::FromRow;
use uuid::Uuid;

use crate::{Db, DbError};

#[derive(Debug, Clone, FromRow)]
pub struct TokenUsageRow {
    pub id: Uuid,
    pub user_id: Uuid,
    pub connector_id: Option<Uuid>,
    pub model: String,
    pub prompt_tokens: i32,
    pub completion_tokens: i32,
    pub total_tokens: i32,
    pub conversation_id: Option<Uuid>,
    pub created_at: DateTime<Utc>,
}

/// Aggregated token usage summary per model.
#[derive(Debug, Clone, FromRow)]
pub struct UsageSummary {
    pub model: String,
    pub prompt_tokens: i64,
    pub completion_tokens: i64,
    pub total_tokens: i64,
}

/// Per-user token usage with user metadata (from a JOIN).
#[derive(Debug, Clone, FromRow)]
pub struct UserUsageSummary {
    pub user_id: Uuid,
    pub email: String,
    pub display_name: String,
    pub model: String,
    pub prompt_tokens: i64,
    pub completion_tokens: i64,
    pub total_tokens: i64,
}

pub async fn record(
    db: &Db,
    user_id: Uuid,
    connector_id: Option<Uuid>,
    model: &str,
    prompt_tokens: u32,
    completion_tokens: u32,
    total_tokens: u32,
    conversation_id: Option<Uuid>,
) -> Result<(), DbError> {
    sqlx::query(
        "INSERT INTO token_usage (user_id, connector_id, model, prompt_tokens, completion_tokens, total_tokens, conversation_id)
         VALUES ($1, $2, $3, $4, $5, $6, $7)",
    )
    .bind(user_id)
    .bind(connector_id)
    .bind(model)
    .bind(prompt_tokens as i32)
    .bind(completion_tokens as i32)
    .bind(total_tokens as i32)
    .bind(conversation_id)
    .execute(db.pool())
    .await?;
    Ok(())
}

/// Aggregate usage for a single user, optionally since a given date.
pub async fn get_user_summary(
    db: &Db,
    user_id: Uuid,
    since: Option<DateTime<Utc>>,
) -> Result<Vec<UsageSummary>, DbError> {
    let rows = if let Some(since) = since {
        sqlx::query_as::<_, UsageSummary>(
            "SELECT model,
                    SUM(prompt_tokens)::BIGINT AS prompt_tokens,
                    SUM(completion_tokens)::BIGINT AS completion_tokens,
                    SUM(total_tokens)::BIGINT AS total_tokens
             FROM token_usage
             WHERE user_id = $1 AND created_at >= $2
             GROUP BY model
             ORDER BY total_tokens DESC",
        )
        .bind(user_id)
        .bind(since)
        .fetch_all(db.pool())
        .await?
    } else {
        sqlx::query_as::<_, UsageSummary>(
            "SELECT model,
                    SUM(prompt_tokens)::BIGINT AS prompt_tokens,
                    SUM(completion_tokens)::BIGINT AS completion_tokens,
                    SUM(total_tokens)::BIGINT AS total_tokens
             FROM token_usage
             WHERE user_id = $1
             GROUP BY model
             ORDER BY total_tokens DESC",
        )
        .bind(user_id)
        .fetch_all(db.pool())
        .await?
    };
    Ok(rows)
}

/// Aggregate usage across all users, grouped by user + model.
pub async fn get_all_users_summary(
    db: &Db,
    since: Option<DateTime<Utc>>,
) -> Result<Vec<UserUsageSummary>, DbError> {
    let rows = if let Some(since) = since {
        sqlx::query_as::<_, UserUsageSummary>(
            "SELECT t.user_id, u.email, u.display_name, t.model,
                    SUM(t.prompt_tokens)::BIGINT AS prompt_tokens,
                    SUM(t.completion_tokens)::BIGINT AS completion_tokens,
                    SUM(t.total_tokens)::BIGINT AS total_tokens
             FROM token_usage t
             JOIN users u ON u.id = t.user_id
             WHERE t.created_at >= $1
             GROUP BY t.user_id, u.email, u.display_name, t.model
             ORDER BY total_tokens DESC",
        )
        .bind(since)
        .fetch_all(db.pool())
        .await?
    } else {
        sqlx::query_as::<_, UserUsageSummary>(
            "SELECT t.user_id, u.email, u.display_name, t.model,
                    SUM(t.prompt_tokens)::BIGINT AS prompt_tokens,
                    SUM(t.completion_tokens)::BIGINT AS completion_tokens,
                    SUM(t.total_tokens)::BIGINT AS total_tokens
             FROM token_usage t
             JOIN users u ON u.id = t.user_id
             GROUP BY t.user_id, u.email, u.display_name, t.model
             ORDER BY total_tokens DESC",
        )
        .fetch_all(db.pool())
        .await?
    };
    Ok(rows)
}
