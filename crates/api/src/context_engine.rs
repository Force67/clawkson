/// Context engine: 4-stage pipeline for smart context management.
///
/// 1. Ingest  — before saving user message
/// 2. Assemble — after loading history
/// 3. Compact — when tokens exceed budget
/// 4. AfterTurn — after assistant response
use clawkson_core::MessageRole;
use clawkson_plugin::{ContextEnginePlugin, ContextPipelineState};
use serde_json::json;
use std::sync::Arc;
use uuid::Uuid;

use crate::state::AppState;

/// Estimate token count for a history entry (rough: 1 token ≈ 4 chars).
fn estimate_message_tokens(content: &str, images: &[String]) -> usize {
    content.len() / 4 + images.iter().map(|img| img.len() / 4).sum::<usize>()
}

/// Estimate total tokens for a history.
pub fn estimate_history_tokens(history: &[(MessageRole, String, Vec<String>)]) -> usize {
    history
        .iter()
        .map(|(_, content, images)| estimate_message_tokens(content, images))
        .sum()
}

/// Run the Ingest stage: call all context engine plugins before saving the user message.
pub async fn run_ingest(
    plugins: &[Arc<dyn ContextEnginePlugin>],
    agent_id: Uuid,
    conversation_id: Uuid,
    user_id: Uuid,
    user_message: &str,
) -> anyhow::Result<()> {
    if plugins.is_empty() {
        return Ok(());
    }

    let mut state = ContextPipelineState {
        agent_id,
        conversation_id,
        user_id,
        history: vec![("user".to_string(), user_message.to_string(), vec![])],
        metadata: json!({}),
    };

    for plugin in plugins {
        if let Err(e) = plugin.on_ingest(&mut state).await {
            tracing::warn!(error = %e, "context engine ingest plugin failed");
        }
    }

    Ok(())
}

/// Run the Assemble stage: call all plugins after loading history to inject additional context.
pub async fn run_assemble(
    plugins: &[Arc<dyn ContextEnginePlugin>],
    agent_id: Uuid,
    conversation_id: Uuid,
    user_id: Uuid,
    history: &mut Vec<(MessageRole, String, Vec<String>)>,
) -> anyhow::Result<()> {
    if plugins.is_empty() {
        return Ok(());
    }

    let mut state = ContextPipelineState {
        agent_id,
        conversation_id,
        user_id,
        history: history
            .iter()
            .map(|(r, c, imgs)| (format!("{:?}", r).to_lowercase(), c.clone(), imgs.clone()))
            .collect(),
        metadata: json!({}),
    };

    for plugin in plugins {
        if let Err(e) = plugin.on_assemble(&mut state).await {
            tracing::warn!(error = %e, "context engine assemble plugin failed");
        }
    }

    Ok(())
}

/// Run the Compact stage: call all plugins when tokens exceed budget.
pub async fn run_compact(
    plugins: &[Arc<dyn ContextEnginePlugin>],
    agent_id: Uuid,
    conversation_id: Uuid,
    user_id: Uuid,
    history: &mut Vec<(MessageRole, String, Vec<String>)>,
) -> anyhow::Result<()> {
    if plugins.is_empty() {
        return Ok(());
    }

    let mut state = ContextPipelineState {
        agent_id,
        conversation_id,
        user_id,
        history: history
            .iter()
            .map(|(r, c, imgs)| (format!("{:?}", r).to_lowercase(), c.clone(), imgs.clone()))
            .collect(),
        metadata: json!({}),
    };

    for plugin in plugins {
        if let Err(e) = plugin.on_compact(&mut state).await {
            tracing::warn!(error = %e, "context engine compact plugin failed");
        }
    }

    Ok(())
}

/// Run the AfterTurn stage: call all plugins after the assistant response.
pub async fn run_after_turn(
    plugins: &[Arc<dyn ContextEnginePlugin>],
    agent_id: Uuid,
    conversation_id: Uuid,
    user_id: Uuid,
    assistant_response: &str,
) -> anyhow::Result<()> {
    if plugins.is_empty() {
        return Ok(());
    }

    let mut state = ContextPipelineState {
        agent_id,
        conversation_id,
        user_id,
        history: vec![],
        metadata: json!({}),
    };

    for plugin in plugins {
        if let Err(e) = plugin.after_turn(&mut state, assistant_response).await {
            tracing::warn!(error = %e, "context engine after_turn plugin failed");
        }
    }

    Ok(())
}

/// Smart context truncation: keep the most recent messages within token budget,
/// optionally prepending a summary of dropped messages.
pub fn truncate_with_summary(
    history: &[(MessageRole, String, Vec<String>)],
    max_tokens: usize,
    summary: Option<&str>,
) -> Vec<(MessageRole, String, Vec<String>)> {
    let total = estimate_history_tokens(history);
    if total <= max_tokens {
        return history.to_vec();
    }

    // Reserve tokens for summary if present
    let summary_tokens = summary.map(|s| s.len() / 4).unwrap_or(0);
    let available = max_tokens.saturating_sub(summary_tokens);

    // Walk from the end, accumulating tokens until we hit the budget
    let mut budget = available;
    let mut start_idx = history.len();
    for (i, (_, content, images)) in history.iter().enumerate().rev() {
        let msg_tokens = estimate_message_tokens(content, images);
        if msg_tokens > budget {
            break;
        }
        budget -= msg_tokens;
        start_idx = i;
    }

    // Always keep at least the last message
    if start_idx >= history.len() {
        start_idx = history.len().saturating_sub(1);
    }

    let mut result = Vec::new();

    // Prepend summary as a system message if we dropped messages
    if start_idx > 0 {
        if let Some(summary_text) = summary {
            result.push((
                MessageRole::System,
                format!("[Earlier conversation summary] {}", summary_text),
                vec![],
            ));
        }

        tracing::info!(
            total_messages = history.len(),
            kept = history.len() - start_idx,
            dropped = start_idx,
            estimated_total_tokens = total,
            "compacted conversation history"
        );
    }

    result.extend_from_slice(&history[start_idx..]);
    result
}
