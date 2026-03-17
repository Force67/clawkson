/// LLM adapter backed by vendored denkwerk providers.
use anyhow::Result;
use clawkson_core::{LlmConnector, LlmProviderType, MessageRole};
use denkwerk::{
    providers::{
        azure_openai::{AzureOpenAI, AzureOpenAIConfig},
        openai::{OpenAI, OpenAIConfig},
        openrouter::{OpenRouter, OpenRouterConfig},
    },
    ChatMessage, CompletionRequest, FunctionRegistry, LLMProvider,
    MessageRole as DenkMessageRole, ReasoningEffort as DenkReasoningEffort, StreamEvent,
};
use futures::StreamExt;

use crate::routes::conversations::ReasoningEffort;

/// Strip tool-call metadata artifacts that some models leak into text content.
/// Some models (especially via Azure) emit tool calls as text rather than structured
/// tool_calls. This strips those artifacts so the user sees clean output.
fn sanitize_tool_artifacts(text: &str) -> String {
    let mut result = text.to_string();

    // Strip "to=functions.xxx" patterns and everything after
    if let Some(pos) = result.find("to=functions.") {
        result = result[..pos].to_string();
    }

    // Strip inline JSON blobs that look like tool call arguments
    // e.g. {"language":"python","code":"..."} or {"path":"/workspace/..."}
    // We look for JSON objects that contain known tool parameter keys
    let tool_param_patterns = [
        r#""language""#, r#""code""#, r#""path""#, r#""count""#, r#""entries""#,
        r#""timeout""#, r#""exit_code""#, r#""stderr""#, r#""stdout""#,
    ];

    // Walk through and remove JSON-like blocks containing tool params
    let mut cleaned = String::new();
    let mut i = 0;

    while i < result.len() {
        if result[i..].starts_with('{') {
            // Try to find matching closing brace
            if let Some(end) = find_matching_brace(&result[i..]) {
                let block = &result[i..i + end + 1];
                let is_tool_artifact = tool_param_patterns.iter().any(|p| block.contains(p));
                if is_tool_artifact {
                    // Skip this block
                    i += end + 1;
                    continue;
                }
            }
        }
        if let Some(ch) = result[i..].chars().next() {
            cleaned.push(ch);
            i += ch.len_utf8();
        } else {
            break;
        }
    }
    // Strip trailing non-ASCII garbled characters (common with some models)
    let trimmed = cleaned.trim_end();
    let clean_end = trimmed
        .char_indices()
        .rev()
        .find(|(_, c)| c.is_ascii() || *c == '\n')
        .map(|(i, c)| i + c.len_utf8())
        .unwrap_or(trimmed.len());
    trimmed[..clean_end].trim().to_string()
}

/// Find the index of the matching closing brace for a JSON-like block.
fn find_matching_brace(s: &str) -> Option<usize> {
    let mut depth = 0i32;
    let mut in_string = false;
    let mut escape_next = false;

    for (i, c) in s.char_indices() {
        if escape_next {
            escape_next = false;
            continue;
        }
        if c == '\\' && in_string {
            escape_next = true;
            continue;
        }
        if c == '"' {
            in_string = !in_string;
            continue;
        }
        if in_string {
            continue;
        }
        match c {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return Some(i);
                }
            }
            _ => {}
        }
    }
    None
}

fn resolve_base_url(connector: &LlmConnector) -> String {
    match &connector.provider_type {
        LlmProviderType::OpenRouter => "https://openrouter.ai/api/v1".to_string(),
        LlmProviderType::OpenAi => "https://api.openai.com/v1".to_string(),
        _ => connector.api_base_url.clone(),
    }
}

fn role_to_denkwerk(role: &MessageRole) -> DenkMessageRole {
    match role {
        MessageRole::User => DenkMessageRole::User,
        MessageRole::Assistant => DenkMessageRole::Assistant,
        MessageRole::System => DenkMessageRole::System,
        MessageRole::Tool => DenkMessageRole::Tool,
    }
}

fn map_reasoning_effort(effort: &ReasoningEffort) -> DenkReasoningEffort {
    match effort {
        ReasoningEffort::Low => DenkReasoningEffort::Low,
        ReasoningEffort::Medium => DenkReasoningEffort::Medium,
        ReasoningEffort::High => DenkReasoningEffort::High,
    }
}

