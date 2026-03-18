use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::{
        sse::{Event, Sse},
        IntoResponse,
    },
    routing::get,
    Json, Router,
};
use clawkson_core::{Conversation, LlmConnector, LlmProviderType, Message, MessageRole};
use futures::stream;
use serde::{Deserialize, Serialize};
use std::convert::Infallible;
use uuid::Uuid;

use crate::auth::AuthUser;
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", get(list_conversations).post(create_conversation).delete(delete_all_conversations))
        .route("/{id}", get(get_conversation).patch(patch_conversation).delete(delete_conversation))
        .route("/{id}/messages", get(list_messages).post(send_message).delete(clear_messages))
        .route("/{id}/chat", axum::routing::post(chat))
        .route("/{id}/chat/stream", axum::routing::post(chat_stream))
}

// ── Request / Response types ───────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct CreateConversationRequest {
    pub title: String,
    pub agent_id: Uuid,
}

#[derive(Debug, Deserialize)]
pub struct SendMessageRequest {
    pub content: String,
    pub role: MessageRole,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ReasoningEffort {
    Low,
    Medium,
    High,
}

#[derive(Debug, Deserialize)]
pub struct ChatRequest {
    pub content: String,
    /// When set, enables extended thinking / chain-of-thought.
    #[serde(default)]
    pub reasoning_effort: Option<ReasoningEffort>,
    /// When false, knowledge-base search tools are excluded even if the agent
    /// has linked KBs. Defaults to true.
    #[serde(default = "default_true")]
    pub search_enabled: bool,
    /// IDs of previously-uploaded attachments to associate with this message.
    #[serde(default)]
    pub attachment_ids: Vec<Uuid>,
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Deserialize)]
pub struct PatchConversationRequest {
    pub title: Option<String>,
    pub pinned: Option<bool>,
}

#[derive(Debug, Serialize)]
pub struct ChatResponse {
    pub user_message: Message,
    pub assistant_message: Message,
}

// ── Type mapping helpers ─────────────────────────────────────────

fn conv_to_api(row: clawkson_db::conversation::Conversation) -> Conversation {
    Conversation {
        id: row.id,
        title: row.title,
        agent_id: row.agent_id.unwrap_or(Uuid::nil()),
        owner_id: row.owner_id,
        pinned: row.pinned,
        archived: row.archived,
        created_at: row.created_at,
        updated_at: row.updated_at,
    }
}

fn msg_to_api(row: clawkson_db::message::Message) -> Message {
    Message {
        id: row.id,
        conversation_id: row.conversation_id,
        role: match row.role {
            clawkson_db::message::MessageRole::User => MessageRole::User,
            clawkson_db::message::MessageRole::Assistant => MessageRole::Assistant,
            clawkson_db::message::MessageRole::System => MessageRole::System,
            clawkson_db::message::MessageRole::Tool => MessageRole::Tool,
        },
        content: row.content,
        created_at: row.created_at,
        attachments: Vec::new(),
    }
}

fn role_to_db(role: &MessageRole) -> clawkson_db::message::MessageRole {
    match role {
        MessageRole::User => clawkson_db::message::MessageRole::User,
        MessageRole::Assistant => clawkson_db::message::MessageRole::Assistant,
        MessageRole::System => clawkson_db::message::MessageRole::System,
        MessageRole::Tool => clawkson_db::message::MessageRole::Tool,
    }
}

// ── Access helpers ────────────────────────────────────────────────

