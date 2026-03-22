/// Channel plugin template — copy this to create a new channel integration.
///
/// Replace TEMPLATE with your channel name throughout.
use std::collections::HashSet;
use std::sync::Arc;

use clawkson_plugin::{
    ChannelProvider, ClawksonPlugin, InboundMessage, OutboundMessage,
    PluginCapability, PluginContext, PluginManifest,
};
use serde_json::Value;

#[derive(Debug)]
pub struct TemplateChannelPlugin;

#[async_trait::async_trait]
impl ClawksonPlugin for TemplateChannelPlugin {
    fn manifest(&self) -> &PluginManifest {
        // Use a lazy static or const in real plugins
        todo!("return plugin manifest")
    }

    async fn init(&self, _ctx: &PluginContext) -> anyhow::Result<()> {
        tracing::info!("TEMPLATE channel plugin initialized");
        Ok(())
    }

    async fn shutdown(&self) -> anyhow::Result<()> {
        tracing::info!("TEMPLATE channel plugin shutting down");
        Ok(())
    }
}

#[async_trait::async_trait]
impl ChannelProvider for TemplateChannelPlugin {
    fn channel_type(&self) -> &str {
        "template"
    }

    fn config_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "api_key": { "type": "string", "description": "API key for the service" }
            },
            "required": ["api_key"]
        })
    }

    async fn start(
        &self,
        _config: Value,
        _on_message: Box<dyn Fn(InboundMessage) + Send + Sync>,
    ) -> anyhow::Result<()> {
        todo!("start listening for messages")
    }

    async fn stop(&self) -> anyhow::Result<()> {
        todo!("stop listening")
    }

    async fn send_message(&self, _msg: OutboundMessage) -> anyhow::Result<()> {
        todo!("send outbound message")
    }
}
