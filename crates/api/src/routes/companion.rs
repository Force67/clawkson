/// Companion App API routes.
///
/// Provides lightweight endpoints for a mobile/desktop companion application
/// including quick one-shot chat, push notification subscription,
/// media upload, and health/status checks.
use axum::{
    extract::{Multipart, State},
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::auth::AuthUser;
use crate::state::AppState;

// ── Request Types ───────────────────────────────────────────────

/// Request for a quick one-shot chat (no conversation persistence needed).
#[derive(Debug, Deserialize)]
pub struct QuickChatRequest {
    /// The user's message.
    pub message: String,
    /// Optional agent ID to route to (uses default agent if omitted).
    pub agent_id: Option<Uuid>,
    /// Optional model override.
    pub model: Option<String>,
    /// Maximum tokens in the response.
    #[serde(default = "default_max_tokens")]
    pub max_tokens: u32,
}

fn default_max_tokens() -> u32 {
    1024
}

/// Request to register a push notification endpoint.
#[derive(Debug, Deserialize)]
pub struct PushSubscribeRequest {
    /// Push service endpoint URL (e.g. FCM, APNs, Web Push).
    pub endpoint: String,
    /// Platform identifier: "ios", "android", "web", "desktop".
    pub platform: String,
    /// Device-specific token or registration ID.
    pub device_token: String,
    /// Optional human-readable device name.
    pub device_name: Option<String>,
}

// ── Response Types ──────────────────────────────────────────────

/// Response for a quick one-shot chat.
#[derive(Debug, Serialize)]
pub struct QuickChatResponse {
    /// The assistant's reply.
    pub reply: String,
    /// Token usage for this request.
    pub tokens_used: u32,
    /// Model that was used.
    pub model: String,
}

/// Response for push notification subscription.
#[derive(Debug, Serialize)]
pub struct PushSubscribeResponse {
    /// Subscription ID for managing this registration.
    pub subscription_id: Uuid,
    /// Whether this is a new subscription or an update.
    pub is_new: bool,
}

/// Response for media upload.
#[derive(Debug, Serialize)]
pub struct MediaUploadResponse {
    /// ID of the uploaded media.
    pub media_id: Uuid,
    /// Filename of the uploaded media.
    pub filename: String,
    /// Content type of the uploaded media.
    pub content_type: String,
    /// Size in bytes.
    pub size_bytes: u64,
}

/// Response for companion app status check.
#[derive(Debug, Serialize)]
pub struct CompanionStatusResponse {
    /// Server status.
    pub status: String,
    /// Server version.
    pub version: String,
    /// Whether the user has any active agents.
    pub has_agents: bool,
    /// Number of unread conversations.
    pub unread_count: u32,
}

// ── Router ──────────────────────────────────────────────────────

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/quick-chat", post(quick_chat))
        .route("/push-subscribe", post(push_subscribe))
        .route("/media-upload", post(media_upload))
        .route("/status", get(companion_status))
}

// ── Handlers ────────────────────────────────────────────────────

/// POST /api/companion/quick-chat
///
/// Quick one-shot chat endpoint for the companion app. Sends a single message
/// and returns the assistant's reply without creating a persistent conversation.
async fn quick_chat(
    auth: AuthUser,
    State(_state): State<AppState>,
    Json(payload): Json<QuickChatRequest>,
) -> Result<Json<QuickChatResponse>, (StatusCode, Json<serde_json::Value>)> {
    tracing::info!(
        user_id = %auth.id(),
        agent_id = ?payload.agent_id,
        message_len = payload.message.len(),
        max_tokens = payload.max_tokens,
        "Companion: quick-chat (stub)"
    );

    // Stub: In a full implementation, this would:
    // 1. Resolve the agent (use default if agent_id is None)
    // 2. Build a minimal chat context (system prompt + single user message)
    // 3. Call the LLM and return the response
    // 4. Track usage but skip conversation persistence
    Ok(Json(QuickChatResponse {
        reply: "Quick chat is not yet implemented.".to_string(),
        tokens_used: 0,
        model: payload.model.unwrap_or_else(|| "default".to_string()),
    }))
}

/// POST /api/companion/push-subscribe
///
/// Register a device for push notifications. Supports iOS (APNs),
/// Android (FCM), Web Push, and desktop platforms.
async fn push_subscribe(
    auth: AuthUser,
    State(_state): State<AppState>,
    Json(payload): Json<PushSubscribeRequest>,
) -> Result<Json<PushSubscribeResponse>, (StatusCode, Json<serde_json::Value>)> {
    tracing::info!(
        user_id = %auth.id(),
        platform = %payload.platform,
        endpoint = %payload.endpoint,
        device_name = ?payload.device_name,
        "Companion: push-subscribe (stub)"
    );

    // Stub: In a full implementation, this would:
    // 1. Validate the platform and endpoint format
    // 2. Upsert the subscription in the database (keyed by user + device_token)
    // 3. Return the subscription ID
    Ok(Json(PushSubscribeResponse {
        subscription_id: Uuid::new_v4(),
        is_new: true,
    }))
}

/// POST /api/companion/media-upload
///
/// Upload media (images, audio, documents) from the companion app.
/// Accepts multipart/form-data with a "file" field.
async fn media_upload(
    auth: AuthUser,
    State(_state): State<AppState>,
    mut multipart: Multipart,
) -> Result<Json<MediaUploadResponse>, (StatusCode, Json<serde_json::Value>)> {
    let mut filename = String::new();
    let mut content_type = String::new();
    let mut size_bytes: u64 = 0;

    while let Ok(Some(field)) = multipart.next_field().await {
        if field.name() == Some("file") {
            filename = field
                .file_name()
                .unwrap_or("unnamed")
                .to_string();
            content_type = field
                .content_type()
                .unwrap_or("application/octet-stream")
                .to_string();

            match field.bytes().await {
                Ok(data) => {
                    size_bytes = data.len() as u64;
                }
                Err(e) => {
                    return Err((
                        StatusCode::BAD_REQUEST,
                        Json(serde_json::json!({
                            "error": format!("Failed to read file data: {e}")
                        })),
                    ));
                }
            }
        }
    }

    if filename.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "error": "No file uploaded. Use field name 'file'."
            })),
        ));
    }

    tracing::info!(
        user_id = %auth.id(),
        filename = %filename,
        content_type = %content_type,
        size_bytes = size_bytes,
        "Companion: media-upload (stub)"
    );

    // Stub: In a full implementation, this would:
    // 1. Upload to S3 / object storage
    // 2. Record metadata in the database
    // 3. Return the media ID for referencing in chat
    Ok(Json(MediaUploadResponse {
        media_id: Uuid::new_v4(),
        filename,
        content_type,
        size_bytes,
    }))
}

/// GET /api/companion/status
///
/// Health check and status endpoint for the companion app.
/// Returns server status, version, and user-specific summary info.
async fn companion_status(
    auth: AuthUser,
    State(_state): State<AppState>,
) -> Json<CompanionStatusResponse> {
    tracing::info!(
        user_id = %auth.id(),
        "Companion: status check (stub)"
    );

    // Stub: In a full implementation, this would:
    // 1. Check server health
    // 2. Query agent count for this user
    // 3. Query unread conversation count
    Json(CompanionStatusResponse {
        status: "ok".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        has_agents: false,
        unread_count: 0,
    })
}
