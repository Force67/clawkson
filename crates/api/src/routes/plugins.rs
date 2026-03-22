use axum::{
    extract::State,
    http::StatusCode,
    routing::get,
    Json, Router,
};
use clawkson_plugin::PluginManifest;

use crate::auth::AuthUser;
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new().route("/", get(list_plugins))
}

/// GET /api/plugins — list all loaded plugins with their manifests.
async fn list_plugins(
    _auth: AuthUser,
    State(state): State<AppState>,
) -> Result<Json<Vec<PluginManifest>>, StatusCode> {
    let manifests = state.plugins.manifests().await;
    Ok(Json(manifests))
}
