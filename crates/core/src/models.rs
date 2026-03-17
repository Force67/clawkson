use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

// ── Agent ──────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Agent {
    pub id: Uuid,
    pub name: String,
    pub description: String,
    pub status: AgentStatus,
    /// Which LLM connector this agent uses. Falls back to the default connector in Settings.
    pub llm_connector_id: Option<Uuid>,
    /// System prompt prepended to every conversation with this agent.
    pub system_prompt: Option<String>,
    /// Sampling temperature (0.0–2.0). None uses the provider default.
    pub temperature: Option<f64>,
    /// Maximum tokens in the response. None uses the provider default.
    pub max_tokens: Option<u32>,
    /// Whether this agent has sandbox (container) support enabled.
    #[serde(default)]
    pub container_enabled: bool,
    /// Optional container resource configuration.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub container_config: Option<AgentContainerConfig>,
    /// Connector proxy policies — controls which HTTP requests the agent can
    /// make through each connector. Stored as JSON in the DB.
    #[serde(default)]
    pub connector_policies: Vec<ConnectorPolicy>,
    /// Optional LLM connector for sub-task execution via delegate_tasks.
    /// When set, sub-agents use this (potentially cheaper/faster) model instead of the primary one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub subtask_llm_connector_id: Option<Uuid>,
    /// The user who owns this agent. `None` for legacy agents with no owner.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub owner_id: Option<Uuid>,
    /// Whether this agent is visible to all users.
    #[serde(default)]
    pub shared: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Per-agent container resource limits.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentContainerConfig {
    /// Docker image to use for this agent's container.
    /// Defaults to "python:3.12-slim" if not set.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub image: Option<String>,
    /// CPU limit in cores (e.g. 1.0).
    pub cpu_limit: Option<f64>,
    /// Memory limit in megabytes (e.g. 512).
    pub memory_limit_mb: Option<u64>,
    /// Whether networking is enabled (default: false).
    /// Kept for backward compat; prefer `permissions.network.enabled`.
    #[serde(default)]
    pub network_enabled: bool,
    /// Android-style granular permissions. Defaults applied when absent.
    #[serde(default)]
    pub permissions: AgentPermissions,
}

// ── Android-Style Permissions ─────────────────────────────────────

/// Top-level permission groups for agent sandboxes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentPermissions {
    #[serde(default)]
    pub network: NetworkPermission,
    #[serde(default)]
    pub filesystem: FilesystemPermission,
    #[serde(default)]
    pub execution: ExecutionPermission,
    #[serde(default)]
    pub resources: ResourcePermission,
    #[serde(default)]
    pub data_access: DataAccessPermission,
}

