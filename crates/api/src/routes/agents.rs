use axum::{
    extract::{Path, State},
    http::StatusCode,
    routing::get,
    Json, Router,
};
use chrono::Utc;
use clawkson_core::{Agent, AgentStatus};
use serde::Deserialize;
use uuid::Uuid;

use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", get(list_agents).post(create_agent))
        .route("/{id}", get(get_agent).patch(patch_agent).delete(delete_agent))
}

#[derive(Debug, Deserialize)]
pub struct CreateAgentRequest {
    pub name: String,
    pub description: String,
    pub llm_connector_id: Option<Uuid>,
    pub system_prompt: Option<String>,
    pub temperature: Option<f64>,
    pub max_tokens: Option<u32>,
}

#[derive(Debug, Deserialize)]
pub struct PatchAgentRequest {
    pub name: Option<String>,
    pub description: Option<String>,
    pub llm_connector_id: Option<Uuid>,
    pub system_prompt: Option<String>,
    pub temperature: Option<f64>,
    pub max_tokens: Option<u32>,
    pub status: Option<AgentStatus>,
}

async fn list_agents(State(state): State<AppState>) -> Json<Vec<Agent>> {
    let inner = state.inner.read().await;
    Json(inner.agents.clone())
}

async fn get_agent(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<Agent>, StatusCode> {
    let inner = state.inner.read().await;
    inner
        .agents
        .iter()
        .find(|a| a.id == id)
        .cloned()
        .map(Json)
        .ok_or(StatusCode::NOT_FOUND)
}

async fn create_agent(
    State(state): State<AppState>,
    Json(req): Json<CreateAgentRequest>,
) -> Json<Agent> {
    let now = Utc::now();
    let agent = Agent {
        id: Uuid::new_v4(),
        name: req.name,
        description: req.description,
        status: AgentStatus::Offline,
        llm_connector_id: req.llm_connector_id,
        system_prompt: req.system_prompt,
        temperature: req.temperature,
        max_tokens: req.max_tokens,
        created_at: now,
        updated_at: now,
    };

    let mut inner = state.inner.write().await;
    inner.agents.push(agent.clone());
    Json(agent)
}

async fn patch_agent(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(req): Json<PatchAgentRequest>,
) -> Result<Json<Agent>, StatusCode> {
    let mut inner = state.inner.write().await;
    let agent = inner
        .agents
        .iter_mut()
        .find(|a| a.id == id)
        .ok_or(StatusCode::NOT_FOUND)?;

    if let Some(name) = req.name { agent.name = name; }
    if let Some(desc) = req.description { agent.description = desc; }
    if let Some(status) = req.status { agent.status = status; }
    // Allow explicitly setting these to None by using a serde Option<Option<T>> pattern
    // For simplicity, only update when the field is Some
    if req.llm_connector_id.is_some() || req.temperature.is_none() && req.max_tokens.is_none() {
        // Keep the old llm_connector_id unless explicitly provided
        if req.llm_connector_id.is_some() {
            agent.llm_connector_id = req.llm_connector_id;
        }
    }
    if let Some(sp) = req.system_prompt { agent.system_prompt = Some(sp); }
    if let Some(t) = req.temperature { agent.temperature = Some(t); }
    if let Some(m) = req.max_tokens { agent.max_tokens = Some(m); }
    agent.updated_at = Utc::now();

    Ok(Json(agent.clone()))
}

async fn delete_agent(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> StatusCode {
    let mut inner = state.inner.write().await;
    let before = inner.agents.len();
    inner.agents.retain(|a| a.id != id);
    if inner.agents.len() < before {
        StatusCode::NO_CONTENT
    } else {
        StatusCode::NOT_FOUND
    }
}
