/// Embedding generation using denkwerk's OpenAI-compatible provider.
use anyhow::Result;
use denkwerk::{
    providers::openai::{OpenAI, OpenAIConfig},
    types::EmbeddingRequest,
    LLMProvider,
};

const DEFAULT_BASE_URL: &str = "http://localhost:11434/v1";
const DEFAULT_API_KEY: &str = "ollama";
const DEFAULT_EMBEDDING_MODEL: &str = "qwen3-embedding:8b";

/// Embedding provider configuration loaded from app settings.
#[derive(Debug, Clone)]
pub struct EmbeddingConfig {
    pub base_url: String,
    pub api_key: String,
    pub model: String,
}

impl Default for EmbeddingConfig {
    fn default() -> Self {
        Self {
            base_url: DEFAULT_BASE_URL.to_string(),
            api_key: DEFAULT_API_KEY.to_string(),
            model: DEFAULT_EMBEDDING_MODEL.to_string(),
        }
    }
}

fn build_provider(config: &EmbeddingConfig) -> Result<OpenAI> {
    let cfg = OpenAIConfig::new(&config.api_key)
        .with_base_url(config.base_url.clone());
    Ok(OpenAI::from_config(cfg)?)
}

/// Generate embeddings for one or more texts.
pub async fn generate(
    config: &EmbeddingConfig,
    model: &str,
    texts: Vec<String>,
) -> Result<Vec<Vec<f32>>> {
    let provider = build_provider(config)?;
    let model = if model.is_empty() { &config.model } else { model };
    let total_chars: usize = texts.iter().map(|t| t.len()).sum();

    tracing::info!(
        model = model,
        texts = texts.len(),
        total_chars = total_chars,
        base_url = %config.base_url,
        "Sending embedding request"
    );

    let start = std::time::Instant::now();
    let request = EmbeddingRequest::new(model.to_string(), texts.clone());
    let response = provider.create_embeddings(request).await.map_err(|e| {
        tracing::error!(
            model = model,
            texts = texts.len(),
            base_url = %config.base_url,
            error = %e,
            "Embedding request failed"
        );
        e
    })?;
    let elapsed = start.elapsed();

    let embeddings: Vec<Vec<f32>> = response
        .data
        .into_iter()
        .map(|e| e.embedding)
        .collect();

    tracing::info!(
        model = model,
        vectors = embeddings.len(),
        dim = embeddings.first().map(|v| v.len()).unwrap_or(0),
        elapsed_ms = elapsed.as_millis() as u64,
        "Embedding response received"
    );

    Ok(embeddings)
}

/// Generate a single embedding for a text.
pub async fn generate_one(config: &EmbeddingConfig, model: &str, text: &str) -> Result<Vec<f32>> {
    tracing::info!(model = model, text_len = text.len(), "Generating single embedding");
    let mut results = generate(config, model, vec![text.to_string()]).await?;
    results
        .pop()
        .ok_or_else(|| anyhow::anyhow!("No embedding returned"))
}
