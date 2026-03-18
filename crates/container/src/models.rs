use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::workspace::OutputFile;

/// Configuration for creating a container/sandbox.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContainerConfig {
    /// Docker image to use (ignored by non-Docker runtimes).
    #[serde(default = "default_image")]
    pub image: String,
    /// CPU limit in cores (e.g. 1.0).
    pub cpu_limit: Option<f64>,
    /// Memory limit in megabytes.
    pub memory_limit_mb: Option<u64>,
    /// Whether networking is enabled.
    #[serde(default)]
    pub network_enabled: bool,
    /// Granular permissions from the agent config.
    #[serde(default)]
    pub permissions: clawkson_core::AgentPermissions,
    /// Whether this is a persistent (agent-level) container.
    #[serde(default)]
    pub persistent: bool,
    /// Opaque key-value labels for the runtime (Docker labels, etc.).
    /// The manager populates these with agent_id, conversation_id, etc.
    #[serde(default, skip_serializing_if = "std::collections::HashMap::is_empty")]
    pub labels: std::collections::HashMap<String, String>,
}

fn default_image() -> String {
    "clawkson-sandbox:latest".to_string()
}

impl Default for ContainerConfig {
    fn default() -> Self {
        Self {
            image: default_image(),
            cpu_limit: Some(1.0),
            memory_limit_mb: Some(512),
            network_enabled: false,
            permissions: clawkson_core::AgentPermissions::default(),
            persistent: false,
            labels: std::collections::HashMap::new(),
        }
    }
}

/// Runtime information about a managed container/sandbox.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContainerInfo {
    pub agent_id: Uuid,
    /// The conversation this container is scoped to.
    /// For persistent containers this is `Uuid::nil()` (sentinel).
    pub conversation_id: Uuid,
    /// Runtime-specific identifier (Docker container ID, bwrap encoded ID, etc.).
    pub runtime_id: String,
    /// Which runtime backend manages this container.
    pub runtime_name: String,
    pub state: ContainerState,
    pub image: String,
    pub workspace_path: String,
    /// Container IP on the internal proxy network (if connected).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ip_address: Option<String>,
    /// Whether this is a persistent (agent-level) container.
    #[serde(default)]
    pub persistent: bool,
}

/// Container lifecycle states.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ContainerState {
    Creating,
    Running,
    Stopped,
    Removing,
}

/// Request to execute a command inside a container.
#[derive(Debug, Clone, Deserialize)]
pub struct ExecRequest {
    pub command: String,
    /// Timeout in seconds (default 30, max 300).
    pub timeout: Option<u64>,
    /// If set, scan this workspace-relative directory after execution and
    /// return any files found in `output_files`.  Defaults to "outputs".
    /// Pass an empty string to disable output collection.
    pub output_dir: Option<String>,
}

/// Result of command execution.
#[derive(Debug, Clone, Serialize)]
pub struct ExecResult {
    pub stdout: String,
    pub stderr: String,
    pub exit_code: i64,
    pub timed_out: bool,
    /// Files found in the output directory after execution (if requested).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output_files: Option<Vec<OutputFile>>,
}
