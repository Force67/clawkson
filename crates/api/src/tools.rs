use std::sync::Arc;

use clawkson_container::{ContainerManager, ExecRequest};
use clawkson_db::Db;
use denkwerk::{
    functions::{FunctionParameter, KernelFunction},
    DynKernelFunction, FunctionDefinition,
};
use serde::Deserialize;
use serde_json::Value;
use uuid::Uuid;

// Re-export for use in conversations.rs
pub use http_tool::AuthenticatedHttpTool;

/// A tool that executes code inside an agent's sandboxed container.
pub struct CodeExecutionTool {
    agent_id: Uuid,
    container_manager: Arc<ContainerManager>,
}

impl CodeExecutionTool {
    pub fn new(agent_id: Uuid, container_manager: Arc<ContainerManager>) -> Self {
        Self {
            agent_id,
            container_manager,
        }
    }

    pub fn into_dyn(self) -> DynKernelFunction {
        Arc::new(self)
    }
}

#[derive(Debug, Deserialize)]
struct CodeExecArgs {
    language: String,
    code: String,
}

#[async_trait::async_trait]
impl KernelFunction for CodeExecutionTool {
    fn definition(&self) -> FunctionDefinition {
        let mut def = FunctionDefinition::new("code_execution")
            .with_description("Execute code in a sandboxed container. Use this to run Python or Bash code. The container has a /workspace directory for file operations.");

        def.add_parameter(
            FunctionParameter::new(
                "language",
                serde_json::json!({
                    "type": "string",
                    "enum": ["python", "bash"]
                }),
            )
            .with_description("The programming language to execute: 'python' or 'bash'"),
        );

        def.add_parameter(
            FunctionParameter::new(
                "code",
                serde_json::json!({ "type": "string" }),
            )
            .with_description("The code to execute"),
        );

        def
    }

    async fn invoke(&self, arguments: &Value) -> Result<Value, denkwerk::LLMError> {
        let args: CodeExecArgs = serde_json::from_value(arguments.clone()).map_err(|e| {
            denkwerk::LLMError::InvalidFunctionArguments(format!(
                "Invalid arguments for code_execution: {e}"
            ))
        })?;

        let command = match args.language.as_str() {
            "python" => format!("python3 -c {}", shell_escape(&args.code)),
            "bash" => args.code.clone(),
            other => {
                return Ok(serde_json::json!({
                    "error": format!("Unsupported language: {other}. Use 'python' or 'bash'.")
                }));
            }
        };

        let request = ExecRequest {
            command,
            timeout: Some(30),
        };

        match self.container_manager.exec(self.agent_id, &request).await {
            Ok(result) => Ok(serde_json::json!({
                "stdout": result.stdout,
                "stderr": result.stderr,
                "exit_code": result.exit_code,
                "timed_out": result.timed_out,
            })),
            Err(e) => Ok(serde_json::json!({
                "error": e.to_string(),
            })),
        }
    }
}

fn shell_escape(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}

// ── Knowledge List Tool ───────────────────────────────────────────

/// A tool that lets agents list their linked knowledge bases.
pub struct KnowledgeListTool {
    agent_id: Uuid,
    db: Db,
}

impl KnowledgeListTool {
    pub fn new(agent_id: Uuid, db: Db) -> Self {
        Self { agent_id, db }
    }

    pub fn into_dyn(self) -> DynKernelFunction {
        Arc::new(self)
    }
}

#[async_trait::async_trait]
impl KernelFunction for KnowledgeListTool {
    fn definition(&self) -> FunctionDefinition {
        let mut def = FunctionDefinition::new("knowledge_list")
            .with_description(
                "List all knowledge bases available to this agent. \
                 Returns the name, description, embedding model, and entry count for each. \
                 Use this to discover what knowledge is available before searching.",
            );

        // Add a dummy optional parameter so the schema has a non-empty `properties` object.
        // Some providers reject function schemas without `properties`.
        def.add_parameter(
            FunctionParameter::new(
                "verbose",
                serde_json::json!({ "type": "boolean" }),
            )
            .with_description("If true, include extra details (default: false)")
            .optional(),
        );

        def
    }

