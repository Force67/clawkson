use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;
use uuid::Uuid;

use bollard::container::{
    Config, CreateContainerOptions, ListContainersOptions, LogsOptions, RemoveContainerOptions,
    StopContainerOptions,
};
use bollard::image::CreateImageOptions;
use bollard::models::HostConfig;
use bollard::Docker;
use futures::StreamExt;

use crate::error::ContainerError;
use crate::executor::exec_in_container;
use crate::models::*;
use crate::workspace;

const DEFAULT_TIMEOUT: u64 = 30;
const MAX_TIMEOUT: u64 = 300;
const LABEL_PREFIX: &str = "clawkson";
/// Default output directory scanned after exec (relative to workspace root).
const DEFAULT_OUTPUT_DIR: &str = "outputs";
/// Internal Docker network for proxy access (no internet, host-reachable).
const INTERNAL_NETWORK: &str = "clawkson-internal";

/// Composite key for per-conversation container isolation.
/// For persistent containers, `conversation_id` is `Uuid::nil()`.
type ContainerKey = (Uuid, Uuid); // (agent_id, conversation_id)

/// Sentinel conversation_id used as the key for persistent (agent-level) containers.
pub const PERSISTENT_SENTINEL: Uuid = Uuid::nil();

pub struct ContainerManager {
    docker: Docker,
    containers: Arc<RwLock<HashMap<ContainerKey, ContainerInfo>>>,
    workspace_root: PathBuf,
}

impl ContainerManager {
    /// Connect to Docker and create a new manager.
    pub async fn new(workspace_root: PathBuf) -> Result<Self, ContainerError> {
        let docker = Docker::connect_with_local_defaults()?;

        // Verify Docker connection
        docker.ping().await?;
        tracing::info!("connected to Docker daemon");

        // Ensure the internal proxy network exists.
        // This network has no internet access but is reachable from the host,
        // enabling the reverse proxy to route requests to container web servers.
        if docker
            .inspect_network::<&str>(INTERNAL_NETWORK, None)
            .await
            .is_err()
        {
            use bollard::network::CreateNetworkOptions;
            let net_config = CreateNetworkOptions {
                name: INTERNAL_NETWORK,
                internal: true,
                driver: "bridge",
                ..Default::default()
            };
            match docker.create_network(net_config).await {
                Ok(_) => tracing::info!("created internal Docker network '{INTERNAL_NETWORK}'"),
                Err(e) => tracing::warn!("failed to create internal network: {e} (proxy preview will be unavailable)"),
            }
        }

        std::fs::create_dir_all(&workspace_root)?;

        Ok(Self {
            docker,
            containers: Arc::new(RwLock::new(HashMap::new())),
            workspace_root,
        })
    }

    /// Ensure the base image is available locally.
    pub async fn ensure_image(&self, image: &str) -> Result<(), ContainerError> {
        // Check if image exists
        if self.docker.inspect_image(image).await.is_ok() {
            return Ok(());
        }

        tracing::info!(image, "pulling container image");
        let mut stream = self.docker.create_image(
            Some(CreateImageOptions {
                from_image: image,
                ..Default::default()
            }),
            None,
            None,
        );

        while let Some(result) = stream.next().await {
            match result {
                Ok(info) => {
                    if let Some(status) = info.status {
                        tracing::debug!(status, "image pull progress");
                    }
                }
                Err(e) => return Err(ContainerError::ImagePull(e.to_string())),
            }
        }

        tracing::info!(image, "image pulled successfully");
        Ok(())
    }

    /// Create and start a container for an agent+conversation pair.
    /// Each conversation gets its own isolated container and workspace directory.
    ///
    /// When `config.persistent` is true, a single shared container is used for
    /// all conversations of this agent, keyed by `PERSISTENT_SENTINEL`.
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

        // Ensure image is available
        self.ensure_image(&config.image).await?;

