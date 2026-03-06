use axum::{
    extract::{Path, State},
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
        .route("/{id}", get(get_connector))
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
