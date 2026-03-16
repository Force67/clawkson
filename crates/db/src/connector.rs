use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;

use crate::{Db, DbError};

// ── DB row type ────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, sqlx::Type)]
#[sqlx(type_name = "connector_type", rename_all = "snake_case")]
pub enum ConnectorType {
    Telegram,
    Gmail,
    Slack,
    AzureDevops,
    Custom,
}

#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct ConnectorRow {
    pub id: Uuid,
    pub user_id: Uuid,
    pub name: String,
    pub connector_type: ConnectorType,
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
    pub connector_type: ConnectorType,
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

/// List all enabled connectors of a given type (across all users).
pub async fn list_enabled_by_type(db: &Db, ct: ConnectorType) -> Result<Vec<ConnectorRow>, DbError> {
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

pub async fn delete(db: &Db, id: Uuid, user_id: Uuid) -> Result<bool, DbError> {
    let result = sqlx::query("DELETE FROM connectors WHERE id = $1 AND user_id = $2")
        .bind(id)
        .bind(user_id)
        .execute(db.pool())
        .await?;
    Ok(result.rows_affected() > 0)
}
