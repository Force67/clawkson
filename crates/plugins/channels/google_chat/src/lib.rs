/// Google Chat channel plugin for Clawkson.
/// Integrates with Google Chat via REST webhook / Google Workspace Events API.
use std::collections::HashSet;
use std::sync::Arc;

use clawkson_plugin::{
    ChannelProvider, ClawksonPlugin, FrontendManifest, ConnectorCard,
    InboundMessage, OutboundMessage, PluginCapability, PluginContext, PluginManifest,
};
use serde_json::Value;
use tokio::sync::RwLock;

#[derive(Debug)]
pub struct GoogleChatChannelPlugin {
    manifest: PluginManifest,
    /// Active webhook listener handle (if running).
    webhook_handle: RwLock<Option<tokio::task::JoinHandle<()>>>,
}

impl GoogleChatChannelPlugin {
    pub fn new() -> Self {
        let mut caps = HashSet::new();
        caps.insert(PluginCapability::Channel);

        Self {
            manifest: PluginManifest {
                name: "google-chat-channel".to_string(),
                display_name: "Google Chat".to_string(),
                description: "Connect agents to Google Chat spaces via webhook and REST API.".to_string(),
                version: "0.1.0".to_string(),
                dependencies: vec![],
                capabilities: caps,
                frontend: Some(FrontendManifest {
                    sidebar_items: vec![],
                    routes: vec![],
                    settings_panels: vec![],
                    connector_cards: vec![ConnectorCard {
                        connector_type: "google_chat".to_string(),
                        component: "GoogleChatConnectorCard".to_string(),
                        display_name: "Google Chat".to_string(),
                        icon: "message-square-text".to_string(),
                    }],
                    bundle_url: None,
                }),
            },
            webhook_handle: RwLock::new(None),
        }
    }
}

#[async_trait::async_trait]
impl ClawksonPlugin for GoogleChatChannelPlugin {
    fn manifest(&self) -> &PluginManifest {
        &self.manifest
    }

    async fn init(&self, _ctx: &PluginContext) -> anyhow::Result<()> {
        tracing::info!("Google Chat channel plugin initialized");
        Ok(())
    }

    async fn shutdown(&self) -> anyhow::Result<()> {
        if let Some(handle) = self.webhook_handle.write().await.take() {
            handle.abort();
        }
        tracing::info!("Google Chat channel plugin shut down");
        Ok(())
    }

    fn migrations(&self) -> Vec<&str> {
        vec![
            "CREATE TABLE IF NOT EXISTS google_chat_channels (
                id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
                connector_id UUID NOT NULL,
                project_id TEXT NOT NULL,
                space_name TEXT,
                agent_id UUID NOT NULL,
                created_at TIMESTAMPTZ NOT NULL DEFAULT now()
            )"
        ]
    }
}

#[async_trait::async_trait]
impl ChannelProvider for GoogleChatChannelPlugin {
    fn channel_type(&self) -> &str {
        "google_chat"
    }

    fn config_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "project_id": {
                    "type": "string",
                    "description": "Google Cloud project ID"
                },
                "service_account_key": {
                    "type": "string",
                    "description": "Service account JSON key (stringified) for Google Chat API auth"
                },
                "space_names": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "Google Chat space names to listen in (e.g. [\"spaces/AAAA\"])"
                },
                "webhook_path": {
                    "type": "string",
                    "description": "Webhook endpoint path for incoming events",
                    "default": "/webhooks/google-chat"
                },
                "agent_id": {
                    "type": "string",
                    "description": "Agent ID to route messages to"
                }
            },
            "required": ["project_id", "service_account_key", "agent_id"]
        })
    }

    async fn start(
        &self,
        config: Value,
        _on_message: Box<dyn Fn(InboundMessage) + Send + Sync>,
    ) -> anyhow::Result<()> {
        let project_id = config
            .get("project_id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("project_id required"))?;

        let on_msg = Arc::new(_on_message);
        let project_owned = project_id.to_string();

        let handle = tokio::spawn(async move {
            tracing::info!(
                project = %project_owned,
                "starting Google Chat webhook listener"
            );
            // In production: register webhook endpoint for Google Chat events,
            // authenticate with service account, process MESSAGE events,
            // and forward to on_msg callback. Replies use the Chat REST API.
            let _ = on_msg;
            tracing::warn!("Google Chat bot: webhook listener would run here");
            loop {
                tokio::time::sleep(std::time::Duration::from_secs(3600)).await;
            }
        });

        *self.webhook_handle.write().await = Some(handle);
        Ok(())
    }

    async fn stop(&self) -> anyhow::Result<()> {
        if let Some(handle) = self.webhook_handle.write().await.take() {
            handle.abort();
        }
        Ok(())
    }

    async fn send_message(&self, msg: OutboundMessage) -> anyhow::Result<()> {
        tracing::info!(
            space = %msg.channel_chat_id,
            text_len = msg.text.len(),
            "Google Chat: would send message via Chat REST API"
        );
        Ok(())
    }
}
