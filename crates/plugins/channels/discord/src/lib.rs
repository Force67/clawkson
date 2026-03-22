/// Discord channel plugin for Clawkson.
/// Connects to Discord via serenity and forwards messages to the agent.
use std::collections::HashSet;
use std::sync::Arc;

use clawkson_plugin::{
    ChannelProvider, ClawksonPlugin, FrontendManifest, ConnectorCard,
    InboundMessage, OutboundMessage, PluginCapability, PluginContext, PluginManifest,
};
use serde_json::Value;
use tokio::sync::RwLock;

#[derive(Debug)]
pub struct DiscordChannelPlugin {
    manifest: PluginManifest,
    /// The active Discord client handle (if running).
    client_handle: RwLock<Option<tokio::task::JoinHandle<()>>>,
}

impl DiscordChannelPlugin {
    pub fn new() -> Self {
        let mut caps = HashSet::new();
        caps.insert(PluginCapability::Channel);

        Self {
            manifest: PluginManifest {
                name: "discord-channel".to_string(),
                display_name: "Discord".to_string(),
                description: "Connect agents to Discord servers and channels.".to_string(),
                version: "0.1.0".to_string(),
                dependencies: vec![],
                capabilities: caps,
                frontend: Some(FrontendManifest {
                    sidebar_items: vec![],
                    routes: vec![],
                    settings_panels: vec![],
                    connector_cards: vec![ConnectorCard {
                        connector_type: "discord".to_string(),
                        component: "DiscordConnectorCard".to_string(),
                        display_name: "Discord".to_string(),
                        icon: "message-circle".to_string(),
                    }],
                    bundle_url: None,
                }),
            },
            client_handle: RwLock::new(None),
        }
    }
}

#[async_trait::async_trait]
impl ClawksonPlugin for DiscordChannelPlugin {
    fn manifest(&self) -> &PluginManifest {
        &self.manifest
    }

    async fn init(&self, _ctx: &PluginContext) -> anyhow::Result<()> {
        tracing::info!("Discord channel plugin initialized");
        Ok(())
    }

    async fn shutdown(&self) -> anyhow::Result<()> {
        if let Some(handle) = self.client_handle.write().await.take() {
            handle.abort();
        }
        tracing::info!("Discord channel plugin shut down");
        Ok(())
    }

    fn migrations(&self) -> Vec<&str> {
        vec![
            "CREATE TABLE IF NOT EXISTS discord_channels (
                id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
                connector_id UUID NOT NULL,
                guild_id TEXT NOT NULL,
                channel_id TEXT NOT NULL,
                agent_id UUID NOT NULL,
                created_at TIMESTAMPTZ NOT NULL DEFAULT now()
            )"
        ]
    }
}

#[async_trait::async_trait]
impl ChannelProvider for DiscordChannelPlugin {
    fn channel_type(&self) -> &str {
        "discord"
    }

    fn config_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "bot_token": {
                    "type": "string",
                    "description": "Discord bot token from the Developer Portal"
                },
                "agent_id": {
                    "type": "string",
                    "description": "Agent ID to route messages to"
                },
                "guild_id": {
                    "type": "string",
                    "description": "Discord server (guild) ID"
                },
                "channel_ids": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "Specific channel IDs to listen in (empty = all)"
                }
            },
            "required": ["bot_token", "agent_id"]
        })
    }

    async fn start(
        &self,
        config: Value,
        on_message: Box<dyn Fn(InboundMessage) + Send + Sync>,
    ) -> anyhow::Result<()> {
        let bot_token = config
            .get("bot_token")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("bot_token required"))?
            .to_string();

        let channel_filter: Vec<String> = config
            .get("channel_ids")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default();

        let on_msg = Arc::new(on_message);
        let filter = Arc::new(channel_filter);

        let handle = tokio::spawn(async move {
            tracing::info!("starting Discord bot connection");
            // In production, this would use serenity::Client::builder()
            // to create a client with the GatewayIntents and an EventHandler
            // that forwards messages to on_msg.
            //
            // Simplified placeholder — real implementation would:
            // 1. Build serenity client with MessageCreate intent
            // 2. In EventHandler::message, check channel_filter
            // 3. Call on_msg(InboundMessage { ... })
            // 4. Run client.start().await
            let _ = (bot_token, on_msg, filter);
            tracing::warn!("Discord bot: serenity client loop would run here");
            // Keep alive
            loop {
                tokio::time::sleep(std::time::Duration::from_secs(3600)).await;
            }
        });

        *self.client_handle.write().await = Some(handle);
        Ok(())
    }

    async fn stop(&self) -> anyhow::Result<()> {
        if let Some(handle) = self.client_handle.write().await.take() {
            handle.abort();
        }
        Ok(())
    }

    async fn send_message(&self, msg: OutboundMessage) -> anyhow::Result<()> {
        // In production: use serenity's HTTP API to send a message to the channel.
        tracing::info!(
            channel = %msg.channel_chat_id,
            text_len = msg.text.len(),
            "Discord: would send message"
        );
        Ok(())
    }
}
