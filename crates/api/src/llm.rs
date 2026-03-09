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
    MessageRole as DenkMessageRole, StreamEvent,
};
use futures::StreamExt;

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

fn build_request(
    connector: &LlmConnector,
    system_prompt: Option<&str>,
    history: &[(MessageRole, String)],
    temperature: Option<f64>,
    max_tokens: Option<u32>,
) -> CompletionRequest {
    let mut messages = Vec::new();

    if let Some(system_prompt) = system_prompt {
        if !system_prompt.trim().is_empty() {
            messages.push(ChatMessage::system(system_prompt));
        }
    }

    for (role, content) in history {
        messages.push(ChatMessage::new(role_to_denkwerk(role), content.clone()));
    }

    let mut request = CompletionRequest::new(connector.model.clone(), messages);
    if let Some(temperature) = temperature {
        request = request.with_temperature(temperature as f32);
    }
    if let Some(max_tokens) = max_tokens {
        request = request.with_max_tokens(max_tokens);
    }

    request
}

fn build_provider(connector: &LlmConnector) -> Result<Box<dyn LLMProvider>> {
    match connector.provider_type {
        LlmProviderType::Azure => {
            let mut config =
                AzureOpenAIConfig::new(connector.api_key.clone(), connector.api_base_url.clone());
            if let Some(version) = &connector.azure_api_version {
                config = config.with_api_version(version.clone());
            }
            Ok(Box::new(AzureOpenAI::from_config(config)?))
        }
        LlmProviderType::OpenRouter => {
            let mut config = OpenRouterConfig::new(connector.api_key.clone());
            config.base_url = resolve_base_url(connector);
            config.referer = Some("https://clawkson.app".to_string());
            config.title = Some("Clawkson".to_string());
            Ok(Box::new(OpenRouter::from_config(config)?))
        }
        LlmProviderType::OpenAi | LlmProviderType::Custom => {
            let config = OpenAIConfig::new(connector.api_key.clone())
                .with_base_url(resolve_base_url(connector));
            Ok(Box::new(OpenAI::from_config(config)?))
        }
    }
}

/// Perform a blocking (non-streaming) chat completion.
pub async fn complete(
    connector: &LlmConnector,
    system_prompt: Option<&str>,
    history: &[(MessageRole, String)],
    temperature: Option<f64>,
    max_tokens: Option<u32>,
) -> Result<String> {
    let provider = build_provider(connector)?;
    let request = build_request(connector, system_prompt, history, temperature, max_tokens);
    let response = provider.complete(request).await?;

    Ok(response.message.content.unwrap_or_default())
}

/// Perform a completion with tool-calling loop.
/// Runs up to `max_rounds` iterations: send to LLM, invoke tool calls, append results, repeat.
pub async fn complete_with_tools(
    connector: &LlmConnector,
    system_prompt: Option<&str>,
    history: &[(MessageRole, String)],
    temperature: Option<f64>,
    max_tokens: Option<u32>,
    registry: &FunctionRegistry,
    max_rounds: usize,
) -> Result<String> {
    let provider = build_provider(connector)?;

    // Build initial messages
    let mut messages = Vec::new();
    if let Some(sp) = system_prompt {
        if !sp.trim().is_empty() {
            messages.push(ChatMessage::system(sp));
        }
    }
    for (role, content) in history {
        messages.push(ChatMessage::new(role_to_denkwerk(role), content.clone()));
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

        let response = provider.complete(request).await?;

        if response.message.tool_calls.is_empty() {
            // No tool calls — return the text response
            return Ok(response.message.content.unwrap_or_default());
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
            let result = registry.invoke(&call.function).await;
            let result_str = match result {
                Ok(value) => serde_json::to_string(&value).unwrap_or_default(),
                Err(err) => serde_json::json!({ "error": err.to_string() }).to_string(),
            };
            messages.push(ChatMessage::tool(&call_id, result_str));
        }
    }

    // If we exhausted rounds, do one final completion without tools
    let request = CompletionRequest::new(connector.model.clone(), messages);
    let response = provider.complete(request).await?;
    Ok(response.message.content.unwrap_or_default())
}

/// Stream a chat completion, yielding text deltas via a callback.
pub async fn stream_complete(
    connector: &LlmConnector,
    system_prompt: Option<&str>,
    history: &[(MessageRole, String)],
    temperature: Option<f64>,
    max_tokens: Option<u32>,
    mut on_chunk: impl FnMut(String),
) -> Result<String> {
    let provider = build_provider(connector)?;
    let request = build_request(connector, system_prompt, history, temperature, max_tokens);
    let mut stream = provider.stream_completion(request).await?;
    let mut full_text = String::new();
    let mut completed_text: Option<String> = None;

    while let Some(event) = stream.next().await {
        match event? {
            StreamEvent::MessageDelta(text) => {
                full_text.push_str(&text);
                on_chunk(text);
            }
            StreamEvent::Completed(response) => {
                completed_text = response.message.content;
            }
            StreamEvent::ReasoningDelta(_) | StreamEvent::ToolCallDelta { .. } => {}
        }
    }

    if full_text.is_empty() {
        Ok(completed_text.unwrap_or_default())
    } else {
        Ok(full_text)
    }
}
