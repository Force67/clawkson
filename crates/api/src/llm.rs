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
    TokenUsage,
};
use futures::StreamExt;

use crate::routes::conversations::ReasoningEffort;

/// Result of an LLM completion, carrying both the text and optional token usage.
pub struct CompletionResult {
    pub text: String,
    pub usage: Option<TokenUsage>,
}

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
) -> Result<CompletionResult> {
    let provider = build_provider(connector, timeout_secs)?;
    let request = build_request(connector, system_prompt, history, temperature, max_tokens, reasoning_effort);
    let response = provider.complete(request).await?;

    Ok(CompletionResult {
        text: response.message.content.unwrap_or_default(),
        usage: response.usage,
    })
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
) -> Result<CompletionResult> {
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

    // Accumulate usage across all rounds
    let mut total_usage = TokenUsage { prompt_tokens: 0, completion_tokens: 0, total_tokens: 0 };

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

        if let Some(u) = &response.usage {
            total_usage.prompt_tokens += u.prompt_tokens;
            total_usage.completion_tokens += u.completion_tokens;
            total_usage.total_tokens += u.total_tokens;
        }

        tracing::debug!(
            "LLM round {}: tool_calls={}, content_len={}",
            _round,
            response.message.tool_calls.len(),
            response.message.content.as_ref().map(|c| c.len()).unwrap_or(0),
        );

        if response.message.tool_calls.is_empty() {
            // No tool calls — return the text response
            return Ok(CompletionResult {
                text: sanitize_tool_artifacts(&response.message.content.unwrap_or_default()),
                usage: Some(total_usage),
            });
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
    if let Some(u) = &response.usage {
        total_usage.prompt_tokens += u.prompt_tokens;
        total_usage.completion_tokens += u.completion_tokens;
        total_usage.total_tokens += u.total_tokens;
    }
    Ok(CompletionResult {
        text: sanitize_tool_artifacts(&response.message.content.unwrap_or_default()),
        usage: Some(total_usage),
    })
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
        "browser" => {
            let action = args.get("action").and_then(|v| v.as_str()).unwrap_or("browse");
            match action {
                "navigate" => {
                    let url = args.get("url").and_then(|v| v.as_str()).unwrap_or("");
                    let short = url.strip_prefix("https://").or_else(|| url.strip_prefix("http://")).unwrap_or(url);
                    let end = short.char_indices().nth(40).map(|(i, _)| i).unwrap_or(short.len());
                    format!("Navigating to {}", &short[..end])
                }
                "click" => {
                    let sel = args.get("selector").and_then(|v| v.as_str()).unwrap_or("element");
                    format!("Clicking {sel}")
                }
                "type" => "Typing text".into(),
                "screenshot" => "Taking screenshot".into(),
                "evaluate" => "Running JavaScript".into(),
                "scroll" => {
                    let dir = args.get("direction").and_then(|v| v.as_str()).unwrap_or("down");
                    format!("Scrolling {dir}")
                }
                "back" => "Going back".into(),
                _ => format!("Browser: {action}"),
            }
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
            "browser" => {
                let title = v.get("title").and_then(|v| v.as_str()).unwrap_or("");
                let url = v.get("url").and_then(|v| v.as_str()).unwrap_or("");
                if !title.is_empty() {
                    format!("{} — {}", title.chars().take(40).collect::<String>(), url.chars().take(40).collect::<String>())
                } else if !url.is_empty() {
                    url.chars().take(60).collect()
                } else {
                    "done".into()
                }
            }
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
    workspace_path: Option<std::path::PathBuf>,
    cancel: tokio_util::sync::CancellationToken,
) -> Result<CompletionResult> {
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

    let mut total_usage = TokenUsage { prompt_tokens: 0, completion_tokens: 0, total_tokens: 0 };

    for round in 0..max_rounds {
        // Check cancellation between rounds
        if cancel.is_cancelled() {
            tracing::info!("tool loop cancelled by client at round {round}");
            return Ok(CompletionResult { text: "[Response stopped by user]".to_string(), usage: Some(total_usage) });
        }

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

        // Race the LLM call against cancellation so we don't wait up to
        // timeout_secs for a response the user no longer wants.
        let response = tokio::select! {
            res = provider.complete(request) => res?,
            _ = cancel.cancelled() => {
                tracing::info!("LLM call cancelled by client during round {round}");
                return Ok(CompletionResult { text: "[Response stopped by user]".to_string(), usage: Some(total_usage) });
            }
        };

        if let Some(u) = &response.usage {
            total_usage.prompt_tokens += u.prompt_tokens;
            total_usage.completion_tokens += u.completion_tokens;
            total_usage.total_tokens += u.total_tokens;
        }

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
            return Ok(CompletionResult { text, usage: Some(total_usage) });
        }

        // Add the assistant message with tool calls
        let assistant_msg = ChatMessage::assistant(
            response.message.content.clone().unwrap_or_default(),
        )
        .with_tool_calls(response.message.tool_calls.clone());
        messages.push(assistant_msg);

        // Invoke each tool call and emit events
        for call in &response.message.tool_calls {
            // Check cancellation before each tool invocation
            if cancel.is_cancelled() {
                tracing::info!("tool loop cancelled before invoking {}", call.function.name);
                return Ok(CompletionResult { text: "[Response stopped by user]".to_string(), usage: Some(total_usage) });
            }

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

            // Emit image_output events for screenshots produced by code_execution / browser
            if (call.function.name == "code_execution" || call.function.name == "browser") && ok {
                if let Some(ref ws_path) = workspace_path {
                    emit_image_outputs(&result_str, ws_path, tx).await;
                }
            }

            messages.push(ChatMessage::tool(&call_id, result_str));
        }
    }

    // Exhausted rounds — final completion without tools
    if cancel.is_cancelled() {
        return Ok(CompletionResult { text: "[Response stopped by user]".to_string(), usage: Some(total_usage) });
    }
    let request = CompletionRequest::new(connector.model.clone(), messages);
    let response = tokio::select! {
        res = provider.complete(request) => res?,
        _ = cancel.cancelled() => {
            return Ok(CompletionResult { text: "[Response stopped by user]".to_string(), usage: Some(total_usage) });
        }
    };
    if let Some(u) = &response.usage {
        total_usage.prompt_tokens += u.prompt_tokens;
        total_usage.completion_tokens += u.completion_tokens;
        total_usage.total_tokens += u.total_tokens;
    }
    let text = sanitize_tool_artifacts(&response.message.content.unwrap_or_default());
    for line in text.split_inclusive('\n') {
        let _ = tx.try_send(line.to_string());
    }
    Ok(CompletionResult { text, usage: Some(total_usage) })
}

/// After a successful `code_execution` tool call, scan output_files for images
/// and emit them as `image_output` events through the SSE channel so the frontend
/// can display live previews before the stream completes.
async fn emit_image_outputs(
    result_str: &str,
    workspace_path: &std::path::Path,
    tx: &tokio::sync::mpsc::Sender<String>,
) {
    const MAX_IMAGE_SIZE: u64 = 2 * 1024 * 1024; // 2 MB
    const MAX_IMAGES_PER_ROUND: usize = 5;
    const IMAGE_EXTENSIONS: &[&str] = &["png", "jpg", "jpeg", "gif", "webp", "svg"];

    let Ok(val) = serde_json::from_str::<serde_json::Value>(result_str) else {
        return;
    };
    let Some(output_files) = val.get("output_files").and_then(|v| v.as_array()) else {
        return;
    };

    let mut emitted = 0usize;
    for file in output_files {
        if emitted >= MAX_IMAGES_PER_ROUND {
            break;
        }
        let Some(path_str) = file.get("path").and_then(|v| v.as_str()) else {
            continue;
        };
        let ext = path_str
            .rsplit('.')
            .next()
            .unwrap_or("")
            .to_ascii_lowercase();
        if !IMAGE_EXTENSIONS.contains(&ext.as_str()) {
            continue;
        }

        // Resolve full path on host filesystem
        let full_path = workspace_path.join(
            path_str.strip_prefix('/').unwrap_or(path_str),
        );
        let Ok(metadata) = tokio::fs::metadata(&full_path).await else {
            continue;
        };
        if metadata.len() > MAX_IMAGE_SIZE {
            continue;
        }
        let Ok(bytes) = tokio::fs::read(&full_path).await else {
            continue;
        };

        use base64::Engine;
        let b64 = base64::engine::general_purpose::STANDARD.encode(&bytes);
        let mime = match ext.as_str() {
            "png" => "image/png",
            "jpg" | "jpeg" => "image/jpeg",
            "gif" => "image/gif",
            "webp" => "image/webp",
            "svg" => "image/svg+xml",
            _ => "application/octet-stream",
        };
        let filename = std::path::Path::new(path_str)
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("image");

        let evt = serde_json::json!({
            "type": "image_output",
            "url": format!("data:{mime};base64,{b64}"),
            "filename": filename,
        });
        let _ = tx.try_send(format!("\x02{evt}"));
        emitted += 1;
    }
}

/// Stream a chat completion, yielding text deltas via a callback.
/// When reasoning is enabled, reasoning tokens are forwarded via `on_reasoning`.
/// The `cancel` token allows aborting mid-stream.
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
    cancel: tokio_util::sync::CancellationToken,
) -> Result<CompletionResult> {
    let provider = build_provider(connector, timeout_secs)?;
    let request = build_request(connector, system_prompt, history, temperature, max_tokens, reasoning_effort);
    let mut stream = provider.stream_completion(request).await?;
    let mut full_text = String::new();
    let mut completed_text: Option<String> = None;
    let mut usage: Option<TokenUsage> = None;

    loop {
        tokio::select! {
            event = stream.next() => {
                let Some(event) = event else { break };
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
                        usage = response.usage;
                    }
                    StreamEvent::ToolCallDelta { .. } => {}
                }
            }
            _ = cancel.cancelled() => {
                tracing::info!("stream_complete cancelled by client");
                return Ok(CompletionResult {
                    text: if full_text.is_empty() {
                        "[Response stopped by user]".to_string()
                    } else {
                        full_text
                    },
                    usage,
                });
            }
        }
    }

    let text = if full_text.is_empty() {
        completed_text.unwrap_or_default()
    } else {
        full_text
    };
    Ok(CompletionResult { text, usage })
}
