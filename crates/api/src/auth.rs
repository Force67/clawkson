use axum::{
    extract::FromRequestParts,
    http::{header, request::Parts, StatusCode},
};
use clawkson_core::{User, UserRole};
use uuid::Uuid;

use crate::state::AppState;

/// Session cookie name.
pub const SESSION_COOKIE: &str = "clawkson_session";

/// Session duration in days.
pub const SESSION_DURATION_DAYS: i64 = 30;

/// Extract the authenticated user from the session cookie.
/// Returns 401 if no valid session.
pub struct AuthUser(pub User);

impl AuthUser {
    pub fn id(&self) -> Uuid {
        self.0.id
    }

    pub fn is_admin(&self) -> bool {
        self.0.role == UserRole::Admin
    }
}

impl<S: Send + Sync> FromRequestParts<S> for AuthUser
where
    AppState: FromRef<S>,
{
    type Rejection = StatusCode;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let app_state = AppState::from_ref(state);

        // Extract session token from cookie header
        let token = parts
            .headers
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
            .ok_or(StatusCode::UNAUTHORIZED)?;

        // Validate session in DB
        let pool = app_state.db.pool();
        let session = clawkson_db::session::get_valid(pool, token)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
            .ok_or(StatusCode::UNAUTHORIZED)?;

        // Load user
        let user_row = clawkson_db::user::get_by_id(pool, session.user_id)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
            .ok_or(StatusCode::UNAUTHORIZED)?;

        let user = User {
            id: user_row.id,
            email: user_row.email,
            display_name: user_row.display_name,
            password_hash: String::new(), // never expose
            role: match user_row.role {
                clawkson_db::user::UserRole::Admin => UserRole::Admin,
                clawkson_db::user::UserRole::User => UserRole::User,
            },
            created_at: user_row.created_at,
            updated_at: user_row.updated_at,
        };

        Ok(AuthUser(user))
    }
}

/// Optional auth — extracts user if session present, None otherwise.
pub struct MaybeAuthUser(pub Option<User>);

impl<S: Send + Sync> FromRequestParts<S> for MaybeAuthUser
where
    AppState: FromRef<S>,
{
    type Rejection = StatusCode;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        match AuthUser::from_request_parts(parts, state).await {
            Ok(auth) => Ok(MaybeAuthUser(Some(auth.0))),
            Err(_) => Ok(MaybeAuthUser(None)),
        }
    }
}

// Trait for FromRef to extract AppState from state
pub trait FromRef<S> {
    fn from_ref(state: &S) -> Self;
}

impl FromRef<AppState> for AppState {
    fn from_ref(state: &AppState) -> Self {
        state.clone()
    }
}

/// Generate a cryptographically random session token.
pub fn generate_session_token() -> String {
    use rand::Rng;
    let mut rng = rand::thread_rng();
    let bytes: [u8; 32] = rng.gen();
    hex::encode(bytes)
}

/// Hash a password using Argon2.
pub fn hash_password(password: &str) -> Result<String, argon2::password_hash::Error> {
    use argon2::{
        password_hash::{rand_core::OsRng, SaltString},
        Argon2, PasswordHasher,
    };
    let salt = SaltString::generate(&mut OsRng);
    let argon2 = Argon2::default();
    let hash = argon2.hash_password(password.as_bytes(), &salt)?;
    Ok(hash.to_string())
}

/// Verify a password against a hash.
pub fn verify_password(password: &str, hash: &str) -> bool {
    use argon2::{Argon2, PasswordHash, PasswordVerifier};
    let Ok(parsed) = PasswordHash::new(hash) else {
        return false;
    };
    Argon2::default().verify_password(password.as_bytes(), &parsed).is_ok()
}
