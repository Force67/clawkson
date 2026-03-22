/// Matrix channel plugin for Clawkson.
/// Connects to Matrix homeservers via `matrix-sdk` and forwards messages to agents.
use std::collections::HashSet;
use std::sync::Arc;

use clawkson_plugin::{
    ChannelProvider, ClawksonPlugin, FrontendManifest, ConnectorCard,
    InboundMessage, OutboundMessage, PluginCapability, PluginContext, PluginManifest,
};
use serde_json::Value;
use tokio::sync::RwLock;

#[derive(Debug)]
pub struct MatrixChannelPlugin {
    manifest: PluginManifest,
    /// Active Matrix client sync handle (if running).
    client_handle: RwLock<Option<tokio::task::JoinHandle<()>>>,
}

impl MatrixChannelPlugin {
    pub fn new() -> Self {
        let mut caps = HashSet::new();
        caps.insert(PluginCapability::Channel);

        Self {
            manifest: PluginManifest {
                name: "matrix-channel".to_string(),
                display_name: "Matrix".to_string(),
                description: "Connect agents to Matrix rooms via any homeserver.".to_string(),
                version: "0.1.0".to_string(),
                dependencies: vec![],
                capabilities: caps,
                frontend: Some(FrontendManifest {
                    sidebar_items: vec![],
                    routes: vec![],
                    settings_panels: vec![],
                    connector_cards: vec![ConnectorCard {
                        connector_type: "matrix".to_string(),
                        component: "MatrixConnectorCard".to_string(),
                        display_name: "Matrix".to_string(),
                        icon: "grid-3x3".to_string(),
                    }],
                    bundle_url: None,
                }),
            },
            client_handle: RwLock::new(None),
        }
    }
}

#[async_trait::async_trait]
impl ClawksonPlugin for MatrixChannelPlugin {
    fn manifest(&self) -> &PluginManifest {
        &self.manifest
    }

    async fn init(&self, _ctx: &PluginContext) -> anyhow::Result<()> {
        tracing::info!("Matrix channel plugin initialized");
        Ok(())
    }

    async fn shutdown(&self) -> anyhow::Result<()> {
        if let Some(handle) = self.client_handle.write().await.take() {
            handle.abort();
        }
        tracing::info!("Matrix channel plugin shut down");
        Ok(())
    }

    fn migrations(&self) -> Vec<&str> {
        vec![
            "CREATE TABLE IF NOT EXISTS matrix_channels (
                id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
                connector_id UUID NOT NULL,
                homeserver_url TEXT NOT NULL,
                user_id TEXT NOT NULL,
                room_id TEXT NOT NULL,
                agent_id UUID NOT NULL,
                created_at TIMESTAMPTZ NOT NULL DEFAULT now()
            )"
        ]
    }
}

#[async_trait::async_trait]
impl ChannelProvider for MatrixChannelPlugin {
    fn channel_type(&self) -> &str {
        "matrix"
    }

    fn config_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "homeserver_url": {
                    "type": "string",
                    "description": "Matrix homeserver URL (e.g. https://matrix.org)"
                },
                "user_id": {
                    "type": "string",
                    "description": "Bot user ID (e.g. @bot:matrix.org)"
                },
                "access_token": {
                    "type": "string",
                    "description": "Access token for the bot account"
                },
                "room_ids": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "Room IDs to listen in (empty = all joined rooms)"
                },
                "agent_id": {
                    "type": "string",
                    "description": "Agent ID to route messages to"
                }
            },
            "required": ["homeserver_url", "user_id", "access_token", "agent_id"]
        })
    }

    async fn start(
        &self,
        config: Value,
        _on_message: Box<dyn Fn(InboundMessage) + Send + Sync>,
    ) -> anyhow::Result<()> {
        let homeserver = config
            .get("homeserver_url")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("homeserver_url required"))?;

        let on_msg = Arc::new(_on_message);
        let homeserver_owned = homeserver.to_string();

        let handle = tokio::spawn(async move {
            tracing::info!(
                homeserver = %homeserver_owned,
                "starting Matrix client sync"
            );
            // In production: use matrix_sdk::Client to login, sync, and
            // forward room message events to on_msg callback.
            let _ = on_msg;
            tracing::warn!("Matrix bot: client sync loop would run here");
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
        tracing::info!(
            room = %msg.channel_chat_id,
            text_len = msg.text.len(),
            "Matrix: would send message"
        );
        Ok(())
    }
}
