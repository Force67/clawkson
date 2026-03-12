use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    routing::get,
    Json, Router,
};
use chrono::NaiveDate;
use clawkson_core::{CalendarEvent, CalendarShare, SharePermission};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::auth::AuthUser;
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", get(list_events).post(create_event))
        .route("/{id}", get(get_event).put(update_event).delete(delete_event))
        .route("/{id}/complete", axum::routing::patch(toggle_complete))
        .route("/shares", get(list_shares).post(create_share))
        .route("/shares/{user_id}", axum::routing::delete(remove_share))
        .route("/shared", get(list_shared_calendars))
        .route("/shared/{owner_id}", get(list_shared_events))
}

// ── Request / Response types ───────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct CreateEventRequest {
    pub title: String,
    pub date: String,
    pub start_time: String,
    pub end_time: String,
    #[serde(default = "default_category")]
    pub category: String,
    #[serde(default)]
    pub location: Option<String>,
    #[serde(default)]
    pub notes: Option<String>,
}

fn default_category() -> String {
    "work".to_string()
}

#[derive(Debug, Deserialize)]
pub struct UpdateEventRequest {
    pub title: String,
    pub date: String,
    pub start_time: String,
    pub end_time: String,
    pub category: String,
    #[serde(default)]
    pub location: Option<String>,
    #[serde(default)]
    pub notes: Option<String>,
    #[serde(default)]
    pub completed: bool,
}

#[derive(Debug, Deserialize)]
pub struct ToggleCompleteRequest {
    pub completed: bool,
}

#[derive(Debug, Deserialize)]
pub struct ListEventsQuery {
    pub from: Option<String>,
    pub to: Option<String>,
    /// View another user's calendar (if shared with you).
    pub owner_id: Option<Uuid>,
}

#[derive(Debug, Deserialize)]
pub struct CreateShareRequest {
    pub email: String,
    pub permission: SharePermission,
}

#[derive(Debug, Serialize)]
pub struct ShareResponse {
    pub share: CalendarShare,
    pub user: ShareUserInfo,
}

#[derive(Debug, Serialize)]
pub struct SharedCalendar {
    pub owner_id: Uuid,
    pub email: String,
    pub display_name: String,
    pub permission: SharePermission,
}

#[derive(Debug, Serialize)]
pub struct ShareUserInfo {
    pub id: Uuid,
    pub email: String,
    pub display_name: String,
}

// ── Helpers ────────────────────────────────────────────────────────

fn parse_date(s: &str) -> Result<NaiveDate, StatusCode> {
    NaiveDate::parse_from_str(s, "%Y-%m-%d").map_err(|_| StatusCode::BAD_REQUEST)
}

fn parse_time(s: &str) -> Result<chrono::NaiveTime, StatusCode> {
    chrono::NaiveTime::parse_from_str(s, "%H:%M").map_err(|_| StatusCode::BAD_REQUEST)
}

fn db_to_api(row: clawkson_db::calendar_event::CalendarEvent) -> CalendarEvent {
    CalendarEvent {
        id: row.id,
        owner_id: row.owner_id,
        title: row.title,
        date: row.date.format("%Y-%m-%d").to_string(),
        start_time: row.start_time.format("%H:%M").to_string(),
        end_time: row.end_time.format("%H:%M").to_string(),
        category: row.category,
        location: row.location,
        notes: row.notes,
        completed: row.completed,
        created_at: row.created_at,
        updated_at: row.updated_at,
    }
}

fn share_to_api(row: &clawkson_db::calendar_share::CalendarShareRow) -> CalendarShare {
    CalendarShare {
        id: row.id,
        owner_id: row.owner_id,
        shared_with: row.shared_with,
        permission: match row.permission {
            clawkson_db::share::SharePermission::Read => SharePermission::Read,
            clawkson_db::share::SharePermission::Write => SharePermission::Write,
        },
        created_at: row.created_at,
    }
}

