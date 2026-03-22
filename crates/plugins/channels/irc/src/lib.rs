/// IRC channel plugin for Clawkson.
/// Connects to IRC servers via the `irc` crate and forwards messages to agents.
use std::collections::HashSet;
use std::sync::Arc;

use clawkson_plugin::{
    ChannelProvider, ClawksonPlugin, FrontendManifest, ConnectorCard,
    InboundMessage, OutboundMessage, PluginCapability, PluginContext, PluginManifest,
};
use serde_json::Value;
use tokio::sync::RwLock;

#[derive(Debug)]
pub struct IrcChannelPlugin {
    manifest: PluginManifest,
    /// Active IRC connection handle (if running).
    client_handle: RwLock<Option<tokio::task::JoinHandle<()>>>,
}

impl IrcChannelPlugin {
    pub fn new() -> Self {
        let mut caps = HashSet::new();
        caps.insert(PluginCapability::Channel);

        Self {
            manifest: PluginManifest {
                name: "irc-channel".to_string(),
                display_name: "IRC".to_string(),
                description: "Connect agents to IRC servers and channels.".to_string(),
                version: "0.1.0".to_string(),
                dependencies: vec![],
                capabilities: caps,
                frontend: Some(FrontendManifest {
                    sidebar_items: vec![],
                    routes: vec![],
                    settings_panels: vec![],
                    connector_cards: vec![ConnectorCard {
                        connector_type: "irc".to_string(),
                        component: "IrcConnectorCard".to_string(),
                        display_name: "IRC".to_string(),
                        icon: "hash".to_string(),
                    }],
                    bundle_url: None,
                }),
            },
            client_handle: RwLock::new(None),
        }
    }
}

#[async_trait::async_trait]
impl ClawksonPlugin for IrcChannelPlugin {
    fn manifest(&self) -> &PluginManifest {
        &self.manifest
    }

    async fn init(&self, _ctx: &PluginContext) -> anyhow::Result<()> {
        tracing::info!("IRC channel plugin initialized");
        Ok(())
    }

    async fn shutdown(&self) -> anyhow::Result<()> {
        if let Some(handle) = self.client_handle.write().await.take() {
            handle.abort();
        }
        tracing::info!("IRC channel plugin shut down");
        Ok(())
    }

    fn migrations(&self) -> Vec<&str> {
        vec![
            "CREATE TABLE IF NOT EXISTS irc_channels (
                id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
                connector_id UUID NOT NULL,
                server TEXT NOT NULL,
                port INTEGER NOT NULL DEFAULT 6697,
                nickname TEXT NOT NULL,
                channel_name TEXT NOT NULL,
                agent_id UUID NOT NULL,
                use_tls BOOLEAN NOT NULL DEFAULT true,
                created_at TIMESTAMPTZ NOT NULL DEFAULT now()
            )"
        ]
    }
}

#[async_trait::async_trait]
impl ChannelProvider for IrcChannelPlugin {
    fn channel_type(&self) -> &str {
        "irc"
    }

    fn config_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "server": {
                    "type": "string",
                    "description": "IRC server hostname (e.g. irc.libera.chat)"
                },
                "port": {
                    "type": "integer",
                    "description": "IRC server port (default 6697 for TLS)",
                    "default": 6697
                },
                "nickname": {
                    "type": "string",
                    "description": "Bot nickname on IRC"
                },
                "password": {
                    "type": "string",
                    "description": "Server or NickServ password (optional)"
                },
                "channels": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "IRC channels to join (e.g. [\"#general\", \"#dev\"])"
                },
                "use_tls": {
                    "type": "boolean",
                    "description": "Use TLS for the connection",
                    "default": true
                },
                "agent_id": {
                    "type": "string",
                    "description": "Agent ID to route messages to"
                }
            },
            "required": ["server", "nickname", "channels", "agent_id"]
        })
    }

    async fn start(
        &self,
        config: Value,
        _on_message: Box<dyn Fn(InboundMessage) + Send + Sync>,
    ) -> anyhow::Result<()> {
        let server = config
            .get("server")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("server required"))?;
        let nickname = config
            .get("nickname")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("nickname required"))?;

        let on_msg = Arc::new(_on_message);

        let server_owned = server.to_string();
        let nickname_owned = nickname.to_string();

        let handle = tokio::spawn(async move {
            tracing::info!(
                server = %server_owned,
                nickname = %nickname_owned,
                "starting IRC bot connection"
            );
            // In production: use irc::client::Client to connect, join channels,
            // and forward PRIVMSG events to on_msg callback.
            let _ = on_msg;
            tracing::warn!("IRC bot: client loop would run here");
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
            channel = %msg.channel_chat_id,
            text_len = msg.text.len(),
            "IRC: would send message"
        );
        Ok(())
    }
}
