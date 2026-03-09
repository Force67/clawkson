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
use chrono::Utc;
use clawkson_core::{Conversation, Message, MessageRole};
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

#[derive(Debug, Deserialize)]
pub struct ChatRequest {
    pub content: String,
}

#[derive(Debug, Serialize)]
pub struct ChatResponse {
    pub user_message: Message,
    pub assistant_message: Message,
}

// ── Access helpers ────────────────────────────────────────────────

/// Check if a user can access a conversation (owner, shared, or admin).
async fn can_access(state: &AppState, conv_id: Uuid, user_id: Uuid, is_admin: bool) -> Result<bool, StatusCode> {
    {
        let inner = state.inner.read().await;
        if let Some(conv) = inner.conversations.iter().find(|c| c.id == conv_id) {
            if is_admin || conv.owner_id == Some(user_id) {
                return Ok(true);
            }
        } else {
            return Err(StatusCode::NOT_FOUND);
        }
    }
    // Check shares
    let pool = state.db.pool();
    let share = clawkson_db::share::get_user_share(pool, conv_id, user_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(share.is_some())
}

/// Check if a user can write to a conversation (owner, write-share, or admin).
async fn can_write(state: &AppState, conv_id: Uuid, user_id: Uuid, is_admin: bool) -> Result<bool, StatusCode> {
    {
        let inner = state.inner.read().await;
        if let Some(conv) = inner.conversations.iter().find(|c| c.id == conv_id) {
            if is_admin || conv.owner_id == Some(user_id) {
                return Ok(true);
            }
        } else {
            return Err(StatusCode::NOT_FOUND);
        }
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
    conversation: &Conversation,
) -> Option<Uuid> {
    let inner = state.inner.read().await;
    let agent = inner.agents.iter().find(|a| a.id == conversation.agent_id)?;
    agent.llm_connector_id
        .or(inner.settings.default_llm_connector_id)
}

// ── Handlers ───────────────────────────────────────────────────────

async fn list_conversations(
    auth: AuthUser,
    State(state): State<AppState>,
) -> Json<Vec<Conversation>> {
    let user_id = auth.id();
    let is_admin = auth.is_admin();

    let inner = state.inner.read().await;
    if is_admin {
        return Json(inner.conversations.clone());
    }

    // Get shared conversation IDs
    let pool = state.db.pool();
    let shared_ids: Vec<Uuid> = clawkson_db::share::list_shared_with_user(pool, user_id)
        .await
        .unwrap_or_default()
        .into_iter()
        .map(|s| s.conversation_id)
        .collect();

    let convs: Vec<_> = inner
        .conversations
        .iter()
        .filter(|c| c.owner_id == Some(user_id) || shared_ids.contains(&c.id))
        .cloned()
        .collect();
    Json(convs)
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

    let inner = state.inner.read().await;
    inner
        .conversations
        .iter()
        .find(|c| c.id == id)
        .cloned()
        .map(Json)
        .ok_or(StatusCode::NOT_FOUND)
}

async fn create_conversation(
    auth: AuthUser,
    State(state): State<AppState>,
    Json(req): Json<CreateConversationRequest>,
) -> Json<Conversation> {
    let now = Utc::now();
    let conv = Conversation {
        id: Uuid::new_v4(),
        title: req.title,
        agent_id: req.agent_id,
        owner_id: Some(auth.id()),
        created_at: now,
        updated_at: now,
    };

    let mut inner = state.inner.write().await;
    inner.conversations.push(conv.clone());
    Json(conv)
}

async fn delete_conversation(
    auth: AuthUser,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> StatusCode {
    // Only owner or admin can delete
    {
        let inner = state.inner.read().await;
        if let Some(conv) = inner.conversations.iter().find(|c| c.id == id) {
            if !auth.is_admin() && conv.owner_id != Some(auth.id()) {
                return StatusCode::FORBIDDEN;
            }
        } else {
            return StatusCode::NOT_FOUND;
        }
    }

    let mut inner = state.inner.write().await;
    let before = inner.conversations.len();
    inner.conversations.retain(|c| c.id != id);
    inner.messages.retain(|m| m.conversation_id != id);
    if inner.conversations.len() < before {
        StatusCode::NO_CONTENT
    } else {
        StatusCode::NOT_FOUND
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

    let inner = state.inner.read().await;
    let msgs: Vec<_> = inner
        .messages
        .iter()
        .filter(|m| m.conversation_id == id)
        .cloned()
        .collect();
    Ok(Json(msgs))
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

    let msg = Message {
        id: Uuid::new_v4(),
        conversation_id: conv_id,
        role: req.role,
        content: req.content,
        created_at: Utc::now(),
    };

    let mut inner = state.inner.write().await;
    inner.messages.push(msg.clone());
    Ok(Json(msg))
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

    // 1. Verify the conversation exists
    let conversation = {
        let inner = state.inner.read().await;
        inner.conversations.iter().find(|c| c.id == conv_id).cloned()
    };
    let Some(conversation) = conversation else {
        return (StatusCode::NOT_FOUND, Json(serde_json::json!({"error": "conversation not found"}))).into_response();
    };

    // 2. Save user message
    let user_msg = Message {
        id: Uuid::new_v4(),
        conversation_id: conv_id,
        role: MessageRole::User,
        content: req.content.clone(),
        created_at: Utc::now(),
    };
    {
        let mut inner = state.inner.write().await;
        inner.messages.push(user_msg.clone());
    }

    // 3. Resolve LLM connector
    let connector_id = resolve_connector_id(&state, &conversation).await;
    let Some(connector_id) = connector_id else {
        let err_msg = Message {
            id: Uuid::new_v4(),
            conversation_id: conv_id,
            role: MessageRole::Assistant,
            content: "⚠️ No LLM connector configured for this agent. Please add an inference connector in Settings and assign it to the agent.".to_string(),
            created_at: Utc::now(),
        };
        let mut inner = state.inner.write().await;
        inner.messages.push(err_msg.clone());
        return Json(ChatResponse { user_message: user_msg, assistant_message: err_msg }).into_response();
    };

    // 4. Load agent config and message history
    let (system_prompt, temperature, max_tokens, connector, history) = {
        let inner = state.inner.read().await;
        let agent = inner.agents.iter().find(|a| a.id == conversation.agent_id).cloned();
        let connector = inner.llm_connectors.iter().find(|c| c.id == connector_id).cloned();
        let history: Vec<(MessageRole, String)> = inner
            .messages
            .iter()
            .filter(|m| m.conversation_id == conv_id && m.id != user_msg.id)
            .map(|m| (m.role.clone(), m.content.clone()))
            .collect();
        (
            agent.as_ref().and_then(|a| a.system_prompt.clone()),
            agent.as_ref().and_then(|a| a.temperature),
            agent.as_ref().and_then(|a| a.max_tokens),
            connector,
            history,
        )
    };

    let Some(connector) = connector else {
        let err_msg = Message {
            id: Uuid::new_v4(),
            conversation_id: conv_id,
            role: MessageRole::Assistant,
            content: "⚠️ Configured LLM connector not found. Please check your connector settings.".to_string(),
            created_at: Utc::now(),
        };
        let mut inner = state.inner.write().await;
        inner.messages.push(err_msg.clone());
        return Json(ChatResponse { user_message: user_msg, assistant_message: err_msg }).into_response();
    };

    // 5. Build full history including the user message
    let mut full_history = history;
    full_history.push((MessageRole::User, req.content));

    // 6. Call LLM
    let result = crate::llm::complete(
        &connector,
        system_prompt.as_deref(),
        &full_history,
        temperature,
        max_tokens,
    )
    .await;

    let assistant_content = match result {
        Ok(text) => text,
        Err(e) => {
            tracing::error!("LLM completion failed: {e}");
            format!("⚠️ Error calling LLM: {e}")
        }
    };

    // 7. Save assistant message
    let assistant_msg = Message {
        id: Uuid::new_v4(),
        conversation_id: conv_id,
        role: MessageRole::Assistant,
        content: assistant_content,
        created_at: Utc::now(),
    };
    {
        let mut inner = state.inner.write().await;
        inner.messages.push(assistant_msg.clone());
        if let Some(c) = inner.conversations.iter_mut().find(|c| c.id == conv_id) {
            c.updated_at = Utc::now();
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
                Ok::<Event, Infallible>(
                    Event::default().data(r#"{"error":"forbidden"}"#),
                )
            });
            return Sse::new(s).into_response();
        }
    }

    // Verify conversation exists
    let conversation = {
        let inner = state.inner.read().await;
        inner.conversations.iter().find(|c| c.id == conv_id).cloned()
    };
    let Some(conversation) = conversation else {
        let s = stream::once(async {
            Ok::<Event, Infallible>(
                Event::default().data(r#"{"error":"conversation not found"}"#),
            )
        });
        return Sse::new(s).into_response();
    };

    // Save user message
    let user_msg_id = Uuid::new_v4();
    {
        let mut inner = state.inner.write().await;
        inner.messages.push(Message {
            id: user_msg_id,
            conversation_id: conv_id,
            role: MessageRole::User,
            content: req.content.clone(),
            created_at: Utc::now(),
        });
    }

    // Resolve connector
    let connector_id = resolve_connector_id(&state, &conversation).await;
    let Some(connector_id) = connector_id else {
        let s = stream::once(async {
            Ok::<Event, Infallible>(
                Event::default().data(r#"{"error":"no LLM connector configured"}"#),
            )
        });
        return Sse::new(s).into_response();
    };

    // Load agent config + history
    let (system_prompt, temperature, max_tokens, connector, history) = {
        let inner = state.inner.read().await;
        let agent = inner.agents.iter().find(|a| a.id == conversation.agent_id).cloned();
        let connector = inner.llm_connectors.iter().find(|c| c.id == connector_id).cloned();
        let history: Vec<(MessageRole, String)> = inner
            .messages
            .iter()
            .filter(|m| m.conversation_id == conv_id)
            .map(|m| (m.role.clone(), m.content.clone()))
            .collect();
        (
            agent.as_ref().and_then(|a| a.system_prompt.clone()),
            agent.as_ref().and_then(|a| a.temperature),
            agent.as_ref().and_then(|a| a.max_tokens),
            connector,
            history,
        )
    };

    let Some(connector) = connector else {
        let s = stream::once(async {
            Ok::<Event, Infallible>(
                Event::default().data(r#"{"error":"LLM connector not found"}"#),
            )
        });
        return Sse::new(s).into_response();
    };

    // Stream via channel
    let (tx, mut rx) = mpsc::channel::<String>(64);
    let state2 = state.clone();

    tokio::spawn(async move {
        let result = crate::llm::stream_complete(
            &connector,
            system_prompt.as_deref(),
            &history,
            temperature,
            max_tokens,
            |chunk| {
                let _ = tx.try_send(chunk);
            },
        )
        .await;

        let assistant_content = match result {
            Ok(text) => text,
            Err(e) => {
                tracing::error!("LLM streaming failed: {e}");
                format!("⚠️ Error: {e}")
            }
        };

        let msg_id = Uuid::new_v4();
        let mut inner = state2.inner.write().await;
        inner.messages.push(Message {
            id: msg_id,
            conversation_id: conv_id,
            role: MessageRole::Assistant,
            content: assistant_content,
            created_at: Utc::now(),
        });
        if let Some(c) = inner.conversations.iter_mut().find(|c| c.id == conv_id) {
            c.updated_at = Utc::now();
        }
        let _ = tx.try_send(format!("\x00DONE:{msg_id}"));
    });

    let sse_stream = async_stream::stream! {
        while let Some(msg) = rx.recv().await {
            if let Some(id) = msg.strip_prefix("\x00DONE:") {
                let data = format!(r#"{{"done":true,"id":"{id}"}}"#);
                yield Ok::<Event, Infallible>(Event::default().data(data));
                break;
            } else {
                let escaped = msg.replace('\\', "\\\\").replace('"', "\\\"").replace('\n', "\\n");
                let data = format!(r#"{{"delta":"{escaped}"}}"#);
                yield Ok::<Event, Infallible>(Event::default().data(data));
            }
        }
    };

    Sse::new(sse_stream).into_response()
}
