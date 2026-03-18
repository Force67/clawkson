use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;
use uuid::Uuid;

use crate::error::ContainerError;
use crate::models::*;
use crate::runtime::{ContainerRuntime, RuntimeCapabilities};
use crate::workspace;

/// Default output directory scanned after exec (relative to workspace root).
const DEFAULT_OUTPUT_DIR: &str = "outputs";
const LABEL_PREFIX: &str = "clawkson";

/// Composite key for per-conversation container isolation.
/// For persistent containers, `conversation_id` is `Uuid::nil()`.
type ContainerKey = (Uuid, Uuid); // (agent_id, conversation_id)

/// Sentinel conversation_id used as the key for persistent (agent-level) containers.
pub const PERSISTENT_SENTINEL: Uuid = Uuid::nil();

/// Orchestrates container/sandbox lifecycle across one or more runtime backends.
///
/// Business logic (container map, persistent sentinel, workspace paths, output
/// file collection) lives here. The actual runtime operations are delegated to
/// [`ContainerRuntime`] implementations.
pub struct ContainerManager {
    runtime: Arc<dyn ContainerRuntime>,
    containers: Arc<RwLock<HashMap<ContainerKey, ContainerInfo>>>,
    workspace_root: PathBuf,
}

impl ContainerManager {
    /// Create a new manager backed by the given runtime.
    pub fn new(
        runtime: Arc<dyn ContainerRuntime>,
        workspace_root: PathBuf,
    ) -> Result<Self, ContainerError> {
        std::fs::create_dir_all(&workspace_root)?;
        Ok(Self {
            runtime,
            containers: Arc::new(RwLock::new(HashMap::new())),
            workspace_root,
        })
    }

    /// Name of the active runtime backend.
    pub fn runtime_name(&self) -> &str {
        self.runtime.name()
    }

    /// Capabilities of the active runtime.
    pub fn capabilities(&self) -> RuntimeCapabilities {
        self.runtime.capabilities()
    }

    // ── Container lifecycle ──────────────────────────────────────