    async fn invoke(&self, _arguments: &Value) -> Result<Value, denkwerk::LLMError> {
        let pool = self.db.pool();

        let kbs = clawkson_db::knowledge_base::agent_list_kbs(pool, self.agent_id)
            .await
            .map_err(|e| denkwerk::LLMError::FunctionExecution {
                function: "knowledge_list".to_string(),
                message: format!("Failed to list KBs: {e}"),
            })?;

        let result: Vec<Value> = kbs
            .iter()
            .map(|kb| {
                serde_json::json!({
                    "id": kb.id.to_string(),
                    "name": kb.name,
                    "description": kb.description,
                    "embedding_model": kb.embedding_model,
                    "entry_count": kb.entry_count,
                })
            })
            .collect();

        Ok(serde_json::json!({
            "knowledge_bases": result,
            "total": result.len(),
        }))
    }
}

// ── Knowledge Search Tool ─────────────────────────────────────────

/// A tool that lets agents search their linked knowledge bases via vector similarity.
pub struct KnowledgeSearchTool {
    agent_id: Uuid,
    db: Db,
}

impl KnowledgeSearchTool {
    pub fn new(agent_id: Uuid, db: Db) -> Self {
        Self { agent_id, db }
    }

    pub fn into_dyn(self) -> DynKernelFunction {
        Arc::new(self)
    }
}

#[derive(Debug, Deserialize)]
struct KnowledgeSearchArgs {
    query: String,
    limit: Option<i64>,
}

#[async_trait::async_trait]
impl KernelFunction for KnowledgeSearchTool {
    fn definition(&self) -> FunctionDefinition {
        let mut def = FunctionDefinition::new("knowledge_search")
            .with_description(
                "Search through your linked knowledge bases using semantic similarity. \
                 Returns the most relevant text passages for the given query. \
                 Use this whenever you need to look up information, cite sources, or answer questions based on uploaded documents.",
            );

        def.add_parameter(
            FunctionParameter::new("query", serde_json::json!({ "type": "string" }))
                .with_description("The search query — describe what information you are looking for"),
        );

        def.add_parameter(
            FunctionParameter::new(
                "limit",
                serde_json::json!({ "type": "integer", "minimum": 1, "maximum": 10 }),
            )
            .with_description("Maximum number of results to return (default: 5)"),
        );

        def
    }

    async fn invoke(&self, arguments: &Value) -> Result<Value, denkwerk::LLMError> {
        let args: KnowledgeSearchArgs =
            serde_json::from_value(arguments.clone()).map_err(|e| {
                denkwerk::LLMError::InvalidFunctionArguments(format!(
                    "Invalid arguments for knowledge_search: {e}"
                ))
            })?;

        let pool = self.db.pool();

        // Get knowledge bases linked to this agent
        let kbs = clawkson_db::knowledge_base::agent_list_kbs(pool, self.agent_id)
            .await
            .map_err(|e| {
                denkwerk::LLMError::FunctionExecution {
                    function: "knowledge_search".to_string(),
                    message: format!("Failed to list agent KBs: {e}"),
                }
            })?;

        if kbs.is_empty() {
            return Ok(serde_json::json!({
                "results": [],
                "message": "No knowledge bases are linked to this agent."
            }));
        }

        // Use the embedding model from the first KB (they should all use the same one)
        let model = &kbs[0].embedding_model;
        let kb_ids: Vec<Uuid> = kbs.iter().map(|kb| kb.id).collect();

        // Generate embedding for the query
        let query_vec = match crate::embeddings::generate_one(model, &args.query).await {
            Ok(v) => v,
            Err(e) => {
                return Ok(serde_json::json!({
                    "error": format!("Failed to generate query embedding: {e}")
                }));
            }
        };

        let limit = args.limit.unwrap_or(5).min(10);

        let results =
            clawkson_db::knowledge_entry::search(pool, &kb_ids, &query_vec, limit)
                .await
                .map_err(|e| {
                    denkwerk::LLMError::FunctionExecution {
                        function: "knowledge_search".to_string(),
                        message: format!("Vector search failed: {e}"),
                    }
                })?;

        // Find KB names for context
        let kb_name = |id: Uuid| -> String {
            kbs.iter()
                .find(|kb| kb.id == id)
                .map(|kb| kb.name.clone())
                .unwrap_or_else(|| "Unknown".to_string())
        };

        let result_json: Vec<Value> = results
            .iter()
            .map(|r| {
                let mut entry = serde_json::json!({
                    "title": r.title,
                    "content": r.content,
                    "score": format!("{:.3}", r.score),
                    "knowledge_base": kb_name(r.knowledge_base_id),
                });
                if let Some(doc_id) = r.source_document_id {
                    entry["document_url"] = serde_json::json!(
                        format!("/api/knowledge/{}/documents/{doc_id}/download", r.knowledge_base_id)
                    );
                }
                entry
            })
            .collect();

        Ok(serde_json::json!({
            "results": result_json,
            "total": result_json.len(),
        }))
    }
}

