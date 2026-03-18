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

/// Maximum bytes read back from a single workspace file to include inline in a tool result.
/// Files larger than this are described by path + size but their content is omitted.
const MAX_INLINE_FILE_BYTES: u64 = 64 * 1024; // 64 KB

// Re-export for use in conversations.rs
pub use http_tool::AuthenticatedHttpTool;

/// A tool that executes code inside an agent's sandboxed container.
/// Scoped to a specific conversation for workspace isolation.
pub struct CodeExecutionTool {
    agent_id: Uuid,
    conversation_id: Uuid,
    container_manager: Arc<ContainerManager>,
    workspace_root: std::path::PathBuf,
}

impl CodeExecutionTool {
    pub fn new(agent_id: Uuid, conversation_id: Uuid, container_manager: Arc<ContainerManager>, workspace_root: std::path::PathBuf) -> Self {
        Self {
            agent_id,
            conversation_id,
            container_manager,
            workspace_root,
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
            .with_description(
                "Execute code REMOTELY in a sandboxed Docker container. \
                 You are NOT inside the container — this tool sends your code to a separate, \
                 isolated container for execution and returns the output. \
                 Use this for running Python or Bash code. \
                 Proactively install any needed packages (pip install, apt-get install -y) \
                 without asking — the container is ephemeral and safe to modify. \
                 The container's filesystem is read-only except for /workspace, which is \
                 the only writable persistent location. Always use /workspace for file operations — \
                 read inputs from /workspace/inputs/ and write outputs to /workspace/outputs/. \
                 Never fall back to /tmp or other paths. \
                 After execution, any files written to /workspace/outputs/ are automatically \
                 returned to you so you can read or summarise their contents.",
            );

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
            output_dir: Some("outputs".to_string()),
        };

        let exec_result = match self.container_manager.exec(self.agent_id, self.conversation_id, &request).await {
            Err(clawkson_container::ContainerError::NotFound(_)) => {
                // Container gone — try to auto-restart and retry once
                tracing::info!(agent_id = %self.agent_id, conversation_id = %self.conversation_id, "container not found, attempting auto-restart");
                let config = clawkson_container::ContainerConfig::default();
                if let Err(e) = self.container_manager.start_container(self.agent_id, self.conversation_id, &config).await {
                    return Ok(serde_json::json!({
                        "error": format!("Container lost and restart failed: {e}. Please try again."),
                    }));
                }
                self.container_manager.exec(self.agent_id, self.conversation_id, &request).await
            }
            other => other,
        };

        match exec_result {
            Ok(result) => {
                let mut response = serde_json::json!({
                    "stdout": result.stdout,
                    "stderr": result.stderr,
                    "exit_code": result.exit_code,
                    "timed_out": result.timed_out,
                });

                // Read output file contents back so the LLM can see them directly.
                if let Some(output_files) = &result.output_files {
                    if !output_files.is_empty() {
                        let workspace = self.workspace_root
                            .join(self.agent_id.to_string())
                            .join(self.conversation_id.to_string());
                        let files_json: Vec<Value> = output_files.iter().map(|f| {
                            let abs = workspace.join(&f.path);
                            let content = if f.size <= MAX_INLINE_FILE_BYTES {
                                match std::fs::read(&abs) {
                                    Ok(bytes) => match std::str::from_utf8(&bytes) {
                                        Ok(text) => Value::String(text.to_string()),
                                        Err(_) => Value::String(
                                            format!("[binary file, {} bytes]", bytes.len())
                                        ),
                                    },
                                    Err(e) => Value::String(format!("[read error: {e}]")),
                                }
                            } else {
                                Value::String(format!(
                                    "[file too large to inline ({} KB) — download via workspace API]",
                                    f.size / 1024
                                ))
                            };
                            serde_json::json!({
                                "path": f.path,
                                "size_bytes": f.size,
                                "content": content,
                            })
                        }).collect();

                        response["output_files"] = Value::Array(files_json);
                    }
                }

                Ok(response)
            }
            Err(e) => Ok(serde_json::json!({
                "error": e.to_string(),
            })),
        }
    }
}

fn shell_escape(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}

// ── Workspace Read Tool ───────────────────────────────────────────

