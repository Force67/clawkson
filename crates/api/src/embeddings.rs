/// Embedding generation using denkwerk's OpenAI-compatible provider pointed at Ollama.
use anyhow::Result;
use denkwerk::{
    providers::openai::{OpenAI, OpenAIConfig},
    types::EmbeddingRequest,
    LLMProvider,
};

const OLLAMA_BASE_URL: &str = "http://localhost:11434/v1";
const DEFAULT_EMBEDDING_MODEL: &str = "qwen3-embedding:8b";

fn build_ollama_provider() -> Result<OpenAI> {
    // Ollama doesn't need a real API key, but denkwerk requires one
    let config = OpenAIConfig::new("ollama")
        .with_base_url(OLLAMA_BASE_URL.to_string());
    Ok(OpenAI::from_config(config)?)
}

/// Generate embeddings for one or more texts.
pub async fn generate(
    model: &str,
    texts: Vec<String>,
) -> Result<Vec<Vec<f32>>> {
    let provider = build_ollama_provider()?;
    let model = if model.is_empty() { DEFAULT_EMBEDDING_MODEL } else { model };
    let total_chars: usize = texts.iter().map(|t| t.len()).sum();

    tracing::info!(
        model = model,
        texts = texts.len(),
        total_chars = total_chars,
        ollama_url = OLLAMA_BASE_URL,
        "Sending embedding request to Ollama"
    );

    let start = std::time::Instant::now();
    let request = EmbeddingRequest::new(model.to_string(), texts.clone());
    let response = provider.create_embeddings(request).await.map_err(|e| {
        tracing::error!(
            model = model,
            texts = texts.len(),
            ollama_url = OLLAMA_BASE_URL,
            error = %e,
            "Ollama embedding request failed"
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
        "Ollama embedding response received"
    );

    Ok(embeddings)
}

/// Generate a single embedding for a text.
pub async fn generate_one(model: &str, text: &str) -> Result<Vec<f32>> {
    tracing::info!(model = model, text_len = text.len(), "Generating single embedding");
    let mut results = generate(model, vec![text.to_string()]).await?;
    results
        .pop()
        .ok_or_else(|| anyhow::anyhow!("No embedding returned"))
}
