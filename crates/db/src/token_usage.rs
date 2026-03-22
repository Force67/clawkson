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

/// Aggregated token usage with cost estimate from model_pricing JOIN.
#[derive(Debug, Clone, FromRow)]
pub struct UsageSummaryWithCost {
    pub model: String,
    pub prompt_tokens: i64,
    pub completion_tokens: i64,
    pub total_tokens: i64,
    pub estimated_cost_usd: f64,
}

/// Time-bucketed usage with cost.
#[derive(Debug, Clone, FromRow)]
pub struct UsageTimeBucket {
    pub bucket: DateTime<Utc>,
    pub model: String,
    pub prompt_tokens: i64,
    pub completion_tokens: i64,
    pub total_tokens: i64,
    pub estimated_cost_usd: f64,
}

/// Aggregate usage for one conversation, joined with pricing.
pub async fn get_conversation_summary(
    db: &Db,
    conversation_id: Uuid,
) -> Result<Vec<UsageSummaryWithCost>, DbError> {
    let rows = sqlx::query_as::<_, UsageSummaryWithCost>(
        "SELECT t.model,
                SUM(t.prompt_tokens)::BIGINT AS prompt_tokens,
                SUM(t.completion_tokens)::BIGINT AS completion_tokens,
                SUM(t.total_tokens)::BIGINT AS total_tokens,
                COALESCE(SUM(
                    t.prompt_tokens::NUMERIC * p.prompt_cost_per_million / 1000000
                    + t.completion_tokens::NUMERIC * p.completion_cost_per_million / 1000000
                ), 0)::FLOAT8 AS estimated_cost_usd
         FROM token_usage t
         LEFT JOIN model_pricing p ON p.model = t.model
         WHERE t.conversation_id = $1
         GROUP BY t.model
         ORDER BY total_tokens DESC",
    )
    .bind(conversation_id)
    .fetch_all(db.pool())
    .await?;
    Ok(rows)
}

/// Aggregate usage for a single user with cost estimates.
pub async fn get_user_summary_with_cost(
    db: &Db,
    user_id: Uuid,
    since: Option<DateTime<Utc>>,
) -> Result<Vec<UsageSummaryWithCost>, DbError> {
    let rows = if let Some(since) = since {
        sqlx::query_as::<_, UsageSummaryWithCost>(
            "SELECT t.model,
                    SUM(t.prompt_tokens)::BIGINT AS prompt_tokens,
                    SUM(t.completion_tokens)::BIGINT AS completion_tokens,
                    SUM(t.total_tokens)::BIGINT AS total_tokens,
                    COALESCE(SUM(
                        t.prompt_tokens::NUMERIC * p.prompt_cost_per_million / 1000000
                        + t.completion_tokens::NUMERIC * p.completion_cost_per_million / 1000000
                    ), 0)::FLOAT8 AS estimated_cost_usd
             FROM token_usage t
             LEFT JOIN model_pricing p ON p.model = t.model
             WHERE t.user_id = $1 AND t.created_at >= $2
             GROUP BY t.model
             ORDER BY total_tokens DESC",
        )
        .bind(user_id)
        .bind(since)
        .fetch_all(db.pool())
        .await?
    } else {
        sqlx::query_as::<_, UsageSummaryWithCost>(
            "SELECT t.model,
                    SUM(t.prompt_tokens)::BIGINT AS prompt_tokens,
                    SUM(t.completion_tokens)::BIGINT AS completion_tokens,
                    SUM(t.total_tokens)::BIGINT AS total_tokens,
                    COALESCE(SUM(
                        t.prompt_tokens::NUMERIC * p.prompt_cost_per_million / 1000000
                        + t.completion_tokens::NUMERIC * p.completion_cost_per_million / 1000000
                    ), 0)::FLOAT8 AS estimated_cost_usd
             FROM token_usage t
             LEFT JOIN model_pricing p ON p.model = t.model
             WHERE t.user_id = $1
             GROUP BY t.model
             ORDER BY total_tokens DESC",
        )
        .bind(user_id)
        .fetch_all(db.pool())
        .await?
    };
    Ok(rows)
}

/// Time-series usage with cost, bucketed by day/hour/week.
pub async fn get_time_series(
    db: &Db,
    user_id: Option<Uuid>,
    since: DateTime<Utc>,
    bucket: &str,
) -> Result<Vec<UsageTimeBucket>, DbError> {
    // bucket must be one of: hour, day, week
    let trunc = match bucket {
        "hour" => "hour",
        "week" => "week",
        _ => "day",
    };

    let query = format!(
        "SELECT date_trunc('{trunc}', t.created_at) AS bucket,
                t.model,
                SUM(t.prompt_tokens)::BIGINT AS prompt_tokens,
                SUM(t.completion_tokens)::BIGINT AS completion_tokens,
                SUM(t.total_tokens)::BIGINT AS total_tokens,
                COALESCE(SUM(
                    t.prompt_tokens::NUMERIC * p.prompt_cost_per_million / 1000000
                    + t.completion_tokens::NUMERIC * p.completion_cost_per_million / 1000000
                ), 0)::FLOAT8 AS estimated_cost_usd
         FROM token_usage t
         LEFT JOIN model_pricing p ON p.model = t.model
         WHERE t.created_at >= $1 {user_filter}
         GROUP BY bucket, t.model
         ORDER BY bucket, t.model",
        user_filter = if user_id.is_some() { "AND t.user_id = $2" } else { "" },
    );

    let rows = if let Some(uid) = user_id {
        sqlx::query_as::<_, UsageTimeBucket>(&query)
            .bind(since)
            .bind(uid)
            .fetch_all(db.pool())
            .await?
    } else {
        sqlx::query_as::<_, UsageTimeBucket>(&query)
            .bind(since)
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
