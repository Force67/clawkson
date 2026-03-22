/// Google Cloud Vertex AI LLM provider plugin for Clawkson.
use std::collections::HashSet;

use clawkson_plugin::{
    ClawksonPlugin, LlmProviderFactory, PluginCapability, PluginContext, PluginManifest,
};
use denkwerk::LLMProvider;
use serde_json::Value;

#[derive(Debug)]
pub struct VertexPlugin {
    manifest: PluginManifest,
}

impl VertexPlugin {
    pub fn new() -> Self {
        let mut caps = HashSet::new();
        caps.insert(PluginCapability::LlmProvider);

        Self {
            manifest: PluginManifest {
                name: "vertex".to_string(),
                display_name: "Google Vertex AI".to_string(),
                description: "Google Cloud Vertex AI provider for Gemini and PaLM models.".to_string(),
                version: "0.1.0".to_string(),
                dependencies: vec![],
                capabilities: caps,
                frontend: None,
            },
        }
    }
}

impl Default for VertexPlugin {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl ClawksonPlugin for VertexPlugin {
    fn manifest(&self) -> &PluginManifest {
        &self.manifest
    }

    async fn init(&self, _ctx: &PluginContext) -> anyhow::Result<()> {
        tracing::info!("Google Vertex AI provider plugin initialized");
        Ok(())
    }

    async fn shutdown(&self) -> anyhow::Result<()> {
        tracing::info!("Google Vertex AI provider plugin shut down");
        Ok(())
    }
}

#[async_trait::async_trait]
impl LlmProviderFactory for VertexPlugin {
    fn provider_type(&self) -> &str {
        "vertex"
    }

    fn display_name(&self) -> &str {
        "Google Vertex AI"
    }

    fn default_base_url(&self) -> Option<&str> {
        // Vertex uses project-specific endpoints:
        // https://{region}-aiplatform.googleapis.com/v1/projects/{project}/locations/{region}/publishers/google/models/{model}
        None
    }

    async fn build(
        &self,
        config: Value,
        _timeout: std::time::Duration,
    ) -> anyhow::Result<Box<dyn LLMProvider>> {
        let _project_id = config
            .get("project_id")
            .and_then(|v| v.as_str())
            .unwrap_or_default();
        let _region = config
            .get("region")
            .and_then(|v| v.as_str())
            .unwrap_or("us-central1");
        let _model = config
            .get("model")
            .and_then(|v| v.as_str())
            .unwrap_or("gemini-2.0-flash");

        tracing::info!("Google Vertex AI provider build requested (stub)");

        anyhow::bail!(
            "Google Vertex AI provider build is not yet implemented. \
             Requires OAuth2 service account auth and Vertex AI REST/gRPC client."
        )
    }
}
