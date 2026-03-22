/// Auto-compaction: summarize older messages when context exceeds threshold.
use clawkson_core::{LlmConnector, MessageRole};

use crate::context_engine::estimate_history_tokens;

/// The fraction of the token budget at which compaction triggers.
const COMPACTION_THRESHOLD: f64 = 0.80;

/// Number of recent messages to always keep verbatim (not summarized).
const KEEP_VERBATIM: usize = 10;

/// Check if compaction is needed based on current token usage.
pub fn needs_compaction(history: &[(MessageRole, String, Vec<String>)], budget: usize) -> bool {
    let tokens = estimate_history_tokens(history);
    tokens as f64 > budget as f64 * COMPACTION_THRESHOLD
}

/// Generate a summary of older messages using a cheap LLM call.
pub async fn summarize_old_messages(
    connector: &LlmConnector,
    history: &[(MessageRole, String, Vec<String>)],
    keep_recent: usize,
    timeout_secs: u64,
) -> anyhow::Result<Option<String>> {
    if history.len() <= keep_recent {
        return Ok(None);
    }

    let to_summarize = &history[..history.len() - keep_recent];
    if to_summarize.is_empty() {
        return Ok(None);
    }

    // Build a condensed representation of messages to summarize
    let mut summary_input = String::new();
    for (role, content, _) in to_summarize {
        let role_str = match role {
            MessageRole::User => "User",
            MessageRole::Assistant => "Assistant",
            MessageRole::System => "System",
            MessageRole::Tool => "Tool",
        };
        // Truncate very long messages for the summary prompt
        let truncated = if content.len() > 500 {
            format!("{}...", &content[..500])
        } else {
            content.clone()
        };
        summary_input.push_str(&format!("{}: {}\n", role_str, truncated));
    }

    let system_prompt = "You are a conversation summarizer. Produce a concise summary of the \
        conversation below. Focus on key decisions, actions taken, and important context. \
        Keep the summary under 200 words.";

    let result = crate::llm::complete(
        connector,
        Some(system_prompt),
        &[(MessageRole::User, summary_input, vec![])],
        Some(0.3),
        Some(300),
        None,
        timeout_secs,
    )
    .await?;

    Ok(Some(result.text))
}

/// Perform auto-compaction on a history vector.
/// Returns the compacted history and an optional SSE event type.
pub async fn auto_compact(
    connector: &LlmConnector,
    history: &[(MessageRole, String, Vec<String>)],
    budget: usize,
    timeout_secs: u64,
) -> anyhow::Result<(Vec<(MessageRole, String, Vec<String>)>, bool)> {
    if !needs_compaction(history, budget) {
        return Ok((history.to_vec(), false));
    }

    tracing::info!(
        messages = history.len(),
        estimated_tokens = estimate_history_tokens(history),
        budget,
        "auto-compacting conversation history"
    );

    let summary = summarize_old_messages(connector, history, KEEP_VERBATIM, timeout_secs).await?;
    let compacted = crate::context_engine::truncate_with_summary(
        history,
        budget,
        summary.as_deref(),
    );

    Ok((compacted, true))
}
