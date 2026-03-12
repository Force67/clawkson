use std::sync::Arc;

use clawkson_container::ContainerManager;
use clawkson_db::Db;

use crate::memory::MemoryEmbedder;
use crate::s3::S3Storage;

/// Shared application state.
#[derive(Clone)]
pub struct AppState {
    pub db: Db,
    /// Optional container manager — None if Docker is unavailable.
    pub container_manager: Option<Arc<ContainerManager>>,
    /// Optional S3-compatible object storage — None if not configured.
    pub s3: Option<Arc<S3Storage>>,
    /// Debounced conversation memory embedder.
    pub memory: MemoryEmbedder,
}

impl AppState {
    pub fn new(
        db: Db,
        container_manager: Option<Arc<ContainerManager>>,
        s3: Option<Arc<S3Storage>>,
    ) -> Self {
        let memory = MemoryEmbedder::new(db.clone());
        Self {
            db,
            container_manager,
            s3,
            memory,
        }
    }
}
