/// Cohere LLM provider plugin for Clawkson.
/// Implements Cohere's custom Chat API format (v2).
use std::collections::HashSet;

use clawkson_plugin::{
    ClawksonPlugin, LlmProviderFactory, PluginCapability, PluginContext, PluginManifest,
};
use denkwerk::LLMProvider;
use serde_json::Value;

#[derive(Debug)]
pub struct CoherePlugin {
    manifest: PluginManifest,
}

impl CoherePlugin {
    pub fn new() -> Self {
        let mut caps = HashSet::new();
        caps.insert(PluginCapability::LlmProvider);

        Self {
            manifest: PluginManifest {
                name: "cohere".to_string(),
                display_name: "Cohere".to_string(),
                description: "Cohere Chat API provider for Command R and Command R+ models.".to_string(),
                version: "0.1.0".to_string(),
                dependencies: vec![],
                capabilities: caps,
                frontend: None,
            },
        }
    }
}

impl Default for CoherePlugin {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl ClawksonPlugin for CoherePlugin {
    fn manifest(&self) -> &PluginManifest {
        &self.manifest
    }

    async fn init(&self, _ctx: &PluginContext) -> anyhow::Result<()> {
        tracing::info!("Cohere provider plugin initialized");
        Ok(())
    }

    async fn shutdown(&self) -> anyhow::Result<()> {
        tracing::info!("Cohere provider plugin shut down");
        Ok(())
    }
}

#[async_trait::async_trait]
impl LlmProviderFactory for CoherePlugin {
    fn provider_type(&self) -> &str {
        "cohere"
    }

    fn display_name(&self) -> &str {
        "Cohere"
    }

    fn default_base_url(&self) -> Option<&str> {
        Some("https://api.cohere.com/v2")
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
            .unwrap_or("command-r-plus");

        tracing::info!("Cohere provider build requested (stub)");

        anyhow::bail!(
            "Cohere provider build is not yet implemented. \
             Requires custom Chat API v2 client with streaming SSE support."
        )
    }
}
