use axum::{
    extract::{Path, State},
    http::StatusCode,
    routing::get,
    Json, Router,
};
use clawkson_core::{Connector, ConnectorType};
use clawkson_db::connector as db_connector;
use serde::Deserialize;
use uuid::Uuid;

use crate::auth::AuthUser;
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", get(list_connectors).post(create_connector))
        .route("/{id}", get(get_connector).patch(patch_connector).delete(delete_connector))
}

// ── Helpers ────────────────────────────────────────────────────────

fn db_type_to_core(t: &db_connector::ConnectorType) -> ConnectorType {
    match t {
        db_connector::ConnectorType::Telegram => ConnectorType::Telegram,
        db_connector::ConnectorType::Gmail => ConnectorType::Gmail,
        db_connector::ConnectorType::Slack => ConnectorType::Slack,
        db_connector::ConnectorType::AzureDevops => ConnectorType::AzureDevops,
        db_connector::ConnectorType::Custom => ConnectorType::Custom,
    }
}

fn core_type_to_db(t: &ConnectorType) -> db_connector::ConnectorType {
    match t {
        ConnectorType::Telegram => db_connector::ConnectorType::Telegram,
        ConnectorType::Gmail => db_connector::ConnectorType::Gmail,
        ConnectorType::Slack => db_connector::ConnectorType::Slack,
        ConnectorType::AzureDevops => db_connector::ConnectorType::AzureDevops,
        ConnectorType::Custom => db_connector::ConnectorType::Custom,
    }
}

fn row_to_connector(row: db_connector::ConnectorRow) -> Connector {
    Connector {
        id: row.id,
        user_id: row.user_id,
        name: row.name,
        connector_type: db_type_to_core(&row.connector_type),
        enabled: row.enabled,
        config: row.config,
        created_at: row.created_at,
        updated_at: row.updated_at,
    }
}

// ── Handlers ───────────────────────────────────────────────────────

async fn list_connectors(
    auth: AuthUser,
    State(state): State<AppState>,
) -> Result<Json<Vec<Connector>>, StatusCode> {
    let rows = db_connector::list_for_user(&state.db, auth.id())
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(rows.into_iter().map(row_to_connector).collect()))
}

async fn get_connector(
    auth: AuthUser,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<Connector>, StatusCode> {
    let row = db_connector::get(&state.db, id, auth.id())
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;
    Ok(Json(row_to_connector(row)))
}

#[derive(Debug, Deserialize)]
pub struct CreateConnectorRequest {
    pub name: String,
    pub connector_type: ConnectorType,
    pub config: serde_json::Value,
}

async fn create_connector(
    auth: AuthUser,
    State(state): State<AppState>,
    Json(req): Json<CreateConnectorRequest>,
) -> Result<Json<Connector>, StatusCode> {
    let row = db_connector::create(
        &state.db,
        db_connector::CreateConnector {
            user_id: auth.id(),
            name: req.name,
            connector_type: core_type_to_db(&req.connector_type),
            config: req.config,
        },
    )
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(row_to_connector(row)))
}

#[derive(Debug, Deserialize)]
pub struct PatchConnectorRequest {
    pub enabled: Option<bool>,
}

async fn patch_connector(
    auth: AuthUser,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(req): Json<PatchConnectorRequest>,
) -> Result<Json<Connector>, StatusCode> {
    let enabled = req.enabled.ok_or(StatusCode::BAD_REQUEST)?;
    let row = db_connector::set_enabled(&state.db, id, auth.id(), enabled)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;
    Ok(Json(row_to_connector(row)))
}

async fn delete_connector(
    auth: AuthUser,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> StatusCode {
    match db_connector::delete(&state.db, id, auth.id()).await {
        Ok(true) => StatusCode::NO_CONTENT,
        Ok(false) => StatusCode::NOT_FOUND,
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR,
    }
}