/// A tool that lets the LLM read a file from the conversation's workspace.
pub struct WorkspaceReadTool {
    agent_id: Uuid,
    conversation_id: Uuid,
    workspace_root: std::path::PathBuf,
}

impl WorkspaceReadTool {
    pub fn new(agent_id: Uuid, conversation_id: Uuid, workspace_root: std::path::PathBuf) -> Self {
        Self { agent_id, conversation_id, workspace_root }
    }

    pub fn into_dyn(self) -> DynKernelFunction {
        Arc::new(self)
    }
}

#[derive(Debug, Deserialize)]
struct WorkspaceReadArgs {
    path: String,
}

#[async_trait::async_trait]
impl KernelFunction for WorkspaceReadTool {
    fn definition(&self) -> FunctionDefinition {
        let mut def = FunctionDefinition::new("workspace_read")
            .with_description(
                "Read the contents of a file in the agent's container workspace (/workspace). \
                 Use this to inspect input files placed by the user or output files produced \
                 by previous code_execution calls. Paths are relative to /workspace \
                 (e.g. 'inputs/data.csv' or 'outputs/result.txt').",
            );

        def.add_parameter(
            FunctionParameter::new("path", serde_json::json!({ "type": "string" }))
                .with_description("Workspace-relative path of the file to read (e.g. 'inputs/data.csv')"),
        );

        def
    }

    async fn invoke(&self, arguments: &Value) -> Result<Value, denkwerk::LLMError> {
        let args: WorkspaceReadArgs = serde_json::from_value(arguments.clone()).map_err(|e| {
            denkwerk::LLMError::InvalidFunctionArguments(format!(
                "Invalid arguments for workspace_read: {e}"
            ))
        })?;

        let workspace = self.workspace_root
            .join(self.agent_id.to_string())
            .join(self.conversation_id.to_string());

        match clawkson_container::workspace::sandbox_path(&workspace, &args.path) {
            Err(e) => Ok(serde_json::json!({ "error": format!("Invalid path: {e}") })),
            Ok(abs) => {
                if !abs.exists() {
                    return Ok(serde_json::json!({ "error": format!("File not found: {}", args.path) }));
                }
                if abs.is_dir() {
                    // List directory contents instead of erroring
                    match clawkson_container::workspace::list_workspace(&workspace, &args.path) {
                        Ok(listing) => {
                            let entries: Vec<Value> = listing.entries.iter().map(|e| serde_json::json!({
                                "name": e.name,
                                "path": e.path,
                                "is_dir": e.is_dir,
                                "size_bytes": e.size,
                            })).collect();
                            Ok(serde_json::json!({
                                "type": "directory",
                                "path": args.path,
                                "entries": entries,
                            }))
                        }
                        Err(e) => Ok(serde_json::json!({ "error": format!("{e}") })),
                    }
                } else {
                    let metadata = abs.metadata().map(|m| m.len()).unwrap_or(0);
                    if metadata > MAX_INLINE_FILE_BYTES {
                        return Ok(serde_json::json!({
                            "error": format!(
                                "File too large to read inline ({} KB). Download it via the workspace API.",
                                metadata / 1024
                            )
                        }));
                    }
                    match tokio::fs::read(&abs).await {
                        Ok(bytes) => match std::str::from_utf8(&bytes) {
                            Ok(text) => Ok(serde_json::json!({
                                "path": args.path,
                                "size_bytes": bytes.len(),
                                "content": text,
                            })),
                            Err(_) => Ok(serde_json::json!({
                                "path": args.path,
                                "size_bytes": bytes.len(),
                                "content": format!("[binary file — {} bytes, not displayable as text]", bytes.len()),
                            })),
                        },
                        Err(e) => Ok(serde_json::json!({ "error": format!("Read error: {e}") })),
                    }
                }
            }
        }
    }
}

// ── Workspace Write Tool ──────────────────────────────────────────

/// A tool that lets the LLM write a file into the conversation's workspace.
pub struct WorkspaceWriteTool {
    agent_id: Uuid,
    conversation_id: Uuid,
    workspace_root: std::path::PathBuf,
}

impl WorkspaceWriteTool {
    pub fn new(agent_id: Uuid, conversation_id: Uuid, workspace_root: std::path::PathBuf) -> Self {
        Self { agent_id, conversation_id, workspace_root }
    }

    pub fn into_dyn(self) -> DynKernelFunction {
        Arc::new(self)
    }
}

