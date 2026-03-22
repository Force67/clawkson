use axum::Router;

/// Extension trait for plugins that provide HTTP API routes.
pub trait RouteProvider: Send + Sync {
    /// URL prefix for this plugin's routes (e.g. "/api/plugins/canvas").
    fn prefix(&self) -> &str;

    /// Return an Axum router with the plugin's routes.
    /// The router receives `()` state — plugins should capture their own state via Arc.
    fn routes(&self) -> Router;
}
