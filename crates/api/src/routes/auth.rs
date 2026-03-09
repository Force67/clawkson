use axum::{
    extract::State,
    http::{header, StatusCode},
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use chrono::{Duration, Utc};
use clawkson_core::{User, UserRole};
use serde::{Deserialize, Serialize};

use crate::auth::{self, AuthUser, SESSION_COOKIE, SESSION_DURATION_DAYS};
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/register", post(register))
        .route("/login", post(login))
        .route("/logout", post(logout))
        .route("/me", get(me))
}

#[derive(Debug, Deserialize)]
pub struct RegisterRequest {
    pub email: String,
    pub password: String,
    pub display_name: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct LoginRequest {
    pub email: String,
    pub password: String,
}

#[derive(Debug, Serialize)]
pub struct AuthResponse {
    pub user: User,
}

/// POST /api/auth/register
/// First user becomes admin automatically.
async fn register(
    State(state): State<AppState>,
    Json(req): Json<RegisterRequest>,
) -> impl IntoResponse {
    let pool = state.db.pool();

    // Validate
    let email = req.email.trim().to_lowercase();
    if email.is_empty() || !email.contains('@') {
        return (StatusCode::BAD_REQUEST, Json(serde_json::json!({"error": "Invalid email"}))).into_response();
    }
    if req.password.len() < 6 {
        return (StatusCode::BAD_REQUEST, Json(serde_json::json!({"error": "Password must be at least 6 characters"}))).into_response();
    }

    // Check if email taken
    if let Ok(Some(_)) = clawkson_db::user::get_by_email(pool, &email).await {
        return (StatusCode::CONFLICT, Json(serde_json::json!({"error": "Email already registered"}))).into_response();
    }

    // Hash password
    let password_hash = match auth::hash_password(&req.password) {
        Ok(h) => h,
        Err(_) => return (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": "Failed to hash password"}))).into_response(),
    };

    let display_name = req.display_name.unwrap_or_else(|| email.split('@').next().unwrap_or("").to_string());

    // Atomically assigns admin role to the very first user (race-safe)
    let user_row = match clawkson_db::user::create_first_user_aware(pool, &email, &display_name, &password_hash).await {
        Ok(u) => u,
        Err(e) => {
            tracing::error!("Failed to create user: {e}");
            return (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": "Failed to create user"}))).into_response();
        }
    };

    // Create session
    let token = auth::generate_session_token();
    let expires_at = Utc::now() + Duration::days(SESSION_DURATION_DAYS);
    if let Err(e) = clawkson_db::session::create(pool, &token, user_row.id, expires_at).await {
        tracing::error!("Failed to create session: {e}");
        return (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": "Failed to create session"}))).into_response();
    }

    let user = row_to_user(&user_row);
    let cookie = format!("{SESSION_COOKIE}={token}; Path=/; HttpOnly; SameSite=Lax; Max-Age={}", SESSION_DURATION_DAYS * 86400);

    (
        StatusCode::CREATED,
        [(header::SET_COOKIE, cookie)],
        Json(AuthResponse { user }),
    ).into_response()
}

/// POST /api/auth/login
async fn login(
    State(state): State<AppState>,
    Json(req): Json<LoginRequest>,
) -> impl IntoResponse {
    let pool = state.db.pool();
    let email = req.email.trim().to_lowercase();

    let user_row = match clawkson_db::user::get_by_email(pool, &email).await {
        Ok(Some(u)) => u,
        _ => return (StatusCode::UNAUTHORIZED, Json(serde_json::json!({"error": "Invalid credentials"}))).into_response(),
    };

    if !auth::verify_password(&req.password, &user_row.password_hash) {
        return (StatusCode::UNAUTHORIZED, Json(serde_json::json!({"error": "Invalid credentials"}))).into_response();
    }

    // Purge expired sessions opportunistically
    let _ = clawkson_db::session::purge_expired(pool).await;

    // Create session
    let token = auth::generate_session_token();
    let expires_at = Utc::now() + Duration::days(SESSION_DURATION_DAYS);
    if let Err(e) = clawkson_db::session::create(pool, &token, user_row.id, expires_at).await {
        tracing::error!("Failed to create session: {e}");
        return (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": "Failed to create session"}))).into_response();
    }

    let user = row_to_user(&user_row);
    let cookie = format!("{SESSION_COOKIE}={token}; Path=/; HttpOnly; SameSite=Lax; Max-Age={}", SESSION_DURATION_DAYS * 86400);

    (
        [(header::SET_COOKIE, cookie)],
        Json(AuthResponse { user }),
    ).into_response()
}

/// POST /api/auth/logout
async fn logout(
    State(state): State<AppState>,
    headers: axum::http::HeaderMap,
) -> impl IntoResponse {
    let pool = state.db.pool();

    if let Some(token) = extract_token(&headers) {
        let _ = clawkson_db::session::delete(pool, token).await;
    }

    let cookie = format!("{SESSION_COOKIE}=; Path=/; HttpOnly; SameSite=Lax; Max-Age=0");
    ([(header::SET_COOKIE, cookie)], StatusCode::NO_CONTENT).into_response()
}

/// GET /api/auth/me
async fn me(auth: AuthUser) -> Json<AuthResponse> {
    Json(AuthResponse { user: auth.0 })
}

fn extract_token(headers: &axum::http::HeaderMap) -> Option<&str> {
    headers
        .get(header::COOKIE)
        .and_then(|v| v.to_str().ok())
        .and_then(|cookies| {
            cookies
                .split(';')
                .find_map(|c| {
                    let c = c.trim();
                    c.strip_prefix(&format!("{SESSION_COOKIE}="))
                })
        })
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