    /// Create and start a container for an agent+conversation pair.
    pub async fn start_container(
        &self,
        agent_id: Uuid,
        conversation_id: Uuid,
        config: &ContainerConfig,
    ) -> Result<ContainerInfo, ContainerError> {
        let is_persistent = config.persistent;
        let effective_conv_id = if is_persistent { PERSISTENT_SENTINEL } else { conversation_id };
        let key = (agent_id, effective_conv_id);

        // Stop existing container if any
        if self.containers.read().await.contains_key(&key) {
            self.stop_container(agent_id, effective_conv_id).await.ok();
        }

        // Ensure image is available (no-op for non-Docker runtimes)
        self.runtime.ensure_image(&config.image).await?;

        // Create workspace directory
        let workspace = if is_persistent {
            self.workspace_root.join(agent_id.to_string()).join("shared")
        } else {
            self.workspace_root.join(agent_id.to_string()).join(conversation_id.to_string())
        };
        for dir in [&workspace, &workspace.join("inputs"), &workspace.join("outputs")] {
            std::fs::create_dir_all(dir)?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o777))?;
            }
        }

        // Build container name hint
        let name = if is_persistent {
            format!("clawkson-{}-persistent", &agent_id.as_simple().to_string()[..8])
        } else {
            format!(
                "clawkson-{}-{}",
                &agent_id.as_simple().to_string()[..8],
                &conversation_id.as_simple().to_string()[..8],
            )
        };

        // Populate labels for the runtime (Docker uses these for orphan cleanup)
        let mut config = config.clone();
        config.labels.insert(format!("{LABEL_PREFIX}.agent_id"), agent_id.to_string());
        config.labels.insert(format!("{LABEL_PREFIX}.conversation_id"), effective_conv_id.to_string());

        let rt = self.runtime.create_and_start(&config, &workspace, &name).await?;

        let workspace_str = workspace
            .canonicalize()
            .unwrap_or(workspace.clone())
            .to_string_lossy()
            .to_string();

        let info = ContainerInfo {
            agent_id,
            conversation_id: effective_conv_id,
            runtime_id: rt.runtime_id,
            runtime_name: self.runtime.name().to_string(),
            state: ContainerState::Running,
            image: config.image.clone(),
            workspace_path: workspace_str,
            ip_address: rt.ip_address,
            persistent: is_persistent,
        };

        self.containers.write().await.insert(key, info.clone());
        tracing::info!(
            %agent_id, %effective_conv_id,
            persistent = is_persistent,
            runtime = self.runtime.name(),
            "container started",
        );

        Ok(info)
    }

    /// Get a running persistent container, or start one.
    pub async fn get_or_start_persistent(
        &self,
        agent_id: Uuid,
        config: &ContainerConfig,
    ) -> Result<ContainerInfo, ContainerError> {
        let key = (agent_id, PERSISTENT_SENTINEL);

        // 1. Check in-memory map
        if let Some(info) = self.containers.read().await.get(&key).cloned() {
            if info.state == ContainerState::Running {
                return Ok(info);
            }
        }

        // 2. Try to re-adopt (Docker: inspect by name; bwrap: check workspace exists)
        if let Some(info) = self.try_readopt_persistent(agent_id).await {
            return Ok(info);
        }

        // 3. Start fresh
        self.start_container(agent_id, PERSISTENT_SENTINEL, config).await
    }

    /// Try to re-adopt a persistent container after a server restart.
    async fn try_readopt_persistent(&self, agent_id: Uuid) -> Option<ContainerInfo> {
        let name = format!(
            "clawkson-{}-persistent",
            &agent_id.as_simple().to_string()[..8],
        );

        let state = self.runtime.inspect(&name).await.ok()??;

        if !state.running {
            return None;
        }

        let workspace_path = state.workspace_bind.unwrap_or_else(|| {
            self.workspace_root
                .join(agent_id.to_string())
                .join("shared")
                .to_string_lossy()
                .to_string()
        });

        let info = ContainerInfo {
            agent_id,
            conversation_id: PERSISTENT_SENTINEL,
            runtime_id: name.clone(),
            runtime_name: self.runtime.name().to_string(),
            state: ContainerState::Running,
            image: state.image.unwrap_or_else(|| "unknown".to_string()),
            workspace_path,
            ip_address: state.ip_address,
            persistent: true,
        };

        let key = (agent_id, PERSISTENT_SENTINEL);
        self.containers.write().await.insert(key, info.clone());
        tracing::info!(%agent_id, runtime = self.runtime.name(), "re-adopted persistent container");

        Some(info)
    }

    /// Stop a container.
    pub async fn stop_container(&self, agent_id: Uuid, conversation_id: Uuid) -> Result<(), ContainerError> {
        let key = (agent_id, conversation_id);
        let info = {
            let containers = self.containers.read().await;
            containers.get(&key).cloned().ok_or(ContainerError::NotFound(agent_id))?
        };

        self.runtime.stop(&info.runtime_id).await.ok();

        if let Some(c) = self.containers.write().await.get_mut(&key) {
            c.state = ContainerState::Stopped;
        }

        tracing::info!(%agent_id, %conversation_id, "container stopped");
        Ok(())
    }

    /// Remove a container and optionally its workspace.
    pub async fn remove_container(
        &self,
        agent_id: Uuid,
        conversation_id: Uuid,
        remove_workspace: bool,
    ) -> Result<(), ContainerError> {
        let key = (agent_id, conversation_id);
        let info = self.containers.write().await.remove(&key);

        if let Some(info) = &info {
            self.runtime.remove(&info.runtime_id).await.ok();

            if remove_workspace {
                let workspace = PathBuf::from(&info.workspace_path);
                if workspace.exists() {
                    std::fs::remove_dir_all(&workspace).ok();
                }
            }

            tracing::info!(%agent_id, %conversation_id, "container removed");
        }

        Ok(())
    }

    // ── Query ────────────────────────────────────────────────────

    /// Get container status for a specific conversation.
    pub async fn get_container(&self, agent_id: Uuid, conversation_id: Uuid) -> Option<ContainerInfo> {
        self.containers.read().await.get(&(agent_id, conversation_id)).cloned()
    }

    /// List all containers for a given agent.
    pub async fn list_agent_containers(&self, agent_id: Uuid) -> Vec<ContainerInfo> {
        self.containers.read().await.values()
            .filter(|info| info.agent_id == agent_id)
            .cloned()
            .collect()
    }

    /// List all managed containers.
    pub async fn list_all_containers(&self) -> Vec<ContainerInfo> {
        self.containers.read().await.values().cloned().collect()
    }

    // ── Execution ────────────────────────────────────────────────

    /// Execute a command in the container.
    pub async fn exec(
        &self,
        agent_id: Uuid,
        conversation_id: Uuid,
        request: &ExecRequest,
    ) -> Result<ExecResult, ContainerError> {
        let key = (agent_id, conversation_id);
        let info = {
            let containers = self.containers.read().await;
            containers.get(&key).cloned().ok_or(ContainerError::NotFound(agent_id))?
        };

        if info.state != ContainerState::Running {
            return Err(ContainerError::NotRunning(agent_id));
        }

        let mut result = match self.runtime.exec(&info.runtime_id, request).await {
            Ok(r) => r,
            Err(e) => {
                // If the runtime reports the container is gone, clean up our map
                let is_gone = matches!(&e, ContainerError::Docker(de) if de.to_string().contains("404"))
                    || matches!(&e, ContainerError::NotFound(_));
                if is_gone {
                    tracing::warn!(%agent_id, %conversation_id, "container gone, removing stale entry");
                    self.containers.write().await.remove(&key);
                    return Err(ContainerError::NotFound(agent_id));
                }
                return Err(e);
            }
        };

        // Collect output files if requested (runtime-agnostic, reads from host workspace).
        if result.output_files.is_none() {
            let output_dir = request.output_dir.as_deref().unwrap_or(DEFAULT_OUTPUT_DIR);
            if !output_dir.is_empty() {
                let workspace = PathBuf::from(&info.workspace_path);
                match workspace::collect_output_files(&workspace, output_dir) {
                    Ok(files) if !files.is_empty() => {
                        result.output_files = Some(files);
                    }
                    _ => {}
                }
            }
        }

        Ok(result)
    }

    // ── Workspace ────────────────────────────────────────────────

    /// List files in a workspace directory.
    pub async fn workspace_list(
        &self,
        agent_id: Uuid,
        conversation_id: Uuid,
        rel: &str,
    ) -> Result<workspace::WorkspaceListing, ContainerError> {
        let workspace = self.conversation_workspace(agent_id, conversation_id).await?;
        workspace::list_workspace(&workspace, rel)
    }

    /// Resolve the workspace path for a conversation.
    pub async fn conversation_workspace(&self, agent_id: Uuid, conversation_id: Uuid) -> Result<PathBuf, ContainerError> {
        let key = (agent_id, conversation_id);
        if let Some(info) = self.containers.read().await.get(&key) {
            return Ok(PathBuf::from(&info.workspace_path));
        }
        let persistent_key = (agent_id, PERSISTENT_SENTINEL);
        if let Some(info) = self.containers.read().await.get(&persistent_key) {
            return Ok(PathBuf::from(&info.workspace_path));
        }
        let workspace = self.workspace_root
            .join(agent_id.to_string())
            .join(conversation_id.to_string());
        Ok(workspace)
    }

    /// Return the root directory where all agent workspaces are stored.
    pub fn workspace_root(&self) -> &std::path::Path {
        &self.workspace_root
    }

    // ── Logs ─────────────────────────────────────────────────────

    /// Get container logs.
    pub async fn logs(
        &self,
        agent_id: Uuid,
        conversation_id: Uuid,
        tail: Option<usize>,
    ) -> Result<String, ContainerError> {
        let key = (agent_id, conversation_id);
        let info = {
            let containers = self.containers.read().await;
            containers.get(&key).cloned().ok_or(ContainerError::NotFound(agent_id))?
        };

        self.runtime.logs(&info.runtime_id, tail).await
    }

    // ── Lifecycle ────────────────────────────────────────────────

    /// Clean up orphan containers from previous runs.
    pub async fn cleanup_orphans(&self) -> Result<usize, ContainerError> {
        let managed = self.runtime.list_managed().await?;

        let mut removed = 0usize;
        let mut readopted = 0usize;

        for mc in &managed {
            if mc.persistent {
                if let Some(agent_id) = mc.agent_id {
                    if mc.running {
                        if self.try_readopt_persistent(agent_id).await.is_some() {
                            readopted += 1;
                            continue;
                        }
                    }
                }
            }

            // Remove non-persistent or failed-to-readopt containers
            self.runtime.remove(&mc.runtime_id).await.ok();
            removed += 1;
        }

        if removed > 0 {
            tracing::info!(removed, "cleaned up orphan containers");
        }
        if readopted > 0 {
            tracing::info!(readopted, "re-adopted persistent containers");
        }

        Ok(removed)
    }

    /// Graceful shutdown — stop temporal containers, leave persistent ones running.
    pub async fn shutdown(&self) {
        let containers: Vec<ContainerInfo> =
            self.containers.read().await.values().cloned().collect();

        for info in containers {
            if info.persistent {
                tracing::info!(agent_id = %info.agent_id, "shutdown: leaving persistent container running");
                continue;
            }
            self.runtime.stop(&info.runtime_id).await.ok();
            tracing::info!(agent_id = %info.agent_id, "shutdown: stopped container");
        }

        self.runtime.shutdown().await;
    }
}
