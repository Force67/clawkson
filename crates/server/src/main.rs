use anyhow::Result;
use axum::{extract::DefaultBodyLimit, Router};
use clawkson_db::DbConfig;
use tower_http::cors::CorsLayer;
use axum::http::{HeaderValue, Method, header};
use tower_http::trace::TraceLayer;
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> Result<()> {
    dotenvy::dotenv().ok();

    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| "info,sqlx=warn".into()),
        )
        .init();

    // ── Database ──────────────────────────────────────────────────
    let db_config = DbConfig::from_env()?;

    tracing::info!(
        host = %db_config.host,
        port = %db_config.port,
        database = %db_config.database_name,
        "bootstrapping database",
    );

    let admin_db = clawkson_db::connect(db_config.admin_connect_options()?).await?;
    clawkson_db::bootstrap_database(&admin_db, &db_config).await?;

    let migration_db = clawkson_db::connect(db_config.migration_connect_options()?).await?;
    clawkson_db::run_migrations(&migration_db).await?;
    clawkson_db::grant_app_permissions(&migration_db, &db_config.database_user).await?;

    let db = clawkson_db::connect(db_config.app_connect_options()?).await?;

    tracing::info!(
        database = %db_config.database_name,
        user = %db_config.database_user,
        "database ready",
    );

    // ── Container manager ────────────────────────────────────────
    let workspace_root = std::env::var("CLAWKSON_WORKSPACE_ROOT")
        .unwrap_or_else(|_| "/tmp/clawkson-workspaces".to_string());

    let container_manager = match clawkson_container::ContainerManager::new(
        std::path::PathBuf::from(&workspace_root),
    )
    .await
    {
        Ok(cm) => {
            // Clean up orphans from previous runs
            if let Err(e) = cm.cleanup_orphans().await {
                tracing::warn!("failed to clean up orphan containers: {e}");
            }
            tracing::info!(%workspace_root, "container manager ready");
            Some(std::sync::Arc::new(cm))
        }
        Err(e) => {
            tracing::warn!("Docker not available, containers disabled: {e}");
            None
        }
    };

    // ── S3 storage ─────────────────────────────────────────────────
    let s3 = clawkson_api::s3::S3Storage::try_connect().await;
    if s3.is_none() {
        tracing::warn!("S3 storage unavailable — document storage disabled (is RustFS running?)");
    }

    // ── HTTP server ───────────────────────────────────────────────
    let state = clawkson_api::state::AppState::new(db, container_manager.clone(), s3);

    let frontend_origin = std::env::var("FRONTEND_ORIGIN")
        .unwrap_or_else(|_| "http://localhost:5173".to_string());
    let cors = CorsLayer::new()
        .allow_origin(frontend_origin.parse::<HeaderValue>().unwrap())
        .allow_methods([Method::GET, Method::POST, Method::PATCH, Method::DELETE, Method::OPTIONS])
        .allow_headers([header::CONTENT_TYPE, header::COOKIE, header::AUTHORIZATION])
        .allow_credentials(true);

    let app = Router::new()
        .nest("/api", clawkson_api::routes::api_router())
        .layer(DefaultBodyLimit::max(16 * 1024 * 1024)) // 16 MB
        .layer(cors)
        .layer(TraceLayer::new_for_http())
        .with_state(state);

    let addr = "0.0.0.0:47821";
    tracing::info!("Clawkson listening on {addr}");

    let listener = tokio::net::TcpListener::bind(addr).await?;

    // Graceful shutdown: stop containers on SIGTERM/SIGINT
    let cm_shutdown = container_manager.clone();
    axum::serve(listener, app)
        .with_graceful_shutdown(async move {
            let _ = tokio::signal::ctrl_c().await;
            tracing::info!("shutdown signal received");
            if let Some(cm) = cm_shutdown {
                cm.shutdown().await;
            }
        })
        .await?;

    Ok(())
}
