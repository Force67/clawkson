use axum::{
    extract::{Path, State},
    http::StatusCode,
    routing::get,
    Json, Router,
};
use clawkson_core::Credential;
use serde::Deserialize;
use uuid::Uuid;

use crate::auth::AuthUser;
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", get(list_credentials).post(create_credential))
        .route("/{id}", get(get_credential).patch(patch_credential).delete(delete_credential))
        .route("/{id}/agents", get(list_credential_agents))
}

fn row_to_credential(row: clawkson_db::credential::CredentialRow) -> Credential {
    Credential {
        id: row.id,
        owner_id: row.owner_id,
        name: row.name,
        description: row.description,
        credential_type: row.credential_type,
        // value is intentionally omitted
        metadata: row.metadata,
        created_at: row.created_at,
        updated_at: row.updated_at,
    }
}

#[derive(Debug, Deserialize)]
pub struct CreateCredentialRequest {
    pub name: String,
    pub description: Option<String>,
    pub credential_type: Option<String>,
    pub value: String,
    pub metadata: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
pub struct PatchCredentialRequest {
    pub name: Option<String>,
    pub description: Option<String>,
    pub credential_type: Option<String>,
    pub value: Option<String>,
    pub metadata: Option<serde_json::Value>,
}

const VALID_TYPES: &[&str] = &["api_key", "password", "token", "secret", "header"];

async fn list_credentials(
    auth: AuthUser,
    State(state): State<AppState>,
) -> Result<Json<Vec<Credential>>, StatusCode> {
    let rows = clawkson_db::credential::list_for_user(&state.db, auth.id())
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(rows.into_iter().map(row_to_credential).collect()))
}

async fn get_credential(
    auth: AuthUser,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<Credential>, StatusCode> {
    let row = clawkson_db::credential::get_by_id(&state.db, id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;

    if row.owner_id != auth.id() && !auth.is_admin() {
        return Err(StatusCode::FORBIDDEN);
    }

    Ok(Json(row_to_credential(row)))
}

async fn create_credential(
    auth: AuthUser,
    State(state): State<AppState>,
    Json(req): Json<CreateCredentialRequest>,
) -> Result<Json<Credential>, StatusCode> {
    let name = req.name.trim().to_lowercase();
    if name.is_empty() || name.len() > 128 {
        return Err(StatusCode::BAD_REQUEST);
    }
    // Validate: only lowercase letters, numbers, and hyphens
    if !name.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '_') {
        return Err(StatusCode::BAD_REQUEST);
    }

    let cred_type = req.credential_type.as_deref().unwrap_or("api_key");
    if !VALID_TYPES.contains(&cred_type) {
        return Err(StatusCode::BAD_REQUEST);
    }

    if req.value.is_empty() {
        return Err(StatusCode::BAD_REQUEST);
    }

    let metadata = req.metadata.as_ref().cloned().unwrap_or(serde_json::json!({}));

    let row = clawkson_db::credential::create(
        &state.db,
        auth.id(),
        &name,
        req.description.as_deref().unwrap_or("").trim(),
        cred_type,
        &req.value,
        &metadata,
    )
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(row_to_credential(row)))
}

async fn patch_credential(
    auth: AuthUser,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(req): Json<PatchCredentialRequest>,
) -> Result<Json<Credential>, StatusCode> {
    let existing = clawkson_db::credential::get_by_id(&state.db, id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;

    if existing.owner_id != auth.id() && !auth.is_admin() {
        return Err(StatusCode::FORBIDDEN);
    }

    if let Some(ref name) = req.name {
        let name = name.trim().to_lowercase();
        if name.is_empty() || name.len() > 128 {
            return Err(StatusCode::BAD_REQUEST);
        }
        if !name.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '_') {
            return Err(StatusCode::BAD_REQUEST);
        }
    }

    if let Some(ref cred_type) = req.credential_type {
        if !VALID_TYPES.contains(&cred_type.as_str()) {
            return Err(StatusCode::BAD_REQUEST);
        }
    }

    let row = clawkson_db::credential::update(
        &state.db,
        id,
        req.name.as_deref(),
        req.description.as_deref(),
        req.credential_type.as_deref(),
        req.value.as_deref(),
        req.metadata.as_ref(),
    )
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
    .ok_or(StatusCode::NOT_FOUND)?;

    Ok(Json(row_to_credential(row)))
}

async fn delete_credential(
    auth: AuthUser,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> StatusCode {
    let existing = match clawkson_db::credential::get_by_id(&state.db, id).await {
        Ok(Some(row)) => row,
        Ok(None) => return StatusCode::NOT_FOUND,
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR,
    };

    if existing.owner_id != auth.id() && !auth.is_admin() {
        return StatusCode::FORBIDDEN;
    }

    match clawkson_db::credential::delete(&state.db, id).await {
        Ok(true) => StatusCode::NO_CONTENT,
        Ok(false) => StatusCode::NOT_FOUND,
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR,
    }
}

async fn list_credential_agents(
    auth: AuthUser,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<Vec<Uuid>>, StatusCode> {
    // Verify ownership
    let existing = clawkson_db::credential::get_by_id(&state.db, id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;

    if existing.owner_id != auth.id() && !auth.is_admin() {
        return Err(StatusCode::FORBIDDEN);
    }

    let ids = clawkson_db::credential::credential_list_agents(state.db.pool(), id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(ids))
}
