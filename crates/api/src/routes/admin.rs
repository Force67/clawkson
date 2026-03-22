use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    routing::get,
    Json, Router,
};
use chrono::{Duration, Utc};
use clawkson_core::{LlmAccessEntry, ModelPricing, TokenUsageSummary, UsageTimeBucket, User, UserRole, UserTokenUsage};
use serde::Deserialize;
use uuid::Uuid;

use crate::auth::AuthUser;
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/users", get(list_users))
        .route("/users/{id}/role", axum::routing::patch(update_user_role))
        .route("/users/{id}", axum::routing::delete(delete_user))
        .route(
            "/llm-connectors/{id}/access",
            get(get_connector_access).put(set_connector_access),
        )
        .route("/usage", get(get_usage))
        .route("/usage/time-series", get(get_usage_time_series))
        .route("/usage/{user_id}", get(get_user_usage))
        .route(
            "/pricing",
            get(list_pricing).post(upsert_pricing),
        )
        .route("/pricing/{id}", axum::routing::delete(delete_pricing))
}

/// GET /api/admin/users — list all users (admin only)
async fn list_users(
    auth: AuthUser,
    State(state): State<AppState>,
) -> Result<Json<Vec<User>>, StatusCode> {
    if !auth.is_admin() {
        return Err(StatusCode::FORBIDDEN);
    }

    let pool = state.db.pool();
    let rows = clawkson_db::user::list_all(pool)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let users: Vec<User> = rows.iter().map(row_to_user).collect();
    Ok(Json(users))
}

#[derive(Debug, Deserialize)]
pub struct UpdateRoleRequest {
    pub role: UserRole,
}

/// PATCH /api/admin/users/{id}/role — change user role (admin only)
async fn update_user_role(
    auth: AuthUser,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(req): Json<UpdateRoleRequest>,
) -> Result<Json<User>, StatusCode> {
    if !auth.is_admin() {
        return Err(StatusCode::FORBIDDEN);
    }

    // Prevent admin from demoting themselves
    if id == auth.id() && req.role == UserRole::User {
        return Err(StatusCode::BAD_REQUEST);
    }

    let pool = state.db.pool();
    let db_role = match req.role {
        UserRole::Admin => clawkson_db::user::UserRole::Admin,
        UserRole::User => clawkson_db::user::UserRole::User,
    };

    let row = clawkson_db::user::update_role(pool, id, db_role)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;

    Ok(Json(row_to_user(&row)))
}

/// DELETE /api/admin/users/{id} — delete a user (admin only)
async fn delete_user(
    auth: AuthUser,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> StatusCode {
    if !auth.is_admin() {
        return StatusCode::FORBIDDEN;
    }

    // Prevent admin from deleting themselves
    if id == auth.id() {
        return StatusCode::BAD_REQUEST;
    }

    let pool = state.db.pool();
    match clawkson_db::user::delete(pool, id).await {
        Ok(true) => StatusCode::NO_CONTENT,
        Ok(false) => StatusCode::NOT_FOUND,
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR,
    }
}

// ── LLM Connector Access ────────────────────────────────────────

/// GET /api/admin/llm-connectors/{id}/access — list users with access
async fn get_connector_access(
    auth: AuthUser,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<Vec<LlmAccessEntry>>, StatusCode> {
    if !auth.is_admin() {
        return Err(StatusCode::FORBIDDEN);
    }

    let user_ids = clawkson_db::llm_connector::list_access(&state.db, id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let pool = state.db.pool();
    let mut entries = Vec::with_capacity(user_ids.len());
    for uid in user_ids {
        if let Ok(Some(row)) = clawkson_db::user::get_by_id(pool, uid).await {
            entries.push(LlmAccessEntry {
                user_id: row.id,
                email: row.email,
                display_name: row.display_name,
            });
        }
    }

    Ok(Json(entries))
}

#[derive(Debug, Deserialize)]
pub struct SetAccessRequest {
    pub user_ids: Vec<Uuid>,
}

/// PUT /api/admin/llm-connectors/{id}/access — replace access list
async fn set_connector_access(
    auth: AuthUser,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(req): Json<SetAccessRequest>,
) -> StatusCode {
    if !auth.is_admin() {
        return StatusCode::FORBIDDEN;
    }

    // Verify connector exists
    match clawkson_db::llm_connector::get_by_id(&state.db, id).await {
        Ok(Some(_)) => {}
        Ok(None) => return StatusCode::NOT_FOUND,
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR,
    }

    match clawkson_db::llm_connector::set_access(&state.db, id, &req.user_ids).await {
        Ok(()) => StatusCode::NO_CONTENT,
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR,
    }
}

// ── Token Usage ────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct UsageQuery {
    /// Time range filter: "24h", "7d", "30d", or omit for all time.
    pub since: Option<String>,
}

fn parse_since(s: &str) -> Option<chrono::DateTime<Utc>> {
    let now = Utc::now();
    match s {
        "24h" => Some(now - Duration::hours(24)),
        "7d" => Some(now - Duration::days(7)),
        "30d" => Some(now - Duration::days(30)),
        _ => None,
    }
}

/// GET /api/admin/usage — per-user, per-model token usage summary
async fn get_usage(
    auth: AuthUser,
    State(state): State<AppState>,
    Query(q): Query<UsageQuery>,
) -> Result<Json<Vec<UserTokenUsage>>, StatusCode> {
    if !auth.is_admin() {
        return Err(StatusCode::FORBIDDEN);
    }

    let since = q.since.as_deref().and_then(parse_since);
    let rows = clawkson_db::token_usage::get_all_users_summary(&state.db, since)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    // Group by user_id
    let mut map: std::collections::HashMap<Uuid, UserTokenUsage> =
        std::collections::HashMap::new();
    for row in rows {
        let entry = map.entry(row.user_id).or_insert_with(|| UserTokenUsage {
            user_id: row.user_id,
            email: row.email.clone(),
            display_name: row.display_name.clone(),
            models: Vec::new(),
        });
        entry.models.push(TokenUsageSummary {
            model: row.model,
            prompt_tokens: row.prompt_tokens,
            completion_tokens: row.completion_tokens,
            total_tokens: row.total_tokens,
        });
    }

    let mut result: Vec<UserTokenUsage> = map.into_values().collect();
    result.sort_by(|a, b| {
        let a_total: i64 = a.models.iter().map(|m| m.total_tokens).sum();
        let b_total: i64 = b.models.iter().map(|m| m.total_tokens).sum();
        b_total.cmp(&a_total)
    });

    Ok(Json(result))
}

/// GET /api/admin/usage/{user_id} — single user's usage breakdown
async fn get_user_usage(
    auth: AuthUser,
    State(state): State<AppState>,
    Path(user_id): Path<Uuid>,
    Query(q): Query<UsageQuery>,
) -> Result<Json<Vec<TokenUsageSummary>>, StatusCode> {
    if !auth.is_admin() {
        return Err(StatusCode::FORBIDDEN);
    }

    let since = q.since.as_deref().and_then(parse_since);
    let rows = clawkson_db::token_usage::get_user_summary(&state.db, user_id, since)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let summaries: Vec<TokenUsageSummary> = rows
        .into_iter()
        .map(|r| TokenUsageSummary {
            model: r.model,
            prompt_tokens: r.prompt_tokens,
            completion_tokens: r.completion_tokens,
            total_tokens: r.total_tokens,
        })
        .collect();

    Ok(Json(summaries))
}

// ── Time-series Usage ──────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct TimeSeriesQuery {
    pub since: Option<String>,
    pub bucket: Option<String>,
}

/// GET /api/admin/usage/time-series — time-bucketed usage with cost
async fn get_usage_time_series(
    auth: AuthUser,
    State(state): State<AppState>,
    Query(q): Query<TimeSeriesQuery>,
) -> Result<Json<Vec<UsageTimeBucket>>, StatusCode> {
    if !auth.is_admin() {
        return Err(StatusCode::FORBIDDEN);
    }

    let since = q
        .since
        .as_deref()
        .and_then(parse_since)
        .unwrap_or_else(|| Utc::now() - Duration::days(7));
    let bucket = q.bucket.as_deref().unwrap_or("day");

    let rows = clawkson_db::token_usage::get_time_series(&state.db, None, since, bucket)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let buckets: Vec<UsageTimeBucket> = rows
        .into_iter()
        .map(|r| UsageTimeBucket {
            bucket: r.bucket,
            model: r.model,
            prompt_tokens: r.prompt_tokens,
            completion_tokens: r.completion_tokens,
            total_tokens: r.total_tokens,
            estimated_cost_usd: r.estimated_cost_usd,
        })
        .collect();

    Ok(Json(buckets))
}

// ── Model Pricing ──────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct UpsertPricingRequest {
    pub model: String,
    pub prompt_cost_per_million: f64,
    pub completion_cost_per_million: f64,
}

