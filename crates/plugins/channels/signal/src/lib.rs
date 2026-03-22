/// Signal channel plugin for Clawkson.
/// Communicates with Signal via the signal-cli subprocess (JSON-RPC mode).
use std::collections::HashSet;
use std::sync::Arc;

use clawkson_plugin::{
    ChannelProvider, ClawksonPlugin, FrontendManifest, ConnectorCard,
    InboundMessage, OutboundMessage, PluginCapability, PluginContext, PluginManifest,
};
use serde_json::Value;
use tokio::sync::RwLock;

#[derive(Debug)]
pub struct SignalChannelPlugin {
    manifest: PluginManifest,
    /// Active signal-cli subprocess handle (if running).
    process_handle: RwLock<Option<tokio::task::JoinHandle<()>>>,
}

impl SignalChannelPlugin {
    pub fn new() -> Self {
        let mut caps = HashSet::new();
        caps.insert(PluginCapability::Channel);

        Self {
            manifest: PluginManifest {
                name: "signal-channel".to_string(),
                display_name: "Signal".to_string(),
                description: "Connect agents to Signal messenger via signal-cli.".to_string(),
                version: "0.1.0".to_string(),
                dependencies: vec![],
                capabilities: caps,
                frontend: Some(FrontendManifest {
                    sidebar_items: vec![],
                    routes: vec![],
                    settings_panels: vec![],
                    connector_cards: vec![ConnectorCard {
                        connector_type: "signal".to_string(),
                        component: "SignalConnectorCard".to_string(),
                        display_name: "Signal".to_string(),
                        icon: "shield".to_string(),
                    }],
                    bundle_url: None,
                }),
            },
            process_handle: RwLock::new(None),
        }
    }
}

#[async_trait::async_trait]
impl ClawksonPlugin for SignalChannelPlugin {
    fn manifest(&self) -> &PluginManifest {
        &self.manifest
    }

    async fn init(&self, _ctx: &PluginContext) -> anyhow::Result<()> {
        tracing::info!("Signal channel plugin initialized");
        Ok(())
    }

    async fn shutdown(&self) -> anyhow::Result<()> {
        if let Some(handle) = self.process_handle.write().await.take() {
            handle.abort();
        }
        tracing::info!("Signal channel plugin shut down");
        Ok(())
    }

    fn migrations(&self) -> Vec<&str> {
        vec![
            "CREATE TABLE IF NOT EXISTS signal_channels (
                id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
                connector_id UUID NOT NULL,
                phone_number TEXT NOT NULL,
                signal_cli_path TEXT NOT NULL DEFAULT 'signal-cli',
                agent_id UUID NOT NULL,
                created_at TIMESTAMPTZ NOT NULL DEFAULT now()
            )"
        ]
    }
}

#[async_trait::async_trait]
impl ChannelProvider for SignalChannelPlugin {
    fn channel_type(&self) -> &str {
        "signal"
    }

    fn config_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "phone_number": {
                    "type": "string",
                    "description": "Registered Signal phone number (e.g. +1234567890)"
                },
                "signal_cli_path": {
                    "type": "string",
                    "description": "Path to the signal-cli binary",
                    "default": "signal-cli"
                },
                "signal_cli_config_dir": {
                    "type": "string",
                    "description": "Path to signal-cli config directory (optional)"
                },
                "allowed_numbers": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "Phone numbers allowed to chat (empty = all)"
                },
                "agent_id": {
                    "type": "string",
                    "description": "Agent ID to route messages to"
                }
            },
            "required": ["phone_number", "agent_id"]
        })
    }

    async fn start(
        &self,
        config: Value,
        _on_message: Box<dyn Fn(InboundMessage) + Send + Sync>,
    ) -> anyhow::Result<()> {
        let phone = config
            .get("phone_number")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("phone_number required"))?;

        let on_msg = Arc::new(_on_message);
        let phone_owned = phone.to_string();

        let handle = tokio::spawn(async move {
            tracing::info!(
                phone = %phone_owned,
                "starting signal-cli JSON-RPC subprocess"
            );
            // In production: spawn signal-cli in JSON-RPC mode, read stdout
            // for incoming messages, parse JSON, and forward to on_msg callback.
            let _ = on_msg;
            tracing::warn!("Signal bot: signal-cli subprocess would run here");
            loop {
                tokio::time::sleep(std::time::Duration::from_secs(3600)).await;
            }
        });

        *self.process_handle.write().await = Some(handle);
        Ok(())
    }

    async fn stop(&self) -> anyhow::Result<()> {
        if let Some(handle) = self.process_handle.write().await.take() {
            handle.abort();
        }
        Ok(())
    }

    async fn send_message(&self, msg: OutboundMessage) -> anyhow::Result<()> {
        tracing::info!(
            recipient = %msg.channel_chat_id,
            text_len = msg.text.len(),
            "Signal: would send message via signal-cli"
        );
        Ok(())
    }
}
