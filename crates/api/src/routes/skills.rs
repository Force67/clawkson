use axum::{
    extract::{Path, State},
    http::StatusCode,
    routing::get,
    Json, Router,
};
use clawkson_core::Skill;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::auth::AuthUser;
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", get(list_skills).post(create_skill))
        .route("/templates", get(list_templates))
        .route("/{id}", get(get_skill).patch(patch_skill).delete(delete_skill))
        .route("/{id}/agents", get(list_skill_agents))
}

fn row_to_skill(row: clawkson_db::skill::SkillRow) -> Skill {
    Skill {
        id: row.id,
        name: row.name,
        description: row.description,
        instructions: row.instructions,
        created_at: row.created_at,
        updated_at: row.updated_at,
    }
}

#[derive(Debug, Deserialize)]
pub struct CreateSkillRequest {
    pub name: String,
    pub description: String,
    #[serde(default)]
    pub instructions: String,
}

#[derive(Debug, Deserialize)]
pub struct PatchSkillRequest {
    pub name: Option<String>,
    pub description: Option<String>,
    pub instructions: Option<String>,
}

async fn list_skills(
    _auth: AuthUser,
    State(state): State<AppState>,
) -> Result<Json<Vec<Skill>>, StatusCode> {
    let rows = clawkson_db::skill::list_all(&state.db)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(rows.into_iter().map(row_to_skill).collect()))
}

async fn get_skill(
    _auth: AuthUser,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<Skill>, StatusCode> {
    let row = clawkson_db::skill::get_by_id(&state.db, id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;
    Ok(Json(row_to_skill(row)))
}

async fn create_skill(
    auth: AuthUser,
    State(state): State<AppState>,
    Json(req): Json<CreateSkillRequest>,
) -> Result<Json<Skill>, StatusCode> {
    if !auth.is_admin() {
        return Err(StatusCode::FORBIDDEN);
    }

    let name = req.name.trim().to_lowercase();
    if name.is_empty() || name.len() > 64 {
        return Err(StatusCode::BAD_REQUEST);
    }
    // Validate: only lowercase letters, numbers, and hyphens
    if !name.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-') {
        return Err(StatusCode::BAD_REQUEST);
    }

    let row = clawkson_db::skill::create(
        &state.db,
        &name,
        req.description.trim(),
        req.instructions.trim(),
    )
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(row_to_skill(row)))
}

async fn patch_skill(
    auth: AuthUser,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(req): Json<PatchSkillRequest>,
) -> Result<Json<Skill>, StatusCode> {
    if !auth.is_admin() {
        return Err(StatusCode::FORBIDDEN);
    }

    if let Some(ref name) = req.name {
        let name = name.trim().to_lowercase();
        if name.is_empty() || name.len() > 64 {
            return Err(StatusCode::BAD_REQUEST);
        }
        if !name.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-') {
            return Err(StatusCode::BAD_REQUEST);
        }
    }

    let row = clawkson_db::skill::update(
        &state.db,
        id,
        req.name.as_deref(),
        req.description.as_deref(),
        req.instructions.as_deref(),
    )
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
    .ok_or(StatusCode::NOT_FOUND)?;

    Ok(Json(row_to_skill(row)))
}

async fn delete_skill(
    auth: AuthUser,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> StatusCode {
    if !auth.is_admin() {
        return StatusCode::FORBIDDEN;
    }

    match clawkson_db::skill::delete(&state.db, id).await {
        Ok(true) => StatusCode::NO_CONTENT,
        Ok(false) => StatusCode::NOT_FOUND,
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR,
    }
}

async fn list_skill_agents(
    _auth: AuthUser,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<Vec<Uuid>>, StatusCode> {
    let ids = clawkson_db::skill::skill_list_agents(state.db.pool(), id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(ids))
}

// ── Skill Templates ──────────────────────────────────────────────

#[derive(Debug, Serialize, Deserialize)]
pub struct SkillTemplate {
    pub name: String,
    pub description: String,
    pub instructions: String,
}

/// Embedded skill templates from the `skills/` directory at compile time.
fn builtin_templates() -> Vec<SkillTemplate> {
    let template_sources: &[&str] = &[
        include_str!("../../../../skills/devops-user-stories.json"),
        include_str!("../../../../skills/code-reviewer.json"),
        include_str!("../../../../skills/api-caller.json"),
        include_str!("../../../../skills/summarizer.json"),
        include_str!("../../../../skills/technical-writer.json"),
    ];

    template_sources
        .iter()
        .filter_map(|src| serde_json::from_str::<SkillTemplate>(src).ok())
        .collect()
}

async fn list_templates(
    _auth: AuthUser,
) -> Json<Vec<SkillTemplate>> {
    Json(builtin_templates())
}
