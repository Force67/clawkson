use std::sync::Arc;
use tokio::sync::RwLock;

use clawkson_core::*;
use clawkson_db::Db;

/// Shared application state.
#[derive(Clone)]
pub struct AppState {
    pub db: Db,
    pub inner: Arc<RwLock<AppStateInner>>,
}

pub struct AppStateInner {
    pub agents: Vec<Agent>,
    pub conversations: Vec<Conversation>,
    pub messages: Vec<Message>,
    pub knowledge: Vec<KnowledgeEntry>,
    pub connectors: Vec<Connector>,
    pub tools: Vec<Tool>,
    pub llm_connectors: Vec<LlmConnector>,
    pub settings: Settings,
}

impl AppState {
    pub fn new(db: Db) -> Self {
        Self {
            db,
            inner: Arc::new(RwLock::new(AppStateInner {
                agents: Vec::new(),
                conversations: Vec::new(),
                messages: Vec::new(),
                knowledge: Vec::new(),
                connectors: Vec::new(),
                tools: Vec::new(),
                llm_connectors: Vec::new(),
                settings: Settings {
                    default_llm_connector_id: None,
                    theme: "dark".to_string(),
                },
            })),
        }
    }
}
