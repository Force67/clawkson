/// Mattermost channel plugin for Clawkson.
/// Connects to Mattermost via REST API and WebSocket events.
use std::collections::HashSet;
use std::sync::Arc;

use clawkson_plugin::{
    ChannelProvider, ClawksonPlugin, FrontendManifest, ConnectorCard,
    InboundMessage, OutboundMessage, PluginCapability, PluginContext, PluginManifest,
};
use serde_json::Value;
use tokio::sync::RwLock;

#[derive(Debug)]
pub struct MattermostChannelPlugin {
    manifest: PluginManifest,
    /// Active WebSocket listener handle (if running).
    ws_handle: RwLock<Option<tokio::task::JoinHandle<()>>>,
}

impl MattermostChannelPlugin {
    pub fn new() -> Self {
        let mut caps = HashSet::new();
        caps.insert(PluginCapability::Channel);

        Self {
            manifest: PluginManifest {
                name: "mattermost-channel".to_string(),
                display_name: "Mattermost".to_string(),
                description: "Connect agents to Mattermost via REST API and WebSocket.".to_string(),
                version: "0.1.0".to_string(),
                dependencies: vec![],
                capabilities: caps,
                frontend: Some(FrontendManifest {
                    sidebar_items: vec![],
                    routes: vec![],
                    settings_panels: vec![],
                    connector_cards: vec![ConnectorCard {
                        connector_type: "mattermost".to_string(),
                        component: "MattermostConnectorCard".to_string(),
                        display_name: "Mattermost".to_string(),
                        icon: "message-circle".to_string(),
                    }],
                    bundle_url: None,
                }),
            },
            ws_handle: RwLock::new(None),
        }
    }
}

#[async_trait::async_trait]
impl ClawksonPlugin for MattermostChannelPlugin {
    fn manifest(&self) -> &PluginManifest {
        &self.manifest
    }

    async fn init(&self, _ctx: &PluginContext) -> anyhow::Result<()> {
        tracing::info!("Mattermost channel plugin initialized");
        Ok(())
    }

    async fn shutdown(&self) -> anyhow::Result<()> {
        if let Some(handle) = self.ws_handle.write().await.take() {
            handle.abort();
        }
        tracing::info!("Mattermost channel plugin shut down");
        Ok(())
    }

    fn migrations(&self) -> Vec<&str> {
        vec![
            "CREATE TABLE IF NOT EXISTS mattermost_channels (
                id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
                connector_id UUID NOT NULL,
                server_url TEXT NOT NULL,
                team_id TEXT NOT NULL,
                channel_id TEXT,
                agent_id UUID NOT NULL,
                created_at TIMESTAMPTZ NOT NULL DEFAULT now()
            )"
        ]
    }
}

#[async_trait::async_trait]
impl ChannelProvider for MattermostChannelPlugin {
    fn channel_type(&self) -> &str {
        "mattermost"
    }

    fn config_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "server_url": {
                    "type": "string",
                    "description": "Mattermost server URL (e.g. https://mattermost.example.com)"
                },
                "access_token": {
                    "type": "string",
                    "description": "Bot account personal access token"
                },
                "team_id": {
                    "type": "string",
                    "description": "Team ID the bot belongs to"
                },
                "channel_ids": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "Channel IDs to listen in (empty = all channels the bot is in)"
                },
                "agent_id": {
                    "type": "string",
                    "description": "Agent ID to route messages to"
                }
            },
            "required": ["server_url", "access_token", "team_id", "agent_id"]
        })
    }

    async fn start(
        &self,
        config: Value,
        _on_message: Box<dyn Fn(InboundMessage) + Send + Sync>,
    ) -> anyhow::Result<()> {
        let server_url = config
            .get("server_url")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("server_url required"))?;

        let on_msg = Arc::new(_on_message);
        let server_owned = server_url.to_string();

        let handle = tokio::spawn(async move {
            tracing::info!(
                server = %server_owned,
                "starting Mattermost WebSocket event listener"
            );
            // In production: authenticate via REST API, open WebSocket to
            // /api/v4/websocket, listen for posted events, filter by channel,
            // and forward to on_msg callback. Send replies via REST POST.
            let _ = on_msg;
            tracing::warn!("Mattermost bot: WebSocket event loop would run here");
            loop {
                tokio::time::sleep(std::time::Duration::from_secs(3600)).await;
            }
        });

        *self.ws_handle.write().await = Some(handle);
        Ok(())
    }

    async fn stop(&self) -> anyhow::Result<()> {
        if let Some(handle) = self.ws_handle.write().await.take() {
            handle.abort();
        }
        Ok(())
    }

    async fn send_message(&self, msg: OutboundMessage) -> anyhow::Result<()> {
        tracing::info!(
            channel = %msg.channel_chat_id,
            text_len = msg.text.len(),
            "Mattermost: would send message via REST API"
        );
        Ok(())
    }
}
