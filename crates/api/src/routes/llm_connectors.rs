use axum::{
    extract::{Path, State},
    http::StatusCode,
    routing::get,
    Json, Router,
};
use chrono::Utc;
use clawkson_core::{LlmConnector, LlmProviderType};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::auth::AuthUser;
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", get(list).post(create))
        .route("/test", axum::routing::post(test_connection))
        .route("/{id}", get(get_one).patch(patch).delete(delete))
}

#[derive(Debug, Deserialize)]
pub struct CreateLlmConnectorRequest {
    pub name: String,
    pub provider_type: LlmProviderType,
    pub api_key: String,
    pub api_base_url: Option<String>,
    pub model: String,
    pub azure_deployment: Option<String>,
    pub azure_api_version: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct PatchLlmConnectorRequest {
    pub name: Option<String>,
    pub provider_type: Option<LlmProviderType>,
    pub api_key: Option<String>,
    pub api_base_url: Option<String>,
    pub model: Option<String>,
    pub azure_deployment: Option<String>,
    pub azure_api_version: Option<String>,
}

fn row_to_connector(row: clawkson_db::llm_connector::LlmConnectorRow) -> LlmConnector {
    LlmConnector {
        id: row.id,
        name: row.name,
        provider_type: match row.provider_type {
            clawkson_db::llm_connector::LlmProviderType::Azure => LlmProviderType::Azure,
            clawkson_db::llm_connector::LlmProviderType::Openrouter => LlmProviderType::OpenRouter,
            clawkson_db::llm_connector::LlmProviderType::Openai => LlmProviderType::OpenAi,
            clawkson_db::llm_connector::LlmProviderType::Custom => LlmProviderType::Custom,
        },
        api_key: row.api_key,
        api_base_url: row.api_base_url,
        model: row.model,
        azure_deployment: row.azure_deployment,
        azure_api_version: row.azure_api_version,
        created_at: row.created_at,
    }
}

fn provider_to_db(p: &LlmProviderType) -> clawkson_db::llm_connector::LlmProviderType {
    match p {
        LlmProviderType::Azure => clawkson_db::llm_connector::LlmProviderType::Azure,
        LlmProviderType::OpenRouter => clawkson_db::llm_connector::LlmProviderType::Openrouter,
        LlmProviderType::OpenAi => clawkson_db::llm_connector::LlmProviderType::Openai,
        LlmProviderType::Custom => clawkson_db::llm_connector::LlmProviderType::Custom,
    }
}

async fn list(_auth: AuthUser, State(state): State<AppState>) -> Result<Json<Vec<LlmConnector>>, StatusCode> {
    let rows = clawkson_db::llm_connector::list_all(&state.db)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let connectors: Vec<LlmConnector> = rows
        .into_iter()
        .map(row_to_connector)
        .map(|mut c| {
            c.api_key = mask_key(&c.api_key);
            c
        })
        .collect();
    Ok(Json(connectors))
}

async fn get_one(
    _auth: AuthUser,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<LlmConnector>, StatusCode> {
    let row = clawkson_db::llm_connector::get_by_id(&state.db, id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;
    let mut c = row_to_connector(row);
    c.api_key = mask_key(&c.api_key);
    Ok(Json(c))
}

async fn create(
    auth: AuthUser,
    State(state): State<AppState>,
    Json(req): Json<CreateLlmConnectorRequest>,
) -> Result<Json<LlmConnector>, StatusCode> {
    if !auth.is_admin() {
        return Err(StatusCode::FORBIDDEN);
    }

    let base_url = req.api_base_url.unwrap_or_else(|| {
        match req.provider_type {
            LlmProviderType::OpenRouter => "https://openrouter.ai/api/v1".to_string(),
            LlmProviderType::OpenAi => "https://api.openai.com/v1".to_string(),
            _ => String::new(),
        }
    });

    // Auto-set as default if it's the first connector
    let existing = clawkson_db::llm_connector::list_all(&state.db)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let row = clawkson_db::llm_connector::create(
        &state.db,
        &req.name,
        provider_to_db(&req.provider_type),
        &req.api_key,
        &base_url,
        &req.model,
        req.azure_deployment.as_deref(),
        req.azure_api_version.as_deref(),
    )
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    if existing.is_empty() {
        let _ = clawkson_db::settings::update(&state.db, Some(Some(row.id)), None).await;
    }

    let mut c = row_to_connector(row);
    c.api_key = mask_key(&c.api_key);
    Ok(Json(c))
}

async fn patch(
    auth: AuthUser,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(req): Json<PatchLlmConnectorRequest>,
) -> Result<Json<LlmConnector>, StatusCode> {
    if !auth.is_admin() {
        return Err(StatusCode::FORBIDDEN);
    }

    let row = clawkson_db::llm_connector::update(
        &state.db,
        id,
        req.name.as_deref(),
        req.provider_type.as_ref().map(provider_to_db),
        req.api_key.as_deref(),
        req.api_base_url.as_deref(),
        req.model.as_deref(),
        req.azure_deployment.as_ref().map(|s| Some(s.as_str())),
        req.azure_api_version.as_ref().map(|s| Some(s.as_str())),
    )
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
    .ok_or(StatusCode::NOT_FOUND)?;

    let mut c = row_to_connector(row);
    c.api_key = mask_key(&c.api_key);
    Ok(Json(c))
}

async fn delete(
    auth: AuthUser,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> StatusCode {
    if !auth.is_admin() {
        return StatusCode::FORBIDDEN;
    }

    match clawkson_db::llm_connector::delete(&state.db, id).await {
        Ok(true) => {
            // If this was the default, pick the next available
            if let Ok(settings) = clawkson_db::settings::get(&state.db).await {
                if settings.default_llm_connector_id == Some(id) {
                    let next = clawkson_db::llm_connector::list_all(&state.db)
                        .await
                        .ok()
                        .and_then(|v| v.first().map(|c| c.id));
                    let _ = clawkson_db::settings::update(&state.db, Some(next), None).await;
                }
            }
            StatusCode::NO_CONTENT
        }
        Ok(false) => StatusCode::NOT_FOUND,
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR,
    }
}

fn mask_key(key: &str) -> String {
    if key.len() <= 8 {
        "••••••••".to_string()
    } else {
        format!("{}••••••••{}", &key[..4], &key[key.len() - 4..])
    }
}

#[derive(Serialize)]
pub struct TestConnectionResponse {
    pub ok: bool,
    pub latency_ms: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

async fn test_connection(
    _auth: AuthUser,
    Json(req): Json<CreateLlmConnectorRequest>,
) -> Json<TestConnectionResponse> {
    use clawkson_core::MessageRole;
    use std::time::Instant;

    let base_url = req.api_base_url.clone().unwrap_or_else(|| {
        match req.provider_type {
            LlmProviderType::OpenRouter => "https://openrouter.ai/api/v1".to_string(),
            LlmProviderType::OpenAi => "https://api.openai.com/v1".to_string(),
            _ => String::new(),
        }
    });

    let connector = LlmConnector {
        id: Uuid::new_v4(),
        name: req.name,
        provider_type: req.provider_type,
        api_key: req.api_key,
        api_base_url: base_url,
        model: req.model,
        azure_deployment: req.azure_deployment,
        azure_api_version: req.azure_api_version,
        created_at: Utc::now(),
    };

    let start = Instant::now();
    let result = crate::llm::complete(
        &connector,
        None,
        &[(MessageRole::User, "Say \"OK\" in one word.".to_string())],
        None,
        Some(5),
        None,
    )
    .await;

    let latency_ms = start.elapsed().as_millis() as u64;

    match result {
        Ok(_) => Json(TestConnectionResponse { ok: true, latency_ms, error: None }),
        Err(e) => Json(TestConnectionResponse {
            ok: false,
            latency_ms,
            error: Some(e.to_string()),
        }),
    }
}
