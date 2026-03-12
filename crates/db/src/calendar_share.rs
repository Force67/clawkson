use chrono::{DateTime, Utc};
use sqlx::FromRow;
use uuid::Uuid;

use crate::share::SharePermission;
use crate::{Db, DbError};

#[derive(Debug, Clone, FromRow)]
pub struct CalendarShareRow {
    pub id: Uuid,
    pub owner_id: Uuid,
    pub shared_with: Uuid,
    pub permission: SharePermission,
    pub created_at: DateTime<Utc>,
}

/// Share your calendar with another user (upsert).
pub async fn create(
    db: &Db,
    owner_id: Uuid,
    shared_with: Uuid,
    permission: SharePermission,
) -> Result<CalendarShareRow, DbError> {
    let row = sqlx::query_as::<_, CalendarShareRow>(
        r#"
        INSERT INTO calendar_shares (owner_id, shared_with, permission)
        VALUES ($1, $2, $3)
        ON CONFLICT (owner_id, shared_with) DO UPDATE SET permission = $3
        RETURNING *
        "#,
    )
    .bind(owner_id)
    .bind(shared_with)
    .bind(permission)
    .fetch_one(db.pool())
    .await?;

    Ok(row)
}

/// List all users a calendar owner has shared with.
pub async fn list_for_owner(
    db: &Db,
    owner_id: Uuid,
) -> Result<Vec<CalendarShareRow>, DbError> {
    let rows = sqlx::query_as::<_, CalendarShareRow>(
        "SELECT * FROM calendar_shares WHERE owner_id = $1 ORDER BY created_at",
    )
    .bind(owner_id)
    .fetch_all(db.pool())
    .await?;

    Ok(rows)
}

/// List all calendars shared with a specific user.
pub async fn list_shared_with(
    db: &Db,
    user_id: Uuid,
) -> Result<Vec<CalendarShareRow>, DbError> {
    let rows = sqlx::query_as::<_, CalendarShareRow>(
        "SELECT * FROM calendar_shares WHERE shared_with = $1 ORDER BY created_at",
    )
    .bind(user_id)
    .fetch_all(db.pool())
    .await?;

    Ok(rows)
}

/// Check if a user has access to another user's calendar.
pub async fn get_share(
    db: &Db,
    owner_id: Uuid,
    shared_with: Uuid,
) -> Result<Option<CalendarShareRow>, DbError> {
    let row = sqlx::query_as::<_, CalendarShareRow>(
        "SELECT * FROM calendar_shares WHERE owner_id = $1 AND shared_with = $2",
    )
    .bind(owner_id)
    .bind(shared_with)
    .fetch_optional(db.pool())
    .await?;

    Ok(row)
}

/// Remove a calendar share.
pub async fn delete(
    db: &Db,
    owner_id: Uuid,
    shared_with: Uuid,
) -> Result<bool, DbError> {
    let result = sqlx::query(
        "DELETE FROM calendar_shares WHERE owner_id = $1 AND shared_with = $2",
    )
    .bind(owner_id)
    .bind(shared_with)
    .execute(db.pool())
    .await?;

    Ok(result.rows_affected() > 0)
}
