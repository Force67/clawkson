/// Google Custom Search API plugin for Clawkson.
use std::collections::HashSet;

use clawkson_plugin::{
    ClawksonPlugin, PluginCapability, PluginContext, PluginManifest,
    SearchProvider, SearchResult,
};
use serde::Deserialize;

#[derive(Debug)]
pub struct GoogleSearchPlugin {
    manifest: PluginManifest,
    client: reqwest::Client,
    api_key: tokio::sync::RwLock<Option<String>>,
    cx: tokio::sync::RwLock<Option<String>>,
}

#[derive(Debug, Deserialize)]
struct GoogleResponse {
    #[serde(default)]
    items: Vec<GoogleItem>,
}

#[derive(Debug, Deserialize)]
struct GoogleItem {
    title: String,
    link: String,
    #[serde(default)]
    snippet: String,
}

impl GoogleSearchPlugin {
    pub fn new() -> Self {
        let mut caps = HashSet::new();
        caps.insert(PluginCapability::Search);

        Self {
            manifest: PluginManifest {
                name: "google-search".to_string(),
                display_name: "Google Search".to_string(),
                description: "Web search via Google Custom Search JSON API.".to_string(),
                version: "0.1.0".to_string(),
                dependencies: vec![],
                capabilities: caps,
                frontend: None,
            },
            client: reqwest::Client::new(),
            api_key: tokio::sync::RwLock::new(None),
            cx: tokio::sync::RwLock::new(None),
        }
    }
}

impl Default for GoogleSearchPlugin {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl ClawksonPlugin for GoogleSearchPlugin {
    fn manifest(&self) -> &PluginManifest {
        &self.manifest
    }

    async fn init(&self, _ctx: &PluginContext) -> anyhow::Result<()> {
        tracing::info!("Google Custom Search plugin initialized");
        Ok(())
    }

    async fn shutdown(&self) -> anyhow::Result<()> {
        tracing::info!("Google Custom Search plugin shut down");
        Ok(())
    }
}

#[async_trait::async_trait]
impl SearchProvider for GoogleSearchPlugin {
    fn provider_name(&self) -> &str {
        "google"
    }

    async fn search(
        &self,
        query: &str,
        max_results: usize,
    ) -> anyhow::Result<Vec<SearchResult>> {
        let api_key = self.api_key.read().await;
        let api_key = api_key
            .as_deref()
            .ok_or_else(|| anyhow::anyhow!("Google Search API key not configured"))?;

        let cx = self.cx.read().await;
        let cx = cx
            .as_deref()
            .ok_or_else(|| anyhow::anyhow!("Google Custom Search Engine ID (cx) not configured"))?;

        tracing::info!(query = %query, max_results, "Google Search: executing query");

        // Google CSE API allows max 10 results per request
        let num = max_results.min(10);

        let resp = self
            .client
            .get("https://www.googleapis.com/customsearch/v1")
            .query(&[
                ("key", api_key),
                ("cx", cx),
                ("q", query),
                ("num", &num.to_string()),
            ])
            .send()
            .await?
            .error_for_status()?
            .json::<GoogleResponse>()
            .await?;

        let results = resp
            .items
            .into_iter()
            .take(max_results)
            .map(|item| SearchResult {
                title: item.title,
                url: item.link,
                snippet: item.snippet,
            })
            .collect();

        Ok(results)
    }
}
