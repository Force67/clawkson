use axum::{
    extract::{Path, State},
    http::StatusCode,
    routing::get,
    Json, Router,
};
use clawkson_core::{User, UserRole};
use serde::Deserialize;
use uuid::Uuid;

use crate::auth::AuthUser;
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/users", get(list_users))
        .route("/users/{id}/role", axum::routing::patch(update_user_role))
        .route("/users/{id}", axum::routing::delete(delete_user))
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
        created_at: row.created_at,
        updated_at: row.updated_at,
    }
}
