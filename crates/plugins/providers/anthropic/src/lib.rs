/// Anthropic Claude direct API provider plugin for Clawkson.
use std::collections::HashSet;

use clawkson_plugin::{
    ClawksonPlugin, LlmProviderFactory, PluginCapability, PluginContext, PluginManifest,
};
use denkwerk::LLMProvider;
use serde_json::Value;

#[derive(Debug)]
pub struct AnthropicPlugin {
    manifest: PluginManifest,
}

impl AnthropicPlugin {
    pub fn new() -> Self {
        let mut caps = HashSet::new();
        caps.insert(PluginCapability::LlmProvider);

        Self {
            manifest: PluginManifest {
                name: "anthropic".to_string(),
                display_name: "Anthropic".to_string(),
                description: "Direct Anthropic Messages API provider for Claude models.".to_string(),
                version: "0.1.0".to_string(),
                dependencies: vec![],
                capabilities: caps,
                frontend: None,
            },
        }
    }
}

impl Default for AnthropicPlugin {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl ClawksonPlugin for AnthropicPlugin {
    fn manifest(&self) -> &PluginManifest {
        &self.manifest
    }

    async fn init(&self, _ctx: &PluginContext) -> anyhow::Result<()> {
        tracing::info!("Anthropic provider plugin initialized");
        Ok(())
    }

    async fn shutdown(&self) -> anyhow::Result<()> {
        tracing::info!("Anthropic provider plugin shut down");
        Ok(())
    }
}

#[async_trait::async_trait]
impl LlmProviderFactory for AnthropicPlugin {
    fn provider_type(&self) -> &str {
        "anthropic"
    }

    fn display_name(&self) -> &str {
        "Anthropic"
    }

    fn default_base_url(&self) -> Option<&str> {
        Some("https://api.anthropic.com/v1")
    }

    async fn build(
        &self,
        config: Value,
        _timeout: std::time::Duration,
    ) -> anyhow::Result<Box<dyn LLMProvider>> {
        let _api_key = config
            .get("api_key")
            .and_then(|v| v.as_str())
            .unwrap_or_default();
        let _model = config
            .get("model")
            .and_then(|v| v.as_str())
            .unwrap_or("claude-sonnet-4-20250514");

        tracing::info!("Anthropic provider build requested (stub)");

        anyhow::bail!(
            "Anthropic provider build is not yet implemented. \
             Requires Messages API client with streaming support."
        )
    }
}
