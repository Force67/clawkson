use axum::{
    extract::State,
    http::StatusCode,
    routing::get,
    Json, Router,
};
use clawkson_core::Settings;
use serde::Deserialize;
use uuid::Uuid;

use crate::auth::AuthUser;
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new().route("/", get(get_settings).patch(patch_settings))
}

#[derive(Debug, Deserialize)]
pub struct PatchSettingsRequest {
    pub default_llm_connector_id: Option<Uuid>,
    /// Set the LLM connector used for ETL semantic chunking.
    /// Pass `null` explicitly to clear the connector (falls back to heuristic chunking).
    pub etl_llm_connector_id: Option<Uuid>,
    pub theme: Option<String>,
}

async fn get_settings(_auth: AuthUser, State(state): State<AppState>) -> Result<Json<Settings>, StatusCode> {
    let row = clawkson_db::settings::get(&state.db)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(Settings {
        default_llm_connector_id: row.default_llm_connector_id,
        etl_llm_connector_id: row.etl_llm_connector_id,
        theme: row.theme,
    }))
}

async fn patch_settings(
    auth: AuthUser,
    State(state): State<AppState>,
    Json(req): Json<PatchSettingsRequest>,
) -> Result<Json<Settings>, StatusCode> {
    if !auth.is_admin() {
        return Err(StatusCode::FORBIDDEN);
    }

    let row = clawkson_db::settings::update(
        &state.db,
        req.default_llm_connector_id.map(Some),
        req.etl_llm_connector_id.map(Some),
        req.theme.as_deref(),
    )
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(Settings {
        default_llm_connector_id: row.default_llm_connector_id,
        etl_llm_connector_id: row.etl_llm_connector_id,
        theme: row.theme,
    }))
}