fn build_request(
    connector: &LlmConnector,
    system_prompt: Option<&str>,
    history: &[(MessageRole, String, Vec<String>)],
    temperature: Option<f64>,
    max_tokens: Option<u32>,
    reasoning_effort: Option<&ReasoningEffort>,
) -> CompletionRequest {
    let mut messages = Vec::new();

    if let Some(system_prompt) = system_prompt {
        if !system_prompt.trim().is_empty() {
            messages.push(ChatMessage::system(system_prompt));
        }
    }

    for (role, content, images) in history {
        let msg = if !images.is_empty() && matches!(role, MessageRole::User) {
            ChatMessage::user_with_images(content.clone(), images.clone())
        } else {
            ChatMessage::new(role_to_denkwerk(role), content.clone())
        };
        messages.push(msg);
    }

    let mut request = CompletionRequest::new(connector.model.clone(), messages);
    if let Some(temperature) = temperature {
        request = request.with_temperature(temperature as f32);
    }
    if let Some(max_tokens) = max_tokens {
        request = request.with_max_tokens(max_tokens);
    }
    if let Some(effort) = reasoning_effort {
        request = request.with_reasoning_effort(map_reasoning_effort(effort));
    }

    request
}

fn build_provider(connector: &LlmConnector, timeout_secs: u64) -> Result<Box<dyn LLMProvider>> {
    let timeout = std::time::Duration::from_secs(timeout_secs);
    match connector.provider_type {
        LlmProviderType::Azure => {
            let mut config =
                AzureOpenAIConfig::new(connector.api_key.clone(), connector.api_base_url.clone());
            if let Some(version) = &connector.azure_api_version {
                config = config.with_api_version(version.clone());
            }
            config = config.with_timeout(timeout);
            Ok(Box::new(AzureOpenAI::from_config(config)?))
        }
        LlmProviderType::OpenRouter => {
            let mut config = OpenRouterConfig::new(connector.api_key.clone());
            config.base_url = resolve_base_url(connector);
            config.referer = Some("https://clawkson.app".to_string());
            config.title = Some("Clawkson".to_string());
            config.request_timeout = timeout;
            Ok(Box::new(OpenRouter::from_config(config)?))
        }
        LlmProviderType::OpenAi | LlmProviderType::Custom => {
            let config = OpenAIConfig::new(connector.api_key.clone())
                .with_base_url(resolve_base_url(connector))
                .with_timeout(timeout);
            Ok(Box::new(OpenAI::from_config(config)?))
        }
    }
}

/// Return whether the provider configured in `connector` supports image uploads.
pub fn provider_supports_vision(connector: &LlmConnector) -> bool {
    match build_provider(connector, 30) {
        Ok(provider) => provider.capabilities().supports_image_uploads,
        Err(_) => false,
    }
}

/// Perform a blocking (non-streaming) chat completion.
pub async fn complete(
    connector: &LlmConnector,
    system_prompt: Option<&str>,
    history: &[(MessageRole, String, Vec<String>)],
    temperature: Option<f64>,
    max_tokens: Option<u32>,
    reasoning_effort: Option<&ReasoningEffort>,
    timeout_secs: u64,
) -> Result<String> {
    let provider = build_provider(connector, timeout_secs)?;
    let request = build_request(connector, system_prompt, history, temperature, max_tokens, reasoning_effort);
    let response = provider.complete(request).await?;

    Ok(response.message.content.unwrap_or_default())
}

