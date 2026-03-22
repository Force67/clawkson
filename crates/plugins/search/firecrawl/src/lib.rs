/// Firecrawl web scraping and search plugin for Clawkson.
use std::collections::HashSet;

use clawkson_plugin::{
    ClawksonPlugin, PluginCapability, PluginContext, PluginManifest,
    SearchProvider, SearchResult,
};
use serde::Deserialize;

#[derive(Debug)]
pub struct FirecrawlSearchPlugin {
    manifest: PluginManifest,
    client: reqwest::Client,
    api_key: tokio::sync::RwLock<Option<String>>,
}

#[derive(Debug, Deserialize)]
struct FirecrawlResponse {
    #[serde(default)]
    data: Vec<FirecrawlResult>,
}

#[derive(Debug, Deserialize)]
struct FirecrawlResult {
    #[serde(default)]
    url: String,
    #[serde(default)]
    title: Option<String>,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    markdown: Option<String>,
}

impl FirecrawlSearchPlugin {
    pub fn new() -> Self {
        let mut caps = HashSet::new();
        caps.insert(PluginCapability::Search);

        Self {
            manifest: PluginManifest {
                name: "firecrawl-search".to_string(),
                display_name: "Firecrawl".to_string(),
                description: "Web scraping and search via the Firecrawl API.".to_string(),
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

impl Default for FirecrawlSearchPlugin {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl ClawksonPlugin for FirecrawlSearchPlugin {
    fn manifest(&self) -> &PluginManifest {
        &self.manifest
    }

    async fn init(&self, _ctx: &PluginContext) -> anyhow::Result<()> {
        tracing::info!("Firecrawl Search plugin initialized");
        Ok(())
    }

    async fn shutdown(&self) -> anyhow::Result<()> {
        tracing::info!("Firecrawl Search plugin shut down");
        Ok(())
    }
}

#[async_trait::async_trait]
impl SearchProvider for FirecrawlSearchPlugin {
    fn provider_name(&self) -> &str {
        "firecrawl"
    }

    async fn search(
        &self,
        query: &str,
        max_results: usize,
    ) -> anyhow::Result<Vec<SearchResult>> {
        let api_key = self.api_key.read().await;
        let api_key = api_key
            .as_deref()
            .ok_or_else(|| anyhow::anyhow!("Firecrawl API key not configured"))?;

        tracing::info!(query = %query, max_results, "Firecrawl: executing search");

        let body = serde_json::json!({
            "query": query,
            "limit": max_results,
            "scrapeOptions": {
                "formats": ["markdown"]
            }
        });

        let resp = self
            .client
            .post("https://api.firecrawl.dev/v1/search")
            .bearer_auth(api_key)
            .json(&body)
            .send()
            .await?
            .error_for_status()?
            .json::<FirecrawlResponse>()
            .await?;

        let results = resp
            .data
            .into_iter()
            .take(max_results)
            .map(|r| SearchResult {
                title: r.title.unwrap_or_default(),
                url: r.url,
                snippet: r
                    .description
                    .or(r.markdown.map(|m| {
                        // Truncate markdown content to a reasonable snippet length
                        if m.len() > 500 {
                            format!("{}...", &m[..500])
                        } else {
                            m
                        }
                    }))
                    .unwrap_or_default(),
            })
            .collect();

        Ok(results)
    }
}
