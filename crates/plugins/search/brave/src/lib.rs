/// Brave Search API plugin for Clawkson.
use std::collections::HashSet;

use clawkson_plugin::{
    ClawksonPlugin, PluginCapability, PluginContext, PluginManifest,
    SearchProvider, SearchResult,
};
use serde::Deserialize;

#[derive(Debug)]
pub struct BraveSearchPlugin {
    manifest: PluginManifest,
    client: reqwest::Client,
    api_key: tokio::sync::RwLock<Option<String>>,
}

#[derive(Debug, Deserialize)]
struct BraveResponse {
    #[serde(default)]
    web: Option<BraveWebResults>,
}

#[derive(Debug, Deserialize)]
struct BraveWebResults {
    #[serde(default)]
    results: Vec<BraveWebResult>,
}

#[derive(Debug, Deserialize)]
struct BraveWebResult {
    title: String,
    url: String,
    #[serde(default)]
    description: String,
}

impl BraveSearchPlugin {
    pub fn new() -> Self {
        let mut caps = HashSet::new();
        caps.insert(PluginCapability::Search);

        Self {
            manifest: PluginManifest {
                name: "brave-search".to_string(),
                display_name: "Brave Search".to_string(),
                description: "Web search powered by the Brave Search API.".to_string(),
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

impl Default for BraveSearchPlugin {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl ClawksonPlugin for BraveSearchPlugin {
    fn manifest(&self) -> &PluginManifest {
        &self.manifest
    }

    async fn init(&self, _ctx: &PluginContext) -> anyhow::Result<()> {
        tracing::info!("Brave Search plugin initialized");
        Ok(())
    }

    async fn shutdown(&self) -> anyhow::Result<()> {
        tracing::info!("Brave Search plugin shut down");
        Ok(())
    }
}

#[async_trait::async_trait]
impl SearchProvider for BraveSearchPlugin {
    fn provider_name(&self) -> &str {
        "brave"
    }

    async fn search(
        &self,
        query: &str,
        max_results: usize,
    ) -> anyhow::Result<Vec<SearchResult>> {
        let api_key = self.api_key.read().await;
        let api_key = api_key
            .as_deref()
            .ok_or_else(|| anyhow::anyhow!("Brave Search API key not configured"))?;

        tracing::info!(query = %query, max_results, "Brave Search: executing query");

        let resp = self
            .client
            .get("https://api.search.brave.com/res/v1/web/search")
            .header("X-Subscription-Token", api_key)
            .query(&[("q", query), ("count", &max_results.to_string())])
            .send()
            .await?
            .error_for_status()?
            .json::<BraveResponse>()
            .await?;

        let results = resp
            .web
            .map(|w| {
                w.results
                    .into_iter()
                    .take(max_results)
                    .map(|r| SearchResult {
                        title: r.title,
                        url: r.url,
                        snippet: r.description,
                    })
                    .collect()
            })
            .unwrap_or_default();

        Ok(results)
    }
}
