//! Bubblewrap (bwrap) sandbox runtime backend.
//!
//! Uses Linux namespaces via `bwrap` for lightweight per-exec isolation.
//! Unlike Docker, there are no long-lived containers — each [`exec`] call
//! spawns a fresh bwrap process with the host workspace bind-mounted at
//! `/workspace`. The workspace persists on the host between execs.

use std::path::Path;

use crate::error::ContainerError;
use crate::models::{ContainerConfig, ExecRequest, ExecResult};
use crate::runtime::{
    ContainerRuntime, ManagedContainer, RuntimeCapabilities, RuntimeContainer,
    RuntimeContainerState,
};

/// Maximum output size per stream (stdout/stderr) in bytes.
const MAX_OUTPUT_BYTES: usize = 64 * 1024; // 64 KB

/// Default execution timeout in seconds.
const DEFAULT_TIMEOUT: u64 = 30;

/// Hard ceiling for execution timeout in seconds.
const MAX_TIMEOUT: u64 = 300;

/// Bubblewrap sandbox runtime.
///
/// Each [`exec`](ContainerRuntime::exec) call creates an ephemeral bwrap
/// sandbox. There is no persistent container process — `create_and_start`
/// merely records configuration in an opaque `runtime_id` string.
pub struct BwrapRuntime {
    bwrap_path: String,
}

impl BwrapRuntime {
    /// Create a new `BwrapRuntime`, verifying that the `bwrap` binary is
    /// available and functional.
    pub fn new() -> Result<Self, ContainerError> {
        let bwrap_path = which_bwrap()?;

        // Smoke-test the binary.
        let output = std::process::Command::new(&bwrap_path)
            .arg("--version")
            .output()
            .map_err(ContainerError::Io)?;

        if !output.status.success() {
            return Err(ContainerError::ImagePull(
                "bwrap --version failed".into(),
            ));
        }

        let version = String::from_utf8_lossy(&output.stdout);
        tracing::info!(path = %bwrap_path, version = %version.trim(), "bwrap runtime ready");

        Ok(Self { bwrap_path })
    }
}

// ── Runtime ID encoding ──────────────────────────────────────────
//
// The trait's `exec` method only receives `runtime_id` — it has no access
// to `ContainerConfig` or `workspace_path`. We encode the information we
// need inside the opaque runtime_id string:
//
//   bwrap:<net|nonet>:<workspace_path>
//
// This is an implementation detail; callers treat it as an opaque handle.

/// Prefix for all bwrap runtime IDs.
const RUNTIME_PREFIX: &str = "bwrap:";

/// Build a runtime ID that encodes network mode and workspace path.
fn encode_runtime_id(network_enabled: bool, workspace_path: &Path) -> String {
    let net_flag = if network_enabled { "net" } else { "nonet" };
    format!(
        "bwrap:{}:{}",
        net_flag,
        workspace_path.to_string_lossy()
    )
}

/// Decode a runtime ID into `(network_enabled, workspace_path)`.
/// Returns `None` if the format is invalid.
fn decode_runtime_id(runtime_id: &str) -> Option<(bool, &str)> {
    let rest = runtime_id.strip_prefix(RUNTIME_PREFIX)?;
    let (net_flag, workspace_path) = rest.split_once(':')?;
    let network_enabled = match net_flag {
        "net" => true,
        "nonet" => false,
        _ => return None,
    };
    if workspace_path.is_empty() {
        return None;
    }
    Some((network_enabled, workspace_path))
}

// ── Trait implementation ─────────────────────────────────────────

#[async_trait::async_trait]
impl ContainerRuntime for BwrapRuntime {
    fn name(&self) -> &str {
        "bwrap"
    }

    fn capabilities(&self) -> RuntimeCapabilities {
        RuntimeCapabilities {
            lifecycle: false,
            workspace: true,
            logs: false,
            preview: false,
        }
    }

    /// No-op for bwrap — sandboxes are created per-exec. We encode the
    /// workspace path and network flag into the runtime_id so that `exec`
    /// can reconstruct the bwrap invocation later.
    async fn create_and_start(
        &self,
        config: &ContainerConfig,
        workspace_path: &Path,
        _name_hint: &str,
    ) -> Result<RuntimeContainer, ContainerError> {
        // Ensure the workspace directory tree exists.
        for sub in ["", "inputs", "outputs"] {
            let dir = if sub.is_empty() {
                workspace_path.to_path_buf()
            } else {
                workspace_path.join(sub)
            };
            std::fs::create_dir_all(&dir).map_err(ContainerError::Io)?;
        }

        let runtime_id = encode_runtime_id(config.network_enabled, workspace_path);

        tracing::debug!(
            %runtime_id,
            network = config.network_enabled,
            "bwrap sandbox registered (ephemeral, created per-exec)"
        );

        Ok(RuntimeContainer {
            runtime_id,
            ip_address: None,
        })
    }

