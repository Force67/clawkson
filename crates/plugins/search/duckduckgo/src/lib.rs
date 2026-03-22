/// DuckDuckGo search plugin for Clawkson.
/// Uses the DuckDuckGo Instant Answer API (no API key required).
use std::collections::HashSet;

use clawkson_plugin::{
    ClawksonPlugin, PluginCapability, PluginContext, PluginManifest,
    SearchProvider, SearchResult,
};
use serde::Deserialize;

#[derive(Debug)]
pub struct DuckDuckGoSearchPlugin {
    manifest: PluginManifest,
    client: reqwest::Client,
}

#[derive(Debug, Deserialize)]
struct DdgResponse {
    #[serde(rename = "Abstract", default)]
    abstract_text: String,
    #[serde(rename = "AbstractURL", default)]
    abstract_url: String,
    #[serde(rename = "AbstractSource", default)]
    abstract_source: String,
    #[serde(rename = "RelatedTopics", default)]
    related_topics: Vec<DdgTopic>,
}

#[derive(Debug, Deserialize)]
struct DdgTopic {
    #[serde(rename = "Text", default)]
    text: String,
    #[serde(rename = "FirstURL", default)]
    first_url: String,
}

impl DuckDuckGoSearchPlugin {
    pub fn new() -> Self {
        let mut caps = HashSet::new();
        caps.insert(PluginCapability::Search);

        Self {
            manifest: PluginManifest {
                name: "duckduckgo-search".to_string(),
                display_name: "DuckDuckGo".to_string(),
                description: "Web search via the DuckDuckGo Instant Answer API.".to_string(),
                version: "0.1.0".to_string(),
                dependencies: vec![],
                capabilities: caps,
                frontend: None,
            },
            client: reqwest::Client::new(),
        }
    }
}

impl Default for DuckDuckGoSearchPlugin {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl ClawksonPlugin for DuckDuckGoSearchPlugin {
    fn manifest(&self) -> &PluginManifest {
        &self.manifest
    }

    async fn init(&self, _ctx: &PluginContext) -> anyhow::Result<()> {
        tracing::info!("DuckDuckGo Search plugin initialized");
        Ok(())
    }

    async fn shutdown(&self) -> anyhow::Result<()> {
        tracing::info!("DuckDuckGo Search plugin shut down");
        Ok(())
    }
}

#[async_trait::async_trait]
impl SearchProvider for DuckDuckGoSearchPlugin {
    fn provider_name(&self) -> &str {
        "duckduckgo"
    }

    async fn search(
        &self,
        query: &str,
        max_results: usize,
    ) -> anyhow::Result<Vec<SearchResult>> {
        tracing::info!(query = %query, max_results, "DuckDuckGo: executing query");

        let resp = self
            .client
            .get("https://api.duckduckgo.com/")
            .query(&[
                ("q", query),
                ("format", "json"),
                ("no_html", "1"),
                ("skip_disambig", "1"),
            ])
            .send()
            .await?
            .error_for_status()?
            .json::<DdgResponse>()
            .await?;

        let mut results = Vec::new();

        // Include the abstract if present
        if !resp.abstract_text.is_empty() {
            results.push(SearchResult {
                title: resp.abstract_source,
                url: resp.abstract_url,
                snippet: resp.abstract_text,
            });
        }

        // Include related topics
        for topic in resp.related_topics {
            if results.len() >= max_results {
                break;
            }
            if !topic.text.is_empty() {
                results.push(SearchResult {
                    title: topic
                        .text
                        .chars()
                        .take(80)
                        .collect::<String>(),
                    url: topic.first_url,
                    snippet: topic.text,
                });
            }
        }

        Ok(results)
    }
}
