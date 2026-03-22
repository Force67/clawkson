use axum::{
    body::Bytes,
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    routing::get,
    Json, Router,
};
use clawkson_core::{Webhook, WebhookExecution};
use serde::Deserialize;
use uuid::Uuid;

use crate::auth::AuthUser;
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", get(list_webhooks).post(create_webhook))
        .route(
            "/{id}",
            get(get_webhook)
                .patch(update_webhook)
                .delete(delete_webhook),
        )
        .route("/{id}/executions", get(list_executions))
        .route("/{id}/incoming", axum::routing::post(incoming_webhook))
}

// ── Request types ───────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct CreateWebhookRequest {
    pub name: String,
    pub agent_id: Uuid,
    #[serde(default)]
    pub description: String,
    pub payload_template: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct PatchWebhookRequest {
    pub name: Option<String>,
    pub description: Option<String>,
    pub enabled: Option<bool>,
    pub payload_template: Option<Option<String>>,
}

// ── Type mapping ────────────────────────────────────────────────

fn row_to_api(row: clawkson_db::webhook::WebhookRow) -> Webhook {
    Webhook {
        id: row.id,
        owner_id: row.owner_id,
        agent_id: row.agent_id,
        name: row.name,
        description: row.description,
        secret: row.secret,
        enabled: row.enabled,
        payload_template: row.payload_template,
        created_at: row.created_at,
        updated_at: row.updated_at,
    }
}

fn exec_to_api(row: clawkson_db::webhook::WebhookExecutionRow) -> WebhookExecution {
    WebhookExecution {
        id: row.id,
        webhook_id: row.webhook_id,
        conversation_id: row.conversation_id,
        status: row.status,
        result_summary: row.result_summary,
        error_message: row.error_message,
        payload: row.payload,
        started_at: row.started_at,
        completed_at: row.completed_at,
        duration_ms: row.duration_ms,
    }
}

// ── CRUD handlers ───────────────────────────────────────────────

async fn list_webhooks(
    auth: AuthUser,
    State(state): State<AppState>,
) -> Result<Json<Vec<Webhook>>, StatusCode> {
    let rows = clawkson_db::webhook::list_for_user(&state.db, auth.id())
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(rows.into_iter().map(row_to_api).collect()))
}

async fn create_webhook(
    auth: AuthUser,
    State(state): State<AppState>,
    Json(req): Json<CreateWebhookRequest>,
) -> Result<(StatusCode, Json<Webhook>), StatusCode> {
    // Generate a 32-byte hex secret
    use rand::Rng;
    let secret: String = rand::thread_rng()
        .sample_iter(&rand::distributions::Alphanumeric)
        .take(64)
        .map(char::from)
        .collect();

    let row = clawkson_db::webhook::create(
        &state.db,
        auth.id(),
        req.agent_id,
        &req.name,
        &req.description,
        &secret,
        req.payload_template.as_deref(),
    )
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok((StatusCode::CREATED, Json(row_to_api(row))))
}

