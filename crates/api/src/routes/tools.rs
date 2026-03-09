use axum::{
    extract::{Path, State},
    http::StatusCode,
    routing::get,
    Json, Router,
};
use clawkson_core::Tool;
use serde::Deserialize;
use uuid::Uuid;

use crate::auth::AuthUser;
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", get(list_tools).post(create_tool))
        .route("/{id}", get(get_tool))
}

#[derive(Debug, Deserialize)]
pub struct CreateToolRequest {
    pub name: String,
    pub description: String,
    pub connector_id: Uuid,
    pub schema: serde_json::Value,
}

async fn list_tools(_auth: AuthUser, State(state): State<AppState>) -> Json<Vec<Tool>> {
    let inner = state.inner.read().await;
    Json(inner.tools.clone())
}

async fn get_tool(
    _auth: AuthUser,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Json<Option<Tool>> {
    let inner = state.inner.read().await;
    let tool = inner.tools.iter().find(|t| t.id == id).cloned();
    Json(tool)
}

async fn create_tool(
    auth: AuthUser,
    State(state): State<AppState>,
    Json(req): Json<CreateToolRequest>,
) -> Result<Json<Tool>, StatusCode> {
    if !auth.is_admin() {
        return Err(StatusCode::FORBIDDEN);
    }

    let tool = Tool {
        id: Uuid::new_v4(),
        name: req.name,
        description: req.description,
        connector_id: req.connector_id,
        schema: req.schema,
        enabled: true,
    };

    let mut inner = state.inner.write().await;
    inner.tools.push(tool.clone());
    Ok(Json(tool))
}