impl Default for AgentPermissions {
    fn default() -> Self {
        Self {
            network: NetworkPermission::default(),
            filesystem: FilesystemPermission::default(),
            execution: ExecutionPermission::default(),
            resources: ResourcePermission::default(),
            data_access: DataAccessPermission::default(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkPermission {
    /// Master toggle: allow any network access at all.
    #[serde(default)]
    pub enabled: bool,
    /// Allow public internet access.
    #[serde(default)]
    pub internet: bool,
    /// Allow local/private network access (10.x, 172.x, 192.168.x).
    #[serde(default)]
    pub local_network: bool,
    /// If non-empty, restrict outbound to these domains only.
    #[serde(default)]
    pub allowed_domains: Vec<String>,
}

impl Default for NetworkPermission {
    fn default() -> Self {
        Self {
            enabled: false,
            internet: false,
            local_network: false,
            allowed_domains: vec![],
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum FilesystemMode {
    /// Full read/write access to /workspace.
    ReadWrite,
    /// Read-only workspace mount.
    ReadOnly,
    /// No persistent filesystem (tmpfs only).
    None,
}

impl Default for FilesystemMode {
    fn default() -> Self {
        Self::ReadWrite
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FilesystemPermission {
    /// Workspace mount mode.
    #[serde(default)]
    pub mode: FilesystemMode,
    /// Max workspace size in MB (soft limit, for display/quota).
    pub max_workspace_size_mb: Option<u64>,
}

impl Default for FilesystemPermission {
    fn default() -> Self {
        Self {
            mode: FilesystemMode::ReadWrite,
            max_workspace_size_mb: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionPermission {
    /// Allow shell commands (sh/bash).
    #[serde(default = "default_true")]
    pub shell: bool,
    /// Allow Python execution.
    #[serde(default = "default_true")]
    pub python: bool,
    /// Additional allowed runtimes (e.g. "node", "ruby").
    #[serde(default)]
    pub allowed_runtimes: Vec<String>,
    /// Max single-command execution time in seconds.
    pub max_execution_time_secs: Option<u64>,
}

impl Default for ExecutionPermission {
    fn default() -> Self {
        Self {
            shell: true,
            python: true,
            allowed_runtimes: vec![],
            max_execution_time_secs: Some(300),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourcePermission {
    /// Max number of processes (PID limit).
    pub max_processes: Option<i64>,
    /// Max tmp space in MB.
    pub max_tmp_size_mb: Option<u64>,
    /// Writable storage for package installs (/usr/local) in MB.
    /// This tmpfs allows `pip install`, `npm install`, etc. on read-only rootfs.
    pub max_storage_size_mb: Option<u64>,
    /// Read-only root filesystem.
    #[serde(default = "default_true")]
    pub readonly_rootfs: bool,
}

impl Default for ResourcePermission {
    fn default() -> Self {
        Self {
            max_processes: Some(256),
            max_tmp_size_mb: Some(256),
            max_storage_size_mb: Some(512),
            readonly_rootfs: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DataAccessPermission {
    /// Can the agent access linked knowledge bases.
    #[serde(default = "default_true")]
    pub knowledge_bases: bool,
    /// Can the agent read its own conversation history.
    #[serde(default = "default_true")]
    pub conversation_history: bool,
}

impl Default for DataAccessPermission {
    fn default() -> Self {
        Self {
            knowledge_bases: true,
            conversation_history: true,
        }
    }
}

fn default_true() -> bool {
    true
}
fn default_kb_type() -> String {
    "standard".to_string()
}

// ── Connector Proxy Policies ─────────────────────────────────────

/// HTTP methods that can be allowed or denied in proxy rules.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "UPPERCASE")]
pub enum HttpMethod {
    Get,
    Post,
    Put,
    Patch,
    Delete,
    Head,
    Options,
}

impl std::fmt::Display for HttpMethod {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            HttpMethod::Get => write!(f, "GET"),
            HttpMethod::Post => write!(f, "POST"),
            HttpMethod::Put => write!(f, "PUT"),
            HttpMethod::Patch => write!(f, "PATCH"),
            HttpMethod::Delete => write!(f, "DELETE"),
            HttpMethod::Head => write!(f, "HEAD"),
            HttpMethod::Options => write!(f, "OPTIONS"),
        }
    }
}

/// A single allow/deny rule for the connector proxy.
/// Matches an HTTP method + URL path pattern (glob-style).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProxyRule {
    /// HTTP methods this rule applies to. Empty = matches no methods.
    pub methods: Vec<HttpMethod>,
    /// URL path pattern (glob-style, e.g. "/gmail/v1/users/me/messages/*").
    /// Matched against the path portion of the upstream URL.
    pub path_pattern: String,
    /// Human-readable description for display in the UI.
    #[serde(default)]
    pub description: String,
}

/// The full proxy policy for a specific connector assigned to an agent.
/// Controls which HTTP requests the agent can make through this connector.
///
/// Policies are **opt-in restrictions**: if no policy exists for a connector,
/// all requests are allowed. Adding a policy opts the connector into access control.
///
/// When a policy IS defined, evaluation order is:
///   1. Deny rules are checked first — if any deny rule matches, the request is blocked.
///   2. Allow rules are checked next — the request must match at least one allow rule.
///   3. If no allow rule matches, the request is blocked (deny-by-default).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectorPolicy {
    /// The connector this policy applies to.
    pub connector_id: Uuid,
    /// Allow rules. A request must match at least one to be forwarded.
    /// Empty = deny all requests through this connector.
    #[serde(default)]
    pub allow: Vec<ProxyRule>,
    /// Explicit deny rules, checked before allow rules.
    /// If a request matches any deny rule, it is blocked even if an allow rule would match.
    #[serde(default)]
    pub deny: Vec<ProxyRule>,
    /// Rate limit: maximum requests per minute. None = unlimited.
    #[serde(default)]
    pub rate_limit_rpm: Option<u32>,
}

/// Named policy presets for common connector use-cases.
/// Users pick a preset in the UI and can customise from there.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolicyPreset {
    /// Machine-readable name, e.g. "gmail_read_only".
    pub name: String,
    /// Human-readable label, e.g. "Gmail — Read Only".
    pub label: String,
    /// Which connector type this preset applies to.
    pub connector_type: ConnectorType,
    /// The pre-built policy rules.
    pub policy: ConnectorPolicy,
}

/// Permission override that can be set per-conversation/task to further restrict
/// the agent's base permissions. Overrides can only narrow, never widen.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TaskPermissionOverride {
    /// If set, only these connector IDs may be used (intersection with agent grants).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub allowed_connector_ids: Option<Vec<Uuid>>,
    /// If set, only these HTTP methods are allowed across all connectors.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub allowed_methods: Option<Vec<HttpMethod>>,
    /// If true, disable code execution for this task even if the agent allows it.
    #[serde(default)]
    pub disable_code_execution: bool,
    /// If true, disable workspace write for this task.
    #[serde(default)]
    pub disable_workspace_write: bool,
    /// If true, disable knowledge base access for this task.
    #[serde(default)]
    pub disable_knowledge_access: bool,
}

// ── Tool Audit Log ───────────────────────────────────────────────

/// The decision made by the permission guard for a tool invocation.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AuditDecision {
    Allowed,
    Denied,
}

/// A single entry in the tool audit log.
/// Recorded for every tool invocation (allowed or denied) for compliance and debugging.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolAuditEntry {
    pub id: Uuid,
    pub conversation_id: Uuid,
    pub agent_id: Uuid,
    pub user_id: Uuid,
    /// The tool or proxy endpoint that was invoked.
    pub tool_name: String,
    /// For proxy requests: the HTTP method used.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub http_method: Option<String>,
    /// For proxy requests: the target URL path (no query string, no credentials).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_path: Option<String>,
    /// The connector used (if applicable).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub connector_id: Option<Uuid>,
    /// Whether the invocation was allowed or denied.
    pub decision: AuditDecision,
    /// Human-readable reason when denied.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub denial_reason: Option<String>,
    /// How long the tool invocation took in milliseconds (None if denied before execution).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<i64>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AgentStatus {
    Online,
    Offline,
    Busy,
    Error,
}

// ── Conversation ───────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Conversation {
    pub id: Uuid,
    pub title: String,
    pub agent_id: Uuid,
    /// The user who owns this conversation.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub owner_id: Option<Uuid>,
    #[serde(default)]
    pub pinned: bool,
    #[serde(default)]
    pub archived: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub id: Uuid,
    pub conversation_id: Uuid,
    pub role: MessageRole,
    pub content: String,
    pub created_at: DateTime<Utc>,
    /// Attachments linked to this message (populated on read, not on create).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub attachments: Vec<MessageAttachment>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageAttachment {
    pub id: Uuid,
    pub filename: String,
    pub content_type: String,
    pub size_bytes: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MessageRole {
    User,
    Assistant,
    System,
    Tool,
}

// ── Knowledge Base ─────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KnowledgeBase {
    pub id: Uuid,
    pub owner_id: Uuid,
    pub name: String,
    pub description: String,
    #[serde(default = "default_kb_type")]
    pub kb_type: String,
    pub embedding_model: String,
    pub entry_count: i64,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KnowledgeEntry {
    pub id: Uuid,
    pub knowledge_base_id: Uuid,
    pub title: String,
    pub content: String,
    pub token_count: Option<i32>,
    pub has_embedding: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_document_id: Option<Uuid>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KnowledgeDocument {
    pub id: Uuid,
    pub knowledge_base_id: Uuid,
    pub filename: String,
    pub content_type: String,
    pub size_bytes: i64,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KnowledgeSearchResult {
    pub entry: KnowledgeEntry,
    pub score: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub document_url: Option<String>,
}

// ── Skill ─────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Skill {
    pub id: Uuid,
    /// Lowercase, hyphen-separated name used as `/skill-name` in prompts.
    pub name: String,
    pub description: String,
    /// The full instructions loaded when the skill is invoked.
    pub instructions: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

// ── Connector ──────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Connector {
    pub id: Uuid,
    /// The user who owns this connector.
    pub user_id: Uuid,
    pub name: String,
    pub connector_type: ConnectorType,
    pub enabled: bool,
    pub config: serde_json::Value,
    /// Free-text operational context injected when this connector is invoked.
    pub context: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ConnectorType {
    Telegram,
    Gmail,
    Slack,
    AzureDevops,
    Custom,
    Tavily,
    Bing,
}

// ── Tool ───────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Tool {
    pub id: Uuid,
    pub name: String,
    pub description: String,
    pub connector_id: Uuid,
    pub schema: serde_json::Value,
    pub enabled: bool,
}

// ── LLM Connector (bring your own) ────────────────────────────────

/// The inference backend type, used to select the correct API format.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LlmProviderType {
    /// Azure OpenAI Service — uses `api-key` header + deployment URL.
    Azure,
    /// OpenRouter — OpenAI-compatible with `Authorization: Bearer` header.
    OpenRouter,
    /// OpenAI — standard API at api.openai.com.
    OpenAi,
    /// Any OpenAI-compatible endpoint (e.g. Ollama, LM Studio, etc.).
    Custom,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmConnector {
    pub id: Uuid,
    pub name: String,
    pub provider_type: LlmProviderType,
    /// API key. Stored in-memory only (never persisted to disk in this MVP).
    pub api_key: String,
    /// Base URL for the provider.
    /// - Azure: `https://<resource>.openai.azure.com`
    /// - OpenRouter: `https://openrouter.ai/api/v1` (auto-filled)
    /// - OpenAI: `https://api.openai.com/v1` (auto-filled)
    /// - Custom: user-supplied
    pub api_base_url: String,
    /// Model / deployment name.
    pub model: String,
    /// Azure-specific: deployment name (defaults to `model` if blank).
    pub azure_deployment: Option<String>,
    /// Azure-specific: API version string (e.g. `2024-02-01`).
    pub azure_api_version: Option<String>,
    pub created_at: DateTime<Utc>,
}

// ── User ──────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum UserRole {
    Admin,
    User,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct User {
    pub id: Uuid,
    pub email: String,
    pub display_name: String,
    /// Never serialized to the client.
    #[serde(skip_serializing)]
    pub password_hash: String,
    pub role: UserRole,
    /// Free-text context about the user that agents can read.
    pub bio: String,
    /// URL of the user's avatar image (data URL or remote URL).
    pub avatar_url: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    pub token: String,
    pub user_id: Uuid,
    pub expires_at: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
}

// ── Conversation Sharing ──────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SharePermission {
    Read,
    Write,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConversationShare {
    pub id: Uuid,
    pub conversation_id: Uuid,
    pub shared_by: Uuid,
    pub shared_with: Uuid,
    pub permission: SharePermission,
    pub created_at: DateTime<Utc>,
}

// ── Calendar ──────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CalendarEvent {
    pub id: Uuid,
    pub owner_id: Uuid,
    pub title: String,
    /// YYYY-MM-DD
    pub date: String,
    /// HH:MM
    pub start_time: String,
    /// HH:MM
    pub end_time: String,
    pub category: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub location: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
    #[serde(default)]
    pub completed: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CalendarShare {
    pub id: Uuid,
    pub owner_id: Uuid,
    pub shared_with: Uuid,
    pub permission: SharePermission,
    pub created_at: DateTime<Utc>,
}

// ── Scheduled Tasks ─────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScheduledTask {
    pub id: Uuid,
    pub owner_id: Uuid,
    pub agent_id: Uuid,
    pub name: String,
    pub prompt: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cron_expression: Option<String>,
    pub enabled: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_run_at: Option<DateTime<Utc>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_run_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskExecution {
    pub id: Uuid,
    pub task_id: Uuid,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub conversation_id: Option<Uuid>,
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result_summary: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_message: Option<String>,
    pub started_at: DateTime<Utc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub completed_at: Option<DateTime<Utc>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<i64>,
    /// Output files produced by the agent during this execution.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub output_files: Vec<TaskOutputFile>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskOutputFile {
    pub id: Uuid,
    pub filename: String,
    pub content_type: String,
    pub size_bytes: i64,
}

// ── Settings ───────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Settings {
    pub default_llm_connector_id: Option<Uuid>,
    /// LLM connector used during Knowledge Base ETL for semantic chunking.
    /// When set, the LLM is called to find optimal sentence boundaries instead of
    /// the built-in heuristic splitter.
    pub etl_llm_connector_id: Option<Uuid>,
    pub theme: String,
    /// Platform-level system prompt prepended before every agent's own system_prompt.
    /// Use this to set global guardrails, identity, tool-usage rules, and container
    /// permissions that apply to all agents. Empty string means no base prompt.
    pub agent_base_prompt: String,
    /// Maximum seconds to wait for an LLM HTTP response before timing out.
    /// Default is 120. Increase for slow providers or heavy models.
    pub llm_request_timeout_secs: i32,
    /// OpenAI-compatible base URL for the embedding provider.
    pub embedding_api_base_url: String,
    /// API key for the embedding provider (masked on retrieval).
    pub embedding_api_key: String,
    /// Model name for embedding generation.
    pub embedding_model: String,
}
