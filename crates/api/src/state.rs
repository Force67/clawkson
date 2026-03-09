use std::sync::Arc;
use tokio::sync::RwLock;

use clawkson_container::ContainerManager;
use clawkson_core::*;
use clawkson_db::Db;

use crate::s3::S3Storage;

/// Shared application state.
#[derive(Clone)]
pub struct AppState {
    pub db: Db,
    pub inner: Arc<RwLock<AppStateInner>>,
    /// Optional container manager — None if Docker is unavailable.
    pub container_manager: Option<Arc<ContainerManager>>,
    /// Optional S3-compatible object storage — None if not configured.
    pub s3: Option<Arc<S3Storage>>,
}

pub struct AppStateInner {
    // Agents, conversations, messages, LLM connectors, settings are now DB-backed.
    // Only non-persistent entities remain in-memory.
    pub connectors: Vec<Connector>,
    pub tools: Vec<Tool>,
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
            inner: Arc::new(RwLock::new(AppStateInner {
                connectors: Vec::new(),
                tools: Vec::new(),
            })),
        }
    }
}
