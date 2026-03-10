use axum::{
    body::Body,
    extract::{multipart::Multipart, Path, Query, State},
    http::{header, StatusCode},
    response::Response,
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tokio_stream::StreamExt as _;
use uuid::Uuid;

use crate::auth::AuthUser;
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/workspace", get(list_workspace).delete(delete_workspace_entry))
        .route("/workspace/upload", post(upload_to_workspace))
        .route("/workspace/download", get(download_from_workspace))
        .route("/workspace/watch", get(watch_workspace))
}

// ── Shared helpers ────────────────────────────────────────────────

#[derive(Debug, Serialize)]
struct ErrorResponse {
    error: String,
}

fn err(msg: impl Into<String>) -> (StatusCode, Json<ErrorResponse>) {
    (StatusCode::BAD_REQUEST, Json(ErrorResponse { error: msg.into() }))
}

fn err_with(code: StatusCode, msg: impl Into<String>) -> (StatusCode, Json<ErrorResponse>) {
    (code, Json(ErrorResponse { error: msg.into() }))
}

/// Get the workspace root for an agent from the container manager.
/// The container does NOT need to be running — the directory on the host
/// is the source of truth.
async fn get_workspace(state: &AppState, agent_id: Uuid) -> Result<PathBuf, (StatusCode, Json<ErrorResponse>)> {
    let cm = state
        .container_manager
        .as_ref()
        .ok_or_else(|| err_with(StatusCode::SERVICE_UNAVAILABLE, "Docker not available"))?;

    cm.agent_workspace(agent_id)
        .await
        .map_err(|e| err_with(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))
}

// ── GET /api/agents/{id}/container/workspace ──────────────────────

#[derive(Debug, Deserialize)]
struct WorkspaceQuery {
    /// Sub-path within the workspace to list (default: workspace root).
    path: Option<String>,
}

