/// Link understanding tool: fetch a URL and extract readable text content.
use std::sync::Arc;

use denkwerk::functions::{FunctionDefinition, FunctionParameter, KernelFunction};
use denkwerk::DynKernelFunction;
use serde_json::{json, Value};

pub struct FetchUrlTool;

impl FetchUrlTool {
    pub fn new() -> Self { Self }
    pub fn into_dyn(self) -> DynKernelFunction { Arc::new(self) }
}

#[async_trait::async_trait]
impl KernelFunction for FetchUrlTool {
    fn definition(&self) -> FunctionDefinition {
        let mut def = FunctionDefinition::new("fetch_url")
            .with_description("Fetch a URL and extract the readable text content. Useful for reading web pages, docs, and articles.");
        def.add_parameter(
            FunctionParameter::new("url", json!({"type": "string"}))
                .with_description("The URL to fetch"),
        );
        def.add_parameter(
            FunctionParameter::new("max_length", json!({"type": "integer"}))
                .with_description("Maximum length of extracted text (default: 8000)")
                .optional(),
        );
        def
    }

    async fn invoke(&self, arguments: &Value) -> Result<Value, denkwerk::LLMError> {
        let url = arguments.get("url").and_then(|v| v.as_str())
            .ok_or_else(|| denkwerk::LLMError::InvalidFunctionArguments("url is required".into()))?;
        let max_length = arguments.get("max_length").and_then(|v| v.as_u64()).unwrap_or(8000) as usize;

        let client = match reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .user_agent("Clawkson/1.0")
            .build() {
            Ok(c) => c,
            Err(e) => return Ok(json!({"error": format!("client error: {e}"), "url": url})),
        };

        let response = match client.get(url).send().await {
            Ok(r) => r,
            Err(e) => return Ok(json!({"error": format!("fetch failed: {e}"), "url": url})),
        };

        if !response.status().is_success() {
            return Ok(json!({"error": format!("HTTP {}", response.status().as_u16()), "url": url}));
        }

        let ct = response.headers().get("content-type").and_then(|v| v.to_str().ok()).unwrap_or("").to_string();
        let body = match response.text().await {
            Ok(b) => b,
            Err(e) => return Ok(json!({"error": format!("read failed: {e}"), "url": url})),
        };

        let text = if ct.contains("text/html") { extract_html_text(&body) } else { body };
        let text = if text.len() > max_length {
            format!("{}...\n[Truncated at {} chars]", &text[..max_length], max_length)
        } else { text };

        Ok(json!({"url": url, "content_type": ct, "text": text, "length": text.len()}))
    }
}

fn extract_html_text(html: &str) -> String {
    let mut result = String::new();
    let mut in_tag = false;
    let mut in_script = false;
    let mut in_style = false;
    let mut tag_name = String::new();
    let mut collecting_tag = false;

    for ch in html.chars() {
        if ch == '<' { in_tag = true; collecting_tag = true; tag_name.clear(); continue; }
        if ch == '>' {
            in_tag = false; collecting_tag = false;
            let lower = tag_name.to_lowercase();
            match lower.as_str() {
                "script" => in_script = true, "/script" => in_script = false,
                "style" => in_style = true, "/style" => in_style = false,
                "br" | "br/" => { result.push('\n'); }
                "/p" | "/div" | "/h1" | "/h2" | "/h3" | "/li" => { result.push('\n'); }
                _ => {}
            }
            continue;
        }
        if in_tag { if collecting_tag && !ch.is_whitespace() { tag_name.push(ch); } else { collecting_tag = false; } continue; }
        if in_script || in_style { continue; }
        result.push(ch);
    }

    result.replace("&amp;", "&").replace("&lt;", "<").replace("&gt;", ">")
        .replace("&quot;", "\"").replace("&#39;", "'").replace("&nbsp;", " ")
        .lines().map(|l| l.trim()).filter(|l| !l.is_empty()).collect::<Vec<_>>().join("\n")
}
