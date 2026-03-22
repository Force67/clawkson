/// Generic OpenAI-compatible LLM provider plugin for Clawkson.
/// Covers ~80% of providers (Groq, Together, Fireworks, DeepSeek, Mistral)
/// with just config (base_url + headers).
use std::collections::HashSet;

use clawkson_plugin::{
    ClawksonPlugin, LlmProviderFactory, PluginCapability, PluginContext, PluginManifest,
};
use denkwerk::LLMProvider;
use serde_json::Value;

#[derive(Debug)]
pub struct OpenAiCompatPlugin {
    manifest: PluginManifest,
}

impl OpenAiCompatPlugin {
    pub fn new() -> Self {
        let mut caps = HashSet::new();
        caps.insert(PluginCapability::LlmProvider);

        Self {
            manifest: PluginManifest {
                name: "openai-compat".to_string(),
                display_name: "OpenAI Compatible".to_string(),
                description: "Generic OpenAI-compatible provider supporting Groq, Together, Fireworks, DeepSeek, Mistral, and others.".to_string(),
                version: "0.1.0".to_string(),
                dependencies: vec![],
                capabilities: caps,
                frontend: None,
            },
        }
    }
}

impl Default for OpenAiCompatPlugin {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl ClawksonPlugin for OpenAiCompatPlugin {
    fn manifest(&self) -> &PluginManifest {
        &self.manifest
    }

    async fn init(&self, _ctx: &PluginContext) -> anyhow::Result<()> {
        tracing::info!("OpenAI-compatible provider plugin initialized");
        Ok(())
    }

    async fn shutdown(&self) -> anyhow::Result<()> {
        tracing::info!("OpenAI-compatible provider plugin shut down");
        Ok(())
    }
}

#[async_trait::async_trait]
impl LlmProviderFactory for OpenAiCompatPlugin {
    fn provider_type(&self) -> &str {
        "openai_compat"
    }

    fn display_name(&self) -> &str {
        "OpenAI Compatible"
    }

    fn default_base_url(&self) -> Option<&str> {
        // No single default — each sub-provider (Groq, Together, etc.) has its own.
        None
    }

    async fn build(
        &self,
        config: Value,
        _timeout: std::time::Duration,
    ) -> anyhow::Result<Box<dyn LLMProvider>> {
        let base_url = config
            .get("api_base_url")
            .and_then(|v| v.as_str())
            .unwrap_or("https://api.openai.com/v1");
        let _api_key = config
            .get("api_key")
            .and_then(|v| v.as_str())
            .unwrap_or_default();

        tracing::info!(
            base_url = %base_url,
            "OpenAI-compat provider build requested (stub)"
        );

        anyhow::bail!(
            "OpenAI-compatible provider build is not yet implemented. \
             Configure base_url={} with the appropriate API key.",
            base_url
        )
    }
}