    /// Spawn a fresh bwrap sandbox, execute the command, and return the
    /// captured output. Each call is completely independent.
    async fn exec(
        &self,
        runtime_id: &str,
        request: &ExecRequest,
    ) -> Result<ExecResult, ContainerError> {
        let (network_enabled, workspace_path) =
            decode_runtime_id(runtime_id).ok_or_else(|| {
                ContainerError::Io(std::io::Error::new(
                    std::io::ErrorKind::InvalidInput,
                    format!("invalid bwrap runtime_id: {runtime_id}"),
                ))
            })?;

        // Verify workspace still exists.
        if !Path::new(workspace_path).is_dir() {
            return Err(ContainerError::Io(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                format!("workspace directory does not exist: {workspace_path}"),
            )));
        }

        // ── Build the bwrap command ──────────────────────────────

        let mut cmd = tokio::process::Command::new(&self.bwrap_path);

        // Namespace isolation.
        cmd.arg("--unshare-pid")
            .arg("--unshare-uts")
            .arg("--unshare-ipc")
            .arg("--die-with-parent")
            .arg("--new-session");

        if !network_enabled {
            cmd.arg("--unshare-net");
        }

        // ── Read-only system mounts ──────────────────────────────

        cmd.args(["--ro-bind", "/usr", "/usr"]);

        // /lib and /lib64 may or may not exist depending on distro.
        if Path::new("/lib").exists() {
            cmd.args(["--ro-bind", "/lib", "/lib"]);
        }
        if Path::new("/lib64").exists() {
            cmd.args(["--ro-bind", "/lib64", "/lib64"]);
        }
        if Path::new("/lib32").exists() {
            cmd.args(["--ro-bind", "/lib32", "/lib32"]);
        }

        // /bin and /sbin — some distros keep these separate from /usr.
        if Path::new("/bin").is_symlink() || Path::new("/bin").is_dir() {
            cmd.args(["--ro-bind", "/bin", "/bin"]);
        }
        if Path::new("/sbin").is_symlink() || Path::new("/sbin").is_dir() {
            cmd.args(["--ro-bind", "/sbin", "/sbin"]);
        }

        // Alternatives (for python3 / node symlinks on Debian/Ubuntu).
        if Path::new("/etc/alternatives").exists() {
            cmd.args(["--ro-bind", "/etc/alternatives", "/etc/alternatives"]);
        }

        // SSL/TLS certificates.
        if Path::new("/etc/ssl").exists() {
            cmd.args(["--ro-bind", "/etc/ssl", "/etc/ssl"]);
        }
        if Path::new("/etc/ca-certificates").exists() {
            cmd.args(["--ro-bind", "/etc/ca-certificates", "/etc/ca-certificates"]);
        }

        // DNS and networking config — only when network is enabled.
        if network_enabled {
            if Path::new("/etc/resolv.conf").exists() {
                cmd.args(["--ro-bind", "/etc/resolv.conf", "/etc/resolv.conf"]);
            }
            if Path::new("/etc/hosts").exists() {
                cmd.args(["--ro-bind", "/etc/hosts", "/etc/hosts"]);
            }
            if Path::new("/etc/nsswitch.conf").exists() {
                cmd.args(["--ro-bind", "/etc/nsswitch.conf", "/etc/nsswitch.conf"]);
            }
        }

        // ── Virtual filesystems ──────────────────────────────────

        cmd.args(["--proc", "/proc"]);
        cmd.args(["--dev", "/dev"]);

        // Writable tmpfs mounts for scratch areas.
        cmd.args(["--tmpfs", "/tmp"]);
        cmd.args(["--tmpfs", "/home"]);
        cmd.args(["--tmpfs", "/root"]);
        cmd.args(["--tmpfs", "/run"]);
        cmd.args(["--tmpfs", "/var"]);

        // ── Workspace bind mount (read-write) ────────────────────

        cmd.args(["--bind", workspace_path, "/workspace"]);
        cmd.args(["--chdir", "/workspace"]);

        // ── The actual command ────────────────────────────────────

        cmd.arg("--");
        cmd.args(["sh", "-c", &request.command]);

        // Capture stdout/stderr.
        cmd.stdout(std::process::Stdio::piped());
        cmd.stderr(std::process::Stdio::piped());

        // ── Execute with timeout ─────────────────────────────────

        let timeout_secs = request
            .timeout
            .unwrap_or(DEFAULT_TIMEOUT)
            .min(MAX_TIMEOUT);

        let output = tokio::time::timeout(
            std::time::Duration::from_secs(timeout_secs),
            cmd.output(),
        )
        .await;