/// Perform a completion with tool-calling loop.
/// Runs up to `max_rounds` iterations: send to LLM, invoke tool calls, append results, repeat.
pub async fn complete_with_tools(
    connector: &LlmConnector,
    system_prompt: Option<&str>,
    history: &[(MessageRole, String, Vec<String>)],
    temperature: Option<f64>,
    max_tokens: Option<u32>,
    registry: &FunctionRegistry,
    max_rounds: usize,
    reasoning_effort: Option<&ReasoningEffort>,
    timeout_secs: u64,
) -> Result<String> {
    let provider = build_provider(connector, timeout_secs)?;

    // Build initial messages
    let mut messages = Vec::new();
    if let Some(sp) = system_prompt {
        if !sp.trim().is_empty() {
            messages.push(ChatMessage::system(sp));
        }
    }
    for (role, content, images) in history {
        let msg = if !images.is_empty() && matches!(role, MessageRole::User) {
            ChatMessage::user_with_images(content.clone(), images.clone())
        } else {
            ChatMessage::new(role_to_denkwerk(role), content.clone())
        };
        messages.push(msg);
    }

    for _round in 0..max_rounds {
        let mut request = CompletionRequest::new(connector.model.clone(), messages.clone());
        request = request.with_function_registry(registry);
        if let Some(temp) = temperature {
            request = request.with_temperature(temp as f32);
        }
        if let Some(mt) = max_tokens {
            request = request.with_max_tokens(mt);
        }
        if let Some(effort) = reasoning_effort {
            request = request.with_reasoning_effort(map_reasoning_effort(effort));
        }

        let response = provider.complete(request).await?;

        tracing::debug!(
            "LLM round {}: tool_calls={}, content_len={}",
            _round,
            response.message.tool_calls.len(),
            response.message.content.as_ref().map(|c| c.len()).unwrap_or(0),
        );

        if response.message.tool_calls.is_empty() {
            // No tool calls — return the text response
            return Ok(sanitize_tool_artifacts(&response.message.content.unwrap_or_default()));
        }

        // Add the assistant message with tool calls
        let assistant_msg = ChatMessage::assistant(
            response.message.content.clone().unwrap_or_default(),
        )
        .with_tool_calls(response.message.tool_calls.clone());
        messages.push(assistant_msg);

        // Invoke each tool call and add results
        for call in &response.message.tool_calls {
            let call_id = call
                .id
                .clone()
                .unwrap_or_else(|| format!("call_{}", uuid::Uuid::new_v4()));
            tracing::debug!(
                "Tool call: {} args={}",
                call.function.name,
                serde_json::to_string(&call.function.arguments).unwrap_or_default(),
            );
            let result = registry.invoke(&call.function).await;
            let result_str = match result {
                Ok(value) => {
                    let s = serde_json::to_string(&value).unwrap_or_default();
                    tracing::debug!("Tool result ({}): {}…", call.function.name, &s[..s.len().min(500)]);
                    s
                }
                Err(err) => {
                    let s = serde_json::json!({ "error": err.to_string() }).to_string();
                    tracing::warn!("Tool error ({}): {}", call.function.name, s);
                    s
                }
            };
            messages.push(ChatMessage::tool(&call_id, result_str));
        }
    }

    // If we exhausted rounds, do one final completion without tools
    let request = CompletionRequest::new(connector.model.clone(), messages);
    let response = provider.complete(request).await?;
    Ok(sanitize_tool_artifacts(&response.message.content.unwrap_or_default()))
}

// ── Tool-event helpers ──────────────────────────────────────────────

fn tool_description(name: &str, args: &serde_json::Value) -> String {
    match name {
        "code_execution" => {
            let lang = args.get("language").and_then(|v| v.as_str()).unwrap_or("code");
            format!("Running {lang}")
        }
        "workspace_read" => args
            .get("path")
            .and_then(|v| v.as_str())
            .unwrap_or("file")
            .to_string(),
        "workspace_write" => {
            let p = args.get("path").and_then(|v| v.as_str()).unwrap_or("file");
            format!("Writing {p}")
        }
        "workspace_list" => {
            let p = args
                .get("path")
                .and_then(|v| v.as_str())
                .unwrap_or("/workspace");
            format!("Listing {p}")
        }
        "knowledge_search" => {
            let q = args.get("query").and_then(|v| v.as_str()).unwrap_or("");
            if q.is_empty() {
                "Searching".into()
            } else {
                let end = q.char_indices().nth(50).map(|(i, _)| i).unwrap_or(q.len());
                format!("\"{}\"", &q[..end])
            }
        }
        "delegate_tasks" => {
            let count = args.get("tasks").and_then(|v| v.as_array()).map(|a| a.len()).unwrap_or(0);
            format!("Delegating {count} sub-task{}", if count != 1 { "s" } else { "" })
        }
        other => other.replace('_', " "),
    }
}

