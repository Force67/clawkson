use axum::{
    extract::{Path, State},
    http::StatusCode,
    routing::get,
    Json, Router,
};
use chrono::Utc;
use clawkson_core::{Connector, ConnectorType};
use serde::Deserialize;
use uuid::Uuid;

use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", get(list_connectors).post(create_connector))
        .route("/{id}", get(get_connector).patch(patch_connector).delete(delete_connector))
}

#[derive(Debug, Deserialize)]
pub struct CreateConnectorRequest {
    pub name: String,
    pub connector_type: ConnectorType,
    pub config: serde_json::Value,
}

async fn list_connectors(State(state): State<AppState>) -> Json<Vec<Connector>> {
    let inner = state.inner.read().await;
    Json(inner.connectors.clone())
}

async fn get_connector(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Json<Option<Connector>> {
    let inner = state.inner.read().await;
    let connector = inner.connectors.iter().find(|c| c.id == id).cloned();
    Json(connector)
}

async fn create_connector(
    State(state): State<AppState>,
    Json(req): Json<CreateConnectorRequest>,
) -> Json<Connector> {
    let connector = Connector {
        id: Uuid::new_v4(),
        name: req.name,
        connector_type: req.connector_type,
        enabled: true,
        config: req.config,
        created_at: Utc::now(),
    };

    let mut inner = state.inner.write().await;
    inner.connectors.push(connector.clone());
    Json(connector)
}

#[derive(Debug, Deserialize)]
pub struct PatchConnectorRequest {
    pub enabled: Option<bool>,
}

async fn patch_connector(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(req): Json<PatchConnectorRequest>,
) -> Result<Json<Connector>, StatusCode> {
    let mut inner = state.inner.write().await;
    let connector = inner
        .connectors
        .iter_mut()
        .find(|c| c.id == id)
        .ok_or(StatusCode::NOT_FOUND)?;
    if let Some(enabled) = req.enabled {
        connector.enabled = enabled;
    }
    Ok(Json(connector.clone()))
}

async fn delete_connector(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> StatusCode {
    let mut inner = state.inner.write().await;
    let before = inner.connectors.len();
    inner.connectors.retain(|c| c.id != id);
    if inner.connectors.len() < before {
        StatusCode::NO_CONTENT
    } else {
        StatusCode::NOT_FOUND
    }
}