async fn get_webhook(
    auth: AuthUser,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<Webhook>, StatusCode> {
    let row = clawkson_db::webhook::get_by_id(&state.db, id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;

    if row.owner_id != auth.id() && !auth.is_admin() {
        return Err(StatusCode::FORBIDDEN);
    }

    Ok(Json(row_to_api(row)))
}

async fn update_webhook(
    auth: AuthUser,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(req): Json<PatchWebhookRequest>,
) -> Result<Json<Webhook>, StatusCode> {
    let existing = clawkson_db::webhook::get_by_id(&state.db, id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;

    if existing.owner_id != auth.id() && !auth.is_admin() {
        return Err(StatusCode::FORBIDDEN);
    }

    let pt = req
        .payload_template
        .as_ref()
        .map(|o| o.as_deref());

    let row = clawkson_db::webhook::update(
        &state.db,
        id,
        req.name.as_deref(),
        req.description.as_deref(),
        req.enabled,
        pt,
    )
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
    .ok_or(StatusCode::NOT_FOUND)?;

    Ok(Json(row_to_api(row)))
}

async fn delete_webhook(
    auth: AuthUser,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> StatusCode {
    let existing = match clawkson_db::webhook::get_by_id(&state.db, id).await {
        Ok(Some(r)) => r,
        Ok(None) => return StatusCode::NOT_FOUND,
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR,
    };

    if existing.owner_id != auth.id() && !auth.is_admin() {
        return StatusCode::FORBIDDEN;
    }

    match clawkson_db::webhook::delete(&state.db, id).await {
        Ok(true) => StatusCode::NO_CONTENT,
        Ok(false) => StatusCode::NOT_FOUND,
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR,
    }
}

async fn list_executions(
    auth: AuthUser,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<Vec<WebhookExecution>>, StatusCode> {
    let webhook = clawkson_db::webhook::get_by_id(&state.db, id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;

    if webhook.owner_id != auth.id() && !auth.is_admin() {
        return Err(StatusCode::FORBIDDEN);
    }

    let rows = clawkson_db::webhook::list_executions(&state.db, id, 50)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(rows.into_iter().map(exec_to_api).collect()))
}

// ── Incoming webhook handler ────────────────────────────────────

/// POST /api/webhooks/{id}/incoming — trigger webhook (no auth, verified via secret)
async fn incoming_webhook(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Json<serde_json::Value>, StatusCode> {
    // 1. Body size limit (1MB)
    if body.len() > 1_048_576 {
        return Err(StatusCode::PAYLOAD_TOO_LARGE);
    }

    // 2. Load webhook
    let webhook = clawkson_db::webhook::get_by_id(&state.db, id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;

    if !webhook.enabled {
        return Err(StatusCode::NOT_FOUND);
    }

    // 3. Verify auth: check Authorization: Bearer <secret> or X-Webhook-Signature
    let authorized = if let Some(auth) = headers.get("authorization") {
        let auth_str = auth.to_str().unwrap_or("");
        auth_str == format!("Bearer {}", webhook.secret)
    } else if let Some(sig) = headers.get("x-webhook-signature") {
        // Verify HMAC-SHA256
        use hmac::{Hmac, Mac};
        use sha2::Sha256;
        type HmacSha256 = Hmac<Sha256>;

        let sig_str = sig.to_str().unwrap_or("");
        let sig_hex = sig_str.strip_prefix("sha256=").unwrap_or(sig_str);

        if let Ok(mut mac) = HmacSha256::new_from_slice(webhook.secret.as_bytes()) {
            mac.update(&body);
            let expected = hex::encode(mac.finalize().into_bytes());
            expected == sig_hex
        } else {
            false
        }
    } else {
        false
    };

    if !authorized {
        return Err(StatusCode::UNAUTHORIZED);
    }

    // 4. Parse payload
    let payload: Option<serde_json::Value> = serde_json::from_slice(&body).ok();

    // 5. Create execution record
    let exec = clawkson_db::webhook::create_execution(&state.db, id, payload.as_ref())
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let exec_id = exec.id;

    // 6. Spawn background execution task
    let state2 = state.clone();
    let webhook2 = webhook.clone();
    let payload2 = payload.clone();
    tokio::spawn(async move {
        if let Err(e) = execute_webhook(&state2, &webhook2, exec_id, payload2).await {
            tracing::error!(webhook_id = %id, exec_id = %exec_id, "webhook execution failed: {e}");
        }
    });

    // 7. Return 200 immediately with execution ID
    Ok(Json(serde_json::json!({
        "execution_id": exec_id,
        "status": "accepted"
    })))
}

/// Execute the webhook: create conversation, send prompt, run LLM, save result.
async fn execute_webhook(
    state: &AppState,
    webhook: &clawkson_db::webhook::WebhookRow,
    exec_id: Uuid,
    payload: Option<serde_json::Value>,
) -> anyhow::Result<()> {
    use crate::routes::conversations::{
        attach_workspace_outputs, enrich_history, expand_skill_references, load_agent_config,
        load_history, load_llm_connector, resolve_connector_id,
        run_completion_for_task as run_completion, AgentConfig,
    };

    let start = std::time::Instant::now();

    // 1. Create a conversation for this webhook execution
    let title = format!(
        "Webhook: {} @ {}",
        webhook.name,
        chrono::Utc::now().format("%Y-%m-%d %H:%M")
    );
    let conv = match clawkson_db::conversation::create(
        &state.db,
        Some(webhook.agent_id),
        Some(webhook.owner_id),
        &title,
    )
    .await
    {
        Ok(c) => c,
        Err(e) => {
            finish_webhook_execution(state, exec_id, start, "error", None, Some(&e.to_string()))
                .await;
            return Err(e.into());
        }
    };

    let _ =
        clawkson_db::webhook::set_execution_conversation(&state.db, exec_id, conv.id).await;

    // 2. Format prompt from payload
    let prompt = if let Some(ref template) = webhook.payload_template {
        if let Some(ref p) = payload {
            // Simple template: replace {{payload}} with JSON
            template.replace("{{payload}}", &serde_json::to_string_pretty(p).unwrap_or_default())
        } else {
            template.clone()
        }
    } else if let Some(ref p) = payload {
        format!(
            "Webhook '{}' triggered with payload:\n```json\n{}\n```",
            webhook.name,
            serde_json::to_string_pretty(p).unwrap_or_default()
        )
    } else {
        format!("Webhook '{}' triggered (no payload).", webhook.name)
    };

    // 3. Save user message
    let expanded_prompt = expand_skill_references(state, webhook.agent_id, &prompt).await;
    clawkson_db::message::create(
        &state.db,
        conv.id,
        None,
        clawkson_db::message::MessageRole::User,
        &expanded_prompt,
        None,
        None,
    )
    .await?;

    // 4. Resolve LLM connector
    let connector_id = resolve_connector_id(state, webhook.agent_id).await;
    let Some(connector_id) = connector_id else {
        finish_webhook_execution(
            state,
            exec_id,
            start,
            "error",
            None,
            Some("No LLM connector configured"),
        )
        .await;
        return Ok(());
    };
    let connector = load_llm_connector(state, connector_id).await;
    let Some(connector) = connector else {
        finish_webhook_execution(
            state,
            exec_id,
            start,
            "error",
            None,
            Some("LLM connector not found"),
        )
        .await;
        return Ok(());
    };

    // 5. Load agent config
    let agent_cfg = load_agent_config(state, webhook.agent_id).await;
    let default_cfg = AgentConfig {
        agent_id: webhook.agent_id,
        system_prompt: None,
        temperature: None,
        max_tokens: None,
        container_enabled: false,
        container_config: None,
        connector_policies: vec![],
        subtask_llm_connector_id: None,
        subtask_temperature: None,
        subtask_max_tokens: None,
        skill_names: vec![],
    };
    let cfg = agent_cfg.as_ref().unwrap_or(&default_cfg);

    // 6. Load & enrich history
    let raw_history = load_history(state, conv.id)
        .await
        .map_err(|_| anyhow::anyhow!("failed to load history"))?;
    let supports_vision = crate::llm::provider_supports_vision(&connector);
    let container_enabled = cfg.container_enabled;
    let history = enrich_history(state, raw_history, supports_vision, container_enabled).await;

    // 7. Run completion
    let timeout_secs = clawkson_db::settings::get(&state.db)
        .await
        .map(|s| s.llm_request_timeout_secs as u64)
        .unwrap_or(120);

    let assistant_content = match run_completion(
        state,
        &connector,
        cfg,
        &history,
        None,
        conv.id,
        webhook.owner_id,
        true,
        timeout_secs,
    )
    .await
    {
        Ok(cr) => cr.text,
        Err(e) => {
            tracing::error!(webhook_id = %webhook.id, "LLM completion failed: {e}");
            finish_webhook_execution(
                state,
                exec_id,
                start,
                "error",
                None,
                Some(&format!("LLM error: {e}")),
            )
            .await;
            return Ok(());
        }
    };

    // 8. Save assistant message
    let assistant_msg = clawkson_db::message::create(
        &state.db,
        conv.id,
        None,
        clawkson_db::message::MessageRole::Assistant,
        &assistant_content,
        None,
        None,
    )
    .await?;
    let _ = clawkson_db::conversation::touch(&state.db, conv.id).await;

    // 9. Attach workspace outputs
    if cfg.container_enabled {
        attach_workspace_outputs(
            state,
            webhook.agent_id,
            assistant_msg.id,
            webhook.owner_id,
            conv.id,
        )
        .await;
    }

    // 10. Embed memory turn
    {
        let mem = state.memory.clone();
        let title = format!("Webhook: {}", webhook.name);
        let user_content = prompt.clone();
        let asst_content = assistant_content.clone();
        let owner_id = webhook.owner_id;
        let agent_id = webhook.agent_id;
        tokio::spawn(async move {
            mem.push_turn(conv.id, agent_id, owner_id, title, user_content, asst_content)
                .await;
        });
    }

    // 11. Finalize execution
    let summary: String = assistant_content.chars().take(200).collect();
    finish_webhook_execution(state, exec_id, start, "success", Some(&summary), None).await;

    tracing::info!(
        webhook_id = %webhook.id,
        webhook_name = %webhook.name,
        conv_id = %conv.id,
        duration_ms = %start.elapsed().as_millis(),
        "webhook execution completed"
    );

    Ok(())
}

async fn finish_webhook_execution(
    state: &AppState,
    exec_id: Uuid,
    start: std::time::Instant,
    status: &str,
    summary: Option<&str>,
    error: Option<&str>,
) {
    let _ = clawkson_db::webhook::complete_execution(
        &state.db,
        exec_id,
        status,
        summary,
        error,
        start.elapsed().as_millis() as i64,
    )
    .await;
}
