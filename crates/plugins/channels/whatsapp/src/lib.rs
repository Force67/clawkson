/// WhatsApp channel plugin for Clawkson.
/// Communicates via a Baileys-based Node.js sidecar process over IPC.
use std::collections::HashSet;
use std::sync::Arc;

use clawkson_plugin::{
    ChannelProvider, ClawksonPlugin, FrontendManifest, ConnectorCard,
    InboundMessage, OutboundMessage, PluginCapability, PluginContext, PluginManifest,
};
use serde_json::Value;
use tokio::sync::RwLock;

#[derive(Debug)]
pub struct WhatsAppChannelPlugin {
    manifest: PluginManifest,
    /// Active Baileys sidecar process handle (if running).
    sidecar_handle: RwLock<Option<tokio::task::JoinHandle<()>>>,
}

impl WhatsAppChannelPlugin {
    pub fn new() -> Self {
        let mut caps = HashSet::new();
        caps.insert(PluginCapability::Channel);

        Self {
            manifest: PluginManifest {
                name: "whatsapp-channel".to_string(),
                display_name: "WhatsApp".to_string(),
                description: "Connect agents to WhatsApp via a Baileys Node.js sidecar.".to_string(),
                version: "0.1.0".to_string(),
                dependencies: vec![],
                capabilities: caps,
                frontend: Some(FrontendManifest {
                    sidebar_items: vec![],
                    routes: vec![],
                    settings_panels: vec![],
                    connector_cards: vec![ConnectorCard {
                        connector_type: "whatsapp".to_string(),
                        component: "WhatsAppConnectorCard".to_string(),
                        display_name: "WhatsApp".to_string(),
                        icon: "phone".to_string(),
                    }],
                    bundle_url: None,
                }),
            },
            sidecar_handle: RwLock::new(None),
        }
    }
}

#[async_trait::async_trait]
impl ClawksonPlugin for WhatsAppChannelPlugin {
    fn manifest(&self) -> &PluginManifest {
        &self.manifest
    }

    async fn init(&self, _ctx: &PluginContext) -> anyhow::Result<()> {
        tracing::info!("WhatsApp channel plugin initialized");
        Ok(())
    }

    async fn shutdown(&self) -> anyhow::Result<()> {
        if let Some(handle) = self.sidecar_handle.write().await.take() {
            handle.abort();
        }
        tracing::info!("WhatsApp channel plugin shut down");
        Ok(())
    }

    fn migrations(&self) -> Vec<&str> {
        vec![
            "CREATE TABLE IF NOT EXISTS whatsapp_channels (
                id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
                connector_id UUID NOT NULL,
                phone_number TEXT NOT NULL,
                session_data_path TEXT,
                agent_id UUID NOT NULL,
                created_at TIMESTAMPTZ NOT NULL DEFAULT now()
            )"
        ]
    }
}

#[async_trait::async_trait]
impl ChannelProvider for WhatsAppChannelPlugin {
    fn channel_type(&self) -> &str {
        "whatsapp"
    }

    fn config_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "phone_number": {
                    "type": "string",
                    "description": "WhatsApp phone number (e.g. +1234567890)"
                },
                "sidecar_path": {
                    "type": "string",
                    "description": "Path to the Baileys Node.js sidecar script",
                    "default": "./sidecar/whatsapp/index.js"
                },
                "session_data_path": {
                    "type": "string",
                    "description": "Path to persist WhatsApp session data"
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
                "starting Baileys WhatsApp sidecar"
            );
            // In production: spawn the Node.js Baileys sidecar, communicate
            // via stdin/stdout JSON IPC, forward incoming messages to on_msg.
            let _ = on_msg;
            tracing::warn!("WhatsApp bot: Baileys sidecar would run here");
            loop {
                tokio::time::sleep(std::time::Duration::from_secs(3600)).await;
            }
        });

        *self.sidecar_handle.write().await = Some(handle);
        Ok(())
    }

    async fn stop(&self) -> anyhow::Result<()> {
        if let Some(handle) = self.sidecar_handle.write().await.take() {
            handle.abort();
        }
        Ok(())
    }

    async fn send_message(&self, msg: OutboundMessage) -> anyhow::Result<()> {
        tracing::info!(
            chat = %msg.channel_chat_id,
            text_len = msg.text.len(),
            "WhatsApp: would send message via Baileys sidecar"
        );
        Ok(())
    }
}
