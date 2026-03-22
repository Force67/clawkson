/// API endpoints for managing WASM plugins.
///
/// GET  /api/wasm-plugins           — list loaded WASM plugins and their tools
/// POST /api/wasm-plugins/load      — load a WASM plugin from uploaded bytes
/// DELETE /api/wasm-plugins/{name}  — unload a WASM plugin
/// POST /api/wasm-plugins/{name}/invoke — invoke a tool on a WASM plugin
use axum::{
    extract::{Path, State},
    http::StatusCode,
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::auth::AuthUser;
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", get(list_wasm_plugins))
        .route("/load", post(load_wasm_plugin))
        .route("/{name}", axum::routing::delete(unload_wasm_plugin))
        .route("/{name}/invoke", post(invoke_wasm_tool))
}

#[derive(Serialize)]
struct WasmPluginResponse {
    name: String,
    description: String,
    version: String,
    wasm_path: String,
    tools: Vec<WasmToolResponse>,
}

#[derive(Serialize)]
struct WasmToolResponse {
    name: String,
    description: String,
    parameters_schema: serde_json::Value,
}

async fn list_wasm_plugins(
    _auth: AuthUser,
    State(state): State<AppState>,
) -> Json<Vec<WasmPluginResponse>> {
    let plugins = state.wasm.list_plugins().await;
    Json(
        plugins
            .into_iter()
            .map(|p| WasmPluginResponse {
                name: p.name,
                description: p.description,
                version: p.version,
                wasm_path: p.wasm_path,
                tools: p
                    .tools
                    .into_iter()
                    .map(|t| WasmToolResponse {
                        name: t.name,
                        description: t.description,
                        parameters_schema: t.parameters_schema,
                    })
                    .collect(),
            })
            .collect(),
    )
}

#[derive(Deserialize)]
struct LoadWasmPluginRequest {
    /// Base64-encoded .wasm bytes.
    wasm_base64: Option<String>,
    /// Path to a .wasm file on the server filesystem.
    wasm_path: Option<String>,
    /// Plugin config key-value pairs.
    #[serde(default)]
    config: HashMap<String, String>,
    /// Whether the plugin can make network requests.
    #[serde(default)]
    network_enabled: bool,
}

#[derive(Serialize)]
struct LoadWasmPluginResponse {
    name: String,
    description: String,
    version: String,
    tools: Vec<String>,
}

async fn load_wasm_plugin(
    auth: AuthUser,
    State(state): State<AppState>,
    Json(req): Json<LoadWasmPluginRequest>,
) -> Result<Json<LoadWasmPluginResponse>, StatusCode> {
    if !auth.is_admin() {
        return Err(StatusCode::FORBIDDEN);
    }

    let info = if let Some(b64) = &req.wasm_base64 {
        let bytes = base64_decode(b64).map_err(|_| StatusCode::BAD_REQUEST)?;
        state
            .wasm
            .load_plugin_bytes(&bytes, "<upload>".to_string(), req.config, req.network_enabled)
            .await
            .map_err(|e| {
                tracing::error!(error = %e, "WASM plugin load failed");
                StatusCode::INTERNAL_SERVER_ERROR
            })?
    } else if let Some(path) = &req.wasm_path {
        state
            .wasm
            .load_plugin(std::path::Path::new(path), req.config, req.network_enabled)
            .await
            .map_err(|e| {
                tracing::error!(error = %e, path = %path, "WASM plugin load failed");
                StatusCode::INTERNAL_SERVER_ERROR
            })?
    } else {
        return Err(StatusCode::BAD_REQUEST);
    };

    Ok(Json(LoadWasmPluginResponse {
        name: info.name,
        description: info.description,
        version: info.version,
        tools: info.tools.into_iter().map(|t| t.name).collect(),
    }))
}

async fn unload_wasm_plugin(
    auth: AuthUser,
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> StatusCode {
    if !auth.is_admin() {
        return StatusCode::FORBIDDEN;
    }

    if state.wasm.unload_plugin(&name).await {
        StatusCode::NO_CONTENT
    } else {
        StatusCode::NOT_FOUND
    }
}

#[derive(Deserialize)]
struct InvokeWasmToolRequest {
    tool_name: String,
    arguments: serde_json::Value,
}

#[derive(Serialize)]
struct InvokeWasmToolResponse {
    result: serde_json::Value,
}

async fn invoke_wasm_tool(
    _auth: AuthUser,
    State(state): State<AppState>,
    Path(plugin_name): Path<String>,
    Json(req): Json<InvokeWasmToolRequest>,
) -> Result<Json<InvokeWasmToolResponse>, StatusCode> {
    let args_json = serde_json::to_string(&req.arguments)
        .map_err(|_| StatusCode::BAD_REQUEST)?;

    let result = state
        .wasm
        .invoke_tool(&plugin_name, &req.tool_name, &args_json)
        .await
        .map_err(|e| {
            tracing::error!(
                plugin = %plugin_name,
                tool = %req.tool_name,
                error = %e,
                "WASM tool invoke failed"
            );
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    Ok(Json(InvokeWasmToolResponse { result }))
}

fn base64_decode(input: &str) -> Result<Vec<u8>, ()> {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let input = input.trim().replace(['\n', '\r', ' '], "");
    let mut output = Vec::with_capacity(input.len() * 3 / 4);
    let mut buf = 0u32;
    let mut bits = 0u32;
    for ch in input.bytes() {
        if ch == b'=' { break; }
        let val = TABLE.iter().position(|&b| b == ch).ok_or(())? as u32;
        buf = (buf << 6) | val;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            output.push((buf >> bits) as u8);
            buf &= (1 << bits) - 1;
        }
    }
    Ok(output)
}
