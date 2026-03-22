/// Retry and failover logic for LLM requests.
///
/// - Exponential backoff on 429/5xx (up to 3 retries)
/// - Model failover: primary connector → subtask connector → error
use std::time::Duration;

use anyhow::Result;
use clawkson_core::{LlmConnector, MessageRole};
use tokio::time::sleep;

use crate::llm::CompletionResult;
use crate::routes::conversations::ReasoningEffort;

/// Maximum number of retries for transient errors.
const MAX_RETRIES: usize = 3;

/// Initial backoff duration.
const INITIAL_BACKOFF: Duration = Duration::from_millis(500);

/// Maximum backoff duration.
const MAX_BACKOFF: Duration = Duration::from_secs(30);

/// Check if an error is retryable (rate limit or server error).
fn is_retryable(err: &anyhow::Error) -> bool {
    let msg = err.to_string().to_lowercase();
    msg.contains("429")
        || msg.contains("rate limit")
        || msg.contains("too many requests")
        || msg.contains("500")
        || msg.contains("502")
        || msg.contains("503")
        || msg.contains("504")
        || msg.contains("internal server error")
        || msg.contains("bad gateway")
        || msg.contains("service unavailable")
        || msg.contains("gateway timeout")
        || msg.contains("timeout")
}

/// Perform a completion with exponential backoff retry.
pub async fn complete_with_retry(
    connector: &LlmConnector,
    system_prompt: Option<&str>,
    history: &[(MessageRole, String, Vec<String>)],
    temperature: Option<f64>,
    max_tokens: Option<u32>,
    reasoning_effort: Option<&ReasoningEffort>,
    timeout_secs: u64,
) -> Result<CompletionResult> {
    let mut last_error = None;
    let mut backoff = INITIAL_BACKOFF;

    for attempt in 0..=MAX_RETRIES {
        match crate::llm::complete(
            connector,
            system_prompt,
            history,
            temperature,
            max_tokens,
            reasoning_effort,
            timeout_secs,
        )
        .await
        {
            Ok(result) => return Ok(result),
            Err(e) => {
                if attempt < MAX_RETRIES && is_retryable(&e) {
                    tracing::warn!(
                        attempt = attempt + 1,
                        max_retries = MAX_RETRIES,
                        backoff_ms = backoff.as_millis(),
                        error = %e,
                        "retrying LLM request after transient error"
                    );
                    sleep(backoff).await;
                    backoff = std::cmp::min(backoff * 2, MAX_BACKOFF);
                    last_error = Some(e);
                } else {
                    return Err(e);
                }
            }
        }
    }

    Err(last_error.unwrap_or_else(|| anyhow::anyhow!("max retries exceeded")))
}

/// Perform a completion with failover to a secondary connector.
pub async fn complete_with_failover(
    primary: &LlmConnector,
    fallback: Option<&LlmConnector>,
    system_prompt: Option<&str>,
    history: &[(MessageRole, String, Vec<String>)],
    temperature: Option<f64>,
    max_tokens: Option<u32>,
    reasoning_effort: Option<&ReasoningEffort>,
    timeout_secs: u64,
) -> Result<CompletionResult> {
    match complete_with_retry(
        primary,
        system_prompt,
        history,
        temperature,
        max_tokens,
        reasoning_effort,
        timeout_secs,
    )
    .await
    {
        Ok(result) => Ok(result),
        Err(primary_err) => {
            if let Some(fallback_connector) = fallback {
                tracing::warn!(
                    primary = %primary.name,
                    fallback = %fallback_connector.name,
                    error = %primary_err,
                    "primary LLM failed, attempting failover"
                );
                complete_with_retry(
                    fallback_connector,
                    system_prompt,
                    history,
                    temperature,
                    max_tokens,
                    reasoning_effort,
                    timeout_secs,
                )
                .await
                .map_err(|fallback_err| {
                    anyhow::anyhow!(
                        "both primary and fallback failed. Primary: {}. Fallback: {}",
                        primary_err,
                        fallback_err
                    )
                })
            } else {
                Err(primary_err)
            }
        }
    }
}
