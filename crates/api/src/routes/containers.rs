use axum::{
    body::Body,
    extract::{Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
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
        .route("/preview/{*rest}", get(container_preview))
}

/// Standalone router for the /api/containers prefix (list all).
pub fn list_router() -> Router<AppState> {
    Router::new().route("/", get(list_all_containers))
}

#[derive(Debug, Serialize)]
struct ContainerStatusResponse {
    agent_id: String,
    conversation_id: String,
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

/// Query parameter for specifying the conversation a container belongs to.
#[derive(Debug, Deserialize)]
struct ConversationQuery {
    conversation_id: Uuid,
}

/// POST /api/agents/{id}/container/start?conversation_id=...
async fn start_container(
    auth: AuthUser,
    State(state): State<AppState>,
    Path(agent_id): Path<Uuid>,
    Query(query): Query<ConversationQuery>,
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
                image: ac.image.unwrap_or_else(|| "clawkson-sandbox:latest".to_string()),
                cpu_limit: ac.cpu_limit,
                memory_limit_mb: ac.memory_limit_mb,
                network_enabled: ac.network_enabled,
                permissions: ac.permissions,
            })
            .unwrap_or_default()
    };

    let info = cm
        .start_container(agent_id, query.conversation_id, &config)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, err_json(e.to_string())))?;

    Ok(Json(ContainerStatusResponse {
        agent_id: info.agent_id.to_string(),
        conversation_id: info.conversation_id.to_string(),
        state: serde_json::to_value(&info.state)
            .ok()
            .and_then(|v| v.as_str().map(String::from))
            .unwrap_or_else(|| "unknown".to_string()),
        image: info.image,
        workspace_path: info.workspace_path,
    }))
}

/// POST /api/agents/{id}/container/stop?conversation_id=...
async fn stop_container(
    auth: AuthUser,
    State(state): State<AppState>,
    Path(agent_id): Path<Uuid>,
    Query(query): Query<ConversationQuery>,
) -> Result<StatusCode, (StatusCode, Json<ErrorResponse>)> {
    if !auth.is_admin() {
        return Err((StatusCode::FORBIDDEN, err_json("admin only")));
    }

    let cm = state
        .container_manager
        .as_ref()
        .ok_or_else(|| (StatusCode::SERVICE_UNAVAILABLE, err_json("Docker not available")))?;

    cm.stop_container(agent_id, query.conversation_id)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, err_json(e.to_string())))?;

    Ok(StatusCode::NO_CONTENT)
}

/// DELETE /api/agents/{id}/container?conversation_id=...
async fn remove_container(
    auth: AuthUser,
    State(state): State<AppState>,
    Path(agent_id): Path<Uuid>,
    Query(query): Query<ConversationQuery>,
) -> Result<StatusCode, (StatusCode, Json<ErrorResponse>)> {
    if !auth.is_admin() {
        return Err((StatusCode::FORBIDDEN, err_json("admin only")));
    }

    let cm = state
        .container_manager
        .as_ref()
        .ok_or_else(|| (StatusCode::SERVICE_UNAVAILABLE, err_json("Docker not available")))?;

    cm.remove_container(agent_id, query.conversation_id, true)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, err_json(e.to_string())))?;

    Ok(StatusCode::NO_CONTENT)
}

/// GET /api/agents/{id}/container?conversation_id=...
/// Returns the container for a specific conversation.
async fn get_container(
    _auth: AuthUser,
    State(state): State<AppState>,
    Path(agent_id): Path<Uuid>,
    Query(query): Query<ConversationQuery>,
) -> Result<Json<ContainerStatusResponse>, (StatusCode, Json<ErrorResponse>)> {
    let cm = state
        .container_manager
        .as_ref()
        .ok_or_else(|| (StatusCode::SERVICE_UNAVAILABLE, err_json("Docker not available")))?;

    let info = cm
        .get_container(agent_id, query.conversation_id)
        .await
        .ok_or_else(|| (StatusCode::NOT_FOUND, err_json("no container for this agent/conversation")))?;

    Ok(Json(ContainerStatusResponse {
        agent_id: info.agent_id.to_string(),
        conversation_id: info.conversation_id.to_string(),
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
    conversation_id: Uuid,
    tail: Option<usize>,
}

/// GET /api/agents/{id}/container/logs?conversation_id=...
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
        .logs(agent_id, query.conversation_id, query.tail)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, err_json(e.to_string())))?;

    Ok(Json(serde_json::json!({ "logs": logs })))
}

#[derive(Debug, Deserialize)]
struct ExecCommandRequest {
    conversation_id: Uuid,
    #[serde(flatten)]
    exec: ExecRequest,
}