fn tool_result_summary(name: &str, result_str: &str, ok: bool) -> String {
    if !ok {
        return result_str.chars().take(100).collect();
    }
    if let Ok(v) = serde_json::from_str::<serde_json::Value>(result_str) {
        match name {
            "code_execution" => {
                let code = v.get("exit_code").and_then(|v| v.as_i64()).unwrap_or(-1);
                let out = v.get("stdout").and_then(|v| v.as_str()).unwrap_or("");
                let line = out.lines().next().unwrap_or("").trim();
                if code == 0 {
                    if line.is_empty() {
                        "Completed".into()
                    } else {
                        line.chars().take(80).collect()
                    }
                } else {
                    let err = v.get("stderr").and_then(|v| v.as_str()).unwrap_or("");
                    let eline = err.lines().last().unwrap_or("").trim();
                    format!(
                        "exit {code}: {}",
                        eline.chars().take(60).collect::<String>()
                    )
                }
            }
            "workspace_list" => v
                .get("entries")
                .and_then(|v| v.as_array())
                .map(|a| format!("{} items", a.len()))
                .unwrap_or_else(|| "done".into()),
            "knowledge_search" => v
                .as_array()
                .map(|a| format!("{} results", a.len()))
                .unwrap_or_else(|| "done".into()),
            // Pass through full result for start_preview so the frontend gets the URL
            "start_preview" => result_str.to_string(),
            "delegate_tasks" => {
                let completed = v.get("completed").and_then(|v| v.as_u64()).unwrap_or(0);
                let total = v.get("total").and_then(|v| v.as_u64()).unwrap_or(0);
                let all_ok = v.get("all_succeeded").and_then(|v| v.as_bool()).unwrap_or(false);
                if all_ok {
                    format!("{completed}/{total} sub-tasks completed")
                } else {
                    format!("{completed}/{total} sub-tasks (some failed)")
                }
            }
            _ => "done".into(),
        }
    } else {
        result_str.chars().take(80).collect()
    }
}

