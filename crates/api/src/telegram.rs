//! Telegram Bot integration — long-polling service that bridges Telegram chats
//! to Clawkson conversations. Each enabled Telegram connector spawns a background
//! task that polls `getUpdates`, routes incoming messages to the right agent, and
//! sends back the LLM response.

use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use crate::state::AppState;

/// Manages running Telegram polling tasks. One poller per connector.
#[derive(Clone)]
pub struct TelegramManager {
    /// Map from connector_id → cancellation token for the polling task.
    pollers: Arc<Mutex<HashMap<Uuid, CancellationToken>>>,
}

impl TelegramManager {
    pub fn new() -> Self {
        Self {
            pollers: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Start polling for a Telegram connector. If already running, does nothing.
    pub async fn start(&self, state: AppState, connector_id: Uuid, user_id: Uuid, bot_token: String, agent_id: Uuid) {
        let mut map = self.pollers.lock().await;
        if map.contains_key(&connector_id) {
            tracing::debug!(%connector_id, "telegram poller already running, skipping");
            return;
        }
        let token = CancellationToken::new();
        map.insert(connector_id, token.clone());
        drop(map);

        let mgr = self.clone();
        tokio::spawn(async move {
            tracing::info!(%connector_id, %agent_id, "starting telegram poller");
            poll_loop(state, connector_id, user_id, &bot_token, agent_id, token).await;
            // Clean up entry when loop exits
            mgr.pollers.lock().await.remove(&connector_id);
            tracing::info!(%connector_id, "telegram poller stopped");
        });
    }

    /// Stop the poller for a connector.
    pub async fn stop(&self, connector_id: Uuid) {
        let mut map = self.pollers.lock().await;
        if let Some(token) = map.remove(&connector_id) {
            token.cancel();
            tracing::info!(%connector_id, "telegram poller cancellation requested");
        }
    }

    /// Stop all pollers (used during graceful shutdown).
    pub async fn shutdown(&self) {
        let mut map = self.pollers.lock().await;
        for (id, token) in map.drain() {
            tracing::info!(%id, "shutting down telegram poller");
            token.cancel();
        }
    }
}

/// Boot all enabled Telegram connectors on startup.
pub async fn boot_pollers(state: &AppState, mgr: &TelegramManager) {
    let connectors = match clawkson_db::connector::list_enabled_by_type(
        &state.db,
        clawkson_db::connector::ConnectorType::Telegram,
    ).await {
        Ok(cs) => cs,
        Err(e) => {
            tracing::warn!("failed to load telegram connectors on boot: {e}");
            return;
        }
    };

    for conn in connectors {
        let bot_token = conn.config.get("bot_token").and_then(|v| v.as_str());
        let agent_id = conn.config.get("agent_id").and_then(|v| v.as_str()).and_then(|s| Uuid::parse_str(s).ok());

        match (bot_token, agent_id) {
            (Some(token), Some(aid)) => {
                mgr.start(state.clone(), conn.id, conn.user_id, token.to_string(), aid).await;
            }
            _ => {
                tracing::warn!(connector_id = %conn.id, "telegram connector missing bot_token or agent_id, skipping");
            }
        }
    }
}

// ── Telegram Bot API types ────────────────────────────────────────

#[allow(dead_code)]
#[derive(Debug, serde::Deserialize)]
struct TgResponse<T> {
    ok: bool,
    result: Option<T>,
    description: Option<String>,
}

#[allow(dead_code)]
#[derive(Debug, serde::Deserialize)]
struct TgUpdate {
    update_id: i64,
    message: Option<TgMessage>,
}

#[allow(dead_code)]
#[derive(Debug, serde::Deserialize)]
struct TgMessage {
    message_id: i64,
    chat: TgChat,
    from: Option<TgUser>,
    text: Option<String>,
}

#[allow(dead_code)]
#[derive(Debug, serde::Deserialize)]
struct TgChat {
    id: i64,
    #[serde(default)]
    first_name: Option<String>,
    #[serde(default)]
    username: Option<String>,
}

#[allow(dead_code)]
#[derive(Debug, serde::Deserialize)]
struct TgUser {
    id: i64,
    #[serde(default)]
    first_name: Option<String>,
    #[serde(default)]
    username: Option<String>,
}

// ── Polling loop ──────────────────────────────────────────────────

async fn poll_loop(
    state: AppState,
    connector_id: Uuid,
    user_id: Uuid,
    bot_token: &str,
    agent_id: Uuid,
    cancel: CancellationToken,
) {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(90))
        .build()
        .unwrap_or_default();

    let base_url = format!("https://api.telegram.org/bot{bot_token}");
    let mut offset: i64 = 0;

    loop {
        if cancel.is_cancelled() {
            break;
        }

        // Long-poll with 60s timeout (Telegram holds the connection open)
        let url = format!(
            "{base_url}/getUpdates?offset={offset}&timeout=60&allowed_updates=[\"message\"]"
        );

        let resp = tokio::select! {
            _ = cancel.cancelled() => break,
            r = client.get(&url).send() => r,
        };

        let updates: Vec<TgUpdate> = match resp {
            Ok(r) => {
                match r.json::<TgResponse<Vec<TgUpdate>>>().await {
                    Ok(tg) if tg.ok => tg.result.unwrap_or_default(),
                    Ok(tg) => {
                        tracing::error!(%connector_id, desc = ?tg.description, "telegram API error");
                        tokio::time::sleep(std::time::Duration::from_secs(5)).await;
                        continue;
                    }
                    Err(e) => {
                        tracing::error!(%connector_id, "failed to parse telegram response: {e}");
                        tokio::time::sleep(std::time::Duration::from_secs(5)).await;
                        continue;
                    }
                }
            }
            Err(e) => {
                tracing::warn!(%connector_id, "telegram poll failed: {e}");
                tokio::time::sleep(std::time::Duration::from_secs(5)).await;
                continue;
            }
        };

        for update in updates {
            offset = update.update_id + 1;

            let Some(msg) = update.message else { continue };
            let Some(text) = msg.text.as_deref() else { continue };
            if text.is_empty() { continue; }

            let chat_id = msg.chat.id;
            let username = msg.from.as_ref().and_then(|u| u.username.as_deref());
            let first_name = msg.from.as_ref().and_then(|u| u.first_name.as_deref())
                .or(msg.chat.first_name.as_deref());

            // Handle the message in a spawned task so we don't block polling
            let state = state.clone();
            let base_url = base_url.clone();
            let client = client.clone();
            let text = text.to_string();
            let username = username.map(|s| s.to_string());
            let first_name = first_name.map(|s| s.to_string());

            tokio::spawn(async move {
                if let Err(e) = handle_message(
                    &state, &client, &base_url,
                    connector_id, user_id, agent_id,
                    chat_id, &text,
                    username.as_deref(), first_name.as_deref(),
                ).await {
                    tracing::error!(%connector_id, %chat_id, "failed to handle telegram message: {e}");
                    // Try to send error to user
                    let _ = send_message(&client, &base_url, chat_id, "Sorry, something went wrong processing your message.").await;
                }
            });
        }
    }
}

/// Handle a single incoming Telegram message.
async fn handle_message(
    state: &AppState,
    client: &reqwest::Client,
    base_url: &str,
    connector_id: Uuid,
    user_id: Uuid,
    agent_id: Uuid,
    chat_id: i64,
    text: &str,
    username: Option<&str>,
    first_name: Option<&str>,
) -> anyhow::Result<()> {
    use crate::routes::conversations::{
        resolve_connector_id, load_agent_config, load_llm_connector,
        load_history, enrich_history, run_completion, AgentConfig,
    };

    // 1. Find or create the conversation for this telegram chat
    let conv_id = match clawkson_db::telegram_chat::get(&state.db, connector_id, chat_id).await? {
        Some(row) => row.conversation_id,
        None => {
            // Create a new conversation
            let title = format!(
                "Telegram: {}",
                first_name.unwrap_or(username.unwrap_or("Unknown"))
            );
            let conv = clawkson_db::conversation::create(
                &state.db,
                Some(agent_id),
                Some(user_id),
                &title,
            ).await?;
            clawkson_db::telegram_chat::create(
                &state.db,
                connector_id,
                chat_id,
                conv.id,
                username,
                first_name,
            ).await?;
            conv.id
        }
    };

    // 2. Save user message
    clawkson_db::message::create(
        &state.db,
        conv_id,
        None,
        clawkson_db::message::MessageRole::User,
        text,
        None,
        None,
    ).await?;

    // 3. Resolve LLM connector
    let connector_id_llm = resolve_connector_id(state, agent_id).await;
    let Some(connector_id_llm) = connector_id_llm else {
        send_message(client, base_url, chat_id, "No LLM connector configured for this agent.").await?;
        return Ok(());
    };
    let connector = load_llm_connector(state, connector_id_llm).await;
    let Some(connector) = connector else {
        send_message(client, base_url, chat_id, "LLM connector not found.").await?;
        return Ok(());
    };

    // 4. Load agent config
    let agent_cfg = load_agent_config(state, agent_id).await;
    let default_cfg = AgentConfig {
        agent_id,
        system_prompt: None,
        temperature: None,
        max_tokens: None,
        container_enabled: false,
        container_config: None,
    };
    let cfg = agent_cfg.as_ref().unwrap_or(&default_cfg);

    // 5. Load & enrich history
    let raw_history = load_history(state, conv_id).await
        .map_err(|_| anyhow::anyhow!("failed to load history"))?;
    let supports_vision = crate::llm::provider_supports_vision(&connector);
    let history = enrich_history(state, raw_history, supports_vision).await;

    // 6. Run completion
    let timeout_secs = clawkson_db::settings::get(&state.db)
        .await
        .map(|s| s.llm_request_timeout_secs as u64)
        .unwrap_or(120);

    let assistant_content = match run_completion(
        state, &connector, cfg, &history, None, conv_id, user_id, true, timeout_secs,
    ).await {
        Ok(text) => text,
        Err(e) => {
            tracing::error!("LLM completion failed for telegram: {e}");
            format!("Error: {e}")
        }
    };

    // 7. Save assistant message
    clawkson_db::message::create(
        &state.db,
        conv_id,
        None,
        clawkson_db::message::MessageRole::Assistant,
        &assistant_content,
        None,
        None,
    ).await?;
    let _ = clawkson_db::conversation::touch(&state.db, conv_id).await;

    // 8. Embed chat turn for memory
    {
        let mem = state.memory.clone();
        let title = format!("Telegram: {}", first_name.unwrap_or("chat"));
        let user_content = text.to_string();
        let asst_content = assistant_content.clone();
        tokio::spawn(async move {
            mem.push_turn(conv_id, user_id, title, user_content, asst_content).await;
        });
    }

    // 9. Send response back to Telegram (split into chunks if > 4096 chars)
    for chunk in split_message(&assistant_content, 4096) {
        send_message(client, base_url, chat_id, chunk).await?;
    }

    Ok(())
}

/// Send a message to a Telegram chat.
async fn send_message(
    client: &reqwest::Client,
    base_url: &str,
    chat_id: i64,
    text: &str,
) -> anyhow::Result<()> {
    let url = format!("{base_url}/sendMessage");
    let body = serde_json::json!({
        "chat_id": chat_id,
        "text": text,
        "parse_mode": "Markdown",
    });

    let resp = client.post(&url).json(&body).send().await?;
    if !resp.status().is_success() {
        // Retry without parse_mode if Markdown fails (malformed markdown in LLM output)
        let fallback = serde_json::json!({
            "chat_id": chat_id,
            "text": text,
        });
        let _ = client.post(&url).json(&fallback).send().await;
    }
    Ok(())
}

/// Split a message into chunks of at most `max_len` characters,
/// trying to break at newlines when possible.
fn split_message(text: &str, max_len: usize) -> Vec<&str> {
    if text.len() <= max_len {
        return vec![text];
    }
    let mut chunks = Vec::new();
    let mut remaining = text;
    while !remaining.is_empty() {
        if remaining.len() <= max_len {
            chunks.push(remaining);
            break;
        }
        // Try to find a newline to break at
        let split_at = remaining[..max_len]
            .rfind('\n')
            .map(|pos| pos + 1)
            .unwrap_or(max_len);
        chunks.push(&remaining[..split_at]);
        remaining = &remaining[split_at..];
    }
    chunks
}