// ── Authenticated HTTP Tool ──────────────────────────────────────

pub mod http_tool {
    use super::*;

    /// Resolved credentials from a connector, used to authenticate HTTP requests.
    #[derive(Debug, Clone)]
    pub struct ConnectorAuth {
        pub connector_name: String,
        pub connector_type: clawkson_db::connector::ConnectorType,
        pub config: serde_json::Value,
    }

    /// A tool that makes authenticated HTTP requests using connector credentials.
    pub struct AuthenticatedHttpTool {
        connectors: Vec<ConnectorAuth>,
    }

    impl AuthenticatedHttpTool {
        pub fn new(connectors: Vec<ConnectorAuth>) -> Self {
            Self { connectors }
        }

        pub fn into_dyn(self) -> DynKernelFunction {
            Arc::new(self)
        }

        fn find_connector(&self, name: &str) -> Option<&ConnectorAuth> {
            self.connectors.iter().find(|c| {
                c.connector_name.eq_ignore_ascii_case(name)
            })
        }

        fn apply_auth(
            &self,
            auth: &ConnectorAuth,
            mut builder: reqwest::RequestBuilder,
        ) -> reqwest::RequestBuilder {
            match auth.connector_type {
                clawkson_db::connector::ConnectorType::AzureDevops => {
                    // Azure DevOps uses Basic auth with empty user + PAT as password
                    if let Some(pat) = auth.config.get("pat").and_then(|v| v.as_str()) {
                        builder = builder.basic_auth("", Some(pat));
                    }
                }
                _ => {
                    // Generic: look for common auth fields
                    if let Some(token) = auth.config.get("api_key")
                        .or_else(|| auth.config.get("token"))
                        .or_else(|| auth.config.get("bearer_token"))
                        .and_then(|v| v.as_str())
                    {
                        builder = builder.bearer_auth(token);
                    } else if let Some(bot_token) = auth.config.get("bot_token").and_then(|v| v.as_str()) {
                        builder = builder.bearer_auth(bot_token);
                    }
                }
            }
            builder
        }
    }

    #[derive(Debug, Deserialize)]
    struct HttpArgs {
        method: String,
        url: String,
        connector: String,
        #[serde(default)]
        headers: Option<serde_json::Map<String, Value>>,
        #[serde(default)]
        body: Option<Value>,
    }

    #[async_trait::async_trait]
    impl KernelFunction for AuthenticatedHttpTool {
        fn definition(&self) -> FunctionDefinition {
            let connector_names: Vec<&str> = self.connectors.iter()
                .map(|c| c.connector_name.as_str())
                .collect();

            let mut def = FunctionDefinition::new("authenticated_http")
                .with_description(format!(
                    "Make an authenticated HTTP request using a connector's credentials. \
                     Available connectors: {}. \
                     The connector handles authentication headers automatically.",
                    if connector_names.is_empty() { "none".to_string() }
                    else { connector_names.join(", ") }
                ));

            def.add_parameter(
                FunctionParameter::new(
                    "method",
                    serde_json::json!({
                        "type": "string",
                        "enum": ["GET", "POST", "PUT", "PATCH", "DELETE", "HEAD"]
                    }),
                )
                .with_description("HTTP method"),
            );

            def.add_parameter(
                FunctionParameter::new("url", serde_json::json!({ "type": "string" }))
                    .with_description("The full URL to request"),
            );

            def.add_parameter(
                FunctionParameter::new("connector", serde_json::json!({ "type": "string" }))
                    .with_description(format!(
                        "Name of the connector to authenticate with. Available: {}",
                        if connector_names.is_empty() { "none".to_string() }
                        else { connector_names.join(", ") }
                    )),
            );

            def.add_parameter(
                FunctionParameter::new(
                    "headers",
                    serde_json::json!({
                        "type": "object",
                        "additionalProperties": { "type": "string" }
                    }),
                )
                .with_description("Optional extra HTTP headers as key-value pairs")
                .optional(),
            );

            def.add_parameter(
                FunctionParameter::new("body", serde_json::json!({}))
                    .with_description("Optional request body (string or JSON object)")
                    .optional(),
            );

            def
        }

