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
/// Scoped to a specific conversation for workspace isolation,
/// or to the shared persistent container when `persistent` is true.
pub struct CodeExecutionTool {
    agent_id: Uuid,
    conversation_id: Uuid,
    container_manager: Arc<ContainerManager>,
    workspace_root: std::path::PathBuf,
    persistent: bool,
    /// Credential env vars injected into exec commands (CREDENTIAL_NAME=value).
    credential_env: std::collections::HashMap<String, String>,
}

impl CodeExecutionTool {
    pub fn new(agent_id: Uuid, conversation_id: Uuid, container_manager: Arc<ContainerManager>, workspace_root: std::path::PathBuf) -> Self {
        Self {
            agent_id,
            conversation_id,
            container_manager,
            workspace_root,
            persistent: false,
            credential_env: std::collections::HashMap::new(),
        }
    }

    pub fn with_persistent(mut self, persistent: bool) -> Self {
        self.persistent = persistent;
        self
    }

    pub fn with_credentials(mut self, env: std::collections::HashMap<String, String>) -> Self {
        self.credential_env = env;
        self
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
        let desc = if self.persistent {
            "Execute code REMOTELY in a PERSISTENT sandboxed Docker container shared across all \
             conversations for this agent. You are NOT inside the container — this tool sends your \
             code to a separate container and returns the output. \
             Use this for running Python or Bash code. \
             Proactively install any needed packages (pip install, apt-get install -y) \
             without asking — the container is PERSISTENT so installed packages survive across \
             conversations and server restarts. \
             The workspace at /workspace is shared across all conversations. \
             Always use /workspace for file operations — read inputs from /workspace/inputs/ \
             and write outputs to /workspace/outputs/. \
             After execution, any files written to /workspace/outputs/ are automatically \
             returned to you so you can read or summarise their contents."
        } else {
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
             returned to you so you can read or summarise their contents."
        };
        let mut def = FunctionDefinition::new("code_execution")
            .with_description(desc);

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

        // Inject credential env vars as exports prepended to the command
        let command = if self.credential_env.is_empty() {
            command
        } else {
            let exports: Vec<String> = self.credential_env.iter()
                .map(|(k, v)| format!("export {}={}", k, shell_escape(v)))
                .collect();
            format!("{}; {}", exports.join("; "), command)
        };

        let request = ExecRequest {
            command,
            timeout: Some(30),
            output_dir: Some("outputs".to_string()),
        };

        let exec_conv_id = if self.persistent { clawkson_container::PERSISTENT_SENTINEL } else { self.conversation_id };

        let exec_result = match self.container_manager.exec(self.agent_id, exec_conv_id, &request).await {
            Err(clawkson_container::ContainerError::NotFound(_)) => {
                // Container gone — try to auto-restart and retry once
                tracing::info!(agent_id = %self.agent_id, conversation_id = %self.conversation_id, "container not found, attempting auto-restart");
                let config = clawkson_container::ContainerConfig {
                    persistent: self.persistent,
                    ..clawkson_container::ContainerConfig::default()
                };
                if self.persistent {
                    if let Err(e) = self.container_manager.get_or_start_persistent(self.agent_id, &config).await {
                        return Ok(serde_json::json!({
                            "error": format!("Container lost and restart failed: {e}. Please try again."),
                        }));
                    }
                } else if let Err(e) = self.container_manager.start_container(self.agent_id, self.conversation_id, &config).await {
                    return Ok(serde_json::json!({
                        "error": format!("Container lost and restart failed: {e}. Please try again."),
                    }));
                }
                self.container_manager.exec(self.agent_id, exec_conv_id, &request).await
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
                        let workspace = if self.persistent {
                            self.workspace_root
                                .join(self.agent_id.to_string())
                                .join("shared")
                        } else {
                            self.workspace_root
                                .join(self.agent_id.to_string())
                                .join(self.conversation_id.to_string())
                        };
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

// ── Host Execution Tool ───────────────────────────────────────────

/// A tool that executes code directly on the host machine (no container).
/// **Dangerous** — no isolation. Only enabled when the agent is explicitly
/// configured with `execution_mode: host`.
pub struct HostExecutionTool {
    workspace_root: std::path::PathBuf,
    credential_env: std::collections::HashMap<String, String>,
}

impl HostExecutionTool {
    pub fn new(workspace_root: std::path::PathBuf) -> Self {
        Self {
            workspace_root,
            credential_env: std::collections::HashMap::new(),
        }
    }

    pub fn with_credentials(mut self, env: std::collections::HashMap<String, String>) -> Self {
        self.credential_env = env;
        self
    }

    pub fn into_dyn(self) -> DynKernelFunction {
        Arc::new(self)
    }
}

#[async_trait::async_trait]
impl KernelFunction for HostExecutionTool {
    fn definition(&self) -> FunctionDefinition {
        let mut def = FunctionDefinition::new("code_execution")
            .with_description(
                "Execute code DIRECTLY on the HOST MACHINE. \
                 ⚠️ WARNING: This runs WITHOUT any container sandbox — commands have full access \
                 to the host system, filesystem, and network. Use with extreme caution. \
                 Use this for running Python or Bash code. \
                 The working directory is the workspace at the path shown below. \
                 Read inputs from ./inputs/ and write outputs to ./outputs/. \
                 After execution, any files written to ./outputs/ are automatically \
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

        // Inject credential env vars as exports
        let command = if self.credential_env.is_empty() {
            command
        } else {
            let exports: Vec<String> = self.credential_env.iter()
                .map(|(k, v)| format!("export {}={}", k, shell_escape(v)))
                .collect();
            format!("{}; {}", exports.join("; "), command)
        };

        // Ensure workspace exists
        let workspace = &self.workspace_root;
        for dir in ["inputs", "outputs"] {
            let p = workspace.join(dir);
            tokio::fs::create_dir_all(&p).await.ok();
        }

        let output = tokio::time::timeout(
            std::time::Duration::from_secs(300),
            tokio::process::Command::new("bash")
                .arg("-c")
                .arg(&command)
                .current_dir(workspace)
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::piped())
                .output(),
        )
        .await;

        match output {
            Ok(Ok(out)) => {
                let stdout = String::from_utf8_lossy(&out.stdout);
                let stderr = String::from_utf8_lossy(&out.stderr);
                let exit_code = out.status.code().unwrap_or(-1);

                let mut response = serde_json::json!({
                    "stdout": &stdout[..stdout.len().min(65536)],
                    "stderr": &stderr[..stderr.len().min(65536)],
                    "exit_code": exit_code,
                    "timed_out": false,
                    "execution_mode": "host",
                });

                // Collect output files
                let output_dir = workspace.join("outputs");
                if output_dir.is_dir() {
                    if let Ok(files) = clawkson_container::workspace::collect_output_files(workspace, "outputs") {
                        if !files.is_empty() {
                            let files_json: Vec<Value> = files.iter().map(|f| {
                                let abs = workspace.join(&f.path);
                                let content = if f.size <= MAX_INLINE_FILE_BYTES {
                                    match std::fs::read(&abs) {
                                        Ok(bytes) => match std::str::from_utf8(&bytes) {
                                            Ok(text) => Value::String(text.to_string()),
                                            Err(_) => Value::String(format!("[binary file, {} bytes]", bytes.len())),
                                        },
                                        Err(e) => Value::String(format!("[read error: {e}]")),
                                    }
                                } else {
                                    Value::String(format!("[file too large to inline ({} KB)]", f.size / 1024))
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
                }

                Ok(response)
            }
            Ok(Err(e)) => Ok(serde_json::json!({
                "error": format!("Failed to execute command: {e}"),
                "execution_mode": "host",
            })),
            Err(_) => Ok(serde_json::json!({
                "stdout": "",
                "stderr": "Command timed out after 300 seconds",
                "exit_code": -1,
                "timed_out": true,
                "execution_mode": "host",
            })),
        }
    }
}

// ── SSH Execution Tool ────────────────────────────────────────────

/// A tool that executes code on a remote machine via SSH.
/// Requires SSH config (host, username, optional key credential).
pub struct SshExecutionTool {
    ssh_host: String,
    ssh_port: u16,
    ssh_user: String,
    /// Path to a temporary key file (written at tool construction time).
    ssh_key_path: Option<std::path::PathBuf>,
    working_directory: Option<String>,
    credential_env: std::collections::HashMap<String, String>,
}

impl SshExecutionTool {
    pub fn new(
        host: String,
        port: u16,
        user: String,
        key_path: Option<std::path::PathBuf>,
        working_directory: Option<String>,
    ) -> Self {
        Self {
            ssh_host: host,
            ssh_port: port,
            ssh_user: user,
            ssh_key_path: key_path,
            working_directory,
            credential_env: std::collections::HashMap::new(),
        }
    }

    pub fn with_credentials(mut self, env: std::collections::HashMap<String, String>) -> Self {
        self.credential_env = env;
        self
    }

    pub fn into_dyn(self) -> DynKernelFunction {
        Arc::new(self)
    }
}

#[async_trait::async_trait]
impl KernelFunction for SshExecutionTool {
    fn definition(&self) -> FunctionDefinition {
        let target = format!("{}@{}:{}", self.ssh_user, self.ssh_host, self.ssh_port);
        let mut def = FunctionDefinition::new("code_execution")
            .with_description(format!(
                "Execute code on a REMOTE machine via SSH ({target}). \
                 ⚠️ WARNING: Commands run on the remote host with the permissions of the SSH user. \
                 Use this for running Python or Bash code remotely. \
                 {cwd}\
                 Write outputs to ./outputs/ relative to the working directory if you need to produce files.",
                cwd = self.working_directory.as_ref()
                    .map(|d| format!("Working directory: {d}. "))
                    .unwrap_or_default(),
            ));

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
            .with_description("The code to execute on the remote machine"),
        );

        def
    }

    async fn invoke(&self, arguments: &Value) -> Result<Value, denkwerk::LLMError> {
        let args: CodeExecArgs = serde_json::from_value(arguments.clone()).map_err(|e| {
            denkwerk::LLMError::InvalidFunctionArguments(format!(
                "Invalid arguments for code_execution: {e}"
            ))
        })?;

        let remote_cmd = match args.language.as_str() {
            "python" => format!("python3 -c {}", shell_escape(&args.code)),
            "bash" => args.code.clone(),
            other => {
                return Ok(serde_json::json!({
                    "error": format!("Unsupported language: {other}. Use 'python' or 'bash'.")
                }));
            }
        };

        // Prepend credential exports + cd to working directory
        let mut preamble = Vec::new();
        for (k, v) in &self.credential_env {
            preamble.push(format!("export {}={}", k, shell_escape(v)));
        }
        if let Some(ref wd) = self.working_directory {
            preamble.push(format!("cd {} 2>/dev/null || true", shell_escape(wd)));
        }
        let full_cmd = if preamble.is_empty() {
            remote_cmd
        } else {
            format!("{}; {}", preamble.join("; "), remote_cmd)
        };

        // Build SSH command
        let mut cmd = tokio::process::Command::new("ssh");
        cmd.arg("-o").arg("StrictHostKeyChecking=accept-new")
            .arg("-o").arg("ConnectTimeout=10")
            .arg("-o").arg("BatchMode=yes")
            .arg("-p").arg(self.ssh_port.to_string());

        if let Some(ref key_path) = self.ssh_key_path {
            cmd.arg("-i").arg(key_path);
        }

        cmd.arg(format!("{}@{}", self.ssh_user, self.ssh_host))
            .arg("--")
            .arg(&full_cmd)
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped());

        let output = tokio::time::timeout(
            std::time::Duration::from_secs(300),
            cmd.output(),
        )
        .await;

        match output {
            Ok(Ok(out)) => {
                let stdout = String::from_utf8_lossy(&out.stdout);
                let stderr = String::from_utf8_lossy(&out.stderr);
                let exit_code = out.status.code().unwrap_or(-1);

                Ok(serde_json::json!({
                    "stdout": &stdout[..stdout.len().min(65536)],
                    "stderr": &stderr[..stderr.len().min(65536)],
                    "exit_code": exit_code,
                    "timed_out": false,
                    "execution_mode": "ssh",
                    "target": format!("{}@{}:{}", self.ssh_user, self.ssh_host, self.ssh_port),
                }))
            }
            Ok(Err(e)) => Ok(serde_json::json!({
                "error": format!("SSH execution failed: {e}. Make sure `ssh` is installed and the target is reachable."),
                "execution_mode": "ssh",
            })),
            Err(_) => Ok(serde_json::json!({
                "stdout": "",
                "stderr": "SSH command timed out after 300 seconds",
                "exit_code": -1,
                "timed_out": true,
                "execution_mode": "ssh",
            })),
        }
    }
}

// ── Workspace Read Tool ───────────────────────────────────────────

/// A tool that lets the LLM read a file from the conversation's workspace.
pub struct WorkspaceReadTool {
    agent_id: Uuid,
    conversation_id: Uuid,
    workspace_root: std::path::PathBuf,
    persistent: bool,
}

impl WorkspaceReadTool {
    pub fn new(agent_id: Uuid, conversation_id: Uuid, workspace_root: std::path::PathBuf) -> Self {
        Self { agent_id, conversation_id, workspace_root, persistent: false }
    }

    pub fn with_persistent(mut self, persistent: bool) -> Self {
        self.persistent = persistent;
        self
    }

    pub fn into_dyn(self) -> DynKernelFunction {
        Arc::new(self)
    }

    fn workspace_path(&self) -> std::path::PathBuf {
        if self.persistent {
            self.workspace_root.join(self.agent_id.to_string()).join("shared")
        } else {
            self.workspace_root.join(self.agent_id.to_string()).join(self.conversation_id.to_string())
        }
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

        let workspace = self.workspace_path();

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
    persistent: bool,
}

impl WorkspaceWriteTool {
    pub fn new(agent_id: Uuid, conversation_id: Uuid, workspace_root: std::path::PathBuf) -> Self {
        Self { agent_id, conversation_id, workspace_root, persistent: false }
    }

    pub fn with_persistent(mut self, persistent: bool) -> Self {
        self.persistent = persistent;
        self
    }

    pub fn into_dyn(self) -> DynKernelFunction {
        Arc::new(self)
    }

    fn workspace_path(&self) -> std::path::PathBuf {
        if self.persistent {
            self.workspace_root.join(self.agent_id.to_string()).join("shared")
        } else {
            self.workspace_root.join(self.agent_id.to_string()).join(self.conversation_id.to_string())
        }
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

        let workspace = self.workspace_path();

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
    persistent: bool,
}

impl WorkspaceListTool {
    pub fn new(agent_id: Uuid, conversation_id: Uuid, workspace_root: std::path::PathBuf) -> Self {
        Self { agent_id, conversation_id, workspace_root, persistent: false }
    }

    pub fn with_persistent(mut self, persistent: bool) -> Self {
        self.persistent = persistent;
        self
    }

    pub fn into_dyn(self) -> DynKernelFunction {
        Arc::new(self)
    }

    fn workspace_path(&self) -> std::path::PathBuf {
        if self.persistent {
            self.workspace_root.join(self.agent_id.to_string()).join("shared")
        } else {
            self.workspace_root.join(self.agent_id.to_string()).join(self.conversation_id.to_string())
        }
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

        let workspace = self.workspace_path();
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

// ── Knowledge Create Tool ─────────────────────────────────────────

/// A tool that lets agents create new knowledge bases dynamically.
pub struct KnowledgeCreateTool {
    agent_id: Uuid,
    user_id: Uuid,
    db: Db,
}

impl KnowledgeCreateTool {
    pub fn new(agent_id: Uuid, user_id: Uuid, db: Db) -> Self {
        Self { agent_id, user_id, db }
    }

    pub fn into_dyn(self) -> DynKernelFunction {
        Arc::new(self)
    }
}

#[derive(Debug, Deserialize)]
struct KnowledgeCreateArgs {
    name: String,
    description: String,
}

#[async_trait::async_trait]
impl KernelFunction for KnowledgeCreateTool {
    fn definition(&self) -> FunctionDefinition {
        let mut def = FunctionDefinition::new("knowledge_create")
            .with_description(
                "Create a new knowledge base to store and organise information on a topic. \
                 The knowledge base is automatically linked to you (this agent). \
                 Use this when you want to build a persistent collection of notes, research, \
                 or reference material that you can search later with knowledge_search.",
            );

        def.add_parameter(
            FunctionParameter::new("name", serde_json::json!({ "type": "string" }))
                .with_description("A short, descriptive name for the knowledge base (e.g. 'Competitor Analysis', 'Project Requirements')"),
        );

        def.add_parameter(
            FunctionParameter::new("description", serde_json::json!({ "type": "string" }))
                .with_description("A brief description of what this knowledge base contains"),
        );

        def
    }

    async fn invoke(&self, arguments: &Value) -> Result<Value, denkwerk::LLMError> {
        let args: KnowledgeCreateArgs =
            serde_json::from_value(arguments.clone()).map_err(|e| {
                denkwerk::LLMError::InvalidFunctionArguments(format!(
                    "Invalid arguments for knowledge_create: {e}"
                ))
            })?;

        // Load embedding model from settings
        let model = match clawkson_db::settings::get(&self.db).await {
            Ok(s) => s.embedding_model,
            Err(_) => "qwen3-embedding:8b".to_string(),
        };

        let kb = clawkson_db::knowledge_base::create_for_agent(
            self.db.pool(),
            self.agent_id,
            self.user_id,
            &args.name,
            &args.description,
            &model,
        )
        .await
        .map_err(|e| denkwerk::LLMError::FunctionExecution {
            function: "knowledge_create".to_string(),
            message: format!("Failed to create knowledge base: {e}"),
        })?;

        Ok(serde_json::json!({
            "id": kb.id.to_string(),
            "name": kb.name,
            "description": kb.description,
            "message": format!("Knowledge base '{}' created and linked to this agent. Use knowledge_add to add entries.", kb.name),
        }))
    }
}

// ── Knowledge Add Tool ───────────────────────────────────────────

/// A tool that lets agents add entries to a knowledge base and embed them immediately.
pub struct KnowledgeAddTool {
    agent_id: Uuid,
    db: Db,
}

impl KnowledgeAddTool {
    pub fn new(agent_id: Uuid, db: Db) -> Self {
        Self { agent_id, db }
    }

    pub fn into_dyn(self) -> DynKernelFunction {
        Arc::new(self)
    }
}

#[derive(Debug, Deserialize)]
struct KnowledgeAddArgs {
    knowledge_base_id: String,
    title: String,
    content: String,
}

#[async_trait::async_trait]
impl KernelFunction for KnowledgeAddTool {
    fn definition(&self) -> FunctionDefinition {
        let mut def = FunctionDefinition::new("knowledge_add")
            .with_description(
                "Add a new entry to a knowledge base and generate its embedding immediately so it \
                 becomes searchable right away. Use this to save important information, notes, \
                 summaries, or findings that you or other agents should be able to search later.",
            );

        def.add_parameter(
            FunctionParameter::new("knowledge_base_id", serde_json::json!({ "type": "string" }))
                .with_description("The ID of the knowledge base to add the entry to (from knowledge_list or knowledge_create)"),
        );

        def.add_parameter(
            FunctionParameter::new("title", serde_json::json!({ "type": "string" }))
                .with_description("A concise title for this entry"),
        );

        def.add_parameter(
            FunctionParameter::new("content", serde_json::json!({ "type": "string" }))
                .with_description("The full text content to store"),
        );

        def
    }

    async fn invoke(&self, arguments: &Value) -> Result<Value, denkwerk::LLMError> {
        let args: KnowledgeAddArgs =
            serde_json::from_value(arguments.clone()).map_err(|e| {
                denkwerk::LLMError::InvalidFunctionArguments(format!(
                    "Invalid arguments for knowledge_add: {e}"
                ))
            })?;

        let kb_id = Uuid::parse_str(&args.knowledge_base_id).map_err(|e| {
            denkwerk::LLMError::InvalidFunctionArguments(format!(
                "Invalid knowledge_base_id: {e}"
            ))
        })?;

        let pool = self.db.pool();

        // Security check: verify KB is linked to this agent
        let is_linked = clawkson_db::knowledge_base::is_linked_to_agent(pool, kb_id, self.agent_id)
            .await
            .map_err(|e| denkwerk::LLMError::FunctionExecution {
                function: "knowledge_add".to_string(),
                message: format!("Failed to verify KB access: {e}"),
            })?;

        // Also allow if the KB has this agent's agent_id (memory KB or agent-created)
        let kb = clawkson_db::knowledge_base::get_by_id(pool, kb_id)
            .await
            .map_err(|e| denkwerk::LLMError::FunctionExecution {
                function: "knowledge_add".to_string(),
                message: format!("Failed to fetch KB: {e}"),
            })?
            .ok_or_else(|| denkwerk::LLMError::FunctionExecution {
                function: "knowledge_add".to_string(),
                message: "Knowledge base not found".to_string(),
            })?;

        let owns_kb = kb.agent_id == Some(self.agent_id);

        if !is_linked && !owns_kb {
            return Ok(serde_json::json!({
                "error": "This knowledge base is not linked to this agent. Use knowledge_list to see available KBs.",
            }));
        }

        // Create the entry
        let entry = clawkson_db::knowledge_entry::create(pool, kb_id, &args.title, &args.content, None)
            .await
            .map_err(|e| denkwerk::LLMError::FunctionExecution {
                function: "knowledge_add".to_string(),
                message: format!("Failed to create entry: {e}"),
            })?;

        // Generate and store embedding inline so it's immediately searchable
        let embed_result = async {
            let settings = clawkson_db::settings::get(&self.db).await.ok()?;
            let embed_config = crate::embeddings::EmbeddingConfig {
                base_url: settings.embedding_api_base_url,
                api_key: settings.embedding_api_key,
                model: settings.embedding_model,
            };
            let text = format!("{}\n\n{}", args.title, args.content);
            let embedding = crate::embeddings::generate_one(&embed_config, &kb.embedding_model, &text).await.ok()?;
            clawkson_db::knowledge_entry::set_embedding(pool, entry.id, &embedding, None).await.ok()?;
            Some(())
        }.await;

        let embedded = embed_result.is_some();

        Ok(serde_json::json!({
            "entry_id": entry.id.to_string(),
            "title": entry.title,
            "embedded": embedded,
            "message": if embedded {
                "Entry created and embedded — it is now searchable via knowledge_search."
            } else {
                "Entry created but embedding failed. It will be embedded later."
            },
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

    /// Trait for resolving credential values at invocation time.
    #[async_trait::async_trait]
    pub trait CredentialResolver: Send + Sync {
        async fn resolve(&self, credential_name: &str) -> Option<clawkson_db::credential::CredentialRow>;
    }

    /// A tool that makes authenticated HTTP requests using connector credentials.
    pub struct AuthenticatedHttpTool {
        connectors: Vec<ConnectorAuth>,
        credential_resolver: Option<Arc<dyn CredentialResolver>>,
    }

    impl AuthenticatedHttpTool {
        pub fn new(connectors: Vec<ConnectorAuth>) -> Self {
            Self { connectors, credential_resolver: None }
        }

        pub fn with_credential_resolver(mut self, resolver: Arc<dyn CredentialResolver>) -> Self {
            self.credential_resolver = Some(resolver);
            self
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
        /// Optional credential name to use for auth instead of/in addition to connector.
        #[serde(default)]
        credential_name: Option<String>,
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

            def.add_parameter(
                FunctionParameter::new(
                    "credential_name",
                    serde_json::json!({ "type": "string" }),
                )
                .with_description("Optional: name of a credential from <available-credentials> to use for authentication instead of connector auth. The credential value is injected automatically as a Bearer token.")
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

            // Apply credential-based auth if credential_name is provided
            let mut credential_applied = false;
            if let Some(ref cred_name) = args.credential_name {
                if let Some(ref resolver) = self.credential_resolver {
                    if let Some(cred) = resolver.resolve(cred_name).await {
                        match cred.credential_type.as_str() {
                            "header" => {
                                // Header type: use metadata for header_name + value
                                let header_name = cred.metadata.get("header_name")
                                    .and_then(|v| v.as_str())
                                    .unwrap_or("Authorization");
                                builder = builder.header(header_name, &cred.encrypted_value);
                            }
                            _ => {
                                // api_key, token, password, secret — use as Bearer token
                                builder = builder.bearer_auth(&cred.encrypted_value);
                            }
                        }
                        credential_applied = true;
                    } else {
                        return Ok(serde_json::json!({
                            "error": format!("Credential '{}' not found or not linked to this agent", cred_name)
                        }));
                    }
                }
            }

            // Apply connector auth (fallback if no credential was applied)
            if !credential_applied {
                builder = self.apply_auth(auth, builder);
            }

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

// ── ManageScheduledTasksTool ────────────────────────────────────────

/// A tool that lets agents create, list, update, enable/disable, and delete scheduled tasks.
pub struct ManageScheduledTasksTool {
    db: clawkson_db::Db,
    agent_id: Uuid,
    conversation_id: Uuid,
    owner_id: Uuid,
}

impl ManageScheduledTasksTool {
    pub fn new(db: clawkson_db::Db, agent_id: Uuid, conversation_id: Uuid, owner_id: Uuid) -> Self {
        Self { db, agent_id, conversation_id, owner_id }
    }

    pub fn into_dyn(self) -> DynKernelFunction {
        Arc::new(self)
    }
}

#[derive(Debug, Deserialize)]
struct ManageScheduledTasksArgs {
    action: String,
    name: Option<String>,
    agent_id: Option<String>,
    prompt: Option<String>,
    cron_expression: Option<String>,
    task_id: Option<String>,
    enabled: Option<bool>,
}

#[async_trait::async_trait]
impl KernelFunction for ManageScheduledTasksTool {
    fn definition(&self) -> FunctionDefinition {
        let mut def = FunctionDefinition::new("manage_scheduled_tasks")
            .with_description(
                "Create, list, update, enable, disable, or delete scheduled tasks. \
                 This is the platform's built-in scheduler — use this instead of cron, \
                 systemd timers, CI/CD pipelines, or any external scheduling system. \
                 Each task runs an agent with a prompt on a cron schedule. \
                 ALWAYS use this tool when the user asks for recurring, scheduled, \
                 or periodic automation.",
            );

        def.add_parameter(
            FunctionParameter::new(
                "action",
                serde_json::json!({
                    "type": "string",
                    "enum": ["create", "list", "update", "enable", "disable", "delete"]
                }),
            )
            .with_description("The action to perform."),
        );

        def.add_parameter(
            FunctionParameter::new(
                "name",
                serde_json::json!({ "type": "string" }),
            )
            .with_description("Task name (required for create).")
            .optional(),
        );

        def.add_parameter(
            FunctionParameter::new(
                "agent_id",
                serde_json::json!({ "type": "string" }),
            )
            .with_description("UUID of the agent that should run the task. Defaults to the current agent (for create).")
            .optional(),
        );

        def.add_parameter(
            FunctionParameter::new(
                "prompt",
                serde_json::json!({ "type": "string" }),
            )
            .with_description("The prompt text for the task. Can include /skill-name references (for create/update).")
            .optional(),
        );

        def.add_parameter(
            FunctionParameter::new(
                "cron_expression",
                serde_json::json!({ "type": "string" }),
            )
            .with_description(
                "Standard 7-field cron expression: sec min hour day month weekday year. \
                 Examples: '0 0 22 * * * *' (daily at 22:00), '0 0 9 * * MON *' (Monday 9 AM). \
                 Required for create if you want recurring execution.",
            )
            .optional(),
        );

        def.add_parameter(
            FunctionParameter::new(
                "task_id",
                serde_json::json!({ "type": "string" }),
            )
            .with_description("UUID of the task (required for update/enable/disable/delete).")
            .optional(),
        );

        def.add_parameter(
            FunctionParameter::new(
                "enabled",
                serde_json::json!({ "type": "boolean" }),
            )
            .with_description("Set enabled state (for update).")
            .optional(),
        );

        def
    }

    async fn invoke(&self, arguments: &Value) -> Result<Value, denkwerk::LLMError> {
        let args: ManageScheduledTasksArgs = serde_json::from_value(arguments.clone())
            .map_err(|e| denkwerk::LLMError::FunctionExecution {
                function: "manage_scheduled_tasks".to_string(),
                message: format!("Invalid arguments: {e}"),
            })?;

        match args.action.as_str() {
            "list" => {
                let rows = clawkson_db::scheduled_task::list_for_user(&self.db, self.owner_id)
                    .await
                    .map_err(|e| denkwerk::LLMError::FunctionExecution {
                        function: "manage_scheduled_tasks".to_string(),
                        message: format!("Failed to list tasks: {e}"),
                    })?;

                let tasks: Vec<Value> = rows.iter().map(|r| serde_json::json!({
                    "id": r.id.to_string(),
                    "name": r.name,
                    "agent_id": r.agent_id.to_string(),
                    "prompt": r.prompt,
                    "cron_expression": r.cron_expression,
                    "enabled": r.enabled,
                    "last_run_at": r.last_run_at.map(|t| t.to_rfc3339()),
                    "next_run_at": r.next_run_at.map(|t| t.to_rfc3339()),
                })).collect();

                Ok(serde_json::json!({ "tasks": tasks, "count": tasks.len() }))
            }

            "create" => {
                let name = args.name.as_deref().unwrap_or("").trim();
                if name.is_empty() {
                    return Ok(serde_json::json!({ "error": "name is required for create action." }));
                }
                let prompt = args.prompt.as_deref().unwrap_or("").trim();
                if prompt.is_empty() {
                    return Ok(serde_json::json!({ "error": "prompt is required for create action." }));
                }

                let target_agent_id = if let Some(ref aid) = args.agent_id {
                    Uuid::parse_str(aid).map_err(|_| denkwerk::LLMError::FunctionExecution {
                        function: "manage_scheduled_tasks".to_string(),
                        message: "Invalid agent_id UUID.".to_string(),
                    })?
                } else {
                    self.agent_id
                };

                // Validate cron expression if provided
                let next_run = if let Some(ref expr) = args.cron_expression {
                    let next = crate::scheduler::compute_next_run(Some(expr));
                    if next.is_none() {
                        return Ok(serde_json::json!({
                            "error": format!("Invalid cron expression: '{expr}'. Use 7-field format: sec min hour day month weekday year."),
                        }));
                    }
                    next
                } else {
                    None
                };

                let row = clawkson_db::scheduled_task::create_with_provenance(
                    &self.db,
                    self.owner_id,
                    target_agent_id,
                    name,
                    prompt,
                    args.cron_expression.as_deref(),
                    next_run,
                    Some(self.agent_id),
                    Some(self.conversation_id),
                )
                .await
                .map_err(|e| denkwerk::LLMError::FunctionExecution {
                    function: "manage_scheduled_tasks".to_string(),
                    message: format!("Failed to create task: {e}"),
                })?;

                Ok(serde_json::json!({
                    "success": true,
                    "task": {
                        "id": row.id.to_string(),
                        "name": row.name,
                        "cron_expression": row.cron_expression,
                        "enabled": row.enabled,
                        "next_run_at": row.next_run_at.map(|t| t.to_rfc3339()),
                    },
                }))
            }

            "update" => {
                let task_id = parse_task_id(&args.task_id)?;

                // Ownership check
                let existing = clawkson_db::scheduled_task::get_by_id(&self.db, task_id)
                    .await
                    .map_err(|e| denkwerk::LLMError::FunctionExecution {
                        function: "manage_scheduled_tasks".to_string(),
                        message: format!("DB error: {e}"),
                    })?;
                let Some(existing) = existing else {
                    return Ok(serde_json::json!({ "error": "Task not found." }));
                };
                if existing.owner_id != self.owner_id {
                    return Ok(serde_json::json!({ "error": "You do not own this task." }));
                }

                // Recompute next_run if cron changed
                let cron_opt = args.cron_expression.as_ref().map(|c| {
                    if c.is_empty() { None } else { Some(c.as_str()) }
                });
                let next_run_update = cron_opt.map(|c| {
                    if let Some(expr) = c {
                        crate::scheduler::compute_next_run(Some(expr))
                    } else {
                        None
                    }
                });

                let row = clawkson_db::scheduled_task::update(
                    &self.db,
                    task_id,
                    args.name.as_deref(),
                    args.prompt.as_deref(),
                    cron_opt,
                    args.enabled,
                    next_run_update,
                )
                .await
                .map_err(|e| denkwerk::LLMError::FunctionExecution {
                    function: "manage_scheduled_tasks".to_string(),
                    message: format!("Failed to update task: {e}"),
                })?;

                match row {
                    Some(r) => Ok(serde_json::json!({
                        "success": true,
                        "task": {
                            "id": r.id.to_string(),
                            "name": r.name,
                            "enabled": r.enabled,
                            "cron_expression": r.cron_expression,
                        },
                    })),
                    None => Ok(serde_json::json!({ "error": "Task not found." })),
                }
            }

            "enable" | "disable" => {
                let task_id = parse_task_id(&args.task_id)?;
                let enable = args.action == "enable";

                let existing = clawkson_db::scheduled_task::get_by_id(&self.db, task_id)
                    .await
                    .map_err(|e| denkwerk::LLMError::FunctionExecution {
                        function: "manage_scheduled_tasks".to_string(),
                        message: format!("DB error: {e}"),
                    })?;
                let Some(existing) = existing else {
                    return Ok(serde_json::json!({ "error": "Task not found." }));
                };
                if existing.owner_id != self.owner_id {
                    return Ok(serde_json::json!({ "error": "You do not own this task." }));
                }

                // Recompute next_run when enabling
                let next_run_update = if enable {
                    Some(crate::scheduler::compute_next_run(existing.cron_expression.as_deref()))
                } else {
                    Some(None)
                };

                let row = clawkson_db::scheduled_task::update(
                    &self.db,
                    task_id,
                    None,
                    None,
                    None,
                    Some(enable),
                    next_run_update,
                )
                .await
                .map_err(|e| denkwerk::LLMError::FunctionExecution {
                    function: "manage_scheduled_tasks".to_string(),
                    message: format!("Failed to update task: {e}"),
                })?;

                match row {
                    Some(r) => Ok(serde_json::json!({
                        "success": true,
                        "task_id": r.id.to_string(),
                        "enabled": r.enabled,
                    })),
                    None => Ok(serde_json::json!({ "error": "Task not found." })),
                }
            }

            "delete" => {
                let task_id = parse_task_id(&args.task_id)?;

                let existing = clawkson_db::scheduled_task::get_by_id(&self.db, task_id)
                    .await
                    .map_err(|e| denkwerk::LLMError::FunctionExecution {
                        function: "manage_scheduled_tasks".to_string(),
                        message: format!("DB error: {e}"),
                    })?;
                let Some(existing) = existing else {
                    return Ok(serde_json::json!({ "error": "Task not found." }));
                };
                if existing.owner_id != self.owner_id {
                    return Ok(serde_json::json!({ "error": "You do not own this task." }));
                }

                clawkson_db::scheduled_task::delete(&self.db, task_id)
                    .await
                    .map_err(|e| denkwerk::LLMError::FunctionExecution {
                        function: "manage_scheduled_tasks".to_string(),
                        message: format!("Failed to delete task: {e}"),
                    })?;

                Ok(serde_json::json!({ "success": true, "deleted": task_id.to_string() }))
            }

            other => Ok(serde_json::json!({
                "error": format!("Unknown action '{other}'. Valid actions: create, list, update, enable, disable, delete."),
            })),
        }
    }
}

fn parse_task_id(task_id: &Option<String>) -> Result<Uuid, denkwerk::LLMError> {
    let id_str = task_id.as_deref().unwrap_or("");
    Uuid::parse_str(id_str).map_err(|_| denkwerk::LLMError::FunctionExecution {
        function: "manage_scheduled_tasks".to_string(),
        message: "task_id is required and must be a valid UUID.".to_string(),
    })
}

// ── ManageCalendarTool ──────────────────────────────────────────────

/// A tool that lets agents create, list, update, and delete calendar events.
pub struct ManageCalendarTool {
    db: clawkson_db::Db,
    owner_id: Uuid,
}

impl ManageCalendarTool {
    pub fn new(db: clawkson_db::Db, owner_id: Uuid) -> Self {
        Self { db, owner_id }
    }

    pub fn into_dyn(self) -> DynKernelFunction {
        Arc::new(self)
    }
}

#[derive(Debug, Deserialize)]
struct ManageCalendarArgs {
    action: String,
    title: Option<String>,
    date: Option<String>,
    start_time: Option<String>,
    end_time: Option<String>,
    category: Option<String>,
    location: Option<String>,
    notes: Option<String>,
    event_id: Option<String>,
    from_date: Option<String>,
    to_date: Option<String>,
}

#[async_trait::async_trait]
impl KernelFunction for ManageCalendarTool {
    fn definition(&self) -> FunctionDefinition {
        let mut def = FunctionDefinition::new("manage_calendar")
            .with_description(
                "Create, list, update, or delete calendar events on the user's built-in calendar. \
                 Use this to schedule reminders, add workflow markers, or manage calendar entries. \
                 This is the platform's calendar — do not suggest Google Calendar, Outlook, or \
                 external calendar tools.",
            );

        def.add_parameter(
            FunctionParameter::new(
                "action",
                serde_json::json!({
                    "type": "string",
                    "enum": ["create", "list", "update", "delete"]
                }),
            )
            .with_description("The action to perform."),
        );

        def.add_parameter(
            FunctionParameter::new("title", serde_json::json!({ "type": "string" }))
            .with_description("Event title (required for create).")
            .optional(),
        );

        def.add_parameter(
            FunctionParameter::new("date", serde_json::json!({ "type": "string" }))
            .with_description("Date in YYYY-MM-DD format (required for create).")
            .optional(),
        );

        def.add_parameter(
            FunctionParameter::new("start_time", serde_json::json!({ "type": "string" }))
            .with_description("Start time in HH:MM format (required for create).")
            .optional(),
        );

        def.add_parameter(
            FunctionParameter::new("end_time", serde_json::json!({ "type": "string" }))
            .with_description("End time in HH:MM format (required for create).")
            .optional(),
        );

        def.add_parameter(
            FunctionParameter::new("category", serde_json::json!({ "type": "string" }))
            .with_description("Event category: work, personal, meeting, health, travel, creative. Default: work.")
            .optional(),
        );

        def.add_parameter(
            FunctionParameter::new("location", serde_json::json!({ "type": "string" }))
            .with_description("Event location (optional).")
            .optional(),
        );

        def.add_parameter(
            FunctionParameter::new("notes", serde_json::json!({ "type": "string" }))
            .with_description("Event notes (optional).")
            .optional(),
        );

        def.add_parameter(
            FunctionParameter::new("event_id", serde_json::json!({ "type": "string" }))
            .with_description("UUID of the event (required for update/delete).")
            .optional(),
        );

        def.add_parameter(
            FunctionParameter::new("from_date", serde_json::json!({ "type": "string" }))
            .with_description("Start of date range filter for list (YYYY-MM-DD). Defaults to today.")
            .optional(),
        );

        def.add_parameter(
            FunctionParameter::new("to_date", serde_json::json!({ "type": "string" }))
            .with_description("End of date range filter for list (YYYY-MM-DD). Defaults to 30 days from today.")
            .optional(),
        );

        def
    }

    async fn invoke(&self, arguments: &Value) -> Result<Value, denkwerk::LLMError> {
        use chrono::{NaiveDate, NaiveTime};

        let args: ManageCalendarArgs = serde_json::from_value(arguments.clone())
            .map_err(|e| denkwerk::LLMError::FunctionExecution {
                function: "manage_calendar".to_string(),
                message: format!("Invalid arguments: {e}"),
            })?;

        match args.action.as_str() {
            "list" => {
                let today = chrono::Utc::now().date_naive();
                let from = args.from_date.as_deref()
                    .and_then(|s| NaiveDate::parse_from_str(s, "%Y-%m-%d").ok())
                    .unwrap_or(today);
                let to = args.to_date.as_deref()
                    .and_then(|s| NaiveDate::parse_from_str(s, "%Y-%m-%d").ok())
                    .unwrap_or(today + chrono::Duration::days(30));

                let rows = clawkson_db::calendar_event::list_for_user(&self.db, self.owner_id, from, to)
                    .await
                    .map_err(|e| denkwerk::LLMError::FunctionExecution {
                        function: "manage_calendar".to_string(),
                        message: format!("Failed to list events: {e}"),
                    })?;

                let events: Vec<Value> = rows.iter().map(|r| serde_json::json!({
                    "id": r.id.to_string(),
                    "title": r.title,
                    "date": r.date.to_string(),
                    "start_time": r.start_time.format("%H:%M").to_string(),
                    "end_time": r.end_time.format("%H:%M").to_string(),
                    "category": r.category,
                    "location": r.location,
                    "notes": r.notes,
                    "completed": r.completed,
                })).collect();

                Ok(serde_json::json!({ "events": events, "count": events.len() }))
            }

            "create" => {
                let title = args.title.as_deref().unwrap_or("").trim();
                if title.is_empty() {
                    return Ok(serde_json::json!({ "error": "title is required for create." }));
                }

                let date = parse_date(&args.date, "date")?;
                let start_time = parse_time(&args.start_time, "start_time")?;
                let end_time = parse_time(&args.end_time, "end_time")?;
                let category = args.category.as_deref().unwrap_or("work");

                let row = clawkson_db::calendar_event::create(
                    &self.db,
                    self.owner_id,
                    title,
                    date,
                    start_time,
                    end_time,
                    category,
                    args.location.as_deref(),
                    args.notes.as_deref(),
                )
                .await
                .map_err(|e| denkwerk::LLMError::FunctionExecution {
                    function: "manage_calendar".to_string(),
                    message: format!("Failed to create event: {e}"),
                })?;

                Ok(serde_json::json!({
                    "success": true,
                    "event": {
                        "id": row.id.to_string(),
                        "title": row.title,
                        "date": row.date.to_string(),
                        "start_time": row.start_time.format("%H:%M").to_string(),
                        "end_time": row.end_time.format("%H:%M").to_string(),
                        "category": row.category,
                    },
                }))
            }

            "update" => {
                let event_id = parse_event_id(&args.event_id)?;

                let existing = clawkson_db::calendar_event::get_by_id(&self.db, event_id)
                    .await
                    .map_err(|e| denkwerk::LLMError::FunctionExecution {
                        function: "manage_calendar".to_string(),
                        message: format!("DB error: {e}"),
                    })?;
                let Some(existing) = existing else {
                    return Ok(serde_json::json!({ "error": "Event not found." }));
                };
                if existing.owner_id != self.owner_id {
                    return Ok(serde_json::json!({ "error": "You do not own this event." }));
                }

                let title = args.title.as_deref().unwrap_or(&existing.title);
                let date = args.date.as_deref()
                    .and_then(|s| NaiveDate::parse_from_str(s, "%Y-%m-%d").ok())
                    .unwrap_or(existing.date);
                let start_time = args.start_time.as_deref()
                    .and_then(|s| NaiveTime::parse_from_str(s, "%H:%M").ok())
                    .unwrap_or(existing.start_time);
                let end_time = args.end_time.as_deref()
                    .and_then(|s| NaiveTime::parse_from_str(s, "%H:%M").ok())
                    .unwrap_or(existing.end_time);
                let category = args.category.as_deref().unwrap_or(&existing.category);

                let row = clawkson_db::calendar_event::update(
                    &self.db,
                    event_id,
                    title,
                    date,
                    start_time,
                    end_time,
                    category,
                    args.location.as_deref().or(existing.location.as_deref()),
                    args.notes.as_deref().or(existing.notes.as_deref()),
                    existing.completed,
                )
                .await
                .map_err(|e| denkwerk::LLMError::FunctionExecution {
                    function: "manage_calendar".to_string(),
                    message: format!("Failed to update event: {e}"),
                })?;

                match row {
                    Some(r) => Ok(serde_json::json!({
                        "success": true,
                        "event": {
                            "id": r.id.to_string(),
                            "title": r.title,
                            "date": r.date.to_string(),
                        },
                    })),
                    None => Ok(serde_json::json!({ "error": "Event not found." })),
                }
            }

            "delete" => {
                let event_id = parse_event_id(&args.event_id)?;

                let existing = clawkson_db::calendar_event::get_by_id(&self.db, event_id)
                    .await
                    .map_err(|e| denkwerk::LLMError::FunctionExecution {
                        function: "manage_calendar".to_string(),
                        message: format!("DB error: {e}"),
                    })?;
                let Some(existing) = existing else {
                    return Ok(serde_json::json!({ "error": "Event not found." }));
                };
                if existing.owner_id != self.owner_id {
                    return Ok(serde_json::json!({ "error": "You do not own this event." }));
                }

                clawkson_db::calendar_event::delete(&self.db, event_id)
                    .await
                    .map_err(|e| denkwerk::LLMError::FunctionExecution {
                        function: "manage_calendar".to_string(),
                        message: format!("Failed to delete event: {e}"),
                    })?;

                Ok(serde_json::json!({ "success": true, "deleted": event_id.to_string() }))
            }

            other => Ok(serde_json::json!({
                "error": format!("Unknown action '{other}'. Valid actions: create, list, update, delete."),
            })),
        }
    }
}

fn parse_date(val: &Option<String>, field: &str) -> Result<chrono::NaiveDate, denkwerk::LLMError> {
    let s = val.as_deref().unwrap_or("");
    chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d").map_err(|_| denkwerk::LLMError::FunctionExecution {
        function: "manage_calendar".to_string(),
        message: format!("{field} is required and must be in YYYY-MM-DD format."),
    })
}

fn parse_time(val: &Option<String>, field: &str) -> Result<chrono::NaiveTime, denkwerk::LLMError> {
    let s = val.as_deref().unwrap_or("");
    chrono::NaiveTime::parse_from_str(s, "%H:%M").map_err(|_| denkwerk::LLMError::FunctionExecution {
        function: "manage_calendar".to_string(),
        message: format!("{field} is required and must be in HH:MM format."),
    })
}

fn parse_event_id(event_id: &Option<String>) -> Result<Uuid, denkwerk::LLMError> {
    let id_str = event_id.as_deref().unwrap_or("");
    Uuid::parse_str(id_str).map_err(|_| denkwerk::LLMError::FunctionExecution {
        function: "manage_calendar".to_string(),
        message: "event_id is required and must be a valid UUID.".to_string(),
    })
}
