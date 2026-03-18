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
type ContainerKey = (Uuid, Uuid); // (agent_id, conversation_id)

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
    pub async fn start_container(
        &self,
        agent_id: Uuid,
        conversation_id: Uuid,
        config: &ContainerConfig,
    ) -> Result<ContainerInfo, ContainerError> {
        let key = (agent_id, conversation_id);

        // Stop existing container if any
        if self.containers.read().await.contains_key(&key) {
            self.stop_container(agent_id, conversation_id).await.ok();
        }

        // Ensure image is available
        self.ensure_image(&config.image).await?;

        // Create workspace directory scoped to agent+conversation.
        // Layout: {workspace_root}/{agent_id}/{conversation_id}/
        // Set permissions to 777 so the container process can write regardless of
        // which user it runs as (CAP_DROP ALL removes DAC_OVERRIDE from root).
        let workspace = self.workspace_root
            .join(agent_id.to_string())
            .join(conversation_id.to_string());
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

        // Build container config
        let mut labels = HashMap::new();
        labels.insert(
            format!("{LABEL_PREFIX}.agent_id"),
            agent_id.to_string(),
        );
        labels.insert(
            format!("{LABEL_PREFIX}.conversation_id"),
            conversation_id.to_string(),
        );
        labels.insert(format!("{LABEL_PREFIX}.managed"), "true".to_string());

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
        let pids_limit = perms.resources.max_processes;
        let readonly_rootfs = Some(perms.resources.readonly_rootfs);
        let tmp_size = perms.resources.max_tmp_size_mb.unwrap_or(256);
        let storage_size = perms.resources.max_storage_size_mb.unwrap_or(512);

        // Build tmpfs mounts for writable areas the runtime needs.
        // We mount a dedicated /opt/sandbox-packages tmpfs (not /usr/local, which
        // would shadow Python/Node binaries on images like python:3.12-slim).
        // Environment variables direct pip/npm to write into this tmpfs.
        let mut tmpfs_mounts = HashMap::from([
            ("/tmp".to_string(), format!("size={tmp_size}m")),
            ("/var/tmp".to_string(), "size=32m".to_string()),
            ("/root".to_string(), "size=64m".to_string()),
        ]);
        if perms.resources.readonly_rootfs && storage_size > 0 {
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
        if perms.resources.readonly_rootfs && storage_size > 0 {
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

        // Create container — name includes both agent and conversation for uniqueness
        let name = format!(
            "clawkson-{}-{}",
            &agent_id.as_simple().to_string()[..8],
            &conversation_id.as_simple().to_string()[..8],
        );
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
            tracing::info!(%agent_id, %conversation_id, ip, "container reachable on internal network");
        }

        let info = ContainerInfo {
            agent_id,
            conversation_id,
            docker_id: response.id,
            state: ContainerState::Running,
            image: config.image.clone(),
            workspace_path: workspace_str,
            ip_address,
        };

        self.containers.write().await.insert(key, info.clone());
        tracing::info!(%agent_id, %conversation_id, "container started");

        Ok(info)
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
    pub async fn conversation_workspace(&self, agent_id: Uuid, conversation_id: Uuid) -> Result<PathBuf, ContainerError> {
        let key = (agent_id, conversation_id);
        // Check if we have a running/stopped container first.
        if let Some(info) = self.containers.read().await.get(&key) {
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

        let count = containers.len();
        for container in &containers {
            if let Some(id) = &container.id {
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
            }
        }

        if count > 0 {
            tracing::info!(count, "cleaned up orphan containers");
        }

        Ok(count)
    }

    /// Stop all managed containers (for graceful shutdown).
    pub async fn shutdown(&self) {
        let containers: Vec<ContainerInfo> =
            self.containers.read().await.values().cloned().collect();

        for info in containers {
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
