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

pub struct ContainerManager {
    docker: Docker,
    containers: Arc<RwLock<HashMap<Uuid, ContainerInfo>>>,
    workspace_root: PathBuf,
}

impl ContainerManager {
    /// Connect to Docker and create a new manager.
    pub async fn new(workspace_root: PathBuf) -> Result<Self, ContainerError> {
        let docker = Docker::connect_with_local_defaults()?;

        // Verify Docker connection
        docker.ping().await?;
        tracing::info!("connected to Docker daemon");

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

    /// Create and start a container for an agent.
    pub async fn start_container(
        &self,
        agent_id: Uuid,
        config: &ContainerConfig,
    ) -> Result<ContainerInfo, ContainerError> {
        // Stop existing container if any
        if self.containers.read().await.contains_key(&agent_id) {
            self.stop_container(agent_id).await.ok();
        }

        // Ensure image is available
        self.ensure_image(&config.image).await?;

        // Create workspace directory
        let workspace = self.workspace_root.join(agent_id.to_string());
        std::fs::create_dir_all(&workspace)?;

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
        labels.insert(format!("{LABEL_PREFIX}.managed"), "true".to_string());

        let nano_cpus = config.cpu_limit.map(|c| (c * 1e9) as i64);
        let memory = config.memory_limit_mb.map(|m| (m * 1024 * 1024) as i64);

        let network_mode = if config.network_enabled {
            None
        } else {
            Some("none".to_string())
        };

        let host_config = HostConfig {
            binds: Some(vec![format!("{workspace_str}:/workspace")]),
            nano_cpus,
            memory,
            pids_limit: Some(256),
            network_mode,
            cap_drop: Some(vec![
                "ALL".to_string(),
            ]),
            cap_add: Some(vec![
                "CHOWN".to_string(),
                "SETUID".to_string(),
                "SETGID".to_string(),
            ]),
            readonly_rootfs: Some(true),
            // tmpfs for writable areas the runtime needs
            tmpfs: Some(HashMap::from([
                ("/tmp".to_string(), "size=64m".to_string()),
                ("/var/tmp".to_string(), "size=16m".to_string()),
                ("/root".to_string(), "size=16m".to_string()),
            ])),
            ..Default::default()
        };

        let container_config = Config {
            image: Some(config.image.clone()),
            labels: Some(labels),
            host_config: Some(host_config),
            working_dir: Some("/workspace".to_string()),
            cmd: Some(vec!["sleep".to_string(), "infinity".to_string()]),
            ..Default::default()
        };

        // Create container
        let name = format!("clawkson-{}", agent_id.as_simple());
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

        let info = ContainerInfo {
            agent_id,
            docker_id: response.id,
            state: ContainerState::Running,
            image: config.image.clone(),
            workspace_path: workspace_str,
        };

        self.containers.write().await.insert(agent_id, info.clone());
        tracing::info!(%agent_id, "container started");

        Ok(info)
    }

    /// Stop a container.
    pub async fn stop_container(&self, agent_id: Uuid) -> Result<(), ContainerError> {
        let info = {
            let containers = self.containers.read().await;
            containers
                .get(&agent_id)
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

        if let Some(c) = self.containers.write().await.get_mut(&agent_id) {
            c.state = ContainerState::Stopped;
        }

        tracing::info!(%agent_id, "container stopped");
        Ok(())
    }

    /// Remove a container and optionally its workspace.
    pub async fn remove_container(
        &self,
        agent_id: Uuid,
        remove_workspace: bool,
    ) -> Result<(), ContainerError> {
        let info = self.containers.write().await.remove(&agent_id);

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

            tracing::info!(%agent_id, "container removed");
        }

        Ok(())
    }

    /// Get container status.
    pub async fn get_container(&self, agent_id: Uuid) -> Option<ContainerInfo> {
        self.containers.read().await.get(&agent_id).cloned()
    }

    /// Execute a command in the container.
    pub async fn exec(
        &self,
        agent_id: Uuid,
        request: &ExecRequest,
    ) -> Result<ExecResult, ContainerError> {
        let info = {
            let containers = self.containers.read().await;
            containers
                .get(&agent_id)
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
        let mut result = exec_in_container(&self.docker, &info.docker_id, cmd, timeout).await?;

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
        rel: &str,
    ) -> Result<workspace::WorkspaceListing, ContainerError> {
        let workspace = self.agent_workspace(agent_id).await?;
        workspace::list_workspace(&workspace, rel)
    }

    /// Resolve the workspace path for an agent (container need not be running).
    pub async fn agent_workspace(&self, agent_id: Uuid) -> Result<PathBuf, ContainerError> {
        // Check if we have a running/stopped container first.
        if let Some(info) = self.containers.read().await.get(&agent_id) {
            return Ok(PathBuf::from(&info.workspace_path));
        }
        // Fall back to the on-disk workspace directory.
        let workspace = self.workspace_root.join(agent_id.to_string());
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
        tail: Option<usize>,
    ) -> Result<String, ContainerError> {
        let info = {
            let containers = self.containers.read().await;
            containers
                .get(&agent_id)
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
