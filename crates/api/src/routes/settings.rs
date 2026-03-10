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
    /// Platform-level base system prompt prepended before every agent's own system_prompt.
    /// Set to an empty string to clear it. When non-empty, the final system prompt sent
    /// to the LLM will be: `agent_base_prompt + "\n\n" + agent.system_prompt`.
    pub agent_base_prompt: Option<String>,
    /// Maximum seconds to wait for an LLM HTTP response before timing out.
    /// Range: 10–600. Default is 120.
    pub llm_request_timeout_secs: Option<i32>,
}

async fn get_settings(_auth: AuthUser, State(state): State<AppState>) -> Result<Json<Settings>, StatusCode> {
    let row = clawkson_db::settings::get(&state.db)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(Settings {
        default_llm_connector_id: row.default_llm_connector_id,
        etl_llm_connector_id: row.etl_llm_connector_id,
        theme: row.theme,
        agent_base_prompt: row.agent_base_prompt,
        llm_request_timeout_secs: row.llm_request_timeout_secs,
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

    // Clamp timeout to a sane range if provided
    let timeout = req.llm_request_timeout_secs.map(|t| t.clamp(10, 600));

    let row = clawkson_db::settings::update(
        &state.db,
        req.default_llm_connector_id.map(Some),
        req.etl_llm_connector_id.map(Some),
        req.theme.as_deref(),
        req.agent_base_prompt.as_deref(),
        timeout,
    )
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(Settings {
        default_llm_connector_id: row.default_llm_connector_id,
        etl_llm_connector_id: row.etl_llm_connector_id,
        theme: row.theme,
        agent_base_prompt: row.agent_base_prompt,
        llm_request_timeout_secs: row.llm_request_timeout_secs,
    }))
}
