use thiserror::Error;

#[derive(Debug, Error)]
pub enum ContainerError {
    #[error("Docker error: {0}")]
    Docker(#[from] bollard::errors::Error),

    #[error("Container not found: {0}")]
    NotFound(uuid::Uuid),

    #[error("Container not running: {0}")]
    NotRunning(uuid::Uuid),

    #[error("Execution timed out after {0}s")]
    Timeout(u64),

    #[error("Output exceeded maximum size ({0} bytes)")]
    OutputTooLarge(usize),

    #[error("Image pull failed: {0}")]
    ImagePull(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Path escapes workspace: {0}")]
    PathEscape(String),

    #[error("Runtime unavailable: {0}")]
    RuntimeUnavailable(String),
}
