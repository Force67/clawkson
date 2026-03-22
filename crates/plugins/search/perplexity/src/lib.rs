/// Perplexity API search plugin for Clawkson.
use std::collections::HashSet;

use clawkson_plugin::{
    ClawksonPlugin, PluginCapability, PluginContext, PluginManifest,
    SearchProvider, SearchResult,
};
use serde::Deserialize;

#[derive(Debug)]
pub struct PerplexitySearchPlugin {
    manifest: PluginManifest,
    client: reqwest::Client,
    api_key: tokio::sync::RwLock<Option<String>>,
}

#[derive(Debug, Deserialize)]
struct PerplexityResponse {
    #[serde(default)]
    choices: Vec<PerplexityChoice>,
}

#[derive(Debug, Deserialize)]
struct PerplexityChoice {
    message: PerplexityMessage,
}

#[derive(Debug, Deserialize)]
struct PerplexityMessage {
    #[serde(default)]
    content: String,
}

impl PerplexitySearchPlugin {
    pub fn new() -> Self {
        let mut caps = HashSet::new();
        caps.insert(PluginCapability::Search);

        Self {
            manifest: PluginManifest {
                name: "perplexity-search".to_string(),
                display_name: "Perplexity".to_string(),
                description: "AI-powered search via the Perplexity API.".to_string(),
                version: "0.1.0".to_string(),
                dependencies: vec![],
                capabilities: caps,
                frontend: None,
            },
            client: reqwest::Client::new(),
            api_key: tokio::sync::RwLock::new(None),
        }
    }
}

impl Default for PerplexitySearchPlugin {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl ClawksonPlugin for PerplexitySearchPlugin {
    fn manifest(&self) -> &PluginManifest {
        &self.manifest
    }

    async fn init(&self, _ctx: &PluginContext) -> anyhow::Result<()> {
        tracing::info!("Perplexity Search plugin initialized");
        Ok(())
    }

    async fn shutdown(&self) -> anyhow::Result<()> {
        tracing::info!("Perplexity Search plugin shut down");
        Ok(())
    }
}

#[async_trait::async_trait]
impl SearchProvider for PerplexitySearchPlugin {
    fn provider_name(&self) -> &str {
        "perplexity"
    }

    async fn search(
        &self,
        query: &str,
        max_results: usize,
    ) -> anyhow::Result<Vec<SearchResult>> {
        let api_key = self.api_key.read().await;
        let api_key = api_key
            .as_deref()
            .ok_or_else(|| anyhow::anyhow!("Perplexity API key not configured"))?;

        tracing::info!(query = %query, max_results, "Perplexity: executing query");

        let body = serde_json::json!({
            "model": "sonar",
            "messages": [
                {
                    "role": "user",
                    "content": format!("Search for: {query}")
                }
            ]
        });

        let resp = self
            .client
            .post("https://api.perplexity.ai/chat/completions")
            .bearer_auth(api_key)
            .json(&body)
            .send()
            .await?
            .error_for_status()?
            .json::<PerplexityResponse>()
            .await?;

        // Perplexity returns a single AI-synthesized answer rather than
        // a list of URLs. We wrap the response as a single SearchResult.
        let results = resp
            .choices
            .into_iter()
            .take(max_results)
            .map(|c| SearchResult {
                title: format!("Perplexity answer for: {query}"),
                url: String::new(),
                snippet: c.message.content,
            })
            .collect();

        Ok(results)
    }
}