#[derive(Debug, Deserialize)]
struct WorkspaceWriteArgs {
    path: String,
    content: String,
}

#[async_trait::async_trait]
impl KernelFunction for WorkspaceWriteTool {
    fn definition(&self) -> FunctionDefinition {
        let mut def = FunctionDefinition::new("workspace_write")
            .with_description(
                "Write text content to a file in the agent's container workspace (/workspace). \
                 The file is immediately visible to the running container. \
                 Use this to place input data, configuration, or scripts before running code_execution. \
                 Paths are relative to /workspace (e.g. 'inputs/data.csv').",
            );

        def.add_parameter(
            FunctionParameter::new("path", serde_json::json!({ "type": "string" }))
                .with_description("Workspace-relative path to write (e.g. 'inputs/data.csv'). Parent directories are created automatically."),
        );

        def.add_parameter(
            FunctionParameter::new("content", serde_json::json!({ "type": "string" }))
                .with_description("Text content to write to the file"),
        );

        def
    }

    async fn invoke(&self, arguments: &Value) -> Result<Value, denkwerk::LLMError> {
        let args: WorkspaceWriteArgs = serde_json::from_value(arguments.clone()).map_err(|e| {
            denkwerk::LLMError::InvalidFunctionArguments(format!(
                "Invalid arguments for workspace_write: {e}"
            ))
        })?;

        let workspace = self.workspace_root
            .join(self.agent_id.to_string())
            .join(self.conversation_id.to_string());

        match clawkson_container::workspace::sandbox_path(&workspace, &args.path) {
            Err(e) => Ok(serde_json::json!({ "error": format!("Invalid path: {e}") })),
            Ok(abs) => {
                // Ensure parent directory exists
                if let Some(parent) = abs.parent() {
                    if let Err(e) = tokio::fs::create_dir_all(parent).await {
                        return Ok(serde_json::json!({ "error": format!("Failed to create directory: {e}") }));
                    }
                }
                match tokio::fs::write(&abs, args.content.as_bytes()).await {
                    Ok(()) => Ok(serde_json::json!({
                        "path": args.path,
                        "size_bytes": args.content.len(),
                        "written": true,
                    })),
                    Err(e) => Ok(serde_json::json!({ "error": format!("Write error: {e}") })),
                }
            }
        }
    }
}

// ── Workspace List Tool ───────────────────────────────────────────

/// A tool that lets the LLM list files in the conversation's workspace.
pub struct WorkspaceListTool {
    agent_id: Uuid,
    conversation_id: Uuid,
    workspace_root: std::path::PathBuf,
}

impl WorkspaceListTool {
    pub fn new(agent_id: Uuid, conversation_id: Uuid, workspace_root: std::path::PathBuf) -> Self {
        Self { agent_id, conversation_id, workspace_root }
    }

    pub fn into_dyn(self) -> DynKernelFunction {
        Arc::new(self)
    }
}

#[derive(Debug, Deserialize)]
struct WorkspaceListArgs {
    path: Option<String>,
}

#[async_trait::async_trait]
impl KernelFunction for WorkspaceListTool {
    fn definition(&self) -> FunctionDefinition {
        let mut def = FunctionDefinition::new("workspace_list")
            .with_description(
                "List files and directories in the agent's container workspace (/workspace). \
                 Use this to discover what input files are available or what outputs have been produced.",
            );

        def.add_parameter(
            FunctionParameter::new("path", serde_json::json!({ "type": "string" }))
                .with_description("Workspace-relative subdirectory to list (default: workspace root)")
                .optional(),
        );

        def
    }

    async fn invoke(&self, arguments: &Value) -> Result<Value, denkwerk::LLMError> {
        let args: WorkspaceListArgs = serde_json::from_value(arguments.clone()).map_err(|e| {
            denkwerk::LLMError::InvalidFunctionArguments(format!(
                "Invalid arguments for workspace_list: {e}"
            ))
        })?;

        let workspace = self.workspace_root
            .join(self.agent_id.to_string())
            .join(self.conversation_id.to_string());
        let sub = args.path.as_deref().unwrap_or("");

        match clawkson_container::workspace::list_workspace(&workspace, sub) {
            Ok(listing) => {
                let entries: Vec<Value> = listing.entries.iter().map(|e| serde_json::json!({
                    "name": e.name,
                    "path": e.path,
                    "is_dir": e.is_dir,
                    "size_bytes": e.size,
                })).collect();
                Ok(serde_json::json!({
                    "path": listing.path,
                    "entries": entries,
                    "count": entries.len(),
                }))
            }
            Err(e) => Ok(serde_json::json!({ "error": format!("{e}") })),
        }
    }
}

