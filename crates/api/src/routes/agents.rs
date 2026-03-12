use axum::{
    extract::{Path, State},
    http::StatusCode,
    routing::get,
    Json, Router,
};
use clawkson_core::{Agent, AgentContainerConfig, AgentStatus};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::auth::AuthUser;
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", get(list_agents).post(create_agent))
        .route("/{id}", get(get_agent).patch(patch_agent).delete(delete_agent))
        .route("/{id}/skills", get(list_agent_skills).post(link_agent_skill))
        .route("/{id}/skills/full", get(list_agent_skills_full))
        .route("/{id}/skills/{skill_id}", axum::routing::delete(unlink_agent_skill))
}

#[derive(Debug, Deserialize)]
pub struct CreateAgentRequest {
    pub name: String,
    pub description: String,
    pub llm_connector_id: Option<Uuid>,
    pub system_prompt: Option<String>,
    pub temperature: Option<f64>,
    pub max_tokens: Option<u32>,
    pub container_enabled: Option<bool>,
    pub container_config: Option<AgentContainerConfig>,
    #[serde(default)]
    pub shared: bool,
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
    pub container_enabled: Option<bool>,
    pub container_config: Option<AgentContainerConfig>,
    pub shared: Option<bool>,
}

/// Map DB row to API type.
fn row_to_agent(row: clawkson_db::agent::AgentRow) -> Agent {
    Agent {
        id: row.id,
        name: row.name,
        description: row.description,
        status: match row.status {
            clawkson_db::agent::AgentStatus::Online => AgentStatus::Online,
            clawkson_db::agent::AgentStatus::Offline => AgentStatus::Offline,
            clawkson_db::agent::AgentStatus::Busy => AgentStatus::Busy,
            clawkson_db::agent::AgentStatus::Error => AgentStatus::Error,
        },
        llm_connector_id: row.llm_connector_id,
        system_prompt: row.system_prompt,
        temperature: row.temperature,
        max_tokens: row.max_tokens.map(|v| v as u32),
        container_enabled: row.container_enabled,
        container_config: row.container_config.and_then(|v| serde_json::from_value(v).ok()),
        owner_id: row.owner_id,
        shared: row.shared,
        created_at: row.created_at,
        updated_at: row.updated_at,
    }
}

/// Can the user see this agent? (owner, admin, or shared)
fn can_access_agent(agent: &clawkson_db::agent::AgentRow, user_id: Uuid, is_admin: bool) -> bool {
    is_admin || agent.shared || agent.owner_id == Some(user_id)
}

/// Can the user modify/delete this agent? (owner or admin only)
fn can_manage_agent(agent: &clawkson_db::agent::AgentRow, user_id: Uuid, is_admin: bool) -> bool {
    is_admin || agent.owner_id == Some(user_id)
}

fn status_to_db(s: &AgentStatus) -> clawkson_db::agent::AgentStatus {
    match s {
        AgentStatus::Online => clawkson_db::agent::AgentStatus::Online,
        AgentStatus::Offline => clawkson_db::agent::AgentStatus::Offline,
        AgentStatus::Busy => clawkson_db::agent::AgentStatus::Busy,
        AgentStatus::Error => clawkson_db::agent::AgentStatus::Error,
    }
}

async fn list_agents(auth: AuthUser, State(state): State<AppState>) -> Result<Json<Vec<Agent>>, StatusCode> {
    let rows = clawkson_db::agent::list_for_user(&state.db, auth.id(), auth.is_admin())
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(rows.into_iter().map(row_to_agent).collect()))
}

