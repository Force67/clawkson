use bollard::exec::{CreateExecOptions, StartExecResults};
use bollard::Docker;
use futures::StreamExt;

use crate::error::ContainerError;
use crate::models::ExecResult;

const MAX_OUTPUT_BYTES: usize = 64 * 1024; // 64KB

/// Execute a command inside a running container and capture output.
pub async fn exec_in_container(
    docker: &Docker,
    container_id: &str,
    command: Vec<&str>,
    timeout_secs: u64,
) -> Result<ExecResult, ContainerError> {
    let exec = docker
        .create_exec(
            container_id,
            CreateExecOptions {
                cmd: Some(command),
                attach_stdout: Some(true),
                attach_stderr: Some(true),
                working_dir: Some("/workspace"),
                ..Default::default()
            },
        )
        .await?;

    let output = tokio::time::timeout(
        std::time::Duration::from_secs(timeout_secs),
        collect_exec_output(docker, &exec.id),
    )
    .await;

    match output {
        Ok(result) => result,
        Err(_) => Ok(ExecResult {
            stdout: String::new(),
            stderr: format!("Command timed out after {timeout_secs}s"),
            exit_code: -1,
            timed_out: true,
        }),
    }
}

async fn collect_exec_output(
    docker: &Docker,
    exec_id: &str,
) -> Result<ExecResult, ContainerError> {
    let start = docker.start_exec(exec_id, None).await?;

    let mut stdout = String::new();
    let mut stderr = String::new();

    if let StartExecResults::Attached { mut output, .. } = start {
        while let Some(msg) = output.next().await {
            let msg = msg?;
            match msg {
                bollard::container::LogOutput::StdOut { message } => {
                    let text = String::from_utf8_lossy(&message);
                    if stdout.len() + text.len() <= MAX_OUTPUT_BYTES {
                        stdout.push_str(&text);
                    }
                }
                bollard::container::LogOutput::StdErr { message } => {
                    let text = String::from_utf8_lossy(&message);
                    if stderr.len() + text.len() <= MAX_OUTPUT_BYTES {
                        stderr.push_str(&text);
                    }
                }
                _ => {}
            }
        }
    }

    // Get exit code
    let inspect = docker.inspect_exec(exec_id).await?;
    let exit_code = inspect.exit_code.unwrap_or(-1);

    Ok(ExecResult {
        stdout,
        stderr,
        exit_code,
        timed_out: false,
    })
}