        async fn invoke(&self, arguments: &Value) -> Result<Value, denkwerk::LLMError> {
            let args: HttpArgs = serde_json::from_value(arguments.clone()).map_err(|e| {
                denkwerk::LLMError::InvalidFunctionArguments(format!(
                    "Invalid arguments for authenticated_http: {e}"
                ))
            })?;

            let auth = match self.find_connector(&args.connector) {
                Some(a) => a,
                None => {
                    let available: Vec<&str> = self.connectors.iter()
                        .map(|c| c.connector_name.as_str())
                        .collect();
                    return Ok(serde_json::json!({
                        "error": format!(
                            "Connector '{}' not found. Available: {}",
                            args.connector,
                            if available.is_empty() { "none".to_string() }
                            else { available.join(", ") }
                        )
                    }));
                }
            };

            let client = reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(30))
                .build()
                .unwrap_or_default();

            let method = match args.method.to_uppercase().as_str() {
                "GET" => reqwest::Method::GET,
                "POST" => reqwest::Method::POST,
                "PUT" => reqwest::Method::PUT,
                "PATCH" => reqwest::Method::PATCH,
                "DELETE" => reqwest::Method::DELETE,
                "HEAD" => reqwest::Method::HEAD,
                other => {
                    return Ok(serde_json::json!({
                        "error": format!("Unsupported method: {other}")
                    }));
                }
            };

            let mut builder = client.request(method, &args.url);

            // Apply connector auth
            builder = self.apply_auth(auth, builder);

            // Apply custom headers
            if let Some(headers) = &args.headers {
                for (k, v) in headers {
                    if let Some(v_str) = v.as_str() {
                        builder = builder.header(k.as_str(), v_str);
                    }
                }
            }

            // Apply body
            if let Some(body) = &args.body {
                match body {
                    Value::String(s) => {
                        builder = builder.body(s.clone());
                    }
                    _ => {
                        builder = builder
                            .header("Content-Type", "application/json")
                            .json(body);
                    }
                }
            }

            match builder.send().await {
                Ok(response) => {
                    let status = response.status().as_u16();
                    let headers: serde_json::Map<String, Value> = response
                        .headers()
                        .iter()
                        .filter(|(k, _)| {
                            let name = k.as_str().to_lowercase();
                            // Only include useful headers, skip noisy ones
                            matches!(name.as_str(),
                                "content-type" | "x-ms-request-id" | "retry-after" |
                                "x-ratelimit-remaining" | "location" | "link"
                            )
                        })
                        .map(|(k, v)| {
                            (k.to_string(), Value::String(v.to_str().unwrap_or("").to_string()))
                        })
                        .collect();

                    let body_text = response.text().await.unwrap_or_default();

                    // Try to parse as JSON for structured output
                    let body_value = serde_json::from_str::<Value>(&body_text)
                        .unwrap_or(Value::String(body_text));

                    // Truncate very large responses
                    let body_value = match &body_value {
                        Value::String(s) if s.len() > 50_000 => {
                            Value::String(format!("{}... [truncated, {} total bytes]", &s[..50_000], s.len()))
                        }
                        _ => body_value,
                    };

                    Ok(serde_json::json!({
                        "status": status,
                        "headers": headers,
                        "body": body_value,
                    }))
                }
                Err(e) => {
                    Ok(serde_json::json!({
                        "error": format!("HTTP request failed: {e}"),
                    }))
                }
            }
        }
    }
}