        let result = match output {
            Ok(Ok(output)) => {
                let mut stdout = String::from_utf8_lossy(&output.stdout).into_owned();
                let mut stderr = String::from_utf8_lossy(&output.stderr).into_owned();

                // Truncate to avoid unbounded memory usage.
                stdout.truncate(MAX_OUTPUT_BYTES);
                stderr.truncate(MAX_OUTPUT_BYTES);

                ExecResult {
                    stdout,
                    stderr,
                    exit_code: output.status.code().map(|c| c as i64).unwrap_or(-1),
                    timed_out: false,
                    output_files: None,
                }
            }
            Ok(Err(e)) => return Err(ContainerError::Io(e)),
            Err(_elapsed) => ExecResult {
                stdout: String::new(),
                stderr: format!("Command timed out after {timeout_secs}s"),
                exit_code: -1,
                timed_out: true,
                output_files: None,
            },
        };

        // ── Collect output files ─────────────────────────────────

        let mut result = result;
        let output_dir = request.output_dir.as_deref().unwrap_or("outputs");
        if !output_dir.is_empty() {
            let ws = Path::new(workspace_path);
            match crate::workspace::collect_output_files(ws, output_dir) {
                Ok(files) if !files.is_empty() => {
                    result.output_files = Some(files);
                }
                _ => {}
            }
        }

        Ok(result)
    }

    /// No-op — bwrap sandboxes are ephemeral; there is nothing to stop.
    async fn stop(&self, _runtime_id: &str) -> Result<(), ContainerError> {
        Ok(())
    }

    /// No-op — bwrap sandboxes are ephemeral; there is nothing to remove.
    async fn remove(&self, _runtime_id: &str) -> Result<(), ContainerError> {
        Ok(())
    }

    /// Check whether the workspace directory still exists.
    async fn inspect(
        &self,
        runtime_id: &str,
    ) -> Result<Option<RuntimeContainerState>, ContainerError> {
        let Some((_, workspace_path)) = decode_runtime_id(runtime_id) else {
            return Ok(None);
        };

        if !Path::new(workspace_path).is_dir() {
            return Ok(None);
        }

        Ok(Some(RuntimeContainerState {
            // bwrap sandboxes are always "ready" — we spawn on demand.
            running: true,
            ip_address: None,
            image: Some("host".into()),
            workspace_bind: Some(workspace_path.to_string()),
        }))
    }

    /// Not supported — each exec is independent; there are no persistent
    /// logs to retrieve.
    async fn logs(
        &self,
        _runtime_id: &str,
        _tail: Option<usize>,
    ) -> Result<String, ContainerError> {
        Ok(String::new())
    }

    /// Returns an empty list — bwrap has no long-lived managed containers.
    async fn list_managed(&self) -> Result<Vec<ManagedContainer>, ContainerError> {
        Ok(Vec::new())
    }

    /// No-op — bwrap uses the host system directly; there are no images to
    /// pull.
    async fn ensure_image(&self, _image: &str) -> Result<(), ContainerError> {
        Ok(())
    }

    /// No-op — nothing to tear down on shutdown.
    async fn shutdown(&self) {}
}

// ── Helpers ──────────────────────────────────────────────────────

/// Locate the `bwrap` binary on the system.
fn which_bwrap() -> Result<String, ContainerError> {
    for path in ["/usr/bin/bwrap", "/usr/local/bin/bwrap"] {
        if Path::new(path).exists() {
            return Ok(path.to_string());
        }
    }
    // Fall back to PATH lookup via `which`.
    if let Ok(output) = std::process::Command::new("which")
        .arg("bwrap")
        .output()
    {
        if output.status.success() {
            let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if !path.is_empty() {
                return Ok(path);
            }
        }
    }
    Err(ContainerError::ImagePull(
        "bwrap binary not found — install bubblewrap (e.g. `apt install bubblewrap`)".into(),
    ))
}

// ── Tests ────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn runtime_id_roundtrip_nonet() {
        let ws = Path::new("/tmp/clawkson-workspaces/agent-1/conv-2");
        let id = encode_runtime_id(false, ws);
        assert!(id.starts_with("bwrap:nonet:"));

        let (net, path) = decode_runtime_id(&id).unwrap();
        assert!(!net);
        assert_eq!(path, "/tmp/clawkson-workspaces/agent-1/conv-2");
    }

    #[test]
    fn runtime_id_roundtrip_net() {
        let ws = Path::new("/workspace/test");
        let id = encode_runtime_id(true, ws);

        let (net, path) = decode_runtime_id(&id).unwrap();
        assert!(net);
        assert_eq!(path, "/workspace/test");
    }

    #[test]
    fn decode_rejects_invalid_ids() {
        assert!(decode_runtime_id("docker:abc123").is_none());
        assert!(decode_runtime_id("bwrap:").is_none());
        assert!(decode_runtime_id("bwrap:maybe:/tmp").is_none());
        assert!(decode_runtime_id("bwrap:net:").is_none());
        assert!(decode_runtime_id("garbage").is_none());
    }

    #[test]
    fn decode_handles_colons_in_path() {
        // Edge case: workspace path itself contains a colon.
        let id = "bwrap:nonet:/mnt/data:disk1/workspace";
        let (net, path) = decode_runtime_id(id).unwrap();
        assert!(!net);
        assert_eq!(path, "/mnt/data:disk1/workspace");
    }
}
