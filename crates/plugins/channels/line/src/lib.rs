/// LINE channel plugin for Clawkson.
/// Integrates with LINE Messaging API via REST webhooks.
use std::collections::HashSet;
use std::sync::Arc;

use clawkson_plugin::{
    ChannelProvider, ClawksonPlugin, FrontendManifest, ConnectorCard,
    InboundMessage, OutboundMessage, PluginCapability, PluginContext, PluginManifest,
};
use serde_json::Value;
use tokio::sync::RwLock;

#[derive(Debug)]
pub struct LineChannelPlugin {
    manifest: PluginManifest,
    /// Active webhook listener handle (if running).
    webhook_handle: RwLock<Option<tokio::task::JoinHandle<()>>>,
}

impl LineChannelPlugin {
    pub fn new() -> Self {
        let mut caps = HashSet::new();
        caps.insert(PluginCapability::Channel);

        Self {
            manifest: PluginManifest {
                name: "line-channel".to_string(),
                display_name: "LINE".to_string(),
                description: "Connect agents to LINE via the Messaging API webhook.".to_string(),
                version: "0.1.0".to_string(),
                dependencies: vec![],
                capabilities: caps,
                frontend: Some(FrontendManifest {
                    sidebar_items: vec![],
                    routes: vec![],
                    settings_panels: vec![],
                    connector_cards: vec![ConnectorCard {
                        connector_type: "line".to_string(),
                        component: "LineConnectorCard".to_string(),
                        display_name: "LINE".to_string(),
                        icon: "message-square".to_string(),
                    }],
                    bundle_url: None,
                }),
            },
            webhook_handle: RwLock::new(None),
        }
    }
}

#[async_trait::async_trait]
impl ClawksonPlugin for LineChannelPlugin {
    fn manifest(&self) -> &PluginManifest {
        &self.manifest
    }

    async fn init(&self, _ctx: &PluginContext) -> anyhow::Result<()> {
        tracing::info!("LINE channel plugin initialized");
        Ok(())
    }

    async fn shutdown(&self) -> anyhow::Result<()> {
        if let Some(handle) = self.webhook_handle.write().await.take() {
            handle.abort();
        }
        tracing::info!("LINE channel plugin shut down");
        Ok(())
    }

    fn migrations(&self) -> Vec<&str> {
        vec![
            "CREATE TABLE IF NOT EXISTS line_channels (
                id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
                connector_id UUID NOT NULL,
                channel_id TEXT NOT NULL,
                agent_id UUID NOT NULL,
                created_at TIMESTAMPTZ NOT NULL DEFAULT now()
            )"
        ]
    }
}

#[async_trait::async_trait]
impl ChannelProvider for LineChannelPlugin {
    fn channel_type(&self) -> &str {
        "line"
    }

    fn config_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "channel_id": {
                    "type": "string",
                    "description": "LINE Messaging API channel ID"
                },
                "channel_secret": {
                    "type": "string",
                    "description": "LINE Messaging API channel secret (for webhook signature verification)"
                },
                "channel_access_token": {
                    "type": "string",
                    "description": "LINE Messaging API long-lived channel access token"
                },
                "webhook_path": {
                    "type": "string",
                    "description": "Webhook endpoint path (default: /webhooks/line)",
                    "default": "/webhooks/line"
                },
                "agent_id": {
                    "type": "string",
                    "description": "Agent ID to route messages to"
                }
            },
            "required": ["channel_id", "channel_secret", "channel_access_token", "agent_id"]
        })
    }

    async fn start(
        &self,
        config: Value,
        _on_message: Box<dyn Fn(InboundMessage) + Send + Sync>,
    ) -> anyhow::Result<()> {
        let channel_id = config
            .get("channel_id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("channel_id required"))?;

        let on_msg = Arc::new(_on_message);
        let channel_id_owned = channel_id.to_string();

        let handle = tokio::spawn(async move {
            tracing::info!(
                channel_id = %channel_id_owned,
                "starting LINE webhook listener"
            );
            // In production: register a webhook handler that receives LINE
            // platform events, verifies signature with channel_secret,
            // parses message events, and forwards to on_msg callback.
            // Replies use the LINE Messaging API reply/push endpoints.
            let _ = on_msg;
            tracing::warn!("LINE bot: webhook listener would run here");
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
            chat = %msg.channel_chat_id,
            text_len = msg.text.len(),
            "LINE: would send push message via Messaging API"
        );
        Ok(())
    }
}