// ── Start Preview Tool ────────────────────────────────────────────

/// A tool that registers a live preview of a web server running in the sandbox.
/// When the agent starts a server, it calls this tool to make it accessible
/// through the reverse proxy and visible to the user in the chat.
pub struct StartPreviewTool {
    agent_id: Uuid,
    conversation_id: Uuid,
}

impl StartPreviewTool {
    pub fn new(agent_id: Uuid, conversation_id: Uuid) -> Self {
        Self { agent_id, conversation_id }
    }

    pub fn into_dyn(self) -> DynKernelFunction {
        Arc::new(self)
    }
}

#[derive(Debug, Deserialize)]
struct StartPreviewArgs {
    port: u16,
    title: Option<String>,
}

#[async_trait::async_trait]
impl KernelFunction for StartPreviewTool {
    fn definition(&self) -> FunctionDefinition {
        let mut def = FunctionDefinition::new("start_preview")
            .with_description(
                "Register a live preview of a web server running in the sandbox container. \
                 Call this AFTER you have started a web server (e.g. python -m http.server, \
                 flask run, node http-server) to display it inline in the chat. \
                 The preview appears as an interactive iframe the user can see and interact with.",
            );

        def.add_parameter(
            FunctionParameter::new("port", serde_json::json!({ "type": "integer" }))
                .with_description("The port number the web server is listening on"),
        );
        def.add_parameter(
            FunctionParameter::new("title", serde_json::json!({ "type": "string" }))
                .with_description("A display title for the preview (e.g. 'Dashboard', 'Chart')")
                .optional(),
        );

        def
    }

    async fn invoke(&self, arguments: &Value) -> Result<Value, denkwerk::LLMError> {
        let args: StartPreviewArgs = serde_json::from_value(arguments.clone()).map_err(|e| {
            denkwerk::LLMError::InvalidFunctionArguments(format!(
                "Invalid arguments for start_preview: {e}"
            ))
        })?;

        let title = args.title.unwrap_or_else(|| "Live Preview".to_string());
        let preview_url = format!(
            "/api/agents/{}/container/preview/{}/?conversation_id={}",
            self.agent_id, args.port, self.conversation_id,
        );

        Ok(serde_json::json!({
            "preview_url": preview_url,
            "port": args.port,
            "title": title,
            "status": "ready",
        }))
    }
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
    extra_kb_ids: Vec<Uuid>,
}

impl KnowledgeSearchTool {
    pub fn new(agent_id: Uuid, db: Db) -> Self {
        Self { agent_id, db, extra_kb_ids: Vec::new() }
    }

