use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    routing::{delete, get, post},
    Json, Router,
};
use clawkson_container::{ContainerConfig, ExecRequest, ExecResult};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::auth::AuthUser;
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/start", post(start_container))
        .route("/stop", post(stop_container))
        .route("/", delete(remove_container).get(get_container))
        .route("/logs", get(get_logs))
        .route("/exec", post(exec_command))
}

#[derive(Debug, Serialize)]
struct ContainerStatusResponse {
    agent_id: String,
    state: String,
    image: String,
    workspace_path: String,
}

#[derive(Debug, Serialize)]
struct ErrorResponse {
    error: String,
}

fn err_json(msg: impl Into<String>) -> Json<ErrorResponse> {
    Json(ErrorResponse {
        error: msg.into(),
    })
}

/// POST /api/agents/{id}/container/start
async fn start_container(
    auth: AuthUser,
    State(state): State<AppState>,
    Path(agent_id): Path<Uuid>,
) -> Result<Json<ContainerStatusResponse>, (StatusCode, Json<ErrorResponse>)> {
    if !auth.is_admin() {
        return Err((StatusCode::FORBIDDEN, err_json("admin only")));
    }

    let cm = state
        .container_manager
        .as_ref()
        .ok_or_else(|| (StatusCode::SERVICE_UNAVAILABLE, err_json("Docker not available")))?;

    // Get agent container config from DB
    let config = {
        let agent = clawkson_db::agent::get_by_id(&state.db, agent_id)
            .await
            .map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR, err_json("db error")))?
            .ok_or_else(|| (StatusCode::NOT_FOUND, err_json("agent not found")))?;

        if !agent.container_enabled {
            return Err((
                StatusCode::BAD_REQUEST,
                err_json("container not enabled for this agent"),
            ));
        }

        agent.container_config
            .and_then(|v| serde_json::from_value::<clawkson_core::AgentContainerConfig>(v).ok())
            .map(|ac| ContainerConfig {
                image: "python:3.12-slim".to_string(),
                cpu_limit: ac.cpu_limit,
                memory_limit_mb: ac.memory_limit_mb,
                network_enabled: ac.network_enabled,
            })
            .unwrap_or_default()
    };

    let info = cm
        .start_container(agent_id, &config)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, err_json(e.to_string())))?;

    Ok(Json(ContainerStatusResponse {
        agent_id: info.agent_id.to_string(),
        state: serde_json::to_value(&info.state)
            .ok()
            .and_then(|v| v.as_str().map(String::from))
            .unwrap_or_else(|| "unknown".to_string()),
        image: info.image,
        workspace_path: info.workspace_path,
    }))
}

/// POST /api/agents/{id}/container/stop
async fn stop_container(
    auth: AuthUser,
    State(state): State<AppState>,
    Path(agent_id): Path<Uuid>,
) -> Result<StatusCode, (StatusCode, Json<ErrorResponse>)> {
    if !auth.is_admin() {
        return Err((StatusCode::FORBIDDEN, err_json("admin only")));
    }

    let cm = state
        .container_manager
        .as_ref()
        .ok_or_else(|| (StatusCode::SERVICE_UNAVAILABLE, err_json("Docker not available")))?;

    cm.stop_container(agent_id)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, err_json(e.to_string())))?;

    Ok(StatusCode::NO_CONTENT)
}

/// DELETE /api/agents/{id}/container
async fn remove_container(
    auth: AuthUser,
    State(state): State<AppState>,
    Path(agent_id): Path<Uuid>,
) -> Result<StatusCode, (StatusCode, Json<ErrorResponse>)> {
    if !auth.is_admin() {
        return Err((StatusCode::FORBIDDEN, err_json("admin only")));
    }

    let cm = state
        .container_manager
        .as_ref()
        .ok_or_else(|| (StatusCode::SERVICE_UNAVAILABLE, err_json("Docker not available")))?;

    cm.remove_container(agent_id, true)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, err_json(e.to_string())))?;

    Ok(StatusCode::NO_CONTENT)
}

/// GET /api/agents/{id}/container
async fn get_container(
    _auth: AuthUser,
    State(state): State<AppState>,
    Path(agent_id): Path<Uuid>,
) -> Result<Json<ContainerStatusResponse>, (StatusCode, Json<ErrorResponse>)> {
    let cm = state
        .container_manager
        .as_ref()
        .ok_or_else(|| (StatusCode::SERVICE_UNAVAILABLE, err_json("Docker not available")))?;

    let info = cm
        .get_container(agent_id)
        .await
        .ok_or_else(|| (StatusCode::NOT_FOUND, err_json("no container for this agent")))?;

    Ok(Json(ContainerStatusResponse {
        agent_id: info.agent_id.to_string(),
        state: serde_json::to_value(&info.state)
            .ok()
            .and_then(|v| v.as_str().map(String::from))
            .unwrap_or_else(|| "unknown".to_string()),
        image: info.image,
        workspace_path: info.workspace_path,
    }))
}

#[derive(Debug, Deserialize)]
struct LogsQuery {
    tail: Option<usize>,
}

/// GET /api/agents/{id}/container/logs
async fn get_logs(
    _auth: AuthUser,
    State(state): State<AppState>,
    Path(agent_id): Path<Uuid>,
    Query(query): Query<LogsQuery>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<ErrorResponse>)> {
    let cm = state
        .container_manager
        .as_ref()
        .ok_or_else(|| (StatusCode::SERVICE_UNAVAILABLE, err_json("Docker not available")))?;

    let logs = cm
        .logs(agent_id, query.tail)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, err_json(e.to_string())))?;

    Ok(Json(serde_json::json!({ "logs": logs })))
}

/// POST /api/agents/{id}/container/exec
async fn exec_command(
    auth: AuthUser,
    State(state): State<AppState>,
    Path(agent_id): Path<Uuid>,
    Json(req): Json<ExecRequest>,
) -> Result<Json<ExecResult>, (StatusCode, Json<ErrorResponse>)> {
    if !auth.is_admin() {
        return Err((StatusCode::FORBIDDEN, err_json("admin only")));
    }

    let cm = state
        .container_manager
        .as_ref()
        .ok_or_else(|| (StatusCode::SERVICE_UNAVAILABLE, err_json("Docker not available")))?;

    let result = cm
        .exec(agent_id, &req)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, err_json(e.to_string())))?;

    Ok(Json(result))
}
