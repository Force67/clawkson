pub mod admin;
pub mod agents;
pub mod audit;
pub mod auth;
pub mod calendar;
pub mod containers;
pub mod conversations;
pub mod connectors;
pub mod knowledge;
pub mod scheduled_tasks;
pub mod shares;
pub mod skills;
pub mod tools;
pub mod llm_connectors;
pub mod settings;
pub mod uploads;
pub mod workspace;

use axum::Router;
use crate::state::AppState;

pub fn api_router() -> Router<AppState> {
    Router::new()
        .nest("/auth", auth::router())
        .nest("/admin", admin::router())
        .nest("/agents", agents::router())
        .nest("/conversations", conversations::router())
        .nest("/connectors", connectors::router())
        .nest("/knowledge", knowledge::router())
        .nest("/tools", tools::router())
        .nest("/llm-connectors", llm_connectors::router())
        .nest("/settings", settings::router())
        .nest("/containers", containers::list_router())
        .nest("/agents/{id}/container", containers::router())
        .nest("/agents/{id}/container", workspace::router())
        .nest("/skills", skills::router())
        .nest("/calendar", calendar::router())
        .nest("/scheduled-tasks", scheduled_tasks::router())
        .nest("/uploads", uploads::router())
        .nest("/audit-log", audit::router())
        .nest("/policy-presets", audit::presets_router())
        .merge(shares::router())
}
