use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;

use crate::{Db, DbError};

// ── Well-known connector types ────────────────────────────────────
// Stored as TEXT in PostgreSQL — plugins can register additional types.

pub const TELEGRAM: &str = "telegram";
pub const GMAIL: &str = "gmail";
pub const SLACK: &str = "slack";
pub const AZURE_DEVOPS: &str = "azure_devops";
pub const CUSTOM: &str = "custom";
pub const TAVILY: &str = "tavily";
pub const BING: &str = "bing";

/// Check if a connector type is a web search type.
pub fn is_web_search_type(ct: &str) -> bool {
    matches!(ct, TAVILY | BING)
}

#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct ConnectorRow {
    pub id: Uuid,
    pub user_id: Uuid,
    pub name: String,
    pub connector_type: String,
    pub enabled: bool,
    pub config: serde_json::Value,
    /// Free-text operational context injected when this connector is invoked.
    pub context: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

// ── Queries ────────────────────────────────────────────────────────

pub async fn list_for_user(db: &Db, user_id: Uuid) -> Result<Vec<ConnectorRow>, DbError> {
    let rows = sqlx::query_as::<_, ConnectorRow>(
        "SELECT id, user_id, name, connector_type, enabled, config, context, created_at, updated_at
         FROM connectors
         WHERE user_id = $1
         ORDER BY created_at ASC",
    )
    .bind(user_id)
    .fetch_all(db.pool())
    .await?;
    Ok(rows)
}

pub async fn get(db: &Db, id: Uuid, user_id: Uuid) -> Result<Option<ConnectorRow>, DbError> {
    let row = sqlx::query_as::<_, ConnectorRow>(
        "SELECT id, user_id, name, connector_type, enabled, config, context, created_at, updated_at
         FROM connectors
         WHERE id = $1 AND user_id = $2",
    )
    .bind(id)
    .bind(user_id)
    .fetch_optional(db.pool())
    .await?;
    Ok(row)
}

pub struct CreateConnector {
    pub user_id: Uuid,
    pub name: String,
    pub connector_type: String,
    pub config: serde_json::Value,
}

pub async fn create(db: &Db, req: CreateConnector) -> Result<ConnectorRow, DbError> {
    let row = sqlx::query_as::<_, ConnectorRow>(
        "INSERT INTO connectors (user_id, name, connector_type, config)
         VALUES ($1, $2, $3, $4)
         RETURNING id, user_id, name, connector_type, enabled, config, context, created_at, updated_at",
    )
    .bind(req.user_id)
    .bind(req.name)
    .bind(req.connector_type)
    .bind(req.config)
    .fetch_one(db.pool())
    .await?;
    Ok(row)
}

pub async fn set_enabled(
    db: &Db,
    id: Uuid,
    user_id: Uuid,
    enabled: bool,
) -> Result<Option<ConnectorRow>, DbError> {
    let row = sqlx::query_as::<_, ConnectorRow>(
        "UPDATE connectors
         SET enabled = $3, updated_at = now()
         WHERE id = $1 AND user_id = $2
         RETURNING id, user_id, name, connector_type, enabled, config, context, created_at, updated_at",
    )
    .bind(id)
    .bind(user_id)
    .bind(enabled)
    .fetch_optional(db.pool())
    .await?;
    Ok(row)
}

/// Update the free-text context for a connector.
pub async fn set_context(
    db: &Db,
    id: Uuid,
    user_id: Uuid,
    context: &str,
) -> Result<Option<ConnectorRow>, DbError> {
    let row = sqlx::query_as::<_, ConnectorRow>(
        "UPDATE connectors
         SET context = $3, updated_at = now()
         WHERE id = $1 AND user_id = $2
         RETURNING id, user_id, name, connector_type, enabled, config, context, created_at, updated_at",
    )
    .bind(id)
    .bind(user_id)
    .bind(context)
    .fetch_optional(db.pool())
    .await?;
    Ok(row)
}

/// Update the name of a connector.
pub async fn set_name(
    db: &Db,
    id: Uuid,
    user_id: Uuid,
    name: &str,
) -> Result<Option<ConnectorRow>, DbError> {
    let row = sqlx::query_as::<_, ConnectorRow>(
        "UPDATE connectors
         SET name = $3, updated_at = now()
         WHERE id = $1 AND user_id = $2
         RETURNING id, user_id, name, connector_type, enabled, config, context, created_at, updated_at",
    )
    .bind(id)
    .bind(user_id)
    .bind(name)
    .fetch_optional(db.pool())
    .await?;
    Ok(row)
}

/// Replace the entire config JSON blob for a connector.
pub async fn set_config(
    db: &Db,
    id: Uuid,
    user_id: Uuid,
    config: &serde_json::Value,
) -> Result<Option<ConnectorRow>, DbError> {
    let row = sqlx::query_as::<_, ConnectorRow>(
        "UPDATE connectors
         SET config = $3, updated_at = now()
         WHERE id = $1 AND user_id = $2
         RETURNING id, user_id, name, connector_type, enabled, config, context, created_at, updated_at",
    )
    .bind(id)
    .bind(user_id)
    .bind(config)
    .fetch_optional(db.pool())
    .await?;
    Ok(row)
}

/// List all enabled connectors of a given type (across all users).
pub async fn list_enabled_by_type(db: &Db, ct: &str) -> Result<Vec<ConnectorRow>, DbError> {
    let rows = sqlx::query_as::<_, ConnectorRow>(
        "SELECT id, user_id, name, connector_type, enabled, config, context, created_at, updated_at
         FROM connectors
         WHERE connector_type = $1 AND enabled = TRUE
         ORDER BY created_at ASC",
    )
    .bind(ct)
    .fetch_all(db.pool())
    .await?;
    Ok(rows)
}

pub async fn get_by_id(db: &Db, id: Uuid) -> Result<Option<ConnectorRow>, DbError> {
    let row = sqlx::query_as::<_, ConnectorRow>(
        "SELECT id, user_id, name, connector_type, enabled, config, context, created_at, updated_at
         FROM connectors
         WHERE id = $1",
    )
    .bind(id)
    .fetch_optional(db.pool())
    .await?;
    Ok(row)
}

/// Disable all web search connectors for a user except the one with the given id.
pub async fn disable_other_web_search(db: &Db, user_id: Uuid, keep_id: Uuid) -> Result<u64, DbError> {
    let result = sqlx::query(
        "UPDATE connectors
         SET enabled = FALSE, updated_at = now()
         WHERE user_id = $1
           AND id != $2
           AND connector_type IN ('tavily', 'bing')
           AND enabled = TRUE",
    )
    .bind(user_id)
    .bind(keep_id)
    .execute(db.pool())
    .await?;
    Ok(result.rows_affected())
}

pub async fn delete(db: &Db, id: Uuid, user_id: Uuid) -> Result<bool, DbError> {
    let result = sqlx::query("DELETE FROM connectors WHERE id = $1 AND user_id = $2")
        .bind(id)
        .bind(user_id)
        .execute(db.pool())
        .await?;
    Ok(result.rows_affected() > 0)
}
