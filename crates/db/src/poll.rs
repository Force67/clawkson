use chrono::{DateTime, Utc};
use sqlx::FromRow;
use uuid::Uuid;

use crate::{Db, DbError};

#[derive(Debug, Clone, FromRow)]
pub struct PollRow {
    pub id: Uuid,
    pub conversation_id: Uuid,
    pub message_id: Option<Uuid>,
    pub question: String,
    pub allow_multiple: bool,
    pub created_by: Option<Uuid>,
    pub created_at: DateTime<Utc>,
    pub closes_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, FromRow)]
pub struct PollOptionRow {
    pub id: Uuid,
    pub poll_id: Uuid,
    pub label: String,
    pub sort_order: i32,
}

#[derive(Debug, Clone, FromRow)]
pub struct PollVoteRow {
    pub id: Uuid,
    pub option_id: Uuid,
    pub user_id: Uuid,
    pub voted_at: DateTime<Utc>,
}

/// Create a poll with options.
pub async fn create(
    db: &Db,
    conversation_id: Uuid,
    message_id: Option<Uuid>,
    question: &str,
    options: &[String],
    allow_multiple: bool,
    created_by: Option<Uuid>,
    closes_at: Option<DateTime<Utc>>,
) -> Result<PollRow, DbError> {
    let poll = sqlx::query_as::<_, PollRow>(
        "INSERT INTO polls (conversation_id, message_id, question, allow_multiple, created_by, closes_at)
         VALUES ($1, $2, $3, $4, $5, $6)
         RETURNING *",
    )
    .bind(conversation_id)
    .bind(message_id)
    .bind(question)
    .bind(allow_multiple)
    .bind(created_by)
    .bind(closes_at)
    .fetch_one(db.pool())
    .await?;

    for (i, label) in options.iter().enumerate() {
        sqlx::query(
            "INSERT INTO poll_options (poll_id, label, sort_order) VALUES ($1, $2, $3)",
        )
        .bind(poll.id)
        .bind(label)
        .bind(i as i32)
        .execute(db.pool())
        .await?;
    }

    Ok(poll)
}

/// Get a poll by ID.
pub async fn get(db: &Db, id: Uuid) -> Result<Option<PollRow>, DbError> {
    let row = sqlx::query_as::<_, PollRow>("SELECT * FROM polls WHERE id = $1")
        .bind(id)
        .fetch_optional(db.pool())
        .await?;
    Ok(row)
}

/// List polls for a conversation.
pub async fn list_for_conversation(db: &Db, conversation_id: Uuid) -> Result<Vec<PollRow>, DbError> {
    let rows = sqlx::query_as::<_, PollRow>(
        "SELECT * FROM polls WHERE conversation_id = $1 ORDER BY created_at DESC",
    )
    .bind(conversation_id)
    .fetch_all(db.pool())
    .await?;
    Ok(rows)
}

/// List options for a poll.
pub async fn list_options(db: &Db, poll_id: Uuid) -> Result<Vec<PollOptionRow>, DbError> {
    let rows = sqlx::query_as::<_, PollOptionRow>(
        "SELECT * FROM poll_options WHERE poll_id = $1 ORDER BY sort_order",
    )
    .bind(poll_id)
    .fetch_all(db.pool())
    .await?;
    Ok(rows)
}

/// Cast a vote.
pub async fn vote(db: &Db, option_id: Uuid, user_id: Uuid) -> Result<PollVoteRow, DbError> {
    let row = sqlx::query_as::<_, PollVoteRow>(
        "INSERT INTO poll_votes (option_id, user_id)
         VALUES ($1, $2)
         ON CONFLICT (option_id, user_id) DO UPDATE SET voted_at = poll_votes.voted_at
         RETURNING *",
    )
    .bind(option_id)
    .bind(user_id)
    .fetch_one(db.pool())
    .await?;
    Ok(row)
}

/// Remove a vote.
pub async fn unvote(db: &Db, option_id: Uuid, user_id: Uuid) -> Result<bool, DbError> {
    let result = sqlx::query(
        "DELETE FROM poll_votes WHERE option_id = $1 AND user_id = $2",
    )
    .bind(option_id)
    .bind(user_id)
    .execute(db.pool())
    .await?;
    Ok(result.rows_affected() > 0)
}

/// Get vote counts per option.
pub async fn vote_counts(db: &Db, poll_id: Uuid) -> Result<Vec<(Uuid, String, i64)>, DbError> {
    let rows: Vec<(Uuid, String, i64)> = sqlx::query_as(
        "SELECT o.id, o.label, COUNT(v.id) as votes
         FROM poll_options o
         LEFT JOIN poll_votes v ON v.option_id = o.id
         WHERE o.poll_id = $1
         GROUP BY o.id, o.label, o.sort_order
         ORDER BY o.sort_order",
    )
    .bind(poll_id)
    .fetch_all(db.pool())
    .await?;
    Ok(rows)
}
