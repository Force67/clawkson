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

async fn list(State(state): State<AppState>) -> Json<Vec<LlmConnector>> {
    let inner = state.inner.read().await;
    // Mask the API keys in the response for security
    let connectors: Vec<LlmConnector> = inner
        .llm_connectors
        .iter()
        .cloned()
        .map(|mut c| {
            c.api_key = mask_key(&c.api_key);
            c
        })
        .collect();
    Json(connectors)
}

async fn get_one(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<LlmConnector>, StatusCode> {
    let inner = state.inner.read().await;
    let mut c = inner
        .llm_connectors
        .iter()
        .find(|c| c.id == id)
        .cloned()
        .ok_or(StatusCode::NOT_FOUND)?;
    c.api_key = mask_key(&c.api_key);
    Ok(Json(c))
}

async fn create(
    State(state): State<AppState>,
    Json(req): Json<CreateLlmConnectorRequest>,
) -> Json<LlmConnector> {
    let base_url = req.api_base_url.unwrap_or_else(|| {
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

    let mut inner = state.inner.write().await;
    // If this is the first connector, set it as default
    if inner.llm_connectors.is_empty() {
        inner.settings.default_llm_connector_id = Some(connector.id);
    }
    inner.llm_connectors.push(connector.clone());

    let mut resp = connector;
    resp.api_key = mask_key(&resp.api_key);
    Json(resp)
}

async fn patch(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(req): Json<PatchLlmConnectorRequest>,
) -> Result<Json<LlmConnector>, StatusCode> {
    let mut inner = state.inner.write().await;
    let connector = inner
        .llm_connectors
        .iter_mut()
        .find(|c| c.id == id)
        .ok_or(StatusCode::NOT_FOUND)?;

    if let Some(name) = req.name { connector.name = name; }
    if let Some(provider_type) = req.provider_type { connector.provider_type = provider_type; }
    if let Some(key) = req.api_key { connector.api_key = key; }
    if let Some(url) = req.api_base_url { connector.api_base_url = url; }
    if let Some(model) = req.model { connector.model = model; }
    if let Some(dep) = req.azure_deployment { connector.azure_deployment = Some(dep); }
    if let Some(ver) = req.azure_api_version { connector.azure_api_version = Some(ver); }

    let mut resp = connector.clone();
    resp.api_key = mask_key(&resp.api_key);
    Ok(Json(resp))
}

async fn delete(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> StatusCode {
    let mut inner = state.inner.write().await;
    let before = inner.llm_connectors.len();
    inner.llm_connectors.retain(|c| c.id != id);
    if inner.llm_connectors.len() < before {
        // Clear default if it was this connector
        if inner.settings.default_llm_connector_id == Some(id) {
            inner.settings.default_llm_connector_id =
                inner.llm_connectors.first().map(|c| c.id);
        }
        StatusCode::NO_CONTENT
    } else {
        StatusCode::NOT_FOUND
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
