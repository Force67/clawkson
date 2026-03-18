use std::path::Path;
use uuid::Uuid;

use crate::error::ContainerError;
use crate::models::{ContainerConfig, ExecRequest, ExecResult};

/// What a runtime backend supports. Used by the UI/API to show/hide features.
#[derive(Debug, Clone)]
pub struct RuntimeCapabilities {
    /// Supports long-lived containers with start/stop/remove lifecycle.
    pub lifecycle: bool,
    /// Supports host-side workspace bind mounts.
    pub workspace: bool,
    /// Supports fetching container stdout/stderr logs.
    pub logs: bool,
    /// Supports reverse-proxy preview of web servers.
    pub preview: bool,
}

/// Information returned by a runtime after creating/starting a sandbox.
#[derive(Debug, Clone)]
pub struct RuntimeContainer {
    /// Runtime-specific identifier (Docker container ID, bwrap PID, etc.).
    pub runtime_id: String,
    /// IP address on the internal network, if available.
    pub ip_address: Option<String>,
}

/// State of a runtime-managed container, returned by `inspect`.
#[derive(Debug, Clone)]
pub struct RuntimeContainerState {
    pub running: bool,
    pub ip_address: Option<String>,
    pub image: Option<String>,
    /// First workspace bind mount source path, if any.
    pub workspace_bind: Option<String>,
}

/// A container discovered during cleanup/orphan listing.
#[derive(Debug, Clone)]
pub struct ManagedContainer {
    pub runtime_id: String,
    pub agent_id: Option<Uuid>,
    pub conversation_id: Option<Uuid>,
    pub persistent: bool,
    pub running: bool,
}

/// Abstraction over a container/sandbox backend.
///
/// Implementations handle the low-level runtime operations. The
/// [`ContainerManager`](crate::manager::ContainerManager) keeps all
/// business logic (container map, persistent sentinel, workspace paths).
#[async_trait::async_trait]
pub trait ContainerRuntime: Send + Sync + 'static {
    /// Human-readable name of this runtime (e.g. "docker", "bwrap").
    fn name(&self) -> &str;

    /// What this backend supports.
    fn capabilities(&self) -> RuntimeCapabilities;

    /// Create and start a container/sandbox.
    ///
    /// * `config`         — resource limits, permissions, image, etc.
    /// * `workspace_path` — host directory to bind-mount as `/workspace`.
    /// * `name_hint`      — suggested container name (runtime may ignore).
    async fn create_and_start(
        &self,
        config: &ContainerConfig,
        workspace_path: &Path,
        name_hint: &str,
    ) -> Result<RuntimeContainer, ContainerError>;

    /// Execute a command inside an existing sandbox.
    ///
    /// * `runtime_id` — the ID returned by `create_and_start`.
    /// * `request`    — command, timeout, output dir.
    async fn exec(
        &self,
        runtime_id: &str,
        request: &ExecRequest,
    ) -> Result<ExecResult, ContainerError>;

    /// Stop a running sandbox. No-op for stateless runtimes.
    async fn stop(&self, runtime_id: &str) -> Result<(), ContainerError>;

    /// Force-remove a sandbox. No-op for stateless runtimes.
    async fn remove(&self, runtime_id: &str) -> Result<(), ContainerError>;

    /// Inspect a sandbox by its runtime ID.
    async fn inspect(&self, runtime_id: &str) -> Result<Option<RuntimeContainerState>, ContainerError>;

    /// Get stdout/stderr logs. Returns empty string if unsupported.
    async fn logs(&self, runtime_id: &str, tail: Option<usize>) -> Result<String, ContainerError>;

    /// List all containers/sandboxes managed by this runtime.
    async fn list_managed(&self) -> Result<Vec<ManagedContainer>, ContainerError>;

    /// Ensure a container image is available locally. No-op for runtimes
    /// that use the host system (bwrap).
    async fn ensure_image(&self, image: &str) -> Result<(), ContainerError>;

    /// Graceful shutdown hook. Called once on server stop.
    async fn shutdown(&self);
}
