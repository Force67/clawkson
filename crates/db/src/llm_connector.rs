use chrono::{DateTime, Utc};
use sqlx::FromRow;
use uuid::Uuid;

use crate::{Db, DbError};

#[derive(Debug, Clone, PartialEq, Eq, sqlx::Type)]
#[sqlx(type_name = "llm_provider_type", rename_all = "lowercase")]
pub enum LlmProviderType {
    Azure,
    Openrouter,
    Openai,
    Custom,
}

#[derive(Debug, Clone, FromRow)]
pub struct LlmConnectorRow {
    pub id: Uuid,
    pub name: String,
    pub provider_type: LlmProviderType,
    pub api_key: String,
    pub api_base_url: String,
    pub model: String,
    pub azure_deployment: Option<String>,
    pub azure_api_version: Option<String>,
    pub shared_with_all: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

pub async fn create(
    db: &Db,
    name: &str,
    provider_type: LlmProviderType,
    api_key: &str,
    api_base_url: &str,
    model: &str,
    azure_deployment: Option<&str>,
    azure_api_version: Option<&str>,
) -> Result<LlmConnectorRow, DbError> {
    let row = sqlx::query_as::<_, LlmConnectorRow>(
        "INSERT INTO llm_connectors (name, provider_type, api_key, api_base_url, model, azure_deployment, azure_api_version)
         VALUES ($1, $2, $3, $4, $5, $6, $7)
         RETURNING *",
    )
    .bind(name)
    .bind(provider_type)
    .bind(api_key)
    .bind(api_base_url)
    .bind(model)
    .bind(azure_deployment)
    .bind(azure_api_version)
    .fetch_one(db.pool())
    .await?;
    Ok(row)
}

pub async fn get_by_id(db: &Db, id: Uuid) -> Result<Option<LlmConnectorRow>, DbError> {
    let row = sqlx::query_as::<_, LlmConnectorRow>(
        "SELECT * FROM llm_connectors WHERE id = $1",
    )
    .bind(id)
    .fetch_optional(db.pool())
    .await?;
    Ok(row)
}

pub async fn list_all(db: &Db) -> Result<Vec<LlmConnectorRow>, DbError> {
    let rows = sqlx::query_as::<_, LlmConnectorRow>(
        "SELECT * FROM llm_connectors ORDER BY created_at",
    )
    .fetch_all(db.pool())
    .await?;
    Ok(rows)
}

pub async fn update(
    db: &Db,
    id: Uuid,
    name: Option<&str>,
    provider_type: Option<LlmProviderType>,
    api_key: Option<&str>,
    api_base_url: Option<&str>,
    model: Option<&str>,
    azure_deployment: Option<Option<&str>>,
    azure_api_version: Option<Option<&str>>,
) -> Result<Option<LlmConnectorRow>, DbError> {
    // Fetch-then-update pattern (same as agents)
    let existing = match get_by_id(db, id).await? {
        Some(r) => r,
        None => return Ok(None),
    };

    let row = sqlx::query_as::<_, LlmConnectorRow>(
        "UPDATE llm_connectors
         SET name = $2,
             provider_type = $3,
             api_key = $4,
             api_base_url = $5,
             model = $6,
             azure_deployment = $7,
             azure_api_version = $8,
             updated_at = now()
         WHERE id = $1
         RETURNING *",
    )
    .bind(id)
    .bind(name.unwrap_or(&existing.name))
    .bind(provider_type.unwrap_or(existing.provider_type))
    .bind(api_key.unwrap_or(&existing.api_key))
    .bind(api_base_url.unwrap_or(&existing.api_base_url))
    .bind(model.unwrap_or(&existing.model))
    .bind(azure_deployment.unwrap_or(existing.azure_deployment.as_deref()))
    .bind(azure_api_version.unwrap_or(existing.azure_api_version.as_deref()))
    .fetch_optional(db.pool())
    .await?;
    Ok(row)
}

pub async fn delete(db: &Db, id: Uuid) -> Result<bool, DbError> {
    let result = sqlx::query("DELETE FROM llm_connectors WHERE id = $1")
        .bind(id)
        .execute(db.pool())
        .await?;
    Ok(result.rows_affected() > 0)
}

/// List connectors visible to a specific user: shared_with_all=true OR user has explicit access.
pub async fn list_for_user(db: &Db, user_id: Uuid) -> Result<Vec<LlmConnectorRow>, DbError> {
    let rows = sqlx::query_as::<_, LlmConnectorRow>(
        "SELECT c.* FROM llm_connectors c
         WHERE c.shared_with_all = true
            OR EXISTS (
                SELECT 1 FROM user_llm_access a
                WHERE a.connector_id = c.id AND a.user_id = $1
            )
         ORDER BY c.created_at",
    )
    .bind(user_id)
    .fetch_all(db.pool())
    .await?;
    Ok(rows)
}

/// Check if a user has access to a specific connector.
pub async fn has_access(db: &Db, connector_id: Uuid, user_id: Uuid) -> Result<bool, DbError> {
    let row = sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS(
            SELECT 1 FROM llm_connectors c
            WHERE c.id = $1
              AND (c.shared_with_all = true
                   OR EXISTS (SELECT 1 FROM user_llm_access a WHERE a.connector_id = $1 AND a.user_id = $2))
        )",
    )
    .bind(connector_id)
    .bind(user_id)
    .fetch_one(db.pool())
    .await?;
    Ok(row)
}

pub async fn set_shared_with_all(db: &Db, connector_id: Uuid, shared: bool) -> Result<bool, DbError> {
    let result = sqlx::query(
        "UPDATE llm_connectors SET shared_with_all = $2, updated_at = now() WHERE id = $1",
    )
    .bind(connector_id)
    .bind(shared)
    .execute(db.pool())
    .await?;
    Ok(result.rows_affected() > 0)
}

pub async fn grant_access(db: &Db, connector_id: Uuid, user_id: Uuid) -> Result<(), DbError> {
    sqlx::query(
        "INSERT INTO user_llm_access (connector_id, user_id) VALUES ($1, $2) ON CONFLICT DO NOTHING",
    )
    .bind(connector_id)
    .bind(user_id)
    .execute(db.pool())
    .await?;
    Ok(())
}

pub async fn revoke_access(db: &Db, connector_id: Uuid, user_id: Uuid) -> Result<(), DbError> {
    sqlx::query(
        "DELETE FROM user_llm_access WHERE connector_id = $1 AND user_id = $2",
    )
    .bind(connector_id)
    .bind(user_id)
    .execute(db.pool())
    .await?;
    Ok(())
}

/// List user IDs that have explicit access to a connector.
pub async fn list_access(db: &Db, connector_id: Uuid) -> Result<Vec<Uuid>, DbError> {
    let ids = sqlx::query_scalar::<_, Uuid>(
        "SELECT user_id FROM user_llm_access WHERE connector_id = $1 ORDER BY granted_at",
    )
    .bind(connector_id)
    .fetch_all(db.pool())
    .await?;
    Ok(ids)
}

/// Replace the full access list for a connector (delete existing, insert new).
pub async fn set_access(db: &Db, connector_id: Uuid, user_ids: &[Uuid]) -> Result<(), DbError> {
    sqlx::query("DELETE FROM user_llm_access WHERE connector_id = $1")
        .bind(connector_id)
        .execute(db.pool())
        .await?;

    for uid in user_ids {
        sqlx::query(
            "INSERT INTO user_llm_access (connector_id, user_id) VALUES ($1, $2)",
        )
        .bind(connector_id)
        .bind(uid)
        .execute(db.pool())
        .await?;
    }
    Ok(())
}