/// Check if `viewer_id` can see `target_owner_id`'s calendar.
async fn can_view_calendar(
    state: &AppState,
    target_owner_id: Uuid,
    viewer_id: Uuid,
    is_admin: bool,
) -> Result<bool, StatusCode> {
    if target_owner_id == viewer_id || is_admin {
        return Ok(true);
    }
    let share = clawkson_db::calendar_share::get_share(&state.db, target_owner_id, viewer_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(share.is_some())
}

/// Check if `viewer_id` can write to `target_owner_id`'s calendar.
async fn can_write_calendar(
    state: &AppState,
    target_owner_id: Uuid,
    viewer_id: Uuid,
    is_admin: bool,
) -> Result<bool, StatusCode> {
    if target_owner_id == viewer_id || is_admin {
        return Ok(true);
    }
    let share = clawkson_db::calendar_share::get_share(&state.db, target_owner_id, viewer_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(matches!(
        share,
        Some(s) if s.permission == clawkson_db::share::SharePermission::Write
    ))
}

// ── Handlers ───────────────────────────────────────────────────────

/// GET /api/calendar — list events (own or shared calendar)
async fn list_events(
    auth: AuthUser,
    State(state): State<AppState>,
    Query(q): Query<ListEventsQuery>,
) -> Result<Json<Vec<CalendarEvent>>, StatusCode> {
    let target_owner = q.owner_id.unwrap_or_else(|| auth.id());

    // Verify access
    if !can_view_calendar(&state, target_owner, auth.id(), auth.is_admin()).await? {
        return Err(StatusCode::FORBIDDEN);
    }

    let rows = if let (Some(from_str), Some(to_str)) = (&q.from, &q.to) {
        let from = parse_date(from_str)?;
        let to = parse_date(to_str)?;
        clawkson_db::calendar_event::list_for_user(&state.db, target_owner, from, to)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
    } else {
        clawkson_db::calendar_event::list_all_for_user(&state.db, target_owner)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
    };

    Ok(Json(rows.into_iter().map(db_to_api).collect()))
}

/// POST /api/calendar — create event
async fn create_event(
    auth: AuthUser,
    State(state): State<AppState>,
    Json(req): Json<CreateEventRequest>,
) -> Result<(StatusCode, Json<CalendarEvent>), StatusCode> {
    let date = parse_date(&req.date)?;
    let start_time = parse_time(&req.start_time)?;
    let end_time = parse_time(&req.end_time)?;

    let row = clawkson_db::calendar_event::create(
        &state.db,
        auth.id(),
        &req.title,
        date,
        start_time,
        end_time,
        &req.category,
        req.location.as_deref(),
        req.notes.as_deref(),
    )
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok((StatusCode::CREATED, Json(db_to_api(row))))
}

/// GET /api/calendar/{id} — get single event
async fn get_event(
    auth: AuthUser,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<CalendarEvent>, StatusCode> {
    let row = clawkson_db::calendar_event::get_by_id(&state.db, id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;

    if !can_view_calendar(&state, row.owner_id, auth.id(), auth.is_admin()).await? {
        return Err(StatusCode::FORBIDDEN);
    }

    Ok(Json(db_to_api(row)))
}

/// PUT /api/calendar/{id} — update event
async fn update_event(
    auth: AuthUser,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(req): Json<UpdateEventRequest>,
) -> Result<Json<CalendarEvent>, StatusCode> {
    let existing = clawkson_db::calendar_event::get_by_id(&state.db, id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;

    if !can_write_calendar(&state, existing.owner_id, auth.id(), auth.is_admin()).await? {
        return Err(StatusCode::FORBIDDEN);
    }

    let date = parse_date(&req.date)?;
    let start_time = parse_time(&req.start_time)?;
    let end_time = parse_time(&req.end_time)?;

    let row = clawkson_db::calendar_event::update(
        &state.db,
        id,
        &req.title,
        date,
        start_time,
        end_time,
        &req.category,
        req.location.as_deref(),
        req.notes.as_deref(),
        req.completed,
    )
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
    .ok_or(StatusCode::NOT_FOUND)?;

    Ok(Json(db_to_api(row)))
}

/// DELETE /api/calendar/{id} — delete event
async fn delete_event(
    auth: AuthUser,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, StatusCode> {
    let existing = clawkson_db::calendar_event::get_by_id(&state.db, id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;

    if !can_write_calendar(&state, existing.owner_id, auth.id(), auth.is_admin()).await? {
        return Err(StatusCode::FORBIDDEN);
    }

    clawkson_db::calendar_event::delete(&state.db, id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(StatusCode::NO_CONTENT)
}

/// PATCH /api/calendar/{id}/complete — toggle completed
async fn toggle_complete(
    auth: AuthUser,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(req): Json<ToggleCompleteRequest>,
) -> Result<StatusCode, StatusCode> {
    let existing = clawkson_db::calendar_event::get_by_id(&state.db, id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;

    if !can_write_calendar(&state, existing.owner_id, auth.id(), auth.is_admin()).await? {
        return Err(StatusCode::FORBIDDEN);
    }

    clawkson_db::calendar_event::set_completed(&state.db, id, req.completed)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(StatusCode::NO_CONTENT)
}

// ── Calendar Sharing ───────────────────────────────────────────────

/// GET /api/calendar/shares — list who you've shared your calendar with
async fn list_shares(
    auth: AuthUser,
    State(state): State<AppState>,
) -> Result<Json<Vec<ShareResponse>>, StatusCode> {
    let shares = clawkson_db::calendar_share::list_for_owner(&state.db, auth.id())
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let mut result = Vec::new();
    for share in &shares {
        if let Ok(Some(user_row)) =
            clawkson_db::user::get_by_id(state.db.pool(), share.shared_with).await
        {
            result.push(ShareResponse {
                share: share_to_api(share),
                user: ShareUserInfo {
                    id: user_row.id,
                    email: user_row.email,
                    display_name: user_row.display_name,
                },
            });
        }
    }

    Ok(Json(result))
}

/// POST /api/calendar/shares — share your calendar with another user
async fn create_share(
    auth: AuthUser,
    State(state): State<AppState>,
    Json(req): Json<CreateShareRequest>,
) -> Result<Json<ShareResponse>, StatusCode> {
    let target_user = clawkson_db::user::get_by_email(state.db.pool(), &req.email)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;

    if target_user.id == auth.id() {
        return Err(StatusCode::BAD_REQUEST);
    }

    let db_permission = match req.permission {
        SharePermission::Read => clawkson_db::share::SharePermission::Read,
        SharePermission::Write => clawkson_db::share::SharePermission::Write,
    };

    let share_row =
        clawkson_db::calendar_share::create(&state.db, auth.id(), target_user.id, db_permission)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(ShareResponse {
        share: share_to_api(&share_row),
        user: ShareUserInfo {
            id: target_user.id,
            email: target_user.email,
            display_name: target_user.display_name,
        },
    }))
}

/// DELETE /api/calendar/shares/{user_id} — unshare your calendar
async fn remove_share(
    auth: AuthUser,
    State(state): State<AppState>,
    Path(user_id): Path<Uuid>,
) -> Result<StatusCode, StatusCode> {
    let deleted = clawkson_db::calendar_share::delete(&state.db, auth.id(), user_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    if deleted {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(StatusCode::NOT_FOUND)
    }
}

/// GET /api/calendar/shared — list calendars shared with you
async fn list_shared_calendars(
    auth: AuthUser,
    State(state): State<AppState>,
) -> Result<Json<Vec<SharedCalendar>>, StatusCode> {
    let shares = clawkson_db::calendar_share::list_shared_with(&state.db, auth.id())
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let mut result = Vec::new();
    for share in &shares {
        if let Ok(Some(user_row)) =
            clawkson_db::user::get_by_id(state.db.pool(), share.owner_id).await
        {
            result.push(SharedCalendar {
                owner_id: user_row.id,
                email: user_row.email,
                display_name: user_row.display_name,
                permission: match share.permission {
                    clawkson_db::share::SharePermission::Read => SharePermission::Read,
                    clawkson_db::share::SharePermission::Write => SharePermission::Write,
                },
            });
        }
    }

    Ok(Json(result))
}

/// GET /api/calendar/shared/{owner_id} — view another user's events
async fn list_shared_events(
    auth: AuthUser,
    State(state): State<AppState>,
    Path(owner_id): Path<Uuid>,
    Query(q): Query<ListEventsQuery>,
) -> Result<Json<Vec<CalendarEvent>>, StatusCode> {
    if !can_view_calendar(&state, owner_id, auth.id(), auth.is_admin()).await? {
        return Err(StatusCode::FORBIDDEN);
    }

    let rows = if let (Some(from_str), Some(to_str)) = (&q.from, &q.to) {
        let from = parse_date(from_str)?;
        let to = parse_date(to_str)?;
        clawkson_db::calendar_event::list_for_user(&state.db, owner_id, from, to)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
    } else {
        clawkson_db::calendar_event::list_all_for_user(&state.db, owner_id)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
    };

    Ok(Json(rows.into_iter().map(db_to_api).collect()))
}