/// POST /api/agents/{id}/container/exec
async fn exec_command(
    auth: AuthUser,
    State(state): State<AppState>,
    Path(agent_id): Path<Uuid>,
    Json(req): Json<ExecCommandRequest>,
) -> Result<Json<ExecResult>, (StatusCode, Json<ErrorResponse>)> {
    if !auth.is_admin() {
        return Err((StatusCode::FORBIDDEN, err_json("admin only")));
    }

    let cm = state
        .container_manager
        .as_ref()
        .ok_or_else(|| (StatusCode::SERVICE_UNAVAILABLE, err_json("Docker not available")))?;

    let result = cm
        .exec(agent_id, req.conversation_id, &req.exec)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, err_json(e.to_string())))?;

    Ok(Json(result))
}

// ── Container preview reverse proxy ──────────────────────────────────

#[derive(Debug, Deserialize)]
struct PreviewQuery {
    conversation_id: Uuid,
}

/// GET /api/agents/{id}/container/preview/{*rest}
///
/// Reverse-proxy HTTP requests to a web server running inside the container.
/// `{*rest}` captures `{port}/optional/path...` — we split it ourselves.
/// The container must be on the `clawkson-internal` Docker network.
async fn container_preview(
    auth: AuthUser,
    state: State<AppState>,
    Path((agent_id, rest)): Path<(Uuid, String)>,
    query: Query<PreviewQuery>,
    req: axum::http::Request<Body>,
) -> Result<impl IntoResponse, (StatusCode, Json<ErrorResponse>)> {
    // rest = "8000/" or "8000/some/path"
    let (port_str, path) = rest.split_once('/').unwrap_or((&rest, ""));
    let port: u16 = port_str.parse().map_err(|_| {
        (StatusCode::BAD_REQUEST, err_json("invalid port number"))
    })?;
    do_container_preview(auth, state, agent_id, port, path.to_string(), query, req).await
}

async fn do_container_preview(
    _auth: AuthUser,
    State(state): State<AppState>,
    agent_id: Uuid,
    port: u16,
    path: String,
    Query(query): Query<PreviewQuery>,
    req: axum::http::Request<Body>,
) -> Result<impl IntoResponse, (StatusCode, Json<ErrorResponse>)> {

    let cm = state
        .container_manager
        .as_ref()
        .ok_or_else(|| (StatusCode::SERVICE_UNAVAILABLE, err_json("Docker not available")))?;

    let info = cm
        .get_container(agent_id, query.conversation_id)
        .await
        .ok_or_else(|| (StatusCode::NOT_FOUND, err_json("no container for this conversation")))?;

    let ip = info
        .ip_address
        .as_deref()
        .ok_or_else(|| (StatusCode::BAD_GATEWAY, err_json("container has no proxy IP")))?;

    let target_url = format!("http://{}:{}/{}", ip, port, path);

    // Build the proxied request, forwarding the original method and relevant headers.
    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, err_json(e.to_string())))?;

    let mut proxy_req = client.request(req.method().clone(), &target_url);

    // Forward a few useful headers from the original request.
    for key in ["accept", "accept-language", "content-type", "range"] {
        if let Some(val) = req.headers().get(key) {
            proxy_req = proxy_req.header(key, val);
        }
    }

    let proxy_resp = proxy_req
        .send()
        .await
        .map_err(|e| (StatusCode::BAD_GATEWAY, err_json(format!("proxy error: {e}"))))?;

    // Forward the response status, headers, and body.
    let status = StatusCode::from_u16(proxy_resp.status().as_u16())
        .unwrap_or(StatusCode::BAD_GATEWAY);

    let mut builder = axum::http::Response::builder().status(status);

    // Forward content-related headers.
    for key in [
        "content-type",
        "content-length",
        "content-disposition",
        "cache-control",
        "etag",
        "last-modified",
        "location",
    ] {
        if let Some(val) = proxy_resp.headers().get(key) {
            builder = builder.header(key, val);
        }
    }

    let body = proxy_resp
        .bytes()
        .await
        .map_err(|e| (StatusCode::BAD_GATEWAY, err_json(format!("body read error: {e}"))))?;

    builder
        .body(Body::from(body))
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, err_json(e.to_string())))
}

/// GET /api/containers — list all active containers across all agents.
async fn list_all_containers(
    _auth: AuthUser,
    State(state): State<AppState>,
) -> Result<Json<Vec<ContainerStatusResponse>>, (StatusCode, Json<ErrorResponse>)> {
    let cm = state
        .container_manager
        .as_ref()
        .ok_or_else(|| (StatusCode::SERVICE_UNAVAILABLE, err_json("Docker not available")))?;

    let all = cm.list_all_containers().await;

    let items: Vec<ContainerStatusResponse> = all
        .into_iter()
        .map(|info| ContainerStatusResponse {
            agent_id: info.agent_id.to_string(),
            conversation_id: info.conversation_id.to_string(),
            state: serde_json::to_value(&info.state)
                .ok()
                .and_then(|v| v.as_str().map(String::from))
                .unwrap_or_else(|| "unknown".to_string()),
            image: info.image,
            workspace_path: info.workspace_path,
        })
        .collect();

    Ok(Json(items))
}
