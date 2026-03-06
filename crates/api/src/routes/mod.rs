pub mod agents;
pub mod conversations;
pub mod connectors;
pub mod knowledge;
pub mod tools;
pub mod llm_connectors;
pub mod settings;

use axum::Router;
use crate::state::AppState;

pub fn api_router() -> Router<AppState> {
    Router::new()
        .nest("/agents", agents::router())
        .nest("/conversations", conversations::router())
        .nest("/connectors", connectors::router())
        .nest("/knowledge", knowledge::router())
        .nest("/tools", tools::router())
        .nest("/llm-connectors", llm_connectors::router())
        .nest("/settings", settings::router())
}
