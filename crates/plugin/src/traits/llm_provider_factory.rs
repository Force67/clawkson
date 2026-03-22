use denkwerk::LLMProvider;
use serde_json::Value;

/// Extension trait for plugins that provide LLM backends.
#[async_trait::async_trait]
pub trait LlmProviderFactory: Send + Sync {
    /// Return the provider type name (e.g. "anthropic", "bedrock", "groq").
    fn provider_type(&self) -> &str;

    /// Build an LLM provider instance from the given config.
    /// The config contains api_key, api_base_url, model, and provider-specific fields.
    async fn build(
        &self,
        config: Value,
        timeout: std::time::Duration,
    ) -> anyhow::Result<Box<dyn LLMProvider>>;

    /// Human-readable name for the settings UI.
    fn display_name(&self) -> &str;

    /// Default base URL for this provider (if any).
    fn default_base_url(&self) -> Option<&str> {
        None
    }
}
