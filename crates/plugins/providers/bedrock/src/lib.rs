/// AWS Bedrock Runtime LLM provider plugin for Clawkson.
use std::collections::HashSet;

use clawkson_plugin::{
    ClawksonPlugin, LlmProviderFactory, PluginCapability, PluginContext, PluginManifest,
};
use denkwerk::LLMProvider;
use serde_json::Value;

#[derive(Debug)]
pub struct BedrockPlugin {
    manifest: PluginManifest,
}

impl BedrockPlugin {
    pub fn new() -> Self {
        let mut caps = HashSet::new();
        caps.insert(PluginCapability::LlmProvider);

        Self {
            manifest: PluginManifest {
                name: "bedrock".to_string(),
                display_name: "AWS Bedrock".to_string(),
                description: "AWS Bedrock Runtime provider for Claude, Llama, Mistral, and other hosted models.".to_string(),
                version: "0.1.0".to_string(),
                dependencies: vec![],
                capabilities: caps,
                frontend: None,
            },
        }
    }
}

impl Default for BedrockPlugin {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl ClawksonPlugin for BedrockPlugin {
    fn manifest(&self) -> &PluginManifest {
        &self.manifest
    }

    async fn init(&self, _ctx: &PluginContext) -> anyhow::Result<()> {
        tracing::info!("AWS Bedrock provider plugin initialized");
        Ok(())
    }

    async fn shutdown(&self) -> anyhow::Result<()> {
        tracing::info!("AWS Bedrock provider plugin shut down");
        Ok(())
    }
}

#[async_trait::async_trait]
impl LlmProviderFactory for BedrockPlugin {
    fn provider_type(&self) -> &str {
        "bedrock"
    }

    fn display_name(&self) -> &str {
        "AWS Bedrock"
    }

    fn default_base_url(&self) -> Option<&str> {
        // Bedrock uses regional endpoints, not a single base URL.
        None
    }

    async fn build(
        &self,
        config: Value,
        _timeout: std::time::Duration,
    ) -> anyhow::Result<Box<dyn LLMProvider>> {
        let _region = config
            .get("region")
            .and_then(|v| v.as_str())
            .unwrap_or("us-east-1");
        let _model_id = config
            .get("model")
            .and_then(|v| v.as_str())
            .unwrap_or("anthropic.claude-sonnet-4-20250514-v1:0");

        tracing::info!("AWS Bedrock provider build requested (stub)");

        anyhow::bail!(
            "AWS Bedrock provider build is not yet implemented. \
             Requires aws-sdk-bedrockruntime InvokeModelWithResponseStream."
        )
    }
}