    pub fn with_extra_kbs(mut self, kb_ids: Vec<Uuid>) -> Self {
        self.extra_kb_ids = kb_ids;
        self
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
                "Search through your linked knowledge bases and conversation memory using semantic similarity. \
                 Returns the most relevant text passages for the given query. \
                 Use this whenever you need to look up information, recall past conversations, cite sources, or answer questions based on uploaded documents.",
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

        let mut kb_ids: Vec<Uuid> = kbs.iter().map(|kb| kb.id).collect();
        kb_ids.extend(&self.extra_kb_ids);

        if kb_ids.is_empty() {
            return Ok(serde_json::json!({
                "results": [],
                "message": "No knowledge bases are linked to this agent."
            }));
        }

        // Load embedding provider config from settings
        let embed_config = match clawkson_db::settings::get(&self.db).await {
            Ok(row) => crate::embeddings::EmbeddingConfig {
                base_url: row.embedding_api_base_url,
                api_key: row.embedding_api_key,
                model: row.embedding_model,
            },
            Err(_) => crate::embeddings::EmbeddingConfig::default(),
        };

        // Use the embedding model from the first KB, or fall back to settings default
        let model = kbs.first()
            .map(|kb| kb.embedding_model.as_str())
            .unwrap_or(&embed_config.model);

        // Generate embedding for the query
        let query_vec = match crate::embeddings::generate_one(&embed_config, model, &args.query).await {
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

// ── Web Search Tool ──────────────────────────────────────────────

/// Max characters per search result snippet. Keeps tool output lean.
const MAX_SNIPPET_CHARS: usize = 300;
/// Hard cap on the serialized JSON output from web_search (bytes).
/// Prevents token explosion regardless of what the search API returns.
const MAX_SEARCH_OUTPUT_BYTES: usize = 8_000;

/// Which search provider backs the web_search tool.
pub enum SearchProvider {
    Tavily { api_key: String },
    Bing { api_key: String, endpoint: String },
}

/// A tool that searches the web. Supports multiple backends (Tavily, Bing).
/// Registered automatically when a user has an enabled web search connector.
pub struct WebSearchTool {
    provider: SearchProvider,
}

impl WebSearchTool {
    pub fn new(provider: SearchProvider) -> Self {
        Self { provider }
    }

    pub fn into_dyn(self) -> DynKernelFunction {
        Arc::new(self)
    }

    async fn search_tavily(api_key: &str, query: &str, max_results: u8, search_depth: &str, include_answer: bool) -> Result<Value, String> {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .unwrap_or_default();

        let body = serde_json::json!({
            "api_key": api_key,
            "query": query,
            "search_depth": search_depth,
            "include_answer": include_answer,
            "max_results": max_results,
        });

        let response = client
            .post("https://api.tavily.com/search")
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await
            .map_err(|e| format!("Web search request failed: {e}"))?;

        let status = response.status().as_u16();
        let text = response.text().await.unwrap_or_default();
        if status != 200 {
            return Err(format!("Tavily API returned status {status}: {text}"));
        }

        let raw: Value = serde_json::from_str(&text)
            .map_err(|e| format!("Failed to parse Tavily response: {e}"))?;

        let mut result = serde_json::json!({ "query": query });

        if include_answer {
            if let Some(answer) = raw.get("answer").and_then(|v| v.as_str()) {
                result["answer"] = Value::String(answer.to_string());
            }
        }

        if let Some(results) = raw.get("results").and_then(|v| v.as_array()) {
            let formatted: Vec<Value> = results.iter().map(|r| {
                let content = r.get("content").and_then(|v| v.as_str()).unwrap_or("");
                let content = if content.len() > MAX_SNIPPET_CHARS { &content[..MAX_SNIPPET_CHARS] } else { content };
                serde_json::json!({
                    "title": r.get("title").and_then(|v| v.as_str()).unwrap_or(""),
                    "url": r.get("url").and_then(|v| v.as_str()).unwrap_or(""),
                    "content": content,
                    "score": r.get("score").and_then(|v| v.as_f64()).unwrap_or(0.0),
                })
            }).collect();
            let total = formatted.len();
            result["results"] = Value::Array(formatted);
            result["total"] = serde_json::json!(total);
        }

        Ok(result)
    }

    async fn search_bing(api_key: &str, endpoint: &str, query: &str, max_results: u8) -> Result<Value, String> {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .unwrap_or_default();

        let response = client
            .get(endpoint)
            .header("Ocp-Apim-Subscription-Key", api_key)
            .query(&[
                ("q", query),
                ("count", &max_results.to_string()),
                ("textDecorations", "false"),
                ("textFormat", "Raw"),
                ("responseFilter", "Webpages"),
            ])
            .send()
            .await
            .map_err(|e| format!("Web search request failed: {e}"))?;

        let status = response.status().as_u16();
        let text = response.text().await.unwrap_or_default();
        tracing::debug!(status, response_len = text.len(), "Bing search response");
        if status != 200 {
            tracing::warn!(status, body = &text[..text.len().min(500)], "Bing API error");
            return Err(format!("Bing API returned status {status}: {text}"));
        }

        let raw: Value = serde_json::from_str(&text)
            .map_err(|e| format!("Failed to parse Bing response: {e}"))?;

        let mut result = serde_json::json!({ "query": query });

        if let Some(web_pages) = raw.get("webPages").and_then(|w| w.get("value")).and_then(|v| v.as_array()) {
            let formatted: Vec<Value> = web_pages.iter().map(|r| {
                let snippet = r.get("snippet").and_then(|v| v.as_str()).unwrap_or("");
                let content = if snippet.len() > MAX_SNIPPET_CHARS { &snippet[..MAX_SNIPPET_CHARS] } else { snippet };
                serde_json::json!({
                    "title": r.get("name").and_then(|v| v.as_str()).unwrap_or(""),
                    "url": r.get("url").and_then(|v| v.as_str()).unwrap_or(""),
                    "content": content,
                })
            }).collect();
            let total = formatted.len();
            result["results"] = Value::Array(formatted);
            result["total"] = serde_json::json!(total);
        }

        Ok(result)
    }
}

#[derive(Debug, Deserialize)]
struct WebSearchArgs {
    query: String,
    max_results: Option<u8>,
    search_depth: Option<String>,
    include_answer: Option<bool>,
}

#[async_trait::async_trait]
impl KernelFunction for WebSearchTool {
    fn definition(&self) -> FunctionDefinition {
        let mut def = FunctionDefinition::new("web_search")
            .with_description(
                "Search the web for current information. Returns relevant web page titles, URLs, and content snippets. \
                 Use this to answer questions about recent events, look up facts, find documentation, or research topics \
                 that require up-to-date information beyond your training data.",
            );

        def.add_parameter(
            FunctionParameter::new("query", serde_json::json!({ "type": "string" }))
                .with_description("The search query — be specific and descriptive for best results"),
        );

        def.add_parameter(
            FunctionParameter::new(
                "max_results",
                serde_json::json!({ "type": "integer", "minimum": 1, "maximum": 10 }),
            )
            .with_description("Maximum number of results to return (default: 5)")
            .optional(),
        );

        def.add_parameter(
            FunctionParameter::new(
                "search_depth",
                serde_json::json!({ "type": "string", "enum": ["basic", "advanced"] }),
            )
            .with_description("Search depth: 'basic' for quick results, 'advanced' for deeper research. Only applies to Tavily provider. (default: basic)")
            .optional(),
        );

        def.add_parameter(
            FunctionParameter::new(
                "include_answer",
                serde_json::json!({ "type": "boolean" }),
            )
            .with_description("If true, include a short AI-generated answer summary alongside results. Only applies to Tavily provider. (default: false)")
            .optional(),
        );

        def
    }

    async fn invoke(&self, arguments: &Value) -> Result<Value, denkwerk::LLMError> {
        let args: WebSearchArgs = serde_json::from_value(arguments.clone()).map_err(|e| {
            denkwerk::LLMError::InvalidFunctionArguments(format!(
                "Invalid arguments for web_search: {e}"
            ))
        })?;

        let max_results = args.max_results.unwrap_or(5).min(10);

        let result = match &self.provider {
            SearchProvider::Tavily { api_key } => {
                let search_depth = args.search_depth.as_deref().unwrap_or("basic");
                let include_answer = args.include_answer.unwrap_or(false);
                Self::search_tavily(api_key, &args.query, max_results, search_depth, include_answer).await
            }
            SearchProvider::Bing { api_key, endpoint } => {
                Self::search_bing(api_key, endpoint, &args.query, max_results).await
            }
        };

        match result {
            Ok(mut v) => {
                // Hard cap: drop results from the end until output fits the budget
                if let Some(results) = v.get("results").and_then(|r| r.as_array()).cloned() {
                    let mut kept = results;
                    while serde_json::to_string(&v).map(|s| s.len()).unwrap_or(0) > MAX_SEARCH_OUTPUT_BYTES && kept.len() > 1 {
                        kept.pop();
                        v["results"] = Value::Array(kept.clone());
                        v["total"] = serde_json::json!(kept.len());
                    }
                }
                Ok(v)
            }
            Err(e) => Ok(serde_json::json!({ "error": e })),
        }
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

// ── Create Skill Tool ────────────────────────────────────────────

/// A tool that lets agents create new skills directly in the database.
/// Only registered when the agent has the `skill-creator` skill linked.
pub struct CreateSkillTool {
    db: clawkson_db::Db,
    agent_id: Uuid,
}

impl CreateSkillTool {
    pub fn new(db: clawkson_db::Db, agent_id: Uuid) -> Self {
        Self { db, agent_id }
    }

    pub fn into_dyn(self) -> DynKernelFunction {
        Arc::new(self)
    }
}

#[derive(Debug, Deserialize)]
struct CreateSkillArgs {
    name: String,
    description: String,
    instructions: String,
    #[serde(default)]
    link_to_this_agent: bool,
}

#[async_trait::async_trait]
impl KernelFunction for CreateSkillTool {
    fn definition(&self) -> FunctionDefinition {
        let mut def = FunctionDefinition::new("create_skill")
            .with_description(
                "Create a new reusable skill and save it to the skills database. \
                 A skill teaches agents how to perform a specific task consistently. \
                 Only call this after the user has reviewed and approved the skill definition.",
            );

        def.add_parameter(
            FunctionParameter::new(
                "name",
                serde_json::json!({ "type": "string" }),
            )
            .with_description(
                "Lowercase, alphanumeric name with hyphens only, max 64 chars (e.g. 'meeting-notes', 'data-analyzer'). \
                 This becomes the /skill-name invocation command.",
            ),
        );

        def.add_parameter(
            FunctionParameter::new(
                "description",
                serde_json::json!({ "type": "string" }),
            )
            .with_description(
                "A concise one-line description (under 200 chars) of what the skill does. \
                 Used for routing — determines when the skill activates.",
            ),
        );

        def.add_parameter(
            FunctionParameter::new(
                "instructions",
                serde_json::json!({ "type": "string" }),
            )
            .with_description(
                "The full markdown instructions loaded when the skill is invoked. \
                 Should include workflow steps, output format, and guidelines.",
            ),
        );

        def.add_parameter(
            FunctionParameter::new(
                "link_to_this_agent",
                serde_json::json!({ "type": "boolean" }),
            )
            .with_description(
                "If true, automatically link the newly created skill to this agent so it becomes \
                 available immediately. Default: false.",
            )
            .optional(),
        );

        def
    }

    async fn invoke(&self, arguments: &Value) -> Result<Value, denkwerk::LLMError> {
        let args: CreateSkillArgs = serde_json::from_value(arguments.clone())
            .map_err(|e| denkwerk::LLMError::FunctionExecution {
                function: "create_skill".to_string(),
                message: format!("Invalid arguments: {e}"),
            })?;

        // Validate name format
        let name = args.name.trim().to_lowercase();
        if name.is_empty() || name.len() > 64 {
            return Ok(serde_json::json!({
                "error": "Name must be between 1 and 64 characters.",
            }));
        }
        if !name.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-') {
            return Ok(serde_json::json!({
                "error": "Name must contain only lowercase letters, numbers, and hyphens.",
            }));
        }

        let description = args.description.trim().to_string();
        let instructions = args.instructions.trim().to_string();

        if description.is_empty() {
            return Ok(serde_json::json!({
                "error": "Description cannot be empty.",
            }));
        }
        if instructions.is_empty() {
            return Ok(serde_json::json!({
                "error": "Instructions cannot be empty.",
            }));
        }

        // Check if a skill with this name already exists
        match clawkson_db::skill::get_by_name(&self.db, &name).await {
            Ok(Some(_)) => {
                return Ok(serde_json::json!({
                    "error": format!("A skill named '{name}' already exists. Choose a different name or ask the user if they want to update the existing skill."),
                }));
            }
            Ok(None) => {} // good, name is available
            Err(e) => {
                return Err(denkwerk::LLMError::FunctionExecution {
                    function: "create_skill".to_string(),
                    message: format!("Failed to check existing skills: {e}"),
                });
            }
        }

        let row = clawkson_db::skill::create(&self.db, &name, &description, &instructions)
            .await
            .map_err(|e| denkwerk::LLMError::FunctionExecution {
                function: "create_skill".to_string(),
                message: format!("Failed to create skill: {e}"),
            })?;

        // Optionally link to this agent
        if args.link_to_this_agent {
            if let Err(e) = clawkson_db::skill::agent_link(self.db.pool(), self.agent_id, row.id).await {
                tracing::warn!(skill = %name, "failed to auto-link new skill to agent: {e}");
            }
        }

        Ok(serde_json::json!({
            "success": true,
            "skill": {
                "id": row.id.to_string(),
                "name": row.name,
                "description": row.description,
            },
            "linked_to_agent": args.link_to_this_agent,
            "hint": "The skill can now be linked to any agent from the agent's settings page.",
        }))
    }
}
