use axum::{
    extract::State,
    extract::Path,
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
        .route("/{id}", get(get_agent))
}

#[derive(Debug, Deserialize)]
pub struct CreateAgentRequest {
    pub name: String,
    pub description: String,
}

async fn list_agents(State(state): State<AppState>) -> Json<Vec<Agent>> {
    let inner = state.inner.read().await;
    Json(inner.agents.clone())
}

async fn get_agent(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Json<Option<Agent>> {
    let inner = state.inner.read().await;
    let agent = inner.agents.iter().find(|a| a.id == id).cloned();
    Json(agent)
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
        created_at: now,
        updated_at: now,
    };

    let mut inner = state.inner.write().await;
    inner.agents.push(agent.clone());
    Json(agent)
}