        // Create workspace directory.
        // Persistent: {workspace_root}/{agent_id}/shared/
        // Temporal:   {workspace_root}/{agent_id}/{conversation_id}/
        let workspace = if is_persistent {
            self.workspace_root
                .join(agent_id.to_string())
                .join("shared")
        } else {
            self.workspace_root
                .join(agent_id.to_string())
                .join(conversation_id.to_string())
        };
        for dir in [&workspace, &workspace.join("inputs"), &workspace.join("outputs")] {
            std::fs::create_dir_all(dir)?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o777))?;
            }
        }

        let workspace_str = workspace
            .canonicalize()
            .unwrap_or(workspace.clone())
            .to_string_lossy()
            .to_string();

        // Build container labels
        let mut labels = HashMap::new();
        labels.insert(
            format!("{LABEL_PREFIX}.agent_id"),
            agent_id.to_string(),
        );
        labels.insert(
            format!("{LABEL_PREFIX}.conversation_id"),
            effective_conv_id.to_string(),
        );
        labels.insert(format!("{LABEL_PREFIX}.managed"), "true".to_string());
        if is_persistent {
            labels.insert(format!("{LABEL_PREFIX}.persistent"), "true".to_string());
        }

        let nano_cpus = config.cpu_limit.map(|c| (c * 1e9) as i64);
        let memory = config.memory_limit_mb.map(|m| (m * 1024 * 1024) as i64);

        let perms = &config.permissions;

        // Network: when internet access is requested, use the default bridge as
        // the primary network (so DNS + default gateway work) and attach the
        // internal proxy network as secondary.  Without internet, the container
        // only sits on the isolated internal network.
        let net_enabled = perms.network.enabled || config.network_enabled;
        let network_mode = if net_enabled {
            Some("bridge".to_string())
        } else {
            Some(INTERNAL_NETWORK.to_string())
        };

        // Filesystem: bind mount mode from permissions
        let binds = match perms.filesystem.mode {
            clawkson_core::FilesystemMode::ReadWrite => {
                Some(vec![format!("{workspace_str}:/workspace")])
            }
            clawkson_core::FilesystemMode::ReadOnly => {
                Some(vec![format!("{workspace_str}:/workspace:ro")])
            }
            clawkson_core::FilesystemMode::None => None,
        };

        // Resource limits
        // Persistent containers always get a writable rootfs so packages survive restarts.
        let pids_limit = perms.resources.max_processes;
        let effective_readonly = if is_persistent { false } else { perms.resources.readonly_rootfs };
        let readonly_rootfs = Some(effective_readonly);
        let tmp_size = perms.resources.max_tmp_size_mb.unwrap_or(256);
        let storage_size = perms.resources.max_storage_size_mb.unwrap_or(512);

        // Build tmpfs mounts for writable areas the runtime needs.
        // For persistent containers, skip the /opt/sandbox-packages tmpfs —
        // packages install directly onto the writable rootfs and survive restarts.
        let mut tmpfs_mounts = HashMap::from([
            ("/tmp".to_string(), format!("size={tmp_size}m")),
            ("/var/tmp".to_string(), "size=32m".to_string()),
            ("/root".to_string(), "size=64m".to_string()),
        ]);
        if effective_readonly && storage_size > 0 {
            tmpfs_mounts.insert(
                "/opt/sandbox-packages".to_string(),
                format!("size={storage_size}m"),
            );
        }

        // Environment: redirect pip/npm installs to the writable tmpfs and
        // make installed binaries available on PATH.
        let mut pkg_env = vec![
            // Pre-installed Playwright browsers baked into the image
            "PLAYWRIGHT_BROWSERS_PATH=/usr/lib/playwright".to_string(),
        ];
        if effective_readonly && storage_size > 0 {
            pkg_env.extend([
                "PIP_TARGET=/opt/sandbox-packages/pip".to_string(),
                "PYTHONPATH=/opt/sandbox-packages/pip".to_string(),
                "NPM_CONFIG_PREFIX=/opt/sandbox-packages/npm".to_string(),
                "PATH=/opt/sandbox-packages/pip/bin:/opt/sandbox-packages/npm/bin:/usr/local/bin:/usr/bin:/bin".to_string(),
            ]);
        }

        let host_config = HostConfig {
            binds,
            nano_cpus,
            memory,
            pids_limit,
            network_mode,
            cap_drop: Some(vec![
                "ALL".to_string(),
            ]),
            cap_add: Some(vec![
                "CHOWN".to_string(),
                "SETUID".to_string(),
                "SETGID".to_string(),
            ]),
            readonly_rootfs,
            tmpfs: Some(tmpfs_mounts),
            ..Default::default()
        };

        let env = Some(pkg_env);

        let container_config = Config {
            image: Some(config.image.clone()),
            labels: Some(labels),
            host_config: Some(host_config),
            working_dir: Some("/workspace".to_string()),
            cmd: Some(vec!["sleep".to_string(), "infinity".to_string()]),
            env,
            ..Default::default()
        };

        // Create container — name includes both agent and conversation for uniqueness.
        // Persistent containers get a stable name so they can be re-adopted after restart.
        let name = if is_persistent {
            format!(
                "clawkson-{}-persistent",
                &agent_id.as_simple().to_string()[..8],
            )
        } else {
            format!(
                "clawkson-{}-{}",
                &agent_id.as_simple().to_string()[..8],
                &conversation_id.as_simple().to_string()[..8],
            )
        };
        let response = self
            .docker
            .create_container(
                Some(CreateContainerOptions {
                    name: name.as_str(),
                    platform: None,
                }),
                container_config,
            )
            .await?;

        // Start container
        self.docker
            .start_container::<String>(&response.id, None)
            .await?;

        // When internet is enabled the primary network is bridge (for DNS/routing).
        // Also attach the internal proxy network so the host can reach container
        // web servers for live preview.
        if net_enabled {
            use bollard::network::ConnectNetworkOptions;
            use bollard::models::EndpointSettings;
            let connect = ConnectNetworkOptions {
                container: response.id.as_str(),
                endpoint_config: EndpointSettings::default(),
            };
            if let Err(e) = self.docker.connect_network(INTERNAL_NETWORK, connect).await {
                tracing::warn!("failed to connect container to internal network: {e} (preview proxy may be unavailable)");
            }
        }

        // Retrieve the container IP on the internal proxy network.
        let ip_address = self
            .docker
            .inspect_container(&response.id, None)
            .await
            .ok()
            .and_then(|inspect| inspect.network_settings)
            .and_then(|ns| ns.networks)
            .and_then(|nets| nets.get(INTERNAL_NETWORK).cloned())
            .and_then(|ep| ep.ip_address)
            .filter(|ip| !ip.is_empty());

        if let Some(ref ip) = ip_address {
            tracing::info!(%agent_id, %effective_conv_id, ip, "container reachable on internal network");
        }

        let info = ContainerInfo {
            agent_id,
            conversation_id: effective_conv_id,
            docker_id: response.id,
            state: ContainerState::Running,
            image: config.image.clone(),
            workspace_path: workspace_str,
            ip_address,
            persistent: is_persistent,
        };

        self.containers.write().await.insert(key, info.clone());
        tracing::info!(%agent_id, %effective_conv_id, persistent = is_persistent, "container started");

        Ok(info)
    }

    /// Get a running persistent container for an agent, or start one if none exists.
    /// Checks in-memory map first, then tries to re-adopt from Docker, then starts fresh.
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

        // 2. Try to re-adopt an existing Docker container by name
        if let Some(info) = self.try_readopt_persistent(agent_id).await {
            return Ok(info);
        }

        // 3. Start fresh
        self.start_container(agent_id, PERSISTENT_SENTINEL, config).await
    }

    /// Try to re-adopt a persistent container that is still running in Docker
    /// (e.g. after a server restart). Returns `Some(info)` if found and reinserted.
    async fn try_readopt_persistent(&self, agent_id: Uuid) -> Option<ContainerInfo> {
        let name = format!(
            "clawkson-{}-persistent",
            &agent_id.as_simple().to_string()[..8],
        );

        let inspect = self.docker.inspect_container(&name, None).await.ok()?;

        let running = inspect.state.as_ref()
            .and_then(|s| s.running)
            .unwrap_or(false);
        if !running {
            return None;
        }

        let docker_id = inspect.id?.clone();

        let ip_address = inspect.network_settings
            .and_then(|ns| ns.networks)
            .and_then(|nets| nets.get(INTERNAL_NETWORK).cloned())
            .and_then(|ep| ep.ip_address)
            .filter(|ip| !ip.is_empty());

        // Resolve workspace path from the bind mount
        let workspace_path = inspect.host_config
            .and_then(|hc| hc.binds)
            .and_then(|binds| binds.into_iter().next())
            .and_then(|b| b.split(':').next().map(String::from))
            .unwrap_or_else(|| {
                self.workspace_root
                    .join(agent_id.to_string())
                    .join("shared")
                    .to_string_lossy()
                    .to_string()
            });

        let image = inspect.config
            .and_then(|c| c.image)
            .unwrap_or_else(|| "unknown".to_string());

        let info = ContainerInfo {
            agent_id,
            conversation_id: PERSISTENT_SENTINEL,
            docker_id,
            state: ContainerState::Running,
            image,
            workspace_path,
            ip_address,
            persistent: true,
        };

        let key = (agent_id, PERSISTENT_SENTINEL);
        self.containers.write().await.insert(key, info.clone());
        tracing::info!(%agent_id, "re-adopted persistent container");

        Some(info)
    }

    /// Stop a container.
    pub async fn stop_container(&self, agent_id: Uuid, conversation_id: Uuid) -> Result<(), ContainerError> {
        let key = (agent_id, conversation_id);
        let info = {
            let containers = self.containers.read().await;
            containers
                .get(&key)
                .cloned()
                .ok_or(ContainerError::NotFound(agent_id))?
        };

        self.docker
            .stop_container(
                &info.docker_id,
                Some(StopContainerOptions { t: 10 }),
            )
            .await
            .ok(); // Ignore errors from already-stopped containers

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
            // Force remove (stops if running)
            self.docker
                .remove_container(
                    &info.docker_id,
                    Some(RemoveContainerOptions {
                        force: true,
                        ..Default::default()
                    }),
                )
                .await
                .ok();

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

    /// Get container status for a specific conversation.
    pub async fn get_container(&self, agent_id: Uuid, conversation_id: Uuid) -> Option<ContainerInfo> {
        self.containers.read().await.get(&(agent_id, conversation_id)).cloned()
    }

    /// List all containers for a given agent (across all conversations).
    pub async fn list_agent_containers(&self, agent_id: Uuid) -> Vec<ContainerInfo> {
        self.containers.read().await.values()
            .filter(|info| info.agent_id == agent_id)
            .cloned()
            .collect()
    }

    /// List all managed containers across all agents and conversations.
    pub async fn list_all_containers(&self) -> Vec<ContainerInfo> {
        self.containers.read().await.values().cloned().collect()
    }

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
            containers
                .get(&key)
                .cloned()
                .ok_or(ContainerError::NotFound(agent_id))?
        };

        if info.state != ContainerState::Running {
            return Err(ContainerError::NotRunning(agent_id));
        }

        let timeout = request
            .timeout
            .unwrap_or(DEFAULT_TIMEOUT)
            .min(MAX_TIMEOUT);

        let cmd = vec!["sh", "-c", &request.command];
        let mut result = match exec_in_container(&self.docker, &info.docker_id, cmd, timeout).await {
            Ok(r) => r,
            Err(ContainerError::Docker(ref e)) if e.to_string().contains("404") => {
                // Container was removed externally (e.g. Docker prune). Clean up stale entry.
                tracing::warn!(%agent_id, %conversation_id, "container gone from Docker (404), removing stale entry");
                self.containers.write().await.remove(&key);
                return Err(ContainerError::NotFound(agent_id));
            }
            Err(e) => return Err(e),
        };

        // Collect output files if requested (default: scan "outputs/" dir).
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

        Ok(result)
    }

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

    /// Resolve the workspace path for a conversation (container need not be running).
    /// For persistent agents, also checks the sentinel key.
    pub async fn conversation_workspace(&self, agent_id: Uuid, conversation_id: Uuid) -> Result<PathBuf, ContainerError> {
        let key = (agent_id, conversation_id);
        // Check if we have a running/stopped container first.
        if let Some(info) = self.containers.read().await.get(&key) {
            return Ok(PathBuf::from(&info.workspace_path));
        }
        // Check if there's a persistent container for this agent.
        let persistent_key = (agent_id, PERSISTENT_SENTINEL);
        if let Some(info) = self.containers.read().await.get(&persistent_key) {
            return Ok(PathBuf::from(&info.workspace_path));
        }
        // Fall back to the on-disk workspace directory.
        let workspace = self.workspace_root
            .join(agent_id.to_string())
            .join(conversation_id.to_string());
        Ok(workspace)
    }

    /// Return the root directory where all agent workspaces are stored.
    pub fn workspace_root(&self) -> &std::path::Path {
        &self.workspace_root
    }

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
            containers
                .get(&key)
                .cloned()
                .ok_or(ContainerError::NotFound(agent_id))?
        };

        let tail_str = tail.unwrap_or(100).to_string();
        let mut stream = self.docker.logs(
            &info.docker_id,
            Some(LogsOptions::<String> {
                stdout: true,
                stderr: true,
                tail: tail_str,
                ..Default::default()
            }),
        );

        let mut output = String::new();
        while let Some(msg) = stream.next().await {
            if let Ok(log) = msg {
                output.push_str(&log.to_string());
            }
        }

        Ok(output)
    }

    /// Clean up orphan containers from previous runs (label-based).
    /// Persistent containers (label `clawkson.persistent=true`) are re-adopted
    /// into the in-memory map instead of being removed.
    pub async fn cleanup_orphans(&self) -> Result<usize, ContainerError> {
        let mut filters = HashMap::new();
        filters.insert(
            "label".to_string(),
            vec![format!("{LABEL_PREFIX}.managed=true")],
        );

        let containers = self
            .docker
            .list_containers(Some(ListContainersOptions {
                all: true,
                filters,
                ..Default::default()
            }))
            .await?;

        let mut removed = 0usize;
        let mut readopted = 0usize;

        for container in &containers {
            let Some(id) = &container.id else { continue };

            // Check if this container is persistent
            let is_persistent = container.labels.as_ref()
                .and_then(|l| l.get(&format!("{LABEL_PREFIX}.persistent")))
                .map(|v| v == "true")
                .unwrap_or(false);

            if is_persistent {
                // Re-adopt: parse the agent_id from labels and reinsert
                let agent_id = container.labels.as_ref()
                    .and_then(|l| l.get(&format!("{LABEL_PREFIX}.agent_id")))
                    .and_then(|v| Uuid::parse_str(v).ok());

                if let Some(agent_id) = agent_id {
                    if self.try_readopt_persistent(agent_id).await.is_some() {
                        readopted += 1;
                        continue;
                    }
                }
                // If re-adopt failed (container stopped, bad labels, etc.), fall through to remove
            }

            self.docker
                .remove_container(
                    id,
                    Some(RemoveContainerOptions {
                        force: true,
                        ..Default::default()
                    }),
                )
                .await
                .ok();
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

    /// Stop all managed containers (for graceful shutdown).
    /// Persistent containers are left running so they survive restarts.
    pub async fn shutdown(&self) {
        let containers: Vec<ContainerInfo> =
            self.containers.read().await.values().cloned().collect();

        for info in containers {
            if info.persistent {
                tracing::info!(agent_id = %info.agent_id, "shutdown: leaving persistent container running");
                continue;
            }
            self.docker
                .stop_container(
                    &info.docker_id,
                    Some(StopContainerOptions { t: 5 }),
                )
                .await
                .ok();
            tracing::info!(agent_id = %info.agent_id, "shutdown: stopped container");
        }
    }
}