/// GET /api/admin/pricing — list all model pricing
async fn list_pricing(
    auth: AuthUser,
    State(state): State<AppState>,
) -> Result<Json<Vec<ModelPricing>>, StatusCode> {
    if !auth.is_admin() {
        return Err(StatusCode::FORBIDDEN);
    }

    let rows = clawkson_db::model_pricing::list(&state.db)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let pricing: Vec<ModelPricing> = rows
        .into_iter()
        .map(|r| ModelPricing {
            id: r.id,
            model: r.model,
            prompt_cost_per_million: r.prompt_cost_per_million,
            completion_cost_per_million: r.completion_cost_per_million,
            created_at: r.created_at,
            updated_at: r.updated_at,
        })
        .collect();

    Ok(Json(pricing))
}

/// POST /api/admin/pricing — upsert model pricing
async fn upsert_pricing(
    auth: AuthUser,
    State(state): State<AppState>,
    Json(req): Json<UpsertPricingRequest>,
) -> Result<Json<ModelPricing>, StatusCode> {
    if !auth.is_admin() {
        return Err(StatusCode::FORBIDDEN);
    }

    let row = clawkson_db::model_pricing::upsert(
        &state.db,
        &req.model,
        req.prompt_cost_per_million,
        req.completion_cost_per_million,
    )
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(ModelPricing {
        id: row.id,
        model: row.model,
        prompt_cost_per_million: row.prompt_cost_per_million,
        completion_cost_per_million: row.completion_cost_per_million,
        created_at: row.created_at,
        updated_at: row.updated_at,
    }))
}

/// DELETE /api/admin/pricing/{id} — delete a pricing entry
async fn delete_pricing(
    auth: AuthUser,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> StatusCode {
    if !auth.is_admin() {
        return StatusCode::FORBIDDEN;
    }

    match clawkson_db::model_pricing::delete(&state.db, id).await {
        Ok(true) => StatusCode::NO_CONTENT,
        Ok(false) => StatusCode::NOT_FOUND,
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR,
    }
}

fn row_to_user(row: &clawkson_db::user::UserRow) -> User {
    User {
        id: row.id,
        email: row.email.clone(),
        display_name: row.display_name.clone(),
        password_hash: String::new(),
        role: match row.role {
            clawkson_db::user::UserRole::Admin => UserRole::Admin,
            clawkson_db::user::UserRole::User => UserRole::User,
        },
        bio: row.bio.clone(),
        avatar_url: row.avatar_url.clone(),
        created_at: row.created_at,
        updated_at: row.updated_at,
    }
}
