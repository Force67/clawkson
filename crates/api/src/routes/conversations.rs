use axum::{
    extract::{Path, State},
    routing::get,
    Json, Router,
};
use chrono::Utc;
use clawkson_core::{Conversation, Message, MessageRole};
use serde::Deserialize;
use uuid::Uuid;

use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", get(list_conversations).post(create_conversation))
        .route("/{id}", get(get_conversation))
        .route("/{id}/messages", get(list_messages).post(send_message))
}

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

async fn list_conversations(State(state): State<AppState>) -> Json<Vec<Conversation>> {
    let inner = state.inner.read().await;
    Json(inner.conversations.clone())
}

async fn get_conversation(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Json<Option<Conversation>> {
    let inner = state.inner.read().await;
    let conv = inner.conversations.iter().find(|c| c.id == id).cloned();
    Json(conv)
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