/// Like [`complete_with_tools`] but emits tool-call events through a channel.
///
/// Channel messages prefixed with `\x02` are tool-event JSON.
/// Regular (unprefixed) messages are content deltas.
pub async fn complete_with_tools_streaming(
    connector: &LlmConnector,
    system_prompt: Option<&str>,
    history: &[(MessageRole, String, Vec<String>)],
    temperature: Option<f64>,
    max_tokens: Option<u32>,
    registry: &FunctionRegistry,
    max_rounds: usize,
    reasoning_effort: Option<&ReasoningEffort>,
    timeout_secs: u64,
    tx: &tokio::sync::mpsc::Sender<String>,
) -> Result<String> {
    let provider = build_provider(connector, timeout_secs)?;

    let mut messages = Vec::new();
    if let Some(sp) = system_prompt {
        if !sp.trim().is_empty() {
            messages.push(ChatMessage::system(sp));
        }
    }
    for (role, content, images) in history {
        let msg = if !images.is_empty() && matches!(role, MessageRole::User) {
            ChatMessage::user_with_images(content.clone(), images.clone())
        } else {
            ChatMessage::new(role_to_denkwerk(role), content.clone())
        };
        messages.push(msg);
    }

    for round in 0..max_rounds {
        let mut request = CompletionRequest::new(connector.model.clone(), messages.clone());
        request = request.with_function_registry(registry);
        if let Some(temp) = temperature {
            request = request.with_temperature(temp as f32);
        }
        if let Some(mt) = max_tokens {
            request = request.with_max_tokens(mt);
        }
        if let Some(effort) = reasoning_effort {
            request = request.with_reasoning_effort(map_reasoning_effort(effort));
        }

        let response = provider.complete(request).await?;

        tracing::debug!(
            "LLM round {}: tool_calls={}, content_len={}",
            round,
            response.message.tool_calls.len(),
            response.message.content.as_ref().map(|c| c.len()).unwrap_or(0),
        );

        if response.message.tool_calls.is_empty() {
            let text = sanitize_tool_artifacts(&response.message.content.unwrap_or_default());
            // Stream line-by-line for a flowing appearance
            for line in text.split_inclusive('\n') {
                let _ = tx.try_send(line.to_string());
            }
            // If text doesn't end with newline, the last chunk was already sent
            return Ok(text);
        }

        // Add the assistant message with tool calls
        let assistant_msg = ChatMessage::assistant(
            response.message.content.clone().unwrap_or_default(),
        )
        .with_tool_calls(response.message.tool_calls.clone());
        messages.push(assistant_msg);

        // Invoke each tool call and emit events
        for call in &response.message.tool_calls {
            let call_id = call
                .id
                .clone()
                .unwrap_or_else(|| format!("call_{}", uuid::Uuid::new_v4()));

            let desc = tool_description(&call.function.name, &call.function.arguments);

            // Emit tool_start
            let start_evt = serde_json::json!({
                "type": "tool_start",
                "name": call.function.name,
                "round": round + 1,
                "description": desc,
            });
            let _ = tx.try_send(format!("\x02{start_evt}"));

            tracing::debug!(
                "Tool call: {} args={}",
                call.function.name,
                serde_json::to_string(&call.function.arguments).unwrap_or_default(),
            );

            let start = std::time::Instant::now();
            let result = registry.invoke(&call.function).await;
            let duration_ms = start.elapsed().as_millis() as u64;

            let (result_str, ok) = match result {
                Ok(value) => {
                    let s = serde_json::to_string(&value).unwrap_or_default();
                    tracing::debug!(
                        "Tool result ({}): {}…",
                        call.function.name,
                        &s[..s.len().min(500)]
                    );
                    (s, true)
                }
                Err(err) => {
                    let s = serde_json::json!({ "error": err.to_string() }).to_string();
                    tracing::warn!("Tool error ({}): {}", call.function.name, s);
                    (s, false)
                }
            };

            // Emit tool_end
            let summary = tool_result_summary(&call.function.name, &result_str, ok);
            let end_evt = serde_json::json!({
                "type": "tool_end",
                "name": call.function.name,
                "round": round + 1,
                "ok": ok,
                "result": summary,
                "duration_ms": duration_ms,
            });
            let _ = tx.try_send(format!("\x02{end_evt}"));

            messages.push(ChatMessage::tool(&call_id, result_str));
        }
    }

    // Exhausted rounds — final completion without tools
    let request = CompletionRequest::new(connector.model.clone(), messages);
    let response = provider.complete(request).await?;
    let text = sanitize_tool_artifacts(&response.message.content.unwrap_or_default());
    for line in text.split_inclusive('\n') {
        let _ = tx.try_send(line.to_string());
    }
    Ok(text)
}

/// Stream a chat completion, yielding text deltas via a callback.
/// When reasoning is enabled, reasoning tokens are forwarded via `on_reasoning`.
pub async fn stream_complete(
    connector: &LlmConnector,
    system_prompt: Option<&str>,
    history: &[(MessageRole, String, Vec<String>)],
    temperature: Option<f64>,
    max_tokens: Option<u32>,
    reasoning_effort: Option<&ReasoningEffort>,
    timeout_secs: u64,
    mut on_chunk: impl FnMut(String),
    mut on_reasoning: impl FnMut(String),
) -> Result<String> {
    let provider = build_provider(connector, timeout_secs)?;
    let request = build_request(connector, system_prompt, history, temperature, max_tokens, reasoning_effort);
    let mut stream = provider.stream_completion(request).await?;
    let mut full_text = String::new();
    let mut completed_text: Option<String> = None;

    while let Some(event) = stream.next().await {
        match event? {
            StreamEvent::MessageDelta(text) => {
                full_text.push_str(&text);
                on_chunk(text);
            }
            StreamEvent::ReasoningDelta(text) => {
                on_reasoning(text);
            }
            StreamEvent::Completed(response) => {
                completed_text = response.message.content;
            }
            StreamEvent::ToolCallDelta { .. } => {}
        }
    }

    if full_text.is_empty() {
        Ok(completed_text.unwrap_or_default())
    } else {
        Ok(full_text)
    }
}