async fn get_agent(
    auth: AuthUser,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<Agent>, StatusCode> {
    let row = clawkson_db::agent::get_by_id(&state.db, id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;

    if !can_access_agent(&row, auth.id(), auth.is_admin()) {
        return Err(StatusCode::FORBIDDEN);
    }

    Ok(Json(row_to_agent(row)))
}

async fn create_agent(
    auth: AuthUser,
    State(state): State<AppState>,
    Json(req): Json<CreateAgentRequest>,
) -> Result<Json<Agent>, StatusCode> {
    // Only admins can create shared agents
    if req.shared && !auth.is_admin() {
        return Err(StatusCode::FORBIDDEN);
    }

    let container_config_json = req
        .container_config
        .as_ref()
        .and_then(|c| serde_json::to_value(c).ok());

    let row = clawkson_db::agent::create(
        &state.db,
        &req.name,
        &req.description,
        req.llm_connector_id,
        req.system_prompt.as_deref(),
        req.temperature,
        req.max_tokens.map(|v| v as i32),
        req.container_enabled.unwrap_or(false),
        container_config_json,
        auth.id(),
        req.shared,
    )
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(row_to_agent(row)))
}

async fn patch_agent(
    auth: AuthUser,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(req): Json<PatchAgentRequest>,
) -> Result<Json<Agent>, StatusCode> {
    let existing = clawkson_db::agent::get_by_id(&state.db, id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;

    if !can_manage_agent(&existing, auth.id(), auth.is_admin()) {
        return Err(StatusCode::FORBIDDEN);
    }

    // Only admins can toggle shared
    if req.shared.is_some() && !auth.is_admin() {
        return Err(StatusCode::FORBIDDEN);
    }

    let row = clawkson_db::agent::update(
        &state.db,
        id,
        req.name.as_deref(),
        req.description.as_deref(),
        req.status.as_ref().map(status_to_db),
        if req.llm_connector_id.is_some() { Some(req.llm_connector_id) } else { None },
        req.system_prompt.as_ref().map(|s| Some(s.as_str())),
        req.temperature.map(Some),
        req.max_tokens.map(|v| Some(v as i32)),
        req.container_enabled,
        req.container_config.as_ref().map(|c| Some(serde_json::to_value(c).unwrap_or_default())),
        req.shared,
    )
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
    .ok_or(StatusCode::NOT_FOUND)?;

    Ok(Json(row_to_agent(row)))
}

async fn delete_agent(
    auth: AuthUser,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> StatusCode {
    let existing = match clawkson_db::agent::get_by_id(&state.db, id).await {
        Ok(Some(row)) => row,
        Ok(None) => return StatusCode::NOT_FOUND,
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR,
    };

    if !can_manage_agent(&existing, auth.id(), auth.is_admin()) {
        return StatusCode::FORBIDDEN;
    }

    match clawkson_db::agent::delete(&state.db, id).await {
        Ok(true) => StatusCode::NO_CONTENT,
        Ok(false) => StatusCode::NOT_FOUND,
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR,
    }
}

// ── Agent ↔ Skill linking ────────────────────────────────────────

async fn list_agent_skills(
    _auth: AuthUser,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<Vec<Uuid>>, StatusCode> {
    let skills = clawkson_db::skill::agent_list_skills(state.db.pool(), id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(skills.into_iter().map(|s| s.id).collect()))
}

/// Returns full skill objects for an agent — used by the chat UI.
#[derive(Serialize)]
struct AgentSkillInfo {
    id: Uuid,
    name: String,
    description: String,
}

async fn list_agent_skills_full(
    _auth: AuthUser,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<Vec<AgentSkillInfo>>, StatusCode> {
    let skills = clawkson_db::skill::agent_list_skills(state.db.pool(), id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(skills.into_iter().map(|s| AgentSkillInfo {
        id: s.id,
        name: s.name,
        description: s.description,
    }).collect()))
}

#[derive(Debug, Deserialize)]
pub struct LinkSkillRequest {
    pub skill_id: Uuid,
}

async fn link_agent_skill(
    auth: AuthUser,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(req): Json<LinkSkillRequest>,
) -> StatusCode {
    let existing = match clawkson_db::agent::get_by_id(&state.db, id).await {
        Ok(Some(row)) => row,
        Ok(None) => return StatusCode::NOT_FOUND,
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR,
    };
    if !can_manage_agent(&existing, auth.id(), auth.is_admin()) {
        return StatusCode::FORBIDDEN;
    }

    match clawkson_db::skill::agent_link(state.db.pool(), id, req.skill_id).await {
        Ok(()) => StatusCode::NO_CONTENT,
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR,
    }
}

async fn unlink_agent_skill(
    auth: AuthUser,
    State(state): State<AppState>,
    Path((id, skill_id)): Path<(Uuid, Uuid)>,
) -> StatusCode {
    let existing = match clawkson_db::agent::get_by_id(&state.db, id).await {
        Ok(Some(row)) => row,
        Ok(None) => return StatusCode::NOT_FOUND,
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR,
    };
    if !can_manage_agent(&existing, auth.id(), auth.is_admin()) {
        return StatusCode::FORBIDDEN;
    }

    match clawkson_db::skill::agent_unlink(state.db.pool(), id, skill_id).await {
        Ok(_) => StatusCode::NO_CONTENT,
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR,
    }
}
