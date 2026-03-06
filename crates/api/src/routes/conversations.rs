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

use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", get(list_conversations).post(create_conversation))
        .route("/{id}", get(get_conversation))
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

// ── Helpers ────────────────────────────────────────────────────────

/// Resolve the LLM connector for a conversation's agent.
/// Returns the connector ID to use, preferring the agent's connector
/// then falling back to the default in settings.
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

async fn list_conversations(State(state): State<AppState>) -> Json<Vec<Conversation>> {
    let inner = state.inner.read().await;
    Json(inner.conversations.clone())
}

async fn get_conversation(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<Conversation>, StatusCode> {
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
    State(state): State<AppState>,
    Json(req): Json<CreateConversationRequest>,
) -> Json<Conversation> {
    let now = Utc::now();
    let conv = Conversation {
        id: Uuid::new_v4(),
        title: req.title,
        agent_id: req.agent_id,
        created_at: now,
        updated_at: now,
    };

    let mut inner = state.inner.write().await;
    inner.conversations.push(conv.clone());
    Json(conv)
}

async fn list_messages(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Json<Vec<Message>> {
    let inner = state.inner.read().await;
    let msgs: Vec<_> = inner
        .messages
        .iter()
        .filter(|m| m.conversation_id == id)
        .cloned()
        .collect();
    Json(msgs)
}

async fn send_message(
    State(state): State<AppState>,
    Path(conv_id): Path<Uuid>,
    Json(req): Json<SendMessageRequest>,
) -> Json<Message> {
    let msg = Message {
        id: Uuid::new_v4(),
        conversation_id: conv_id,
        role: req.role,
        content: req.content,
        created_at: Utc::now(),
    };

    let mut inner = state.inner.write().await;
    inner.messages.push(msg.clone());
    Json(msg)
}

/// POST /api/conversations/{id}/chat
/// Saves user message, runs LLM completion, saves and returns assistant message.
async fn chat(
    State(state): State<AppState>,
    Path(conv_id): Path<Uuid>,
    Json(req): Json<ChatRequest>,
) -> impl IntoResponse {
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
        // Update conversation updated_at
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
/// Streams the assistant response as Server-Sent Events.
/// Each event is `data: {"delta":"..."}` followed by a final `data: {"done":true,"id":"..."}`.
async fn chat_stream(
    State(state): State<AppState>,
    Path(conv_id): Path<Uuid>,
    Json(req): Json<ChatRequest>,
) -> impl IntoResponse {
    use tokio::sync::mpsc;

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

    // Stream via channel: spawn a task that calls the LLM and sends chunks
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

        // Persist the full assistant message
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
        // Send done event with message id
        let _ = tx.try_send(format!("\x00DONE:{msg_id}"));
    });

    // Convert the channel receiver into an SSE stream
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
