use axum::{
    extract::{Multipart, Path, State},
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use serde::Serialize;
use uuid::Uuid;

use crate::auth::AuthUser;
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", post(upload_files))
        .route("/{id}", get(get_attachment).delete(delete_attachment))
}

// ── Response types ──────────────────────────────────────────────

#[derive(Debug, Serialize)]
pub struct AttachmentInfo {
    pub id: String,
    pub filename: String,
    pub content_type: String,
    pub size_bytes: i64,
    pub created_at: String,
}

impl From<clawkson_db::chat_attachment::ChatAttachmentRow> for AttachmentInfo {
    fn from(row: clawkson_db::chat_attachment::ChatAttachmentRow) -> Self {
        AttachmentInfo {
            id: row.id.to_string(),
            filename: row.filename,
            content_type: row.content_type,
            size_bytes: row.size_bytes,
            created_at: row.created_at.to_rfc3339(),
        }
    }
}

#[derive(Debug, Serialize)]
pub struct UploadResponse {
    pub files: Vec<AttachmentInfo>,
}

// ── Handlers ────────────────────────────────────────────────────

/// POST /api/uploads
/// Accepts multipart/form-data with field name "files". Optionally accepts a
/// "conversation_id" text field to associate uploads with a conversation.
/// Returns metadata for each uploaded file including IDs that can be referenced
/// in subsequent ChatRequest.attachment_ids.
async fn upload_files(
    auth: AuthUser,
    State(state): State<AppState>,
    mut multipart: Multipart,
) -> Result<Json<UploadResponse>, (StatusCode, Json<serde_json::Value>)> {
    let s3 = state.s3.as_ref().ok_or_else(|| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(serde_json::json!({"error": "File storage is not configured"})),
        )
    })?;

    let mut conversation_id: Option<Uuid> = None;
    let mut uploaded: Vec<AttachmentInfo> = Vec::new();

    while let Ok(Some(field)) = multipart.next_field().await {
        let field_name = field.name().unwrap_or("").to_string();

        // Handle conversation_id text field
        if field_name == "conversation_id" {
            if let Ok(text) = field.text().await {
                conversation_id = Uuid::parse_str(&text).ok();
            }
            continue;
        }

        // Handle file fields
        if field_name != "files" {
            continue;
        }

        let filename = field
            .file_name()
            .unwrap_or("unnamed")
            .to_string();
        let content_type = field
            .content_type()
            .unwrap_or("application/octet-stream")
            .to_string();

        let data = field.bytes().await.map_err(|e| {
            (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({"error": format!("Failed to read file data: {e}")})),
            )
        })?;

        let file_id = Uuid::new_v4();
        let s3_key = format!("uploads/{}/{}/{}", auth.id(), file_id, filename);
        let size = data.len() as i64;

        // Upload to S3
        s3.put_object(&s3_key, data.to_vec(), &content_type)
            .await
            .map_err(|e| {
                tracing::error!("S3 upload failed: {e}");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(serde_json::json!({"error": "Failed to store file"})),
                )
            })?;

        // Record in DB
        let row = clawkson_db::chat_attachment::create(
            state.db.pool(),
            file_id,
            auth.id(),
            conversation_id,
            &filename,
            &content_type,
            &s3_key,
            size,
        )
        .await
        .map_err(|e| {
            tracing::error!("DB insert failed: {e}");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": "Failed to record file metadata"})),
            )
        })?;

        uploaded.push(AttachmentInfo::from(row));
    }

    if uploaded.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": "No files uploaded. Use field name 'files'."})),
        ));
    }

    Ok(Json(UploadResponse { files: uploaded }))
}

/// GET /api/uploads/{id}
/// Returns the file content with proper Content-Type header.
async fn get_attachment(
    auth: AuthUser,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<impl IntoResponse, StatusCode> {
    let s3 = state.s3.as_ref().ok_or(StatusCode::SERVICE_UNAVAILABLE)?;

    let row = clawkson_db::chat_attachment::get_by_id(state.db.pool(), id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;

    // Check ownership or admin
    if !auth.is_admin() && row.owner_id != Some(auth.id()) {
        // Also allow access if user can access the conversation
        if let Some(conv_id) = row.conversation_id {
            let conv = clawkson_db::conversation::get_by_id(&state.db, conv_id)
                .await
                .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
                .ok_or(StatusCode::NOT_FOUND)?;
            if conv.owner_id != Some(auth.id()) {
                return Err(StatusCode::FORBIDDEN);
            }
        } else {
            return Err(StatusCode::FORBIDDEN);
        }
    }

    let (data, content_type) = s3
        .get_object(&row.s3_key)
        .await
        .map_err(|e| {
            tracing::error!("S3 download failed: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    Ok((
        StatusCode::OK,
        [
            (axum::http::header::CONTENT_TYPE, content_type),
            (
                axum::http::header::CONTENT_DISPOSITION,
                format!("attachment; filename=\"{}\"", row.filename),
            ),
        ],
        data,
    ))
}

/// DELETE /api/uploads/{id}
async fn delete_attachment(
    auth: AuthUser,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> StatusCode {
    let row = match clawkson_db::chat_attachment::get_by_id(state.db.pool(), id).await {
        Ok(Some(r)) => r,
        Ok(None) => return StatusCode::NOT_FOUND,
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR,
    };

    // Only owner or admin can delete
    if !auth.is_admin() && row.owner_id != Some(auth.id()) {
        return StatusCode::FORBIDDEN;
    }

    // Delete from S3
    if let Some(s3) = &state.s3 {
        if let Err(e) = s3.delete_object(&row.s3_key).await {
            tracing::error!("S3 delete failed: {e}");
        }
    }

    // Delete from DB
    match clawkson_db::chat_attachment::delete(state.db.pool(), id).await {
        Ok(true) => StatusCode::NO_CONTENT,
        Ok(false) => StatusCode::NOT_FOUND,
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR,
    }
}
