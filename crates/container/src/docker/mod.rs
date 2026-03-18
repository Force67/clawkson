pub mod executor;

use std::collections::HashMap;
use std::path::Path;

use bollard::container::{
    Config, CreateContainerOptions, ListContainersOptions, LogsOptions, RemoveContainerOptions,
    StopContainerOptions,
};
use bollard::image::CreateImageOptions;
use bollard::models::HostConfig;
use bollard::Docker;
use futures::StreamExt;
use uuid::Uuid;

use crate::error::ContainerError;
use crate::models::{ContainerConfig, ExecRequest, ExecResult};
use crate::runtime::{
    ContainerRuntime, ManagedContainer, RuntimeCapabilities, RuntimeContainer,
    RuntimeContainerState,
};

const LABEL_PREFIX: &str = "clawkson";
const INTERNAL_NETWORK: &str = "clawkson-internal";
const DEFAULT_TIMEOUT: u64 = 30;
const MAX_TIMEOUT: u64 = 300;

pub struct DockerRuntime {
    docker: Docker,
}

impl DockerRuntime {
    /// Connect to Docker daemon and ensure the internal network exists.
    pub async fn new() -> Result<Self, ContainerError> {
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

        Ok(Self { docker })
    }
}

#[async_trait::async_trait]
impl ContainerRuntime for DockerRuntime {
    fn name(&self) -> &str {
        "docker"
    }

    fn capabilities(&self) -> RuntimeCapabilities {
        RuntimeCapabilities {
            lifecycle: true,
            workspace: true,
            logs: true,
            preview: true,
        }
    }

    async fn create_and_start(
        &self,
        config: &ContainerConfig,
        workspace_path: &Path,
        name_hint: &str,
    ) -> Result<RuntimeContainer, ContainerError> {
        let is_persistent = config.persistent;

        let workspace_str = workspace_path
            .canonicalize()
            .unwrap_or(workspace_path.to_path_buf())
            .to_string_lossy()
            .to_string();

        // Build container labels: start from caller-provided labels (agent_id, etc.)
        // then add managed/persistent markers.
        let mut labels = config.labels.clone();
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
        let effective_readonly = if is_persistent {
            false
        } else {
            perms.resources.readonly_rootfs
        };
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
            cap_drop: Some(vec!["ALL".to_string()]),
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

        let response = self
            .docker
            .create_container(
                Some(CreateContainerOptions {
                    name: name_hint,
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
            use bollard::models::EndpointSettings;
            use bollard::network::ConnectNetworkOptions;
            let connect = ConnectNetworkOptions {
                container: response.id.as_str(),
                endpoint_config: EndpointSettings::default(),
            };
            if let Err(e) = self
                .docker
                .connect_network(INTERNAL_NETWORK, connect)
                .await
            {
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
            tracing::info!(name_hint, ip, "container reachable on internal network");
        }

        Ok(RuntimeContainer {
            runtime_id: response.id,
            ip_address,
        })
    }

    async fn exec(
        &self,
        runtime_id: &str,
        request: &ExecRequest,
    ) -> Result<ExecResult, ContainerError> {
        let timeout = request
            .timeout
            .unwrap_or(DEFAULT_TIMEOUT)
            .min(MAX_TIMEOUT);

        let cmd = vec!["sh", "-c", &request.command];
        match executor::exec_in_container(&self.docker, runtime_id, cmd, timeout).await {
            Ok(r) => Ok(r),
            Err(ContainerError::Docker(ref e)) if e.to_string().contains("404") => {
                tracing::warn!(runtime_id, "container gone from Docker (404)");
                Err(ContainerError::Docker(bollard::errors::Error::DockerResponseServerError {
                    status_code: 404,
                    message: format!("Container {runtime_id} not found — removed externally"),
                }))
            }
            Err(e) => Err(e),
        }
    }

    async fn stop(&self, runtime_id: &str) -> Result<(), ContainerError> {
        self.docker
            .stop_container(runtime_id, Some(StopContainerOptions { t: 10 }))
            .await
            .ok();
        Ok(())
    }

    async fn remove(&self, runtime_id: &str) -> Result<(), ContainerError> {
        self.docker
            .remove_container(
                runtime_id,
                Some(RemoveContainerOptions {
                    force: true,
                    ..Default::default()
                }),
            )
            .await
            .ok();
        Ok(())
    }

    async fn inspect(
        &self,
        runtime_id: &str,
    ) -> Result<Option<RuntimeContainerState>, ContainerError> {
        let inspect = match self.docker.inspect_container(runtime_id, None).await {
            Ok(i) => i,
            Err(_) => return Ok(None),
        };

        let running = inspect
            .state
            .as_ref()
            .and_then(|s| s.running)
            .unwrap_or(false);

        let ip_address = inspect
            .network_settings
            .as_ref()
            .and_then(|ns| ns.networks.as_ref())
            .and_then(|nets| nets.get(INTERNAL_NETWORK))
            .and_then(|ep| ep.ip_address.clone())
            .filter(|ip| !ip.is_empty());

        let image = inspect
            .config
            .as_ref()
            .and_then(|c| c.image.clone());

        let workspace_bind = inspect
            .host_config
            .as_ref()
            .and_then(|hc| hc.binds.as_ref())
            .and_then(|binds| binds.first())
            .and_then(|b| b.split(':').next().map(String::from));

        Ok(Some(RuntimeContainerState {
            running,
            ip_address,
            image,
            workspace_bind,
        }))
    }

    async fn logs(
        &self,
        runtime_id: &str,
        tail: Option<usize>,
    ) -> Result<String, ContainerError> {
        let tail_str = tail.unwrap_or(100).to_string();
        let mut stream = self.docker.logs(
            runtime_id,
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

    async fn list_managed(&self) -> Result<Vec<ManagedContainer>, ContainerError> {
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

        let mut result = Vec::new();
        for container in &containers {
            let Some(id) = &container.id else {
                continue;
            };

            let labels = container.labels.as_ref();

            let agent_id = labels
                .and_then(|l| l.get(&format!("{LABEL_PREFIX}.agent_id")))
                .and_then(|v| Uuid::parse_str(v).ok());

            let conversation_id = labels
                .and_then(|l| l.get(&format!("{LABEL_PREFIX}.conversation_id")))
                .and_then(|v| Uuid::parse_str(v).ok());

            let persistent = labels
                .and_then(|l| l.get(&format!("{LABEL_PREFIX}.persistent")))
                .map(|v| v == "true")
                .unwrap_or(false);

            let running = container
                .state
                .as_deref()
                .map(|s| s == "running")
                .unwrap_or(false);

            result.push(ManagedContainer {
                runtime_id: id.clone(),
                agent_id,
                conversation_id,
                persistent,
                running,
            });
        }

        Ok(result)
    }

    async fn ensure_image(&self, image: &str) -> Result<(), ContainerError> {
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

    async fn shutdown(&self) {
        // No-op — ContainerManager handles which containers to stop.
    }
}
