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
) -> Result<SettingsRow, DbError> {
    let existing = get(db).await?;

    let row = sqlx::query_as::<_, SettingsRow>(
        "UPDATE app_settings
         SET default_llm_connector_id = $1,
             etl_llm_connector_id = $2,
             theme = $3,
             updated_at = now()
         WHERE id = 1
         RETURNING *",
    )
    .bind(default_llm_connector_id.unwrap_or(existing.default_llm_connector_id))
    .bind(etl_llm_connector_id.unwrap_or(existing.etl_llm_connector_id))
    .bind(theme.unwrap_or(&existing.theme))
    .fetch_one(db.pool())
    .await?;
    Ok(row)
}
