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
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Per-agent container resource limits.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentContainerConfig {
    /// CPU limit in cores (e.g. 1.0).
    pub cpu_limit: Option<f64>,
    /// Memory limit in megabytes (e.g. 512).
    pub memory_limit_mb: Option<u64>,
    /// Whether networking is enabled (default: false).
    #[serde(default)]
    pub network_enabled: bool,
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
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KnowledgeSearchResult {
    pub entry: KnowledgeEntry,
    pub score: f64,
}

// ── Connector ──────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Connector {
    pub id: Uuid,
    pub name: String,
    pub connector_type: ConnectorType,
    pub enabled: bool,
    pub config: serde_json::Value,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ConnectorType {
    Telegram,
    Gmail,
    Slack,
    Custom,
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

// ── Settings ───────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Settings {
    pub default_llm_connector_id: Option<Uuid>,
    pub theme: String,
}
