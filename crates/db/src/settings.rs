use chrono::{DateTime, Utc};
use sqlx::FromRow;
use uuid::Uuid;

use crate::{Db, DbError};

#[derive(Debug, Clone, FromRow)]
pub struct SettingsRow {
    pub id: i32,
    pub default_llm_connector_id: Option<Uuid>,
    pub etl_llm_connector_id: Option<Uuid>,
    pub theme: String,
    /// Platform-level base system prompt prepended before every agent's own system_prompt.
    pub agent_base_prompt: String,
    /// Maximum seconds to wait for an LLM HTTP response before timing out (default 120).
    pub llm_request_timeout_secs: i32,
    /// OpenAI-compatible base URL for the embedding provider.
    pub embedding_api_base_url: String,
    /// API key for the embedding provider.
    pub embedding_api_key: String,
    /// Model name for embedding generation.
    pub embedding_model: String,
    pub updated_at: DateTime<Utc>,
}

pub async fn get(db: &Db) -> Result<SettingsRow, DbError> {
    let row = sqlx::query_as::<_, SettingsRow>(
        "SELECT * FROM app_settings WHERE id = 1",
    )
    .fetch_one(db.pool())
    .await?;
    Ok(row)
}

pub async fn update(
    db: &Db,
    default_llm_connector_id: Option<Option<Uuid>>,
    etl_llm_connector_id: Option<Option<Uuid>>,
    theme: Option<&str>,
    agent_base_prompt: Option<&str>,
    llm_request_timeout_secs: Option<i32>,
    embedding_api_base_url: Option<&str>,
    embedding_api_key: Option<&str>,
    embedding_model: Option<&str>,
) -> Result<SettingsRow, DbError> {
    let existing = get(db).await?;

    let row = sqlx::query_as::<_, SettingsRow>(
        "UPDATE app_settings
         SET default_llm_connector_id = $1,
             etl_llm_connector_id = $2,
             theme = $3,
             agent_base_prompt = $4,
             llm_request_timeout_secs = $5,
             embedding_api_base_url = $6,
             embedding_api_key = $7,
             embedding_model = $8,
             updated_at = now()
         WHERE id = 1
         RETURNING *",
    )
    .bind(default_llm_connector_id.unwrap_or(existing.default_llm_connector_id))
    .bind(etl_llm_connector_id.unwrap_or(existing.etl_llm_connector_id))
    .bind(theme.unwrap_or(&existing.theme))
    .bind(agent_base_prompt.unwrap_or(&existing.agent_base_prompt))
    .bind(llm_request_timeout_secs.unwrap_or(existing.llm_request_timeout_secs))
    .bind(embedding_api_base_url.unwrap_or(&existing.embedding_api_base_url))
    .bind(embedding_api_key.unwrap_or(&existing.embedding_api_key))
    .bind(embedding_model.unwrap_or(&existing.embedding_model))
    .fetch_one(db.pool())
    .await?;
    Ok(row)
}
