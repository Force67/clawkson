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

    // ── SOUL.md — platform base prompt ───────────────────────────
    // Read SOUL.md from the repo root (next to the binary or CWD) and seed
    // Settings.agent_base_prompt. The file content after the first `---`
    // separator is used as the prompt, so the markdown preamble is excluded.
    seed_soul_prompt(&db).await;

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

    // ── PDF rendering check ────────────────────────────────────────
    if clawkson_api::pdf::check_poppler_available().await {
        tracing::info!("PDF vision rendering ready (poppler-utils)");
    } else {
        tracing::warn!("poppler-utils not installed — PDF pages will use text extraction fallback (install poppler-utils for vision)");
    }

    // ── HTTP server ───────────────────────────────────────────────
    let state = clawkson_api::state::AppState::new(db, container_manager.clone(), s3);

    // ── Telegram bot pollers ──────────────────────────────────────
    clawkson_api::telegram::boot_pollers(&state, &state.telegram).await;

    let frontend_origin = std::env::var("FRONTEND_ORIGIN")
        .unwrap_or_else(|_| "http://localhost:5173".to_string());
    let cors = CorsLayer::new()
        .allow_origin(frontend_origin.parse::<HeaderValue>().unwrap())
        .allow_methods([Method::GET, Method::POST, Method::PATCH, Method::DELETE, Method::OPTIONS])
        .allow_headers([header::CONTENT_TYPE, header::COOKIE, header::AUTHORIZATION])
        .allow_credentials(true);

    let tg_shutdown = state.telegram.clone();

    let app = Router::new()
        .nest("/api", clawkson_api::routes::api_router())
        .layer(DefaultBodyLimit::max(16 * 1024 * 1024)) // 16 MB
        .layer(cors)
        .layer(TraceLayer::new_for_http())
        .with_state(state);

    let addr = "0.0.0.0:47821";
    tracing::info!("Clawkson listening on {addr}");

    let listener = tokio::net::TcpListener::bind(addr).await?;

    // Graceful shutdown: stop containers and telegram pollers on SIGTERM/SIGINT
    let cm_shutdown = container_manager.clone();
    axum::serve(listener, app)
        .with_graceful_shutdown(async move {
            let _ = tokio::signal::ctrl_c().await;
            tracing::info!("shutdown signal received");
            tg_shutdown.shutdown().await;
            if let Some(cm) = cm_shutdown {
                cm.shutdown().await;
            }
        })
        .await?;

    Ok(())
}

/// Read SOUL.md and write the prompt portion to Settings.agent_base_prompt.
///
/// The file is searched for in the following order:
///   1. The path given by the `CLAWKSON_SOUL_PATH` environment variable
///   2. `SOUL.md` relative to the current working directory
///
/// Everything after the first `---` separator line is treated as the prompt body.
/// If the file is missing, a warning is logged and the DB value is left unchanged.
async fn seed_soul_prompt(db: &clawkson_db::Db) {
    let path = std::env::var("CLAWKSON_SOUL_PATH")
        .unwrap_or_else(|_| "SOUL.md".to_string());

    let raw = match tokio::fs::read_to_string(&path).await {
        Ok(s) => s,
        Err(e) => {
            tracing::warn!(path = %path, "SOUL.md not found, skipping base prompt seed: {e}");
            return;
        }
    };

    // Strip the markdown preamble — everything up to and including the first `---` line.
    let prompt = match raw.split_once("\n---\n") {
        Some((_, body)) => body.trim().to_string(),
        None => raw.trim().to_string(),
    };

    match clawkson_db::settings::update(db, None, None, None, Some(&prompt), None, None, None, None).await {
        Ok(_) => tracing::info!(path = %path, chars = prompt.len(), "agent base prompt seeded from SOUL.md"),
        Err(e) => tracing::error!("failed to seed agent base prompt: {e}"),
    }
}
