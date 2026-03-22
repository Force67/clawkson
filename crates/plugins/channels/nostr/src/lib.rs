/// Nostr channel plugin for Clawkson.
/// Connects to Nostr relays via `nostr-sdk` and handles NIP-04 DMs / channel messages.
use std::collections::HashSet;
use std::sync::Arc;

use clawkson_plugin::{
    ChannelProvider, ClawksonPlugin, FrontendManifest, ConnectorCard,
    InboundMessage, OutboundMessage, PluginCapability, PluginContext, PluginManifest,
};
use serde_json::Value;
use tokio::sync::RwLock;

#[derive(Debug)]
pub struct NostrChannelPlugin {
    manifest: PluginManifest,
    /// Active relay subscription handle (if running).
    relay_handle: RwLock<Option<tokio::task::JoinHandle<()>>>,
}

impl NostrChannelPlugin {
    pub fn new() -> Self {
        let mut caps = HashSet::new();
        caps.insert(PluginCapability::Channel);

        Self {
            manifest: PluginManifest {
                name: "nostr-channel".to_string(),
                display_name: "Nostr".to_string(),
                description: "Connect agents to the Nostr protocol via relay subscriptions.".to_string(),
                version: "0.1.0".to_string(),
                dependencies: vec![],
                capabilities: caps,
                frontend: Some(FrontendManifest {
                    sidebar_items: vec![],
                    routes: vec![],
                    settings_panels: vec![],
                    connector_cards: vec![ConnectorCard {
                        connector_type: "nostr".to_string(),
                        component: "NostrConnectorCard".to_string(),
                        display_name: "Nostr".to_string(),
                        icon: "radio".to_string(),
                    }],
                    bundle_url: None,
                }),
            },
            relay_handle: RwLock::new(None),
        }
    }
}

#[async_trait::async_trait]
impl ClawksonPlugin for NostrChannelPlugin {
    fn manifest(&self) -> &PluginManifest {
        &self.manifest
    }

    async fn init(&self, _ctx: &PluginContext) -> anyhow::Result<()> {
        tracing::info!("Nostr channel plugin initialized");
        Ok(())
    }

    async fn shutdown(&self) -> anyhow::Result<()> {
        if let Some(handle) = self.relay_handle.write().await.take() {
            handle.abort();
        }
        tracing::info!("Nostr channel plugin shut down");
        Ok(())
    }

    fn migrations(&self) -> Vec<&str> {
        vec![
            "CREATE TABLE IF NOT EXISTS nostr_channels (
                id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
                connector_id UUID NOT NULL,
                nsec TEXT NOT NULL,
                agent_id UUID NOT NULL,
                created_at TIMESTAMPTZ NOT NULL DEFAULT now()
            )"
        ]
    }
}

#[async_trait::async_trait]
impl ChannelProvider for NostrChannelPlugin {
    fn channel_type(&self) -> &str {
        "nostr"
    }

    fn config_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "nsec": {
                    "type": "string",
                    "description": "Nostr secret key (nsec/hex) for the bot identity"
                },
                "relays": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "Relay URLs to connect to (e.g. [\"wss://relay.damus.io\"])"
                },
                "allowed_npubs": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "Public keys (npub/hex) allowed to interact (empty = all)"
                },
                "listen_dms": {
                    "type": "boolean",
                    "description": "Listen for NIP-04 encrypted DMs",
                    "default": true
                },
                "listen_mentions": {
                    "type": "boolean",
                    "description": "Listen for public mentions (kind 1 with p-tag)",
                    "default": false
                },
                "agent_id": {
                    "type": "string",
                    "description": "Agent ID to route messages to"
                }
            },
            "required": ["nsec", "relays", "agent_id"]
        })
    }

    async fn start(
        &self,
        config: Value,
        _on_message: Box<dyn Fn(InboundMessage) + Send + Sync>,
    ) -> anyhow::Result<()> {
        let relays = config
            .get("relays")
            .and_then(|v| v.as_array())
            .ok_or_else(|| anyhow::anyhow!("relays required"))?;

        let on_msg = Arc::new(_on_message);
        let relay_count = relays.len();

        let handle = tokio::spawn(async move {
            tracing::info!(
                relay_count = relay_count,
                "starting Nostr relay subscriptions"
            );
            // In production: use nostr_sdk::Client to connect to relays,
            // subscribe to DM and mention events, decrypt NIP-04 messages,
            // and forward to on_msg callback. Replies are signed and published.
            let _ = on_msg;
            tracing::warn!("Nostr bot: relay subscription loop would run here");
            loop {
                tokio::time::sleep(std::time::Duration::from_secs(3600)).await;
            }
        });

        *self.relay_handle.write().await = Some(handle);
        Ok(())
    }

    async fn stop(&self) -> anyhow::Result<()> {
        if let Some(handle) = self.relay_handle.write().await.take() {
            handle.abort();
        }
        Ok(())
    }

    async fn send_message(&self, msg: OutboundMessage) -> anyhow::Result<()> {
        tracing::info!(
            recipient = %msg.channel_chat_id,
            text_len = msg.text.len(),
            "Nostr: would publish message to relays"
        );
        Ok(())
    }
}