async fn can_access(state: &AppState, conv_id: Uuid, user_id: Uuid, is_admin: bool) -> Result<bool, StatusCode> {
    let conv = clawkson_db::conversation::get_by_id(&state.db, conv_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;

    if is_admin || conv.owner_id == Some(user_id) {
        return Ok(true);
    }
    let pool = state.db.pool();
    let share = clawkson_db::share::get_user_share(pool, conv_id, user_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(share.is_some())
}

async fn can_write(state: &AppState, conv_id: Uuid, user_id: Uuid, is_admin: bool) -> Result<bool, StatusCode> {
    let conv = clawkson_db::conversation::get_by_id(&state.db, conv_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;

    if is_admin || conv.owner_id == Some(user_id) {
        return Ok(true);
    }
    let pool = state.db.pool();
    let share = clawkson_db::share::get_user_share(pool, conv_id, user_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(share.map_or(false, |s| s.permission == clawkson_db::share::SharePermission::Write))
}

// ── Helpers ────────────────────────────────────────────────────────

/// Resolve the LLM connector for a conversation's agent.
pub(crate) async fn resolve_connector_id(
    state: &AppState,
    agent_id: Uuid,
) -> Option<Uuid> {
    let agent = clawkson_db::agent::get_by_id(&state.db, agent_id).await.ok()??;
    if let Some(id) = agent.llm_connector_id {
        return Some(id);
    }
    let settings = clawkson_db::settings::get(&state.db).await.ok()?;
    settings.default_llm_connector_id
}

// ── Handlers ───────────────────────────────────────────────────────

async fn list_conversations(
    auth: AuthUser,
    State(state): State<AppState>,
) -> Result<Json<Vec<Conversation>>, StatusCode> {
    let rows = if auth.is_admin() {
        clawkson_db::conversation::list_all(&state.db).await
    } else {
        clawkson_db::conversation::list_for_user(&state.db, auth.id()).await
    };
    let rows = rows.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(rows.into_iter().map(conv_to_api).collect()))
}

async fn get_conversation(
    auth: AuthUser,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<Conversation>, StatusCode> {
    let has_access = can_access(&state, id, auth.id(), auth.is_admin()).await?;
    if !has_access {
        return Err(StatusCode::FORBIDDEN);
    }
    let row = clawkson_db::conversation::get_by_id(&state.db, id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;
    Ok(Json(conv_to_api(row)))
}

async fn create_conversation(
    auth: AuthUser,
    State(state): State<AppState>,
    Json(req): Json<CreateConversationRequest>,
) -> Result<Json<Conversation>, StatusCode> {
    let row = clawkson_db::conversation::create(
        &state.db,
        Some(req.agent_id),
        Some(auth.id()),
        &req.title,
    )
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(conv_to_api(row)))
}

/// Delete S3 attachment objects and the container workspace for a conversation.
/// Called before the DB delete so the attachment records are still available for lookup.
/// Errors are logged but do not prevent conversation deletion.
async fn cleanup_conversation_files(state: &AppState, conversation_id: Uuid, agent_id: Option<Uuid>) {
    // 1. Delete S3 objects for all attachments
    if let Some(s3) = &state.s3 {
        match clawkson_db::chat_attachment::list_for_conversation(state.db.pool(), conversation_id).await {
            Ok(attachments) => {
                for att in attachments {
                    if let Err(e) = s3.delete_object(&att.s3_key).await {
                        tracing::warn!(%conversation_id, s3_key = %att.s3_key, error = %e, "failed to delete S3 object during conversation cleanup");
                    }
                }
            }
            Err(e) => {
                tracing::warn!(%conversation_id, error = %e, "failed to list attachments for cleanup");
            }
        }
    }

    // 2. Remove container and workspace directory
    if let (Some(cm), Some(agent_id)) = (&state.container_manager, agent_id) {
        if let Err(e) = cm.remove_container(agent_id, conversation_id, true).await {
            tracing::debug!(%conversation_id, error = %e, "container cleanup (may not exist)");
        }
        // Also clean up workspace dir even if no container was tracked
        // (workspace may exist from a previous session where the container was already removed)
        let workspace = cm.workspace_root()
            .join(agent_id.to_string())
            .join(conversation_id.to_string());
        if workspace.exists() {
            if let Err(e) = std::fs::remove_dir_all(&workspace) {
                tracing::warn!(%conversation_id, path = %workspace.display(), error = %e, "failed to remove workspace directory");
            }
        }
    }
}

async fn patch_conversation(
    auth: AuthUser,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(req): Json<PatchConversationRequest>,
) -> Result<Json<Conversation>, StatusCode> {
    let writable = can_write(&state, id, auth.id(), auth.is_admin()).await?;
    if !writable {
        return Err(StatusCode::FORBIDDEN);
    }

    if let Some(pinned) = req.pinned {
        clawkson_db::conversation::set_pinned(&state.db, id, pinned)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    }
    if let Some(title) = &req.title {
        clawkson_db::conversation::update_title(&state.db, id, title)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    }

    let row = clawkson_db::conversation::get_by_id(&state.db, id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;
    Ok(Json(conv_to_api(row)))
}

async fn delete_conversation(
    auth: AuthUser,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> StatusCode {
    // Check ownership
    let conv = match clawkson_db::conversation::get_by_id(&state.db, id).await {
        Ok(Some(c)) => c,
        Ok(None) => return StatusCode::NOT_FOUND,
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR,
    };
    if !auth.is_admin() && conv.owner_id != Some(auth.id()) {
        return StatusCode::FORBIDDEN;
    }
    // Clean up S3 objects and workspace before DB cascade deletes the records
    cleanup_conversation_files(&state, id, conv.agent_id).await;
    match clawkson_db::conversation::delete(&state.db, id).await {
        Ok(true) => StatusCode::NO_CONTENT,
        Ok(false) => StatusCode::NOT_FOUND,
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR,
    }
}

/// DELETE /api/conversations/{id}/messages — wipe all messages from a conversation
/// while keeping the conversation record itself. Requires write access.
async fn clear_messages(
    auth: AuthUser,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> StatusCode {
    let writable = match can_write(&state, id, auth.id(), auth.is_admin()).await {
        Ok(v) => v,
        Err(status) => return status,
    };
    if !writable {
        return StatusCode::FORBIDDEN;
    }
    match clawkson_db::message::clear_for_conversation(&state.db, id).await {
        Ok(_) => StatusCode::NO_CONTENT,
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR,
    }
}

/// DELETE /api/conversations — delete ALL conversations owned by the authenticated user.
/// Admin users may delete all conversations in the system.
async fn delete_all_conversations(
    auth: AuthUser,
    State(state): State<AppState>,
) -> StatusCode {
    let rows = if auth.is_admin() {
        clawkson_db::conversation::list_all(&state.db).await
    } else {
        clawkson_db::conversation::list_for_user(&state.db, auth.id()).await
    };
    let rows = match rows {
        Ok(r) => r,
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR,
    };
    for conv in rows {
        cleanup_conversation_files(&state, conv.id, conv.agent_id).await;
        if let Err(_) = clawkson_db::conversation::delete(&state.db, conv.id).await {
            return StatusCode::INTERNAL_SERVER_ERROR;
        }
    }
    StatusCode::NO_CONTENT
}

async fn list_messages(
    auth: AuthUser,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<Vec<Message>>, StatusCode> {
    let has_access = can_access(&state, id, auth.id(), auth.is_admin()).await?;
    if !has_access {
        return Err(StatusCode::FORBIDDEN);
    }
    let rows = clawkson_db::message::list_for_conversation(&state.db, id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let pool = state.db.pool();
    let mut messages: Vec<Message> = Vec::with_capacity(rows.len());
    for row in rows {
        let msg_id = row.id;
        let mut msg = msg_to_api(row);
        if let Ok(atts) = clawkson_db::chat_attachment::list_for_message(pool, msg_id).await {
            msg.attachments = atts
                .into_iter()
                .map(|a| clawkson_core::MessageAttachment {
                    id: a.id,
                    filename: a.filename,
                    content_type: a.content_type,
                    size_bytes: a.size_bytes,
                    metadata: a.metadata,
                })
                .collect();
        }
        messages.push(msg);
    }

    Ok(Json(messages))
}

async fn send_message(
    auth: AuthUser,
    State(state): State<AppState>,
    Path(conv_id): Path<Uuid>,
    Json(req): Json<SendMessageRequest>,
) -> Result<Json<Message>, StatusCode> {
    let writable = can_write(&state, conv_id, auth.id(), auth.is_admin()).await?;
    if !writable {
        return Err(StatusCode::FORBIDDEN);
    }
    let row = clawkson_db::message::create(
        &state.db,
        conv_id,
        None,
        role_to_db(&req.role),
        &req.content,
        None,
        None,
    )
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(msg_to_api(row)))
}

/// Save a message to DB and return the API type.
async fn save_message(
    state: &AppState,
    conv_id: Uuid,
    role: MessageRole,
    content: &str,
) -> Result<Message, StatusCode> {
    let row = clawkson_db::message::create(
        &state.db,
        conv_id,
        None,
        role_to_db(&role),
        content,
        None,
        None,
    )
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(msg_to_api(row))
}

/// After the assistant responds, scan `/workspace/outputs/` for files produced by the
/// container, upload them to S3, and link them as attachments to the assistant message.
/// This makes output files automatically downloadable in the chat UI.
///
/// Files are NOT deleted after upload — the agent may reference them in later messages.
/// A `.clawkson-attached` manifest tracks which files have already been uploaded so they
/// are not re-attached on subsequent turns.
pub(crate) async fn attach_workspace_outputs(
    state: &AppState,
    agent_id: Uuid,
    assistant_msg_id: Uuid,
    owner_id: Uuid,
    conv_id: Uuid,
) {
    let cm = match &state.container_manager {
        Some(cm) => cm,
        None => return,
    };
    let s3 = match &state.s3 {
        Some(s3) => s3,
        None => return,
    };

    let workspace_dir = cm
        .workspace_root()
        .join(agent_id.to_string())
        .join(conv_id.to_string());
    let outputs_dir = workspace_dir.join("outputs");
    let inputs_dir = workspace_dir.join("inputs");

    // Load the set of files already attached in previous turns.
    let manifest_path = workspace_dir.join(".clawkson-attached");
    let already_attached: std::collections::HashSet<String> = tokio::fs::read_to_string(&manifest_path)
        .await
        .unwrap_or_default()
        .lines()
        .map(|l| l.to_string())
        .collect();

    // Collect output files from both /workspace/outputs/ and /workspace/ root
    // (agents sometimes write directly to /workspace/ instead of /workspace/outputs/).
    let mut files_to_attach: Vec<std::path::PathBuf> = Vec::new();

    // Scan /workspace/outputs/
    if let Ok(mut entries) = tokio::fs::read_dir(&outputs_dir).await {
        while let Ok(Some(entry)) = entries.next_entry().await {
            if entry.file_type().await.map(|ft| ft.is_file()).unwrap_or(false) {
                files_to_attach.push(entry.path());
            }
        }
    }

    // Scan /workspace/ root (skip inputs/ and outputs/ subdirs)
    if let Ok(mut entries) = tokio::fs::read_dir(&workspace_dir).await {
        while let Ok(Some(entry)) = entries.next_entry().await {
            let path = entry.path();
            // Skip the inputs and outputs subdirectories themselves
            if path == inputs_dir || path == outputs_dir {
                continue;
            }
            // Skip the manifest file itself
            if path == manifest_path {
                continue;
            }
            if entry.file_type().await.map(|ft| ft.is_file()).unwrap_or(false) {
                files_to_attach.push(path);
            }
        }
    }

    // Filter out files that were already attached in a previous turn.
    files_to_attach.retain(|p| {
        let key = p.strip_prefix(&workspace_dir)
            .map(|r| r.to_string_lossy().to_string())
            .unwrap_or_default();
        !already_attached.contains(&key)
    });

    tracing::debug!(
        "attach_workspace_outputs: agent={agent_id}, found {} new files to attach",
        files_to_attach.len()
    );

    if files_to_attach.is_empty() {
        return;
    }

    let pool = state.db.pool();
    let mut newly_attached: Vec<String> = Vec::new();

    for path in &files_to_attach {
        tracing::debug!("attaching output: {}", path.display());

        let filename = match path.file_name().and_then(|n| n.to_str()) {
            Some(n) => n.to_string(),
            None => continue,
        };

        let data = match tokio::fs::read(&path).await {
            Ok(d) => d,
            Err(e) => {
                tracing::warn!("failed to read output file {filename}: {e}");
                continue;
            }
        };

        let content_type = match filename.rsplit('.').next().unwrap_or("").to_lowercase().as_str() {
            "json" => "application/json",
            "csv" => "text/csv",
            "txt" | "md" | "log" => "text/plain",
            "html" | "htm" => "text/html",
            "png" => "image/png",
            "jpg" | "jpeg" => "image/jpeg",
            "gif" => "image/gif",
            "svg" => "image/svg+xml",
            "pdf" => "application/pdf",
            "zip" => "application/zip",
            "xlsx" => "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
            "docx" => "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
            "pptx" => "application/vnd.openxmlformats-officedocument.presentationml.presentation",
            _ => "application/octet-stream",
        };

        // Extract metadata from Office files (sheet names, slide counts, etc.)
        let metadata = extract_office_metadata(content_type, &data);

        let file_id = Uuid::new_v4();
        let s3_key = format!("outputs/{}/{}/{}", owner_id, file_id, filename);
        let size = data.len() as i64;

        if let Err(e) = s3.put_object(&s3_key, data, content_type).await {
            tracing::warn!("failed to upload output {filename} to S3: {e}");
            continue;
        }

        match clawkson_db::chat_attachment::create_with_metadata(
            pool, file_id, owner_id, Some(conv_id),
            &filename, content_type, &s3_key, size, metadata,
        ).await {
            Ok(_) => {
                if let Err(e) = clawkson_db::chat_attachment::link_to_message(pool, file_id, assistant_msg_id).await {
                    tracing::warn!("failed to link output attachment {filename} to message: {e}");
                }
            }
            Err(e) => {
                tracing::warn!("failed to create DB record for output {filename}: {e}");
            }
        }

        // Record this file as attached so we skip it on subsequent turns.
        let rel_key = path.strip_prefix(&workspace_dir)
            .map(|r| r.to_string_lossy().to_string())
            .unwrap_or_else(|_| filename.clone());
        newly_attached.push(rel_key);
    }

    // Append newly attached files to the manifest.
    if !newly_attached.is_empty() {
        let mut manifest = already_attached.into_iter().collect::<Vec<_>>();
        manifest.extend(newly_attached);
        let content = manifest.join("\n") + "\n";
        if let Err(e) = tokio::fs::write(&manifest_path, content).await {
            tracing::warn!("failed to write attached-files manifest: {e}");
        }
    }
}

/// Extract metadata from Office files (xlsx, pptx, docx) by peeking inside the ZIP archive.
/// Returns None for non-Office files or on any parse error.
fn extract_office_metadata(content_type: &str, data: &[u8]) -> Option<serde_json::Value> {
    use std::io::Cursor;

    let cursor = Cursor::new(data);
    let mut archive = zip::ZipArchive::new(cursor).ok()?;

    match content_type {
        "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet" => {
            // xlsx: extract sheet names from xl/workbook.xml, count worksheets
            let mut sheet_names: Vec<String> = Vec::new();
            if let Ok(mut file) = archive.by_name("xl/workbook.xml") {
                let mut xml = String::new();
                std::io::Read::read_to_string(&mut file, &mut xml).ok();
                // Parse <sheet name="..."/> elements
                for segment in xml.split("<sheet ") {
                    if let Some(name_start) = segment.find("name=\"") {
                        let rest = &segment[name_start + 6..];
                        if let Some(name_end) = rest.find('"') {
                            sheet_names.push(rest[..name_end].to_string());
                        }
                    }
                }
            }
            // Count worksheet files as fallback
            let sheet_count = if sheet_names.is_empty() {
                let mut count = 0usize;
                for i in 0..archive.len() {
                    if let Ok(f) = archive.by_index(i) {
                        let n = f.name().to_string();
                        if n.starts_with("xl/worksheets/sheet") && n.ends_with(".xml") {
                            count += 1;
                        }
                    }
                }
                count
            } else {
                sheet_names.len()
            };

            Some(serde_json::json!({
                "type": "spreadsheet",
                "sheet_count": sheet_count,
                "sheet_names": sheet_names,
            }))
        }
        "application/vnd.openxmlformats-officedocument.presentationml.presentation" => {
            // pptx: count ppt/slides/slide*.xml entries
            let mut slide_count = 0usize;
            for i in 0..archive.len() {
                if let Ok(f) = archive.by_index(i) {
                    let n = f.name().to_string();
                    if n.starts_with("ppt/slides/slide") && n.ends_with(".xml") {
                        slide_count += 1;
                    }
                }
            }

            Some(serde_json::json!({
                "type": "presentation",
                "slide_count": slide_count,
            }))
        }
        "application/vnd.openxmlformats-officedocument.wordprocessingml.document" => {
            // docx: count paragraphs from word/document.xml as a rough metric
            let mut paragraph_count = 0usize;
            if let Ok(mut file) = archive.by_name("word/document.xml") {
                let mut xml = String::new();
                std::io::Read::read_to_string(&mut file, &mut xml).ok();
                paragraph_count = xml.matches("<w:p ").count() + xml.matches("<w:p>").count();
            }

            Some(serde_json::json!({
                "type": "document",
                "paragraph_count": paragraph_count,
            }))
        }
        _ => None,
    }
}

/// Load agent config for chat handlers.
#[derive(Clone)]
pub(crate) struct AgentConfig {
    pub(crate) agent_id: Uuid,
    pub(crate) system_prompt: Option<String>,
    pub(crate) temperature: Option<f64>,
    pub(crate) max_tokens: Option<u32>,
    pub(crate) container_enabled: bool,
    pub(crate) container_config: Option<clawkson_core::AgentContainerConfig>,
    pub(crate) connector_policies: Vec<clawkson_core::ConnectorPolicy>,
    /// Optional LLM connector for sub-task execution. When set, sub-agents use this
    /// (potentially cheaper/faster) model instead of the agent's primary connector.
    pub(crate) subtask_llm_connector_id: Option<Uuid>,
}

pub(crate) async fn load_agent_config(state: &AppState, agent_id: Uuid) -> Option<AgentConfig> {
    let row = clawkson_db::agent::get_by_id(&state.db, agent_id).await.ok()??;

    // Load linked skills and build enriched system prompt
    let skills = clawkson_db::skill::agent_list_skills(state.db.pool(), row.id)
        .await
        .unwrap_or_default();

    // Load the platform-level base prompt from settings (empty string if unset)
    let settings = clawkson_db::settings::get(&state.db).await.ok();
    let base_prompt = settings
        .as_ref()
        .map(|s| s.agent_base_prompt.as_str())
        .unwrap_or("");

    let mut system_prompt = build_system_prompt_with_skills(base_prompt, row.system_prompt.as_deref(), &skills);

    // Append container/sandbox awareness instructions when the agent has a container
    let container_config: Option<clawkson_core::AgentContainerConfig> =
        row.container_config.as_ref().and_then(|v| serde_json::from_value(v.clone()).ok());

    if row.container_enabled {
        let image = container_config.as_ref()
            .and_then(|c| c.image.as_deref())
            .unwrap_or("clawkson-sandbox:latest");
        let network = container_config.as_ref().map(|c| c.network_enabled).unwrap_or(false);

        let sandbox_instructions = format!(
            "\n\n<sandbox-environment>\n\
             IMPORTANT: You do NOT have direct shell access. You are NOT running inside a container \
             or on the user's machine. You are an AI assistant that can execute code REMOTELY in a \
             sandboxed Docker container (image: {image}) by calling the code_execution tool. \
             Every time you need to run code, you MUST use the code_execution tool — it sends your \
             code to the container and returns the output.\n\n\
             Your available tools for working with the container:\n\
             - code_execution: Run Python or Bash code inside the container.\n\
             - workspace_read: Read a file from the container's /workspace directory.\n\
             - workspace_write: Write a file to the container's /workspace directory.\n\
             - workspace_list: List files and directories in /workspace.\n\
             Always use these tools rather than describing code inline or asking the user to run things.\n\n\
             Important guidelines:\n\
             - Proactively install packages and dependencies when needed. Do not ask the user for \
               permission to install — just run `pip install <package>` or `apt-get install -y <package>` \
               via code_execution before using them. The container is ephemeral and isolated, so \
               installing packages is safe and expected.\n\
             - When a task requires a library you are not sure is pre-installed, install it first \
               in a separate code_execution call, then proceed with the actual task.\n\
             - The container has a /workspace directory that is the ONLY writable persistent location. \
               The rest of the filesystem is read-only. Read inputs from /workspace/inputs/ and \
               write outputs to /workspace/outputs/. These directories are pre-created and writable. \
               NEVER fall back to /tmp or other directories — always use /workspace.\n\
             {network_note}\
             - You can chain multiple code_execution calls to build up complex workflows step by step.\n\
             - NEVER show code to the user unless they explicitly ask to see it. Just describe what \
               you are doing and present the results. The user does not want to see code blocks, \
               tool call parameters, or raw scripts in your responses.\n\
             - After generating output files, ALWAYS read them back (using workspace_read) and \
               present the contents or a summary directly in your response. Never ask the user \
               if they want to see the results — just show them. If the user asks a question \
               about the data (e.g. \"how many are there?\"), answer it directly from the data.\n\
             - If a tool call fails (e.g. container error, timeout, permission denied), just retry \
               automatically. NEVER ask the user whether to retry or what to do — just fix the \
               issue and try again silently. The user expects results, not status updates about errors.\n\
             - Files you write to /workspace/outputs/ are AUTOMATICALLY attached to your response \
               and offered to the user as downloadable files in the chat interface. When you generate \
               output files (CSVs, reports, charts, exports, etc.), mention the filenames in your \
               response so the user knows they can download them directly from the chat.\n\
             </sandbox-environment>",
            network_note = if network {
                "- Networking is enabled — you can fetch URLs, call APIs, and download data from the internet.\n"
            } else {
                "- Networking is disabled — you cannot access the internet from the container. All data \
                 must be provided via the workspace.\n"
            },
        );

        match &mut system_prompt {
            Some(ref mut prompt) => prompt.push_str(&sandbox_instructions),
            None => system_prompt = Some(sandbox_instructions.trim_start().to_string()),
        }
    }

    // Deserialize connector policies from the agent's JSONB column
    let connector_policies: Vec<clawkson_core::ConnectorPolicy> =
        serde_json::from_value(row.connector_policies.clone()).unwrap_or_default();

    Some(AgentConfig {
        agent_id: row.id,
        system_prompt,
        temperature: row.temperature,
        max_tokens: row.max_tokens.map(|v| v as u32),
        container_enabled: row.container_enabled,
        container_config: container_config,
        connector_policies,
        subtask_llm_connector_id: row.subtask_llm_connector_id,
    })
}

/// Build the final system prompt by layering:
///   1. Platform base prompt (from Settings.agent_base_prompt) — global steering/guardrails
///   2. Agent system prompt — per-agent persona and task instructions
///   3. Linked skills block — appended skill instructions
///
/// Any layer that is empty is skipped. Layers are separated by double newlines.
fn build_system_prompt_with_skills(
    base_prompt: &str,
    agent_prompt: Option<&str>,
    skills: &[clawkson_db::skill::SkillRow],
) -> Option<String> {
    let mut parts: Vec<&str> = Vec::new();

    if !base_prompt.trim().is_empty() {
        parts.push(base_prompt);
    }
    if let Some(ap) = agent_prompt {
        if !ap.trim().is_empty() {
            parts.push(ap);
        }
    }

    // If no base or agent prompt and no skills, return None (no system message at all)
    if parts.is_empty() && skills.is_empty() {
        return None;
    }

    let mut prompt = parts.join("\n\n");

    if !skills.is_empty() {
        if !prompt.is_empty() {
            prompt.push_str("\n\n");
        }
        prompt.push_str("<available-skills>\n");
        prompt.push_str("You have access to the following skills. Use them automatically when they are relevant ");
        prompt.push_str("to the user's request, or when the user explicitly invokes one with /skill-name.\n\n");
        for skill in skills {
            prompt.push_str(&format!(
                "<skill name=\"{}\">\nDescription: {}\n\n{}\n</skill>\n\n",
                skill.name, skill.description, skill.instructions
            ));
        }
        prompt.push_str("</available-skills>");
    }

    Some(prompt)
}

/// Build a `<user-context>` block containing the current user's identity and their
/// connector metadata (names, types, context/config hints). This gives the LLM enough
/// information to act without asking the user for details it already knows.
async fn build_user_context(state: &AppState, user: &clawkson_core::User) -> String {
    let mut parts: Vec<String> = Vec::new();

    // User identity
    parts.push(format!("User: {} ({})", user.display_name, user.email));
    if !user.bio.trim().is_empty() {
        parts.push(format!("Bio: {}", user.bio.trim()));
    }

    // Connector metadata — expose non-secret config fields + context
    if let Ok(connectors) = clawkson_db::connector::list_for_user(&state.db, user.id).await {
        let enabled: Vec<_> = connectors.into_iter().filter(|c| c.enabled).collect();
        if !enabled.is_empty() {
            parts.push(String::new()); // blank line
            parts.push("Connected services:".to_string());
            for c in &enabled {
                let mut line = format!("- {} ({:?})", c.name, c.connector_type);

                // Extract safe metadata from config (org, project, etc.) — never expose secrets
                let safe_keys = ["organization", "project", "base_url", "instance", "team", "workspace", "channel"];
                let mut meta: Vec<String> = Vec::new();
                if let Some(obj) = c.config.as_object() {
                    for key in &safe_keys {
                        if let Some(val) = obj.get(*key).and_then(|v| v.as_str()) {
                            if !val.is_empty() {
                                meta.push(format!("{key}: {val}"));
                            }
                        }
                    }
                }
                if !meta.is_empty() {
                    line.push_str(&format!(" [{}]", meta.join(", ")));
                }

                // Append connector context if set
                if !c.context.trim().is_empty() {
                    line.push_str(&format!(" — {}", c.context.trim()));
                }

                parts.push(line);
            }
        }
    }

    format!("<user-context>\n{}\n</user-context>", parts.join("\n"))
}

/// Scan user message for `/skill-name` references and add explicit invocation markers.
/// The full instructions are already in the system prompt, so this just signals
/// that the user explicitly wants to use a particular skill.
pub(crate) async fn expand_skill_references(
    state: &AppState,
    agent_id: Uuid,
    content: &str,
) -> String {
    let skills = clawkson_db::skill::agent_list_skills(state.db.pool(), agent_id)
        .await
        .unwrap_or_default();

    if skills.is_empty() {
        return content.to_string();
    }

    let mut invoked: Vec<&str> = Vec::new();

    for skill in &skills {
        let slash_name = format!("/{}", skill.name);
        if content.contains(&slash_name) {
            invoked.push(&skill.name);
        }
    }

    if invoked.is_empty() {
        return content.to_string();
    }

    let names = invoked.iter().map(|n| format!("/{n}")).collect::<Vec<_>>().join(", ");
    format!("{content}\n\n[Skill invoked: {names} — follow the skill instructions from your system prompt.]")
}

/// Load an LLM connector from DB by ID.
pub(crate) async fn load_llm_connector(state: &AppState, id: Uuid) -> Option<LlmConnector> {
    let row = clawkson_db::llm_connector::get_by_id(&state.db, id).await.ok()??;
    Some(row_to_llm_connector(row))
}

fn row_to_llm_connector(row: clawkson_db::llm_connector::LlmConnectorRow) -> LlmConnector {
    LlmConnector {
        id: row.id,
        name: row.name,
        provider_type: match row.provider_type {
            clawkson_db::llm_connector::LlmProviderType::Azure => LlmProviderType::Azure,
            clawkson_db::llm_connector::LlmProviderType::Openrouter => LlmProviderType::OpenRouter,
            clawkson_db::llm_connector::LlmProviderType::Openai => LlmProviderType::OpenAi,
            clawkson_db::llm_connector::LlmProviderType::Custom => LlmProviderType::Custom,
        },
        api_key: row.api_key,
        api_base_url: row.api_base_url,
        model: row.model,
        azure_deployment: row.azure_deployment,
        azure_api_version: row.azure_api_version,
        created_at: row.created_at,
    }
}

/// A history entry: (role, text_content, attachment_rows_for_this_message).
pub(crate) type HistoryEntry = (MessageRole, String, Vec<clawkson_db::chat_attachment::ChatAttachmentRow>);

/// Load message history from DB for a conversation, including attachment metadata per message.
pub(crate) async fn load_history(state: &AppState, conv_id: Uuid) -> Result<Vec<HistoryEntry>, StatusCode> {
    let rows = clawkson_db::message::list_for_conversation(&state.db, conv_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let pool = state.db.pool();
    let mut result = Vec::with_capacity(rows.len());
    for m in rows {
        let role = match m.role {
            clawkson_db::message::MessageRole::User => MessageRole::User,
            clawkson_db::message::MessageRole::Assistant => MessageRole::Assistant,
            clawkson_db::message::MessageRole::System => MessageRole::System,
            clawkson_db::message::MessageRole::Tool => MessageRole::Tool,
        };
        // Only user messages ever have attachments, but querying for all is safe and cheap.
        let attachments = clawkson_db::chat_attachment::list_for_message(pool, m.id)
            .await
            .unwrap_or_default();
        result.push((role, m.content, attachments));
    }
    Ok(result)
}

/// Enrich history: resolve attachment metadata into either base64 data URLs (for
/// vision-capable providers) or appended text descriptions (fallback).
///
/// PDFs are rendered to page images (via poppler-utils) so the LLM can visually
/// read each page. Non-PDF attachments (images) are passed through as data URLs.
///
/// Returns a plain `Vec<(MessageRole, String, Vec<String>)>` ready for `llm.rs`.
pub(crate) async fn enrich_history(
    state: &AppState,
    history: Vec<HistoryEntry>,
    supports_vision: bool,
    container_enabled: bool,
) -> Vec<(MessageRole, String, Vec<String>)> {
    let mut enriched = Vec::with_capacity(history.len());
    for (role, mut content, attachments) in history {
        let mut image_urls: Vec<String> = Vec::new();

        if !attachments.is_empty() {
            if supports_vision {
                if let Some(s3) = &state.s3 {
                    for att in &attachments {
                        match s3.get_object(&att.s3_key).await {
                            Ok((bytes, _ct)) => {
                                if att.content_type == "application/pdf" {
                                    // Render PDF pages to images for visual comprehension.
                                    match crate::pdf::pdf_to_page_images(&bytes).await {
                                        Ok(result) => {
                                            if !result.page_images.is_empty() {
                                                // Prepend a note about the PDF
                                                let page_note = if result.total_pages > result.page_images.len() {
                                                    format!(
                                                        "\n\n[Attached PDF: {} — showing {} of {} pages as images]",
                                                        att.filename, result.page_images.len(), result.total_pages
                                                    )
                                                } else {
                                                    format!(
                                                        "\n\n[Attached PDF: {} — {} page(s)]",
                                                        att.filename, result.total_pages
                                                    )
                                                };
                                                content.push_str(&page_note);
                                                image_urls.extend(result.page_images);
                                            } else if let Some(text) = result.fallback_text {
                                                // Rendering failed, use extracted text
                                                content.push_str(&format!(
                                                    "\n\n[Attached PDF: {} — extracted text:]\n{}",
                                                    att.filename, text
                                                ));
                                            }
                                        }
                                        Err(e) => {
                                            tracing::warn!("failed to render PDF {}: {e}", att.filename);
                                            content.push_str(&format!(
                                                "\n\n[Attached PDF: {} — rendering failed: {e}]",
                                                att.filename
                                            ));
                                        }
                                    }
                                } else if att.content_type.starts_with("image/") {
                                    // Image attachment — pass through as data URL.
                                    let b64 = base64::Engine::encode(
                                        &base64::engine::general_purpose::STANDARD,
                                        &bytes,
                                    );
                                    image_urls.push(format!("data:{};base64,{b64}", att.content_type));
                                } else {
                                    // Non-image, non-PDF attachment — describe inline.
                                    let kb = att.size_bytes / 1024;
                                    if container_enabled {
                                        let safe_name = att.filename.replace(['/', '\\', '\0'], "_");
                                        content.push_str(&format!(
                                            "\n\n[Attached file: {} ({}, {} KB) — available at /workspace/inputs/{}]",
                                            att.filename, att.content_type, kb, safe_name
                                        ));
                                    } else {
                                        content.push_str(&format!(
                                            "\n\n[Attached file: {} ({}, {} KB)]",
                                            att.filename, att.content_type, kb
                                        ));
                                    }
                                }
                            }
                            Err(e) => {
                                tracing::warn!("failed to fetch attachment {} from S3: {e}", att.id);
                            }
                        }
                    }
                }
            } else {
                // Non-vision fallback: extract PDF text or describe the attachment inline.
                for att in &attachments {
                    if att.content_type == "application/pdf" {
                        // Try to extract text even without vision support
                        if let Some(s3) = &state.s3 {
                            if let Ok((bytes, _)) = s3.get_object(&att.s3_key).await {
                                match crate::pdf::pdf_to_page_images(&bytes).await {
                                    Ok(result) if result.fallback_text.is_some() => {
                                        content.push_str(&format!(
                                            "\n\n[Attached PDF: {} — extracted text:]\n{}",
                                            att.filename, result.fallback_text.unwrap()
                                        ));
                                        continue;
                                    }
                                    _ => {}
                                }
                            }
                        }
                    }
                    let kb = att.size_bytes / 1024;
                    if container_enabled {
                        let safe_name = att.filename.replace(['/', '\\', '\0'], "_");
                        content.push_str(&format!(
                            "\n\n[Attached file: {} ({}, {} KB) — available at /workspace/inputs/{}]",
                            att.filename, att.content_type, kb, safe_name
                        ));
                    } else {
                        content.push_str(&format!(
                            "\n\n[Attached file: {} ({}, {} KB) — image content not available for this model]",
                            att.filename, att.content_type, kb
                        ));
                    }
                }
            }
        }

        enriched.push((role, content, image_urls));
    }
    enriched
}

/// Build the tool registry for an agent (code execution + knowledge search + http).
/// When `search_enabled` is false the knowledge tools are omitted even if the
/// agent has linked knowledge bases.
///
/// All tools are wrapped with permission guards that:
///   - Check `TaskPermissionOverride` for built-in tools
///   - Check `ConnectorPolicy` for the authenticated HTTP tool
///   - Record audit log entries for every invocation (allowed or denied)
pub(crate) async fn build_tool_registry(state: &AppState, agent_cfg: &AgentConfig, conversation_id: Uuid, user_id: Uuid, search_enabled: bool) -> denkwerk::FunctionRegistry {
    let mut registry = denkwerk::FunctionRegistry::new();

    // Build the guard context (shared across all guarded tools)
    // We need the connector name→ID mapping for the HTTP tool guard
    let connector_name_to_id: std::collections::HashMap<String, Uuid> =
        match clawkson_db::connector::list_for_user(&state.db, user_id).await {
            Ok(connectors) => connectors
                .into_iter()
                .filter(|c| c.enabled)
                .map(|c| (c.name.to_lowercase(), c.id))
                .collect(),
            Err(_) => std::collections::HashMap::new(),
        };

    let guard_ctx = crate::permission_guard::GuardContext {
        db: state.db.clone(),
        conversation_id,
        agent_id: agent_cfg.agent_id,
        user_id,
        connector_policies: agent_cfg.connector_policies.clone(),
        task_override: None, // TODO: populate from conversation metadata when task overrides are implemented
        connector_name_to_id: connector_name_to_id.clone(),
    };

    // Code execution tool (requires container)
    if agent_cfg.container_enabled {
        if let Some(cm) = &state.container_manager {
            // Auto-start container for this conversation if needed
            if cm.get_container(agent_cfg.agent_id, conversation_id).await.is_none() {
                let config = agent_cfg.container_config
                    .as_ref()
                    .map(|ac| clawkson_container::ContainerConfig {
                        image: ac.image.clone().unwrap_or_else(|| "clawkson-sandbox:latest".to_string()),
                        cpu_limit: ac.cpu_limit,
                        memory_limit_mb: ac.memory_limit_mb,
                        network_enabled: ac.network_enabled,
                        permissions: ac.permissions.clone(),
                    })
                    .unwrap_or_default();
                if let Err(e) = cm.start_container(agent_cfg.agent_id, conversation_id, &config).await {
                    tracing::error!("failed to auto-start container: {e}");
                }
            }
            let workspace_root = cm.workspace_root().to_path_buf();

            let code_tool = crate::tools::CodeExecutionTool::new(agent_cfg.agent_id, conversation_id, cm.clone(), workspace_root.clone());
            let guarded = crate::permission_guard::GuardedBuiltinTool::new(code_tool.into_dyn(), "code_execution".to_string(), guard_ctx.clone());
            registry.register(guarded.into_dyn());

            // Workspace tools — let the LLM read, write, and list files in its workspace
            let read_tool = crate::tools::WorkspaceReadTool::new(agent_cfg.agent_id, conversation_id, workspace_root.clone());
            let guarded = crate::permission_guard::GuardedBuiltinTool::new(read_tool.into_dyn(), "workspace_read".to_string(), guard_ctx.clone());
            registry.register(guarded.into_dyn());

            let write_tool = crate::tools::WorkspaceWriteTool::new(agent_cfg.agent_id, conversation_id, workspace_root.clone());
            let guarded = crate::permission_guard::GuardedBuiltinTool::new(write_tool.into_dyn(), "workspace_write".to_string(), guard_ctx.clone());
            registry.register(guarded.into_dyn());

            let list_tool = crate::tools::WorkspaceListTool::new(agent_cfg.agent_id, conversation_id, workspace_root.clone());
            let guarded = crate::permission_guard::GuardedBuiltinTool::new(list_tool.into_dyn(), "workspace_list".to_string(), guard_ctx.clone());
            registry.register(guarded.into_dyn());

            // Live preview tool — lets the agent register a web server for inline display
            let preview_tool = crate::tools::StartPreviewTool::new(agent_cfg.agent_id, conversation_id);
            let guarded = crate::permission_guard::GuardedBuiltinTool::new(preview_tool.into_dyn(), "start_preview".to_string(), guard_ctx.clone());
            registry.register(guarded.into_dyn());

            // Browser tool — interactive browser control (navigate, click, type, screenshot)
            // Registered when agent has container + network so Chromium can fetch pages.
            let net_enabled = agent_cfg.container_config.as_ref()
                .map(|c| c.permissions.network.enabled || c.network_enabled)
                .unwrap_or(false);
            if net_enabled {
                let browser_tool = crate::browser_tools::BrowserTool::new(
                    agent_cfg.agent_id, conversation_id, cm.clone(), workspace_root,
                );
                let guarded = crate::permission_guard::GuardedBuiltinTool::new(
                    browser_tool.into_dyn(), "browser".to_string(), guard_ctx.clone(),
                );
                registry.register(guarded.into_dyn());
            }
        }
    }

    // Knowledge search tool (available if agent has linked KBs or user has memory, and search is enabled)
    if search_enabled {
        let has_kbs = clawkson_db::knowledge_base::agent_list_kbs(state.db.pool(), agent_cfg.agent_id)
            .await
            .map(|kbs| !kbs.is_empty())
            .unwrap_or(false);

        // Check if user has a memory KB and include it in search scope
        let memory_kb_ids: Vec<Uuid> = match clawkson_db::knowledge_base::get_or_create_memory_kb(
            state.db.pool(),
            user_id,
            "",  // model doesn't matter for lookup, only creation
        ).await {
            Ok(kb) => vec![kb.id],
            Err(_) => vec![],
        };

        let has_memory = !memory_kb_ids.is_empty();

        if has_kbs || has_memory {
            let list_tool = crate::tools::KnowledgeListTool::new(agent_cfg.agent_id, state.db.clone());
            let guarded = crate::permission_guard::GuardedBuiltinTool::new(list_tool.into_dyn(), "knowledge_list".to_string(), guard_ctx.clone());
            registry.register(guarded.into_dyn());

            let search_tool = crate::tools::KnowledgeSearchTool::new(agent_cfg.agent_id, state.db.clone())
                .with_extra_kbs(memory_kb_ids);
            let guarded = crate::permission_guard::GuardedBuiltinTool::new(search_tool.into_dyn(), "knowledge_search".to_string(), guard_ctx.clone());
            registry.register(guarded.into_dyn());
        }
    }

    // Connector-derived tools
    if let Ok(connectors) = clawkson_db::connector::list_for_user(&state.db, user_id).await {
        let mut http_connectors: Vec<crate::tools::http_tool::ConnectorAuth> = Vec::new();

        for c in connectors.into_iter().filter(|c| c.enabled) {
            match c.connector_type {
                clawkson_db::connector::ConnectorType::Tavily => {
                    if let Some(api_key) = c.config.get("api_key").and_then(|v| v.as_str()) {
                        let provider = crate::tools::SearchProvider::Tavily { api_key: api_key.to_string() };
                        let tool = crate::tools::WebSearchTool::new(provider);
                        let guarded = crate::permission_guard::GuardedBuiltinTool::new(
                            tool.into_dyn(), "web_search".to_string(), guard_ctx.clone(),
                        );
                        registry.register(guarded.into_dyn());
                    }
                }
                clawkson_db::connector::ConnectorType::Bing => {
                    if let Some(api_key) = c.config.get("api_key").and_then(|v| v.as_str()) {
                        let endpoint = c.config.get("endpoint")
                            .and_then(|v| v.as_str())
                            .unwrap_or("https://api.bing.microsoft.com/v7.0/search")
                            .to_string();
                        let provider = crate::tools::SearchProvider::Bing { api_key: api_key.to_string(), endpoint };
                        let tool = crate::tools::WebSearchTool::new(provider);
                        let guarded = crate::permission_guard::GuardedBuiltinTool::new(
                            tool.into_dyn(), "web_search".to_string(), guard_ctx.clone(),
                        );
                        registry.register(guarded.into_dyn());
                    }
                }
                _ => {
                    // All other connector types use the generic authenticated_http tool
                    http_connectors.push(crate::tools::http_tool::ConnectorAuth {
                        connector_name: c.name,
                        connector_type: c.connector_type,
                        config: c.config,
                    });
                }
            }
        }

        if !http_connectors.is_empty() {
            let tool = crate::tools::AuthenticatedHttpTool::new(http_connectors);
            let guarded = crate::permission_guard::GuardedHttpTool::new(tool.into_dyn(), guard_ctx.clone());
            registry.register(guarded.into_dyn());
        }
    }

    registry
}

/// Rough token estimate: ~4 chars per token for English text.
/// Includes text content and base64 image data URLs.
fn estimate_tokens(history: &[(MessageRole, String, Vec<String>)]) -> usize {
    history.iter().map(|(_, content, images)| {
        let text_tokens = content.len() / 4;
        let image_tokens: usize = images.iter().map(|img| img.len() / 4).sum();
        text_tokens + image_tokens
    }).sum()
}

/// Truncate history from the front to fit within a token budget, always keeping
/// the most recent messages. Returns a slice of the input starting from the
/// first message that fits.
fn truncate_history(
    history: &[(MessageRole, String, Vec<String>)],
    max_tokens: usize,
) -> &[(MessageRole, String, Vec<String>)] {
    let total = estimate_tokens(history);
    if total <= max_tokens {
        return history;
    }

    // Walk from the end, accumulating tokens until we hit the budget
    let mut budget = max_tokens;
    let mut start_idx = history.len();
    for (i, (_, content, images)) in history.iter().enumerate().rev() {
        let msg_tokens = content.len() / 4
            + images.iter().map(|img| img.len() / 4).sum::<usize>();
        if msg_tokens > budget {
            break;
        }
        budget -= msg_tokens;
        start_idx = i;
    }

    // Always keep at least the last message
    if start_idx >= history.len() {
        start_idx = history.len().saturating_sub(1);
    }

    let trimmed = history.len() - start_idx;
    if trimmed < history.len() {
        tracing::info!(
            total_messages = history.len(),
            kept = trimmed,
            dropped = history.len() - trimmed,
            estimated_total_tokens = total,
            "truncated conversation history to fit token budget"
        );
    }

    &history[start_idx..]
}

/// Run LLM completion with optional tool-calling.
/// Max tokens reserved for conversation history. Leaves room for system prompt,
/// tool definitions, and model output within the provider's context window.
const HISTORY_TOKEN_BUDGET: usize = 200_000;

pub(crate) async fn run_completion(
    state: &AppState,
    connector: &clawkson_core::LlmConnector,
    agent_cfg: &AgentConfig,
    history: &[(MessageRole, String, Vec<String>)],
    reasoning_effort: Option<&ReasoningEffort>,
    conversation_id: Uuid,
    user_id: Uuid,
    search_enabled: bool,
    timeout_secs: u64,
) -> anyhow::Result<String> {
    let history = truncate_history(history, HISTORY_TOKEN_BUDGET);

    let mut registry = build_tool_registry(state, agent_cfg, conversation_id, user_id, search_enabled).await;

    // Register the delegation tool for sub-agent coordination (non-streaming, no tx)
    let delegate_tool = crate::subtask::DelegateTasksTool::new(
        state.clone(),
        agent_cfg.clone(),
        connector.clone(),
        conversation_id,
        user_id,
        search_enabled,
        timeout_secs,
        None,
    );
    registry.register(delegate_tool.into_dyn());

    if !registry.definitions().is_empty() {
        return crate::llm::complete_with_tools(
            connector,
            agent_cfg.system_prompt.as_deref(),
            history,
            agent_cfg.temperature,
            agent_cfg.max_tokens,
            &registry,
            10,
            reasoning_effort,
            timeout_secs,
        )
        .await;
    }

    crate::llm::complete(
        connector,
        agent_cfg.system_prompt.as_deref(),
        history,
        agent_cfg.temperature,
        agent_cfg.max_tokens,
        reasoning_effort,
        timeout_secs,
    )
    .await
}

/// POST /api/conversations/{id}/chat
async fn chat(
    auth: AuthUser,
    State(state): State<AppState>,
    Path(conv_id): Path<Uuid>,
    Json(req): Json<ChatRequest>,
) -> impl IntoResponse {
    // Check write access
    match can_write(&state, conv_id, auth.id(), auth.is_admin()).await {
        Ok(true) => {}
        Ok(false) => return (StatusCode::FORBIDDEN, Json(serde_json::json!({"error": "forbidden"}))).into_response(),
        Err(status) => return (status, Json(serde_json::json!({"error": "not found"}))).into_response(),
    }

    // 1. Get conversation
    let conversation = match clawkson_db::conversation::get_by_id(&state.db, conv_id).await {
        Ok(Some(c)) => c,
        _ => return (StatusCode::NOT_FOUND, Json(serde_json::json!({"error": "conversation not found"}))).into_response(),
    };
    let agent_id = conversation.agent_id.unwrap_or(Uuid::nil());

    // 2. Expand skill references in user message
    let expanded_content = expand_skill_references(&state, agent_id, &req.content).await;

    // 3. Save user message (with expanded skill instructions)
    let user_msg = match save_message(&state, conv_id, MessageRole::User, &expanded_content).await {
        Ok(m) => m,
        Err(s) => return (s, Json(serde_json::json!({"error": "failed to save message"}))).into_response(),
    };

    // 3b. Link any uploaded attachments to the user message
    if !req.attachment_ids.is_empty() {
        let pool = state.db.pool();
        for att_id in &req.attachment_ids {
            if let Err(e) = clawkson_db::chat_attachment::link_to_message(pool, *att_id, user_msg.id).await {
                tracing::warn!("failed to link attachment {att_id} to message {}: {e}", user_msg.id);
            }
        }
    }

    // 3c. If the agent has a container, also write attachments into /workspace/inputs/
    // so the running container can access them directly via the filesystem.
    if !req.attachment_ids.is_empty() {
        if let Some(cm) = &state.container_manager {
            // We need the agent config to check container_enabled; do a quick DB fetch.
            let agent_row = clawkson_db::agent::get_by_id(&state.db, agent_id).await.ok().flatten();
            let container_enabled = agent_row.map(|a| a.container_enabled).unwrap_or(false);

            if container_enabled {
                if let Some(s3) = &state.s3 {
                    let workspace_root = cm.workspace_root().to_path_buf();
                    let inputs_dir = workspace_root
                        .join(agent_id.to_string())
                        .join(conv_id.to_string())
                        .join("inputs");
                    let _ = tokio::fs::create_dir_all(&inputs_dir).await;

                    let pool = state.db.pool();
                    for att_id in &req.attachment_ids {
                        if let Ok(Some(att)) = clawkson_db::chat_attachment::get_by_id(pool, *att_id).await {
                            match s3.get_object(&att.s3_key).await {
                                Ok((bytes, _)) => {
                                    // Sanitise filename (no path separators)
                                    let safe_name = att.filename
                                        .replace(['/', '\\', '\0'], "_");
                                    let dest = inputs_dir.join(&safe_name);
                                    if let Err(e) = tokio::fs::write(&dest, &bytes).await {
                                        tracing::warn!(
                                            "failed to write attachment {} to workspace: {e}",
                                            att.filename
                                        );
                                    } else {
                                        tracing::debug!(
                                            "wrote attachment {} to workspace inputs",
                                            safe_name
                                        );
                                    }
                                }
                                Err(e) => {
                                    tracing::warn!(
                                        "failed to fetch attachment {} from S3 for workspace: {e}",
                                        att_id
                                    );
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    // 4. Resolve LLM connector
    let connector_id = resolve_connector_id(&state, agent_id).await;
    let Some(connector_id) = connector_id else {
        let err_msg = save_message(&state, conv_id, MessageRole::Assistant,
            "No LLM connector configured for this agent. Please add an inference connector in Settings and assign it to the agent."
        ).await.unwrap_or(user_msg.clone());
        return Json(ChatResponse { user_message: user_msg, assistant_message: err_msg }).into_response();
    };

    // 4. Load agent config and connector
    let mut agent_cfg = load_agent_config(&state, agent_id).await;
    // Inject user + connector context into system prompt
    {
        let user_ctx = build_user_context(&state, &auth.0).await;
        if let Some(ref mut cfg) = agent_cfg {
            match &mut cfg.system_prompt {
                Some(ref mut prompt) => { prompt.push_str("\n\n"); prompt.push_str(&user_ctx); }
                None => cfg.system_prompt = Some(user_ctx),
            }
        }
    }
    let connector = load_llm_connector(&state, connector_id).await;
    let Some(connector) = connector else {
        let err_msg = save_message(&state, conv_id, MessageRole::Assistant,
            "Configured LLM connector not found. Please check your connector settings."
        ).await.unwrap_or(user_msg.clone());
        return Json(ChatResponse { user_message: user_msg, assistant_message: err_msg }).into_response();
    };

    // 5. Load history from DB and enrich with attachment data
    let raw_history = match load_history(&state, conv_id).await {
        Ok(h) => h,
        Err(s) => return (s, Json(serde_json::json!({"error": "failed to load history"}))).into_response(),
    };
    let supports_vision = {
        use crate::llm::provider_supports_vision;
        provider_supports_vision(&connector)
    };
    let agent_has_container = agent_cfg.as_ref().map(|c| c.container_enabled).unwrap_or(false);
    let history = enrich_history(&state, raw_history, supports_vision, agent_has_container).await;

    // 6. Call LLM
    let default_cfg = AgentConfig {
        agent_id,
        system_prompt: None,
        temperature: None,
        max_tokens: None,
        container_enabled: false,
        container_config: None,
        connector_policies: vec![],
        subtask_llm_connector_id: None,
    };
    let cfg = agent_cfg.as_ref().unwrap_or(&default_cfg);

    let timeout_secs = clawkson_db::settings::get(&state.db)
        .await
        .map(|s| s.llm_request_timeout_secs as u64)
        .unwrap_or(120);

    let assistant_content = match run_completion(&state, &connector, cfg, &history, req.reasoning_effort.as_ref(), conv_id, auth.id(), req.search_enabled, timeout_secs).await {
        Ok(text) => text,
        Err(e) => {
            tracing::error!("LLM completion failed: {e}");
            format!("Error calling LLM: {e}")
        }
    };

    // 7. Save assistant message + touch conversation
    let mut assistant_msg = save_message(&state, conv_id, MessageRole::Assistant, &assistant_content)
        .await
        .unwrap_or(Message {
            id: Uuid::new_v4(),
            conversation_id: conv_id,
            role: MessageRole::Assistant,
            content: assistant_content.clone(),
            created_at: chrono::Utc::now(),
            attachments: Vec::new(),
        });
    let _ = clawkson_db::conversation::touch(&state.db, conv_id).await;

    // 7b. Debounced: buffer chat turn for memory embedding
    {
        let mem = state.memory.clone();
        let title = conversation.title.clone();
        let user_content = expanded_content.clone();
        let asst_content = assistant_content;
        let uid = auth.id();
        tokio::spawn(async move {
            mem.push_turn(conv_id, uid, title, user_content, asst_content).await;
        });
    }

    // 8. Auto-attach any files the agent wrote to /workspace/outputs/
    if cfg.container_enabled {
        attach_workspace_outputs(&state, agent_id, assistant_msg.id, auth.id(), conv_id).await;
        // Re-fetch attachments so the response includes them
        if let Ok(atts) = clawkson_db::chat_attachment::list_for_message(state.db.pool(), assistant_msg.id).await {
            assistant_msg.attachments = atts
                .into_iter()
                .map(|a| clawkson_core::MessageAttachment {
                    id: a.id,
                    filename: a.filename,
                    content_type: a.content_type,
                    size_bytes: a.size_bytes,
                    metadata: a.metadata,
                })
                .collect();
        }
    }

    Json(ChatResponse {
        user_message: user_msg,
        assistant_message: assistant_msg,
    })
    .into_response()
}

/// POST /api/conversations/{id}/chat/stream
async fn chat_stream(
    auth: AuthUser,
    State(state): State<AppState>,
    Path(conv_id): Path<Uuid>,
    Json(req): Json<ChatRequest>,
) -> impl IntoResponse {
    use tokio::sync::mpsc;

    // Check write access
    match can_write(&state, conv_id, auth.id(), auth.is_admin()).await {
        Ok(true) => {}
        Ok(false) | Err(_) => {
            let s = stream::once(async {
                Ok::<Event, Infallible>(Event::default().data(r#"{"error":"forbidden"}"#))
            });
            return Sse::new(s).into_response();
        }
    }

    // Get conversation
    let conversation = match clawkson_db::conversation::get_by_id(&state.db, conv_id).await {
        Ok(Some(c)) => c,
        _ => {
            let s = stream::once(async {
                Ok::<Event, Infallible>(Event::default().data(r#"{"error":"conversation not found"}"#))
            });
            return Sse::new(s).into_response();
        }
    };
    let agent_id = conversation.agent_id.unwrap_or(Uuid::nil());

    // Expand skill references in user message
    let expanded_content = expand_skill_references(&state, agent_id, &req.content).await;

    // Save user message (with expanded skill instructions)
    let user_msg_id = match clawkson_db::message::create(
        &state.db, conv_id, None,
        clawkson_db::message::MessageRole::User,
        &expanded_content, None, None,
    ).await {
        Ok(row) => Some(row.id),
        Err(e) => {
            tracing::error!("failed to save user message: {e}");
            None
        }
    };

    // Link any uploaded attachments to the user message
    if let Some(msg_id) = user_msg_id {
        if !req.attachment_ids.is_empty() {
            let pool = state.db.pool();
            for att_id in &req.attachment_ids {
                if let Err(e) = clawkson_db::chat_attachment::link_to_message(pool, *att_id, msg_id).await {
                    tracing::warn!("failed to link attachment {att_id} to message {msg_id}: {e}");
                }
            }
        }
    }

    // Write attachments into /workspace/inputs/ if agent has a container
    if !req.attachment_ids.is_empty() {
        if let Some(cm) = &state.container_manager {
            let agent_row = clawkson_db::agent::get_by_id(&state.db, agent_id).await.ok().flatten();
            let container_enabled = agent_row.map(|a| a.container_enabled).unwrap_or(false);
            if container_enabled {
                if let Some(s3) = &state.s3 {
                    let workspace_root = cm.workspace_root().to_path_buf();
                    let inputs_dir = workspace_root
                        .join(agent_id.to_string())
                        .join(conv_id.to_string())
                        .join("inputs");
                    let _ = tokio::fs::create_dir_all(&inputs_dir).await;
                    let pool = state.db.pool();
                    for att_id in &req.attachment_ids {
                        if let Ok(Some(att)) = clawkson_db::chat_attachment::get_by_id(pool, *att_id).await {
                            match s3.get_object(&att.s3_key).await {
                                Ok((bytes, _)) => {
                                    let safe_name = att.filename.replace(['/', '\\', '\0'], "_");
                                    let dest = inputs_dir.join(&safe_name);
                                    if let Err(e) = tokio::fs::write(&dest, &bytes).await {
                                        tracing::warn!("failed to write attachment {} to workspace: {e}", att.filename);
                                    }
                                }
                                Err(e) => {
                                    tracing::warn!("failed to fetch attachment {} from S3 for workspace: {e}", att_id);
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    // Resolve connector
    let connector_id = resolve_connector_id(&state, agent_id).await;
    let Some(connector_id) = connector_id else {
        let s = stream::once(async {
            Ok::<Event, Infallible>(Event::default().data(r#"{"error":"no LLM connector configured"}"#))
        });
        return Sse::new(s).into_response();
    };

    // Load agent config + connector + history
    let mut agent_cfg = load_agent_config(&state, agent_id).await;
    // Inject user + connector context into system prompt
    {
        let user_ctx = build_user_context(&state, &auth.0).await;
        if let Some(ref mut cfg) = agent_cfg {
            match &mut cfg.system_prompt {
                Some(ref mut prompt) => { prompt.push_str("\n\n"); prompt.push_str(&user_ctx); }
                None => cfg.system_prompt = Some(user_ctx),
            }
        }
    }
    let connector = load_llm_connector(&state, connector_id).await;
    let Some(connector) = connector else {
        let s = stream::once(async {
            Ok::<Event, Infallible>(Event::default().data(r#"{"error":"LLM connector not found"}"#))
        });
        return Sse::new(s).into_response();
    };
    let raw_history = match load_history(&state, conv_id).await {
        Ok(h) => h,
        Err(_) => {
            let s = stream::once(async {
                Ok::<Event, Infallible>(Event::default().data(r#"{"error":"failed to load history"}"#))
            });
            return Sse::new(s).into_response();
        }
    };
    let supports_vision = {
        use crate::llm::provider_supports_vision;
        provider_supports_vision(&connector)
    };
    let agent_has_container = agent_cfg.as_ref().map(|c| c.container_enabled).unwrap_or(false);
    let full_history = enrich_history(&state, raw_history, supports_vision, agent_has_container).await;
    // Truncate history to fit within token budget
    let truncated = truncate_history(&full_history, HISTORY_TOKEN_BUDGET);
    let history = truncated.to_vec();

    let default_cfg = AgentConfig {
        agent_id,
        system_prompt: None,
        temperature: None,
        max_tokens: None,
        container_enabled: false,
        container_config: None,
        connector_policies: vec![],
        subtask_llm_connector_id: None,
    };
    let cfg = agent_cfg.unwrap_or(default_cfg);
    let mut registry = build_tool_registry(&state, &cfg, conv_id, auth.id(), req.search_enabled).await;
    let system_prompt = cfg.system_prompt.clone();
    let temperature = cfg.temperature;
    let max_tokens = cfg.max_tokens;
    let reasoning_effort = req.reasoning_effort.clone();
    let container_enabled = cfg.container_enabled;

    let timeout_secs = clawkson_db::settings::get(&state.db)
        .await
        .map(|s| s.llm_request_timeout_secs as u64)
        .unwrap_or(120);

    // Stream via channel — messages are prefixed to distinguish type:
    //   "\x01" + text  = reasoning delta
    //   "\x00DONE:id"  = completion sentinel
    //   anything else  = message delta
    let (tx, mut rx) = mpsc::channel::<String>(64);
    let cancel_token = tokio_util::sync::CancellationToken::new();

    // Register the delegation tool for sub-agent coordination (with streaming tx)
    {
        let delegate_tool = crate::subtask::DelegateTasksTool::new(
            state.clone(),
            cfg.clone(),
            connector.clone(),
            conv_id,
            auth.id(),
            req.search_enabled,
            timeout_secs,
            Some(tx.clone()),
        );
        registry.register(delegate_tool.into_dyn());
    }

    let state2 = state.clone();
    let owner_id = auth.id();
    let task_cancel = cancel_token.clone();

    tokio::spawn(async move {
        let workspace_path = state2.container_manager.as_ref().map(|cm|
            cm.workspace_root().join(agent_id.to_string()).join(conv_id.to_string())
        );

        let has_tools = !registry.definitions().is_empty();
        let result = if has_tools {
            crate::llm::complete_with_tools_streaming(
                &connector,
                system_prompt.as_deref(),
                &history,
                temperature,
                max_tokens,
                &registry,
                10,
                reasoning_effort.as_ref(),
                timeout_secs,
                &tx,
                workspace_path,
                task_cancel,
            )
            .await
        } else {
            crate::llm::stream_complete(
                &connector,
                system_prompt.as_deref(),
                &history,
                temperature,
                max_tokens,
                reasoning_effort.as_ref(),
                timeout_secs,
                |chunk| { let _ = tx.try_send(chunk); },
                |reasoning| { let _ = tx.try_send(format!("\x01{reasoning}")); },
            )
            .await
        };

        let assistant_content = match result {
            Ok(text) => text,
            Err(e) => {
                tracing::error!("LLM streaming failed: {e}");
                format!("Error: {e}")
            }
        };

        // Save assistant message to DB
        let msg_id = match clawkson_db::message::create(
            &state2.db, conv_id, None,
            clawkson_db::message::MessageRole::Assistant,
            &assistant_content, None, None,
        ).await {
            Ok(row) => row.id,
            Err(e) => {
                tracing::error!("failed to save assistant message: {e}");
                Uuid::new_v4()
            }
        };
        let _ = clawkson_db::conversation::touch(&state2.db, conv_id).await;

        // Debounced: buffer chat turn for memory embedding
        {
            let mem = state2.memory.clone();
            let title = conversation.title.clone();
            let user_content = expanded_content.clone();
            let asst_content = assistant_content.clone();
            tokio::spawn(async move {
                mem.push_turn(conv_id, owner_id, title, user_content, asst_content).await;
            });
        }

        // Auto-attach any files the agent wrote to /workspace/outputs/
        tracing::debug!("post-chat: container_enabled={container_enabled}, agent_id={agent_id}, msg_id={msg_id}");
        if container_enabled {
            attach_workspace_outputs(&state2, agent_id, msg_id, owner_id, conv_id).await;
        }

        let _ = tx.try_send(format!("\x00DONE:{msg_id}"));
    });

    // The drop guard cancels the token when the stream is dropped — this fires
    // whether the stream completes normally OR Axum drops it because the client
    // disconnected.  The explicit cancel() at the end of the loop would never
    // run on disconnect because the generator is killed mid-yield.
    let cancel_guard = cancel_token.drop_guard();

    let sse_stream = async_stream::stream! {
        // Move the guard into the generator so it lives (and dies) with it.
        let _guard = cancel_guard;

        while let Some(msg) = rx.recv().await {
            if let Some(id) = msg.strip_prefix("\x00DONE:") {
                let data = format!(r#"{{"done":true,"id":"{id}"}}"#);
                yield Ok::<Event, Infallible>(Event::default().data(data));
                break;
            } else if let Some(reasoning) = msg.strip_prefix("\x01") {
                let escaped = reasoning.replace('\\', "\\\\").replace('"', "\\\"").replace('\n', "\\n");
                let data = format!(r#"{{"reasoning_delta":"{escaped}"}}"#);
                yield Ok::<Event, Infallible>(Event::default().data(data));
            } else if let Some(tool_json) = msg.strip_prefix("\x02") {
                // Tool-call event — already valid JSON from serde_json::json!()
                let data = format!(r#"{{"tool_event":{tool_json}}}"#);
                yield Ok::<Event, Infallible>(Event::default().data(data));
            } else {
                let escaped = msg.replace('\\', "\\\\").replace('"', "\\\"").replace('\n', "\\n");
                let data = format!(r#"{{"delta":"{escaped}"}}"#);
                yield Ok::<Event, Infallible>(Event::default().data(data));
            }
        }
    };

    Sse::new(sse_stream).into_response()
}

