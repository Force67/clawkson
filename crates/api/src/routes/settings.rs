use axum::{
    extract::State,
    routing::get,
    Json, Router,
};
use clawkson_core::Settings;
use serde::Deserialize;
use uuid::Uuid;

use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new().route("/", get(get_settings).patch(patch_settings))
}

#[derive(Debug, Deserialize)]
pub struct PatchSettingsRequest {
    pub default_llm_connector_id: Option<Uuid>,
    pub theme: Option<String>,
}

async fn get_settings(State(state): State<AppState>) -> Json<Settings> {
    let inner = state.inner.read().await;
    Json(inner.settings.clone())
}

async fn patch_settings(
    State(state): State<AppState>,
    Json(req): Json<PatchSettingsRequest>,
) -> Json<Settings> {
    let mut inner = state.inner.write().await;
    if let Some(id) = req.default_llm_connector_id {
        inner.settings.default_llm_connector_id = Some(id);
    }
    if let Some(theme) = req.theme {
        inner.settings.theme = theme;
    }
    Json(inner.settings.clone())
}
