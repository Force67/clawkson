use chrono::{DateTime, NaiveDate, NaiveTime, Utc};
use sqlx::FromRow;
use uuid::Uuid;

use crate::{Db, DbError};

#[derive(Debug, Clone, FromRow, serde::Serialize, serde::Deserialize)]
pub struct CalendarEvent {
    pub id: Uuid,
    pub owner_id: Uuid,
    pub title: String,
    pub date: NaiveDate,
    pub start_time: NaiveTime,
    pub end_time: NaiveTime,
    pub category: String,
    pub location: Option<String>,
    pub notes: Option<String>,
    pub completed: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Create a new calendar event.
pub async fn create(
    db: &Db,
    owner_id: Uuid,
    title: &str,
    date: NaiveDate,
    start_time: NaiveTime,
    end_time: NaiveTime,
    category: &str,
    location: Option<&str>,
    notes: Option<&str>,
) -> Result<CalendarEvent, DbError> {
    let row = sqlx::query_as::<_, CalendarEvent>(
        "INSERT INTO calendar_events (owner_id, title, date, start_time, end_time, category, location, notes)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
         RETURNING *",
    )
    .bind(owner_id)
    .bind(title)
    .bind(date)
    .bind(start_time)
    .bind(end_time)
    .bind(category)
    .bind(location)
    .bind(notes)
    .fetch_one(db.pool())
    .await?;

    Ok(row)
}

/// Fetch a single event by id.
pub async fn get_by_id(db: &Db, id: Uuid) -> Result<Option<CalendarEvent>, DbError> {
    let row = sqlx::query_as::<_, CalendarEvent>(
        "SELECT * FROM calendar_events WHERE id = $1",
    )
    .bind(id)
    .fetch_optional(db.pool())
    .await?;

    Ok(row)
}

/// List events owned by a user within a date range.
pub async fn list_for_user(
    db: &Db,
    owner_id: Uuid,
    from: NaiveDate,
    to: NaiveDate,
) -> Result<Vec<CalendarEvent>, DbError> {
    let rows = sqlx::query_as::<_, CalendarEvent>(
        "SELECT * FROM calendar_events
         WHERE owner_id = $1 AND date >= $2 AND date <= $3
         ORDER BY date, start_time",
    )
    .bind(owner_id)
    .bind(from)
    .bind(to)
    .fetch_all(db.pool())
    .await?;

    Ok(rows)
}

/// List all events for a user (no date filter).
pub async fn list_all_for_user(
    db: &Db,
    owner_id: Uuid,
) -> Result<Vec<CalendarEvent>, DbError> {
    let rows = sqlx::query_as::<_, CalendarEvent>(
        "SELECT * FROM calendar_events
         WHERE owner_id = $1
         ORDER BY date, start_time",
    )
    .bind(owner_id)
    .fetch_all(db.pool())
    .await?;

    Ok(rows)
}

/// Update a calendar event.
pub async fn update(
    db: &Db,
    id: Uuid,
    title: &str,
    date: NaiveDate,
    start_time: NaiveTime,
    end_time: NaiveTime,
    category: &str,
    location: Option<&str>,
    notes: Option<&str>,
    completed: bool,
) -> Result<Option<CalendarEvent>, DbError> {
    let row = sqlx::query_as::<_, CalendarEvent>(
        "UPDATE calendar_events
         SET title = $2, date = $3, start_time = $4, end_time = $5,
             category = $6, location = $7, notes = $8, completed = $9,
             updated_at = now()
         WHERE id = $1
         RETURNING *",
    )
    .bind(id)
    .bind(title)
    .bind(date)
    .bind(start_time)
    .bind(end_time)
    .bind(category)
    .bind(location)
    .bind(notes)
    .bind(completed)
    .fetch_optional(db.pool())
    .await?;

    Ok(row)
}

/// Toggle completed status.
pub async fn set_completed(db: &Db, id: Uuid, completed: bool) -> Result<bool, DbError> {
    let result = sqlx::query(
        "UPDATE calendar_events SET completed = $2, updated_at = now() WHERE id = $1",
    )
    .bind(id)
    .bind(completed)
    .execute(db.pool())
    .await?;

    Ok(result.rows_affected() > 0)
}

/// Delete an event.
pub async fn delete(db: &Db, id: Uuid) -> Result<bool, DbError> {
    let result = sqlx::query("DELETE FROM calendar_events WHERE id = $1")
        .bind(id)
        .execute(db.pool())
        .await?;

    Ok(result.rows_affected() > 0)
}
