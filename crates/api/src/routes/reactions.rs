use axum::{
    extract::{Path, State},
    http::StatusCode,
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::auth::AuthUser;
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route(
            "/conversations/{conv_id}/messages/{msg_id}/reactions",
            get(list_reactions).post(add_reaction),
        )
        .route(
            "/conversations/{conv_id}/messages/{msg_id}/reactions/{emoji}",
            axum::routing::delete(remove_reaction),
        )
}

#[derive(Debug, Serialize)]
struct ReactionResponse {
    id: String,
    message_id: String,
    user_id: String,
    emoji: String,
    created_at: String,
}

#[derive(Debug, Serialize)]
struct ReactionCount {
    emoji: String,
    count: i64,
}

#[derive(Debug, Deserialize)]
struct AddReactionRequest {
    emoji: String,
}

async fn list_reactions(
    _auth: AuthUser,
    State(state): State<AppState>,
    Path((_conv_id, msg_id)): Path<(Uuid, Uuid)>,
) -> Result<Json<Vec<ReactionCount>>, StatusCode> {
    let counts = clawkson_db::reaction::counts_for_message(&state.db, msg_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(
        counts
            .into_iter()
            .map(|(emoji, count)| ReactionCount { emoji, count })
            .collect(),
    ))
}

async fn add_reaction(
    auth: AuthUser,
    State(state): State<AppState>,
    Path((_conv_id, msg_id)): Path<(Uuid, Uuid)>,
    Json(req): Json<AddReactionRequest>,
) -> Result<Json<ReactionResponse>, StatusCode> {
    let row = clawkson_db::reaction::add(&state.db, msg_id, auth.id(), &req.emoji)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(ReactionResponse {
        id: row.id.to_string(),
        message_id: row.message_id.to_string(),
        user_id: row.user_id.to_string(),
        emoji: row.emoji,
        created_at: row.created_at.to_rfc3339(),
    }))
}

async fn remove_reaction(
    auth: AuthUser,
    State(state): State<AppState>,
    Path((_conv_id, msg_id, emoji)): Path<(Uuid, Uuid, String)>,
) -> StatusCode {
    match clawkson_db::reaction::remove(&state.db, msg_id, auth.id(), &emoji).await {
        Ok(true) => StatusCode::NO_CONTENT,
        Ok(false) => StatusCode::NOT_FOUND,
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR,
    }
}
