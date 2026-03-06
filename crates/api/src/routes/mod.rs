pub mod agents;
pub mod conversations;
pub mod connectors;
pub mod knowledge;
pub mod tools;

use axum::Router;
use crate::state::AppState;

pub fn api_router() -> Router<AppState> {
    Router::new()
        .nest("/agents", agents::router())
        .nest("/conversations", conversations::router())
        .nest("/connectors", connectors::router())
        .nest("/knowledge", knowledge::router())
        .nest("/tools", tools::router())
}
