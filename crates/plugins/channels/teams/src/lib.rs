/// Microsoft Teams channel plugin for Clawkson.
/// Integrates with Teams via the Microsoft Graph API and Bot Framework.
use std::collections::HashSet;
use std::sync::Arc;

use clawkson_plugin::{
    ChannelProvider, ClawksonPlugin, FrontendManifest, ConnectorCard,
    InboundMessage, OutboundMessage, PluginCapability, PluginContext, PluginManifest,
};
use serde_json::Value;
use tokio::sync::RwLock;

#[derive(Debug)]
pub struct TeamsChannelPlugin {
    manifest: PluginManifest,
    /// Active webhook listener / polling handle (if running).
    client_handle: RwLock<Option<tokio::task::JoinHandle<()>>>,
}

impl TeamsChannelPlugin {
    pub fn new() -> Self {
        let mut caps = HashSet::new();
        caps.insert(PluginCapability::Channel);

        Self {
            manifest: PluginManifest {
                name: "teams-channel".to_string(),
                display_name: "Microsoft Teams".to_string(),
                description: "Connect agents to Microsoft Teams channels and chats via Graph API.".to_string(),
                version: "0.1.0".to_string(),
                dependencies: vec![],
                capabilities: caps,
                frontend: Some(FrontendManifest {
                    sidebar_items: vec![],
                    routes: vec![],
                    settings_panels: vec![],
                    connector_cards: vec![ConnectorCard {
                        connector_type: "teams".to_string(),
                        component: "TeamsConnectorCard".to_string(),
                        display_name: "Microsoft Teams".to_string(),
                        icon: "users".to_string(),
                    }],
                    bundle_url: None,
                }),
            },
            client_handle: RwLock::new(None),
        }
    }
}

#[async_trait::async_trait]
impl ClawksonPlugin for TeamsChannelPlugin {
    fn manifest(&self) -> &PluginManifest {
        &self.manifest
    }

    async fn init(&self, _ctx: &PluginContext) -> anyhow::Result<()> {
        tracing::info!("Microsoft Teams channel plugin initialized");
        Ok(())
    }

    async fn shutdown(&self) -> anyhow::Result<()> {
        if let Some(handle) = self.client_handle.write().await.take() {
            handle.abort();
        }
        tracing::info!("Microsoft Teams channel plugin shut down");
        Ok(())
    }

    fn migrations(&self) -> Vec<&str> {
        vec![
            "CREATE TABLE IF NOT EXISTS teams_channels (
                id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
                connector_id UUID NOT NULL,
                tenant_id TEXT NOT NULL,
                team_id TEXT,
                channel_id TEXT,
                agent_id UUID NOT NULL,
                created_at TIMESTAMPTZ NOT NULL DEFAULT now()
            )"
        ]
    }
}

#[async_trait::async_trait]
impl ChannelProvider for TeamsChannelPlugin {
    fn channel_type(&self) -> &str {
        "teams"
    }

    fn config_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "tenant_id": {
                    "type": "string",
                    "description": "Azure AD tenant ID"
                },
                "client_id": {
                    "type": "string",
                    "description": "Azure AD app registration client ID"
                },
                "client_secret": {
                    "type": "string",
                    "description": "Azure AD app registration client secret"
                },
                "bot_id": {
                    "type": "string",
                    "description": "Bot Framework bot ID (from Bot Channels Registration)"
                },
                "team_id": {
                    "type": "string",
                    "description": "Teams team ID to listen in (optional, for team-scoped bots)"
                },
                "channel_id": {
                    "type": "string",
                    "description": "Teams channel ID to listen in (optional)"
                },
                "webhook_url": {
                    "type": "string",
                    "description": "Public webhook URL for Bot Framework messages"
                },
                "agent_id": {
                    "type": "string",
                    "description": "Agent ID to route messages to"
                }
            },
            "required": ["tenant_id", "client_id", "client_secret", "agent_id"]
        })
    }

    async fn start(
        &self,
        config: Value,
        _on_message: Box<dyn Fn(InboundMessage) + Send + Sync>,
    ) -> anyhow::Result<()> {
        let tenant_id = config
            .get("tenant_id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("tenant_id required"))?;

        let on_msg = Arc::new(_on_message);
        let tenant_owned = tenant_id.to_string();

        let handle = tokio::spawn(async move {
            tracing::info!(
                tenant = %tenant_owned,
                "starting Microsoft Teams bot via Graph API"
            );
            // In production: authenticate with Azure AD, register webhook endpoint,
            // process Bot Framework activity messages, forward to on_msg callback.
            let _ = on_msg;
            tracing::warn!("Teams bot: Graph API listener would run here");
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
            conversation = %msg.channel_chat_id,
            text_len = msg.text.len(),
            "Teams: would send message via Graph API"
        );
        Ok(())
    }
}
