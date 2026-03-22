use serde::{Deserialize, Serialize};

/// A single search result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResult {
    /// Title of the result.
    pub title: String,
    /// URL of the result.
    pub url: String,
    /// Snippet / summary text.
    pub snippet: String,
}

/// Extension trait for plugins that provide web search backends.
#[async_trait::async_trait]
pub trait SearchProvider: Send + Sync {
    /// Return the search provider name (e.g. "brave", "perplexity").
    fn provider_name(&self) -> &str;

    /// Perform a search and return up to `max_results` results.
    async fn search(
        &self,
        query: &str,
        max_results: usize,
    ) -> anyhow::Result<Vec<SearchResult>>;
}
