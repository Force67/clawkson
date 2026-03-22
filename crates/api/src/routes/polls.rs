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
            "/conversations/{conv_id}/polls",
            get(list_polls).post(create_poll),
        )
        .route(
            "/conversations/{conv_id}/polls/{poll_id}",
            get(get_poll),
        )
        .route(
            "/conversations/{conv_id}/polls/{poll_id}/vote",
            post(cast_vote),
        )
        .route(
            "/conversations/{conv_id}/polls/{poll_id}/vote/{option_id}",
            axum::routing::delete(remove_vote),
        )
}

#[derive(Debug, Serialize)]
struct PollResponse {
    id: String,
    conversation_id: String,
    message_id: Option<String>,
    question: String,
    allow_multiple: bool,
    created_by: Option<String>,
    created_at: String,
    closes_at: Option<String>,
    options: Vec<PollOptionResponse>,
}

#[derive(Debug, Serialize)]
struct PollOptionResponse {
    id: String,
    label: String,
    votes: i64,
}

#[derive(Debug, Deserialize)]
struct CreatePollRequest {
    question: String,
    options: Vec<String>,
    #[serde(default)]
    allow_multiple: bool,
}

#[derive(Debug, Deserialize)]
struct VoteRequest {
    option_id: String,
}

async fn list_polls(
    _auth: AuthUser,
    State(state): State<AppState>,
    Path(conv_id): Path<Uuid>,
) -> Result<Json<Vec<PollResponse>>, StatusCode> {
    let polls = clawkson_db::poll::list_for_conversation(&state.db, conv_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let mut results = Vec::new();
    for poll in polls {
        let counts = clawkson_db::poll::vote_counts(&state.db, poll.id)
            .await
            .unwrap_or_default();
        results.push(PollResponse {
            id: poll.id.to_string(),
            conversation_id: poll.conversation_id.to_string(),
            message_id: poll.message_id.map(|id| id.to_string()),
            question: poll.question,
            allow_multiple: poll.allow_multiple,
            created_by: poll.created_by.map(|id| id.to_string()),
            created_at: poll.created_at.to_rfc3339(),
            closes_at: poll.closes_at.map(|dt| dt.to_rfc3339()),
            options: counts
                .into_iter()
                .map(|(id, label, votes)| PollOptionResponse {
                    id: id.to_string(),
                    label,
                    votes,
                })
                .collect(),
        });
    }

    Ok(Json(results))
}

async fn get_poll(
    _auth: AuthUser,
    State(state): State<AppState>,
    Path((_conv_id, poll_id)): Path<(Uuid, Uuid)>,
) -> Result<Json<PollResponse>, StatusCode> {
    let poll = clawkson_db::poll::get(&state.db, poll_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;

    let counts = clawkson_db::poll::vote_counts(&state.db, poll.id)
        .await
        .unwrap_or_default();

    Ok(Json(PollResponse {
        id: poll.id.to_string(),
        conversation_id: poll.conversation_id.to_string(),
        message_id: poll.message_id.map(|id| id.to_string()),
        question: poll.question,
        allow_multiple: poll.allow_multiple,
        created_by: poll.created_by.map(|id| id.to_string()),
        created_at: poll.created_at.to_rfc3339(),
        closes_at: poll.closes_at.map(|dt| dt.to_rfc3339()),
        options: counts
            .into_iter()
            .map(|(id, label, votes)| PollOptionResponse {
                id: id.to_string(),
                label,
                votes,
            })
            .collect(),
    }))
}

async fn create_poll(
    auth: AuthUser,
    State(state): State<AppState>,
    Path(conv_id): Path<Uuid>,
    Json(req): Json<CreatePollRequest>,
) -> Result<Json<PollResponse>, StatusCode> {
    if req.options.len() < 2 {
        return Err(StatusCode::BAD_REQUEST);
    }

    let poll = clawkson_db::poll::create(
        &state.db,
        conv_id,
        None,
        &req.question,
        &req.options,
        req.allow_multiple,
        Some(auth.id()),
        None,
    )
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let options = clawkson_db::poll::list_options(&state.db, poll.id)
        .await
        .unwrap_or_default();

    Ok(Json(PollResponse {
        id: poll.id.to_string(),
        conversation_id: poll.conversation_id.to_string(),
        message_id: None,
        question: poll.question,
        allow_multiple: poll.allow_multiple,
        created_by: poll.created_by.map(|id| id.to_string()),
        created_at: poll.created_at.to_rfc3339(),
        closes_at: None,
        options: options
            .into_iter()
            .map(|o| PollOptionResponse {
                id: o.id.to_string(),
                label: o.label,
                votes: 0,
            })
            .collect(),
    }))
}

async fn cast_vote(
    auth: AuthUser,
    State(state): State<AppState>,
    Path((_conv_id, _poll_id)): Path<(Uuid, Uuid)>,
    Json(req): Json<VoteRequest>,
) -> Result<StatusCode, StatusCode> {
    let option_id = Uuid::parse_str(&req.option_id).map_err(|_| StatusCode::BAD_REQUEST)?;
    clawkson_db::poll::vote(&state.db, option_id, auth.id())
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(StatusCode::OK)
}

async fn remove_vote(
    auth: AuthUser,
    State(state): State<AppState>,
    Path((_conv_id, _poll_id, option_id)): Path<(Uuid, Uuid, Uuid)>,
) -> StatusCode {
    match clawkson_db::poll::unvote(&state.db, option_id, auth.id()).await {
        Ok(true) => StatusCode::NO_CONTENT,
        Ok(false) => StatusCode::NOT_FOUND,
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR,
    }
}
