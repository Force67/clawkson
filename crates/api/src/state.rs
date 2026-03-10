use std::sync::Arc;

use clawkson_container::ContainerManager;
use clawkson_db::Db;

use crate::s3::S3Storage;

/// Shared application state.
#[derive(Clone)]
pub struct AppState {
    pub db: Db,
    /// Optional container manager — None if Docker is unavailable.
    pub container_manager: Option<Arc<ContainerManager>>,
    /// Optional S3-compatible object storage — None if not configured.
    pub s3: Option<Arc<S3Storage>>,
}

impl AppState {
    pub fn new(
        db: Db,
        container_manager: Option<Arc<ContainerManager>>,
        s3: Option<Arc<S3Storage>>,
    ) -> Self {
        Self {
            db,
            container_manager,
            s3,
        }
    }
}
