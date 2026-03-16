use chrono::{DateTime, Utc};
use sqlx::PgPool;
use uuid::Uuid;

#[derive(Debug, Clone, sqlx::Type, PartialEq, Eq)]
#[sqlx(type_name = "user_role", rename_all = "snake_case")]
pub enum UserRole {
    Admin,
    User,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct UserRow {
    pub id: Uuid,
    pub email: String,
    pub display_name: String,
    pub password_hash: String,
    pub role: UserRole,
    /// Free-text context about the user that agents can read.
    pub bio: String,
    /// URL of the user's avatar image (data URL or remote URL).
    pub avatar_url: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

pub async fn create(
    pool: &PgPool,
    email: &str,
    display_name: &str,
    password_hash: &str,
    role: UserRole,
) -> Result<UserRow, sqlx::Error> {
    sqlx::query_as::<_, UserRow>(
        r#"
        INSERT INTO users (email, display_name, password_hash, role)
        VALUES ($1, $2, $3, $4)
        RETURNING *
        "#,
    )
    .bind(email)
    .bind(display_name)
    .bind(password_hash)
    .bind(role)
    .fetch_one(pool)
    .await
}

/// Atomically create the first user as admin.
/// Uses a single query that checks if any users exist and assigns the role
/// accordingly, preventing race conditions where two concurrent registrations
/// could both become admin.
pub async fn create_first_user_aware(
    pool: &PgPool,
    email: &str,
    display_name: &str,
    password_hash: &str,
) -> Result<UserRow, sqlx::Error> {
    sqlx::query_as::<_, UserRow>(
        r#"
        INSERT INTO users (email, display_name, password_hash, role)
        VALUES (
            $1, $2, $3,
            CASE WHEN (SELECT COUNT(*) FROM users) = 0 THEN 'admin'::user_role
                 ELSE 'user'::user_role
            END
        )
        RETURNING *
        "#,
    )
    .bind(email)
    .bind(display_name)
    .bind(password_hash)
    .fetch_one(pool)
    .await
}

pub async fn get_by_id(pool: &PgPool, id: Uuid) -> Result<Option<UserRow>, sqlx::Error> {
    sqlx::query_as::<_, UserRow>("SELECT * FROM users WHERE id = $1")
        .bind(id)
        .fetch_optional(pool)
        .await
}

pub async fn get_by_email(pool: &PgPool, email: &str) -> Result<Option<UserRow>, sqlx::Error> {
    sqlx::query_as::<_, UserRow>("SELECT * FROM users WHERE lower(email) = lower($1)")
        .bind(email)
        .fetch_optional(pool)
        .await
}

pub async fn list_all(pool: &PgPool) -> Result<Vec<UserRow>, sqlx::Error> {
    sqlx::query_as::<_, UserRow>("SELECT * FROM users ORDER BY created_at ASC")
        .fetch_all(pool)
        .await
}

pub async fn count(pool: &PgPool) -> Result<i64, sqlx::Error> {
    let row: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM users")
        .fetch_one(pool)
        .await?;
    Ok(row.0)
}

pub async fn update_role(pool: &PgPool, id: Uuid, role: UserRole) -> Result<Option<UserRow>, sqlx::Error> {
    sqlx::query_as::<_, UserRow>(
        "UPDATE users SET role = $2, updated_at = now() WHERE id = $1 RETURNING *",
    )
    .bind(id)
    .bind(role)
    .fetch_optional(pool)
    .await
}

pub async fn update_display_name(pool: &PgPool, id: Uuid, name: &str) -> Result<Option<UserRow>, sqlx::Error> {
    sqlx::query_as::<_, UserRow>(
        "UPDATE users SET display_name = $2, updated_at = now() WHERE id = $1 RETURNING *",
    )
    .bind(id)
    .bind(name)
    .fetch_optional(pool)
    .await
}

pub struct UpdateProfile<'a> {
    pub display_name: Option<&'a str>,
    pub bio: Option<&'a str>,
    pub avatar_url: Option<&'a str>,
}

/// Patch any combination of profile fields for the given user.
pub async fn update_profile(pool: &PgPool, id: Uuid, req: UpdateProfile<'_>) -> Result<Option<UserRow>, sqlx::Error> {
    // Build dynamic SET clause only for provided fields.
    let mut parts: Vec<String> = vec![];
    let mut idx = 2u32;

    if req.display_name.is_some() { parts.push(format!("display_name = ${idx}")); idx += 1; }
    if req.bio.is_some()          { parts.push(format!("bio = ${idx}"));           idx += 1; }
    if req.avatar_url.is_some()   { parts.push(format!("avatar_url = ${idx}"));    idx += 1; }

    if parts.is_empty() {
        // Nothing to update — just return current row.
        return sqlx::query_as::<_, UserRow>("SELECT * FROM users WHERE id = $1")
            .bind(id)
            .fetch_optional(pool)
            .await;
    }

    let sql = format!(
        "UPDATE users SET {}, updated_at = now() WHERE id = $1 RETURNING *",
        parts.join(", ")
    );

    let mut q = sqlx::query_as::<_, UserRow>(&sql).bind(id);
    if let Some(v) = req.display_name { q = q.bind(v); }
    if let Some(v) = req.bio          { q = q.bind(v); }
    if let Some(v) = req.avatar_url   { q = q.bind(v); }

    q.fetch_optional(pool).await
}

pub async fn update_password(pool: &PgPool, id: Uuid, password_hash: &str) -> Result<bool, sqlx::Error> {
    let result = sqlx::query(
        "UPDATE users SET password_hash = $2, updated_at = now() WHERE id = $1",
    )
    .bind(id)
    .bind(password_hash)
    .execute(pool)
    .await?;
    Ok(result.rows_affected() > 0)
}

pub async fn delete(pool: &PgPool, id: Uuid) -> Result<bool, sqlx::Error> {
    let result = sqlx::query("DELETE FROM users WHERE id = $1")
        .bind(id)
        .execute(pool)
        .await?;
    Ok(result.rows_affected() > 0)
}
