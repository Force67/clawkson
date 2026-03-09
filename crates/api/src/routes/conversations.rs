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
        .route("/", get(list_conversations).post(create_conversation))
        .route("/{id}", get(get_conversation).delete(delete_conversation))
        .route("/{id}/messages", get(list_messages).post(send_message))
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
async fn resolve_connector_id(
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
    match clawkson_db::conversation::delete(&state.db, id).await {
        Ok(true) => StatusCode::NO_CONTENT,
        Ok(false) => StatusCode::NOT_FOUND,
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR,
    }
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
    Ok(Json(rows.into_iter().map(msg_to_api).collect()))
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

/// Load agent config for chat handlers.
struct AgentConfig {
    agent_id: Uuid,
    system_prompt: Option<String>,
    temperature: Option<f64>,
    max_tokens: Option<u32>,
    container_enabled: bool,
    container_config: Option<clawkson_core::AgentContainerConfig>,
}

async fn load_agent_config(state: &AppState, agent_id: Uuid) -> Option<AgentConfig> {
    let row = clawkson_db::agent::get_by_id(&state.db, agent_id).await.ok()??;
    Some(AgentConfig {
        agent_id: row.id,
        system_prompt: row.system_prompt,
        temperature: row.temperature,
        max_tokens: row.max_tokens.map(|v| v as u32),
        container_enabled: row.container_enabled,
        container_config: row.container_config.and_then(|v| serde_json::from_value(v).ok()),
    })
}

/// Load an LLM connector from DB by ID.
async fn load_llm_connector(state: &AppState, id: Uuid) -> Option<LlmConnector> {
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
type HistoryEntry = (MessageRole, String, Vec<clawkson_db::chat_attachment::ChatAttachmentRow>);

/// Load message history from DB for a conversation, including attachment metadata per message.
async fn load_history(state: &AppState, conv_id: Uuid) -> Result<Vec<HistoryEntry>, StatusCode> {
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
/// Returns a plain `Vec<(MessageRole, String)>` ready for `llm.rs`.
async fn enrich_history(
    state: &AppState,
    history: Vec<HistoryEntry>,
    supports_vision: bool,
) -> Vec<(MessageRole, String, Vec<String>)> {
    let mut enriched = Vec::with_capacity(history.len());
    for (role, mut content, attachments) in history {
        let mut image_urls: Vec<String> = Vec::new();

        if !attachments.is_empty() {
            if supports_vision {
                // Fetch image bytes from S3 and encode as data URLs.
                if let Some(s3) = &state.s3 {
                    for att in &attachments {
                        match s3.get_object(&att.s3_key).await {
                            Ok((bytes, ct)) => {
                                let b64 = base64::Engine::encode(
                                    &base64::engine::general_purpose::STANDARD,
                                    &bytes,
                                );
                                image_urls.push(format!("data:{ct};base64,{b64}"));
                            }
                            Err(e) => {
                                tracing::warn!("failed to fetch attachment {} from S3: {e}", att.id);
                            }
                        }
                    }
                }
            } else {
                // Text fallback: describe each attachment inline.
                for att in &attachments {
                    let kb = att.size_bytes / 1024;
                    content.push_str(&format!(
                        "\n\n[Attached file: {} ({}, {} KB) — image content not available for this model]",
                        att.filename, att.content_type, kb
                    ));
                }
            }
        }

        enriched.push((role, content, image_urls));
    }
    enriched
}

/// Build the tool registry for an agent (code execution + knowledge search).
/// When `search_enabled` is false the knowledge tools are omitted even if the
/// agent has linked knowledge bases.
async fn build_tool_registry(state: &AppState, agent_cfg: &AgentConfig, search_enabled: bool) -> denkwerk::FunctionRegistry {
    let mut registry = denkwerk::FunctionRegistry::new();

    // Code execution tool (requires container)
    if agent_cfg.container_enabled {
        if let Some(cm) = &state.container_manager {
            // Auto-start container if needed
            if cm.get_container(agent_cfg.agent_id).await.is_none() {
                let config = agent_cfg.container_config
                    .as_ref()
                    .map(|ac| clawkson_container::ContainerConfig {
                        image: "python:3.12-slim".to_string(),
                        cpu_limit: ac.cpu_limit,
                        memory_limit_mb: ac.memory_limit_mb,
                        network_enabled: ac.network_enabled,
                    })
                    .unwrap_or_default();
                if let Err(e) = cm.start_container(agent_cfg.agent_id, &config).await {
                    tracing::error!("failed to auto-start container: {e}");
                }
            }
            let tool = crate::tools::CodeExecutionTool::new(agent_cfg.agent_id, cm.clone());
            registry.register(tool.into_dyn());
        }
    }

    // Knowledge search tool (available if agent has linked KBs and search is enabled)
    if search_enabled {
        let has_kbs = clawkson_db::knowledge_base::agent_list_kbs(state.db.pool(), agent_cfg.agent_id)
            .await
            .map(|kbs| !kbs.is_empty())
            .unwrap_or(false);

        if has_kbs {
            let list_tool = crate::tools::KnowledgeListTool::new(agent_cfg.agent_id, state.db.clone());
            registry.register(list_tool.into_dyn());
            let search_tool = crate::tools::KnowledgeSearchTool::new(agent_cfg.agent_id, state.db.clone());
            registry.register(search_tool.into_dyn());
        }
    }

    registry
}

/// Run LLM completion with optional tool-calling.
async fn run_completion(
    state: &AppState,
    connector: &clawkson_core::LlmConnector,
    agent_cfg: &AgentConfig,
    history: &[(MessageRole, String, Vec<String>)],
    reasoning_effort: Option<&ReasoningEffort>,
    search_enabled: bool,
) -> anyhow::Result<String> {
    let registry = build_tool_registry(state, agent_cfg, search_enabled).await;

    if !registry.definitions().is_empty() {
        return crate::llm::complete_with_tools(
            connector,
            agent_cfg.system_prompt.as_deref(),
            history,
            agent_cfg.temperature,
            agent_cfg.max_tokens,
            &registry,
            5,
            reasoning_effort,
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

    // 2. Save user message
    let user_msg = match save_message(&state, conv_id, MessageRole::User, &req.content).await {
        Ok(m) => m,
        Err(s) => return (s, Json(serde_json::json!({"error": "failed to save message"}))).into_response(),
    };

    // 2b. Link any uploaded attachments to the user message
    if !req.attachment_ids.is_empty() {
        let pool = state.db.pool();
        for att_id in &req.attachment_ids {
            if let Err(e) = clawkson_db::chat_attachment::link_to_message(pool, *att_id, user_msg.id).await {
                tracing::warn!("failed to link attachment {att_id} to message {}: {e}", user_msg.id);
            }
        }
    }

    // 3. Resolve LLM connector
    let connector_id = resolve_connector_id(&state, agent_id).await;
    let Some(connector_id) = connector_id else {
        let err_msg = save_message(&state, conv_id, MessageRole::Assistant,
            "No LLM connector configured for this agent. Please add an inference connector in Settings and assign it to the agent."
        ).await.unwrap_or(user_msg.clone());
        return Json(ChatResponse { user_message: user_msg, assistant_message: err_msg }).into_response();
    };

    // 4. Load agent config and connector
    let agent_cfg = load_agent_config(&state, agent_id).await;
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
    let history = enrich_history(&state, raw_history, supports_vision).await;

    // 6. Call LLM
    let default_cfg = AgentConfig {
        agent_id,
        system_prompt: None,
        temperature: None,
        max_tokens: None,
        container_enabled: false,
        container_config: None,
    };
    let cfg = agent_cfg.as_ref().unwrap_or(&default_cfg);

    let assistant_content = match run_completion(&state, &connector, cfg, &history, req.reasoning_effort.as_ref(), req.search_enabled).await {
        Ok(text) => text,
        Err(e) => {
            tracing::error!("LLM completion failed: {e}");
            format!("Error calling LLM: {e}")
        }
    };

    // 7. Save assistant message + touch conversation
    let assistant_msg = save_message(&state, conv_id, MessageRole::Assistant, &assistant_content)
        .await
        .unwrap_or(Message {
            id: Uuid::new_v4(),
            conversation_id: conv_id,
            role: MessageRole::Assistant,
            content: assistant_content,
            created_at: chrono::Utc::now(),
        });
    let _ = clawkson_db::conversation::touch(&state.db, conv_id).await;

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

    // Save user message
    let user_msg_id = match clawkson_db::message::create(
        &state.db, conv_id, None,
        clawkson_db::message::MessageRole::User,
        &req.content, None, None,
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

    // Resolve connector
    let connector_id = resolve_connector_id(&state, agent_id).await;
    let Some(connector_id) = connector_id else {
        let s = stream::once(async {
            Ok::<Event, Infallible>(Event::default().data(r#"{"error":"no LLM connector configured"}"#))
        });
        return Sse::new(s).into_response();
    };

    // Load agent config + connector + history
    let agent_cfg = load_agent_config(&state, agent_id).await;
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
    let history = enrich_history(&state, raw_history, supports_vision).await;

    let default_cfg = AgentConfig {
        agent_id,
        system_prompt: None,
        temperature: None,
        max_tokens: None,
        container_enabled: false,
        container_config: None,
    };
    let cfg = agent_cfg.unwrap_or(default_cfg);
    let registry = build_tool_registry(&state, &cfg, req.search_enabled).await;
    let system_prompt = cfg.system_prompt.clone();
    let temperature = cfg.temperature;
    let max_tokens = cfg.max_tokens;
    let reasoning_effort = req.reasoning_effort.clone();

    // Stream via channel — messages are prefixed to distinguish type:
    //   "\x01" + text  = reasoning delta
    //   "\x00DONE:id"  = completion sentinel
    //   anything else  = message delta
    let (tx, mut rx) = mpsc::channel::<String>(64);
    let state2 = state.clone();

    tokio::spawn(async move {
        let has_tools = !registry.definitions().is_empty();
        let result = if has_tools {
            let tool_result = crate::llm::complete_with_tools(
                &connector,
                system_prompt.as_deref(),
                &history,
                temperature,
                max_tokens,
                &registry,
                5,
                reasoning_effort.as_ref(),
            )
            .await;

            if let Ok(ref text) = tool_result {
                let _ = tx.try_send(text.clone());
            }
            tool_result
        } else {
            crate::llm::stream_complete(
                &connector,
                system_prompt.as_deref(),
                &history,
                temperature,
                max_tokens,
                reasoning_effort.as_ref(),
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
        let _ = tx.try_send(format!("\x00DONE:{msg_id}"));
    });

    let sse_stream = async_stream::stream! {
        while let Some(msg) = rx.recv().await {
            if let Some(id) = msg.strip_prefix("\x00DONE:") {
                let data = format!(r#"{{"done":true,"id":"{id}"}}"#);
                yield Ok::<Event, Infallible>(Event::default().data(data));
                break;
            } else if let Some(reasoning) = msg.strip_prefix("\x01") {
                let escaped = reasoning.replace('\\', "\\\\").replace('"', "\\\"").replace('\n', "\\n");
                let data = format!(r#"{{"reasoning_delta":"{escaped}"}}"#);
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