/// List the contents of a workspace directory.
async fn list_workspace(
    _auth: AuthUser,
    State(state): State<AppState>,
    Path(agent_id): Path<Uuid>,
    Query(query): Query<WorkspaceQuery>,
) -> Result<Json<clawkson_container::WorkspaceListing>, (StatusCode, Json<ErrorResponse>)> {
    let workspace = get_workspace(&state, agent_id).await?;
    let rel = query.path.as_deref().unwrap_or("");

    clawkson_container::list_workspace(&workspace, rel)
        .map(Json)
        .map_err(|e| match e {
            clawkson_container::ContainerError::PathEscape(_) =>
                err_with(StatusCode::BAD_REQUEST, e.to_string()),
            _ => err_with(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
        })
}

// ── POST /api/agents/{id}/container/workspace/upload ─────────────

#[derive(Debug, Serialize)]
struct UploadWorkspaceResponse {
    uploaded: Vec<String>,
    errors: Vec<String>,
}

/// Upload one or more files into a workspace directory.
async fn upload_to_workspace(
    auth: AuthUser,
    State(state): State<AppState>,
    Path(agent_id): Path<Uuid>,
    mut multipart: Multipart,
) -> Result<Json<UploadWorkspaceResponse>, (StatusCode, Json<ErrorResponse>)> {
    if !auth.is_admin() {
        return Err(err_with(StatusCode::FORBIDDEN, "admin only"));
    }

    let workspace = get_workspace(&state, agent_id).await?;

    // We need to collect the target path before or alongside files.
    // Strategy: parse all fields; if "path" appears first, use it; otherwise
    // default to workspace root.
    let mut target_rel = String::new();
    let mut uploaded = Vec::new();
    let mut errors = Vec::new();

    while let Some(field) = multipart.next_field().await.map_err(|e| err(e.to_string()))? {
        let name = field.name().unwrap_or("").to_string();

        if name == "path" {
            target_rel = field.text().await.map_err(|e| err(e.to_string()))?;
            continue;
        }

        // Treat everything else as a file.
        let filename = field
            .file_name()
            .map(|s| s.to_string())
            .unwrap_or_else(|| format!("upload-{}", Uuid::new_v4()));

        let data = match field.bytes().await {
            Ok(b) => b,
            Err(e) => {
                errors.push(format!("{filename}: {e}"));
                continue;
            }
        };

        // Sandbox: resolve target directory, then append filename.
        let dir = match clawkson_container::sandbox_path(&workspace, &target_rel) {
            Ok(d) => d,
            Err(e) => {
                errors.push(format!("{filename}: {e}"));
                continue;
            }
        };

        // The filename itself must not contain path separators.
        let safe_name: String = filename
            .replace(['/', '\\', '\0'], "_");
        let dest = dir.join(&safe_name);

        if let Err(e) = std::fs::create_dir_all(&dir) {
            errors.push(format!("{safe_name}: could not create directory: {e}"));
            continue;
        }

        match std::fs::write(&dest, &data) {
            Ok(_) => {
                let rel = dest
                    .strip_prefix(&workspace)
                    .map(|p| p.to_string_lossy().to_string())
                    .unwrap_or(safe_name.clone());
                uploaded.push(rel);
            }
            Err(e) => errors.push(format!("{safe_name}: {e}")),
        }
    }

    Ok(Json(UploadWorkspaceResponse { uploaded, errors }))
}

// ── GET /api/agents/{id}/container/workspace/download ────────────

#[derive(Debug, Deserialize)]
struct DownloadQuery {
    path: String,
}

/// Download a single file from the workspace.
async fn download_from_workspace(
    _auth: AuthUser,
    State(state): State<AppState>,
    Path(agent_id): Path<Uuid>,
    Query(query): Query<DownloadQuery>,
) -> Result<Response, (StatusCode, Json<ErrorResponse>)> {
    let workspace = get_workspace(&state, agent_id).await?;

    let file_path = clawkson_container::sandbox_path(&workspace, &query.path)
        .map_err(|e| err_with(StatusCode::BAD_REQUEST, e.to_string()))?;

    if !file_path.exists() {
        return Err(err_with(StatusCode::NOT_FOUND, "file not found"));
    }
    if file_path.is_dir() {
        return Err(err_with(StatusCode::BAD_REQUEST, "path is a directory; only files can be downloaded"));
    }

    let data = std::fs::read(&file_path)
        .map_err(|e| err_with(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let filename = file_path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "download".to_string());

    let content_type = mime_guess(&filename);

    let response = Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, content_type)
        .header(
            header::CONTENT_DISPOSITION,
            format!("attachment; filename=\"{filename}\""),
        )
        .header(header::CONTENT_LENGTH, data.len())
        .body(Body::from(data))
        .map_err(|e| err_with(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(response)
}

fn mime_guess(filename: &str) -> &'static str {
    match filename.rsplit('.').next().unwrap_or("").to_lowercase().as_str() {
        "json" => "application/json",
        "csv" => "text/csv",
        "txt" | "md" | "log" => "text/plain",
        "html" | "htm" => "text/html",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "pdf" => "application/pdf",
        "zip" => "application/zip",
        "py" => "text/x-python",
        "rs" => "text/x-rust",
        "js" | "ts" => "text/javascript",
        _ => "application/octet-stream",
    }
}

// ── DELETE /api/agents/{id}/container/workspace ───────────────────

#[derive(Debug, Deserialize)]
struct DeleteWorkspaceRequest {
    path: String,
    #[serde(default)]
    recursive: bool,
}

/// Delete a file or directory from the workspace.
async fn delete_workspace_entry(
    auth: AuthUser,
    State(state): State<AppState>,
    Path(agent_id): Path<Uuid>,
    Json(body): Json<DeleteWorkspaceRequest>,
) -> Result<StatusCode, (StatusCode, Json<ErrorResponse>)> {
    if !auth.is_admin() {
        return Err(err_with(StatusCode::FORBIDDEN, "admin only"));
    }

    let workspace = get_workspace(&state, agent_id).await?;

    let target = clawkson_container::sandbox_path(&workspace, &body.path)
        .map_err(|e| err_with(StatusCode::BAD_REQUEST, e.to_string()))?;

    // Refuse to delete the workspace root itself.
    if target == workspace {
        return Err(err_with(StatusCode::BAD_REQUEST, "cannot delete workspace root"));
    }

    if !target.exists() {
        return Err(err_with(StatusCode::NOT_FOUND, "path not found"));
    }

    if target.is_dir() {
        if !body.recursive {
            return Err(err_with(
                StatusCode::BAD_REQUEST,
                "path is a directory; set recursive=true to delete",
            ));
        }
        std::fs::remove_dir_all(&target)
            .map_err(|e| err_with(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    } else {
        std::fs::remove_file(&target)
            .map_err(|e| err_with(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    }

    Ok(StatusCode::NO_CONTENT)
}

// ── GET /api/agents/{id}/container/workspace/watch (SSE) ─────────

/// SSE event describing a workspace filesystem change.
#[derive(Debug, Clone, Serialize)]
struct WatchEvent {
    #[serde(rename = "type")]
    event_type: String,
    path: String,
}

/// Stream workspace filesystem change events as Server-Sent Events.
///
/// This uses a simple polling approach with a 2-second interval rather
/// than inotify, keeping the implementation cross-platform and dependency-
/// free. A future iteration can swap in the `notify` crate.
async fn watch_workspace(
    _auth: AuthUser,
    State(state): State<AppState>,
    Path(agent_id): Path<Uuid>,
) -> Result<Response, (StatusCode, Json<ErrorResponse>)> {
    let workspace = get_workspace(&state, agent_id).await?;

    // Seed with the current snapshot.
    let initial_snapshot = snapshot_workspace(&workspace);

    let stream = async_stream::stream! {
        let mut prev = initial_snapshot;

        loop {
            tokio::time::sleep(std::time::Duration::from_secs(2)).await;

            let current = snapshot_workspace(&workspace);

            // Detect created and modified.
            for (path, mtime) in &current {
                match prev.get(path) {
                    None => {
                        let event = WatchEvent { event_type: "created".to_string(), path: path.clone() };
                        if let Ok(data) = serde_json::to_string(&event) {
                            yield format!("data: {data}\n\n");
                        }
                    }
                    Some(old_mtime) if old_mtime != mtime => {
                        let event = WatchEvent { event_type: "modified".to_string(), path: path.clone() };
                        if let Ok(data) = serde_json::to_string(&event) {
                            yield format!("data: {data}\n\n");
                        }
                    }
                    _ => {}
                }
            }

            // Detect deleted.
            for path in prev.keys() {
                if !current.contains_key(path) {
                    let event = WatchEvent { event_type: "deleted".to_string(), path: path.clone() };
                    if let Ok(data) = serde_json::to_string(&event) {
                        yield format!("data: {data}\n\n");
                    }
                }
            }

            prev = current;
        }
    };

    let response = Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "text/event-stream")
        .header(header::CACHE_CONTROL, "no-cache")
        .header("X-Accel-Buffering", "no")
        .body(Body::from_stream(stream.map(|s: String| -> Result<String, std::convert::Infallible> { Ok(s) })))
        .map_err(|e| err_with(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(response)
}

/// Build a map of relative-path → mtime (as u64 unix seconds) for all
/// files in the workspace tree, for change detection.
fn snapshot_workspace(root: &std::path::Path) -> std::collections::HashMap<String, u64> {
    let mut map = std::collections::HashMap::new();
    snapshot_recursive(root, root, &mut map);
    map
}

fn snapshot_recursive(
    root: &std::path::Path,
    dir: &std::path::Path,
    map: &mut std::collections::HashMap<String, u64>,
) {
    let Ok(read_dir) = std::fs::read_dir(dir) else { return };
    for entry in read_dir.flatten() {
        let path = entry.path();
        let Ok(meta) = entry.metadata() else { continue };
        let rel = path
            .strip_prefix(root)
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_default();
        if meta.is_dir() {
            snapshot_recursive(root, &path, map);
        } else {
            let mtime = meta
                .modified()
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_secs())
                .unwrap_or(0);
            map.insert(rel, mtime);
        }
    }
}
