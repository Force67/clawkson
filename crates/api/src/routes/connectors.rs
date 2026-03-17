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
        db_connector::ConnectorType::Tavily => ConnectorType::Tavily,
        db_connector::ConnectorType::Bing => ConnectorType::Bing,
    }
}

fn core_type_to_db(t: &ConnectorType) -> db_connector::ConnectorType {
    match t {
        ConnectorType::Telegram => db_connector::ConnectorType::Telegram,
        ConnectorType::Gmail => db_connector::ConnectorType::Gmail,
        ConnectorType::Slack => db_connector::ConnectorType::Slack,
        ConnectorType::AzureDevops => db_connector::ConnectorType::AzureDevops,
        ConnectorType::Custom => db_connector::ConnectorType::Custom,
        ConnectorType::Tavily => db_connector::ConnectorType::Tavily,
        ConnectorType::Bing => db_connector::ConnectorType::Bing,
    }
}

/// Returns true if this connector type provides web search (only one should be active at a time).
fn is_web_search_type(t: &ConnectorType) -> bool {
    matches!(t, ConnectorType::Tavily | ConnectorType::Bing)
}

fn row_to_connector(row: db_connector::ConnectorRow) -> Connector {
    Connector {
        id: row.id,
        user_id: row.user_id,
        name: row.name,
        connector_type: db_type_to_core(&row.connector_type),
        enabled: row.enabled,
        config: row.config,
        context: row.context,
        created_at: row.created_at,
        updated_at: row.updated_at,
    }
}

/// Try to start/stop Telegram polling based on connector state.
async fn sync_telegram_poller(state: &AppState, row: &db_connector::ConnectorRow) {
    if row.connector_type != db_connector::ConnectorType::Telegram {
        return;
    }

    if row.enabled {
        let bot_token = row.config.get("bot_token").and_then(|v| v.as_str());
        let agent_id = row.config.get("agent_id")
            .and_then(|v| v.as_str())
            .and_then(|s| Uuid::parse_str(s).ok());
        if let (Some(token), Some(aid)) = (bot_token, agent_id) {
            state.telegram.start(state.clone(), row.id, row.user_id, token.to_string(), aid).await;
        }
    } else {
        state.telegram.stop(row.id).await;
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

    // Web search connectors are mutually exclusive — disable others when a new one is created
    if is_web_search_type(&req.connector_type) {
        let _ = db_connector::disable_other_web_search(&state.db, auth.id(), row.id).await;
    }

    // Start Telegram polling if applicable
    sync_telegram_poller(&state, &row).await;

    Ok(Json(row_to_connector(row)))
}

#[derive(Debug, Deserialize)]
pub struct PatchConnectorRequest {
    pub enabled: Option<bool>,
    pub context: Option<String>,
    pub name: Option<String>,
    pub config: Option<serde_json::Value>,
}

async fn patch_connector(
    auth: AuthUser,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(req): Json<PatchConnectorRequest>,
) -> Result<Json<Connector>, StatusCode> {
    // Require at least one field.
    if req.enabled.is_none() && req.context.is_none() && req.name.is_none() && req.config.is_none() {
        return Err(StatusCode::BAD_REQUEST);
    }

    // Apply name update first if provided.
    if let Some(ref name) = req.name {
        db_connector::set_name(&state.db, id, auth.id(), name)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
            .ok_or(StatusCode::NOT_FOUND)?;
    }

    // Apply config update if provided.
    if let Some(ref config) = req.config {
        db_connector::set_config(&state.db, id, auth.id(), config)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
            .ok_or(StatusCode::NOT_FOUND)?;
    }

    // Apply context update if provided.
    if let Some(ref ctx) = req.context {
        db_connector::set_context(&state.db, id, auth.id(), ctx)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
            .ok_or(StatusCode::NOT_FOUND)?;
    }

    // Apply enabled toggle last (so Telegram poller sync sees final state).
    if let Some(enabled) = req.enabled {
        let row = db_connector::set_enabled(&state.db, id, auth.id(), enabled)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
            .ok_or(StatusCode::NOT_FOUND)?;

        // Web search connectors are mutually exclusive — disable others when one is enabled
        if enabled && is_web_search_type(&db_type_to_core(&row.connector_type)) {
            let _ = db_connector::disable_other_web_search(&state.db, auth.id(), id).await;
        }

        sync_telegram_poller(&state, &row).await;
        return Ok(Json(row_to_connector(row)));
    }

    // Re-fetch the final state to return it.
    let row = db_connector::get(&state.db, id, auth.id())
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
    // Stop Telegram polling before delete
    state.telegram.stop(id).await;

    match db_connector::delete(&state.db, id, auth.id()).await {
        Ok(true) => StatusCode::NO_CONTENT,
        Ok(false) => StatusCode::NOT_FOUND,
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR,
    }
}
