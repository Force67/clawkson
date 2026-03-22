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

    // ── Sync built-in skills ─────────────────────────────────────
    clawkson_api::routes::skills::sync_builtin_skills(&db).await;

    // ── Container runtime + manager ─────────────────────────────
    let workspace_root = std::env::var("CLAWKSON_WORKSPACE_ROOT")
        .unwrap_or_else(|_| "/tmp/clawkson-workspaces".to_string());

    // Select runtime: try Docker first, fall back to bwrap.
    let runtime: Option<std::sync::Arc<dyn clawkson_container::ContainerRuntime>> =
        match clawkson_container::docker::DockerRuntime::new().await {
            Ok(rt) => {
                tracing::info!("using Docker container runtime");
                Some(std::sync::Arc::new(rt))
            }
            Err(e) => {
                tracing::warn!("Docker not available: {e}, trying bwrap");
                match clawkson_container::bwrap::BwrapRuntime::new() {
                    Ok(rt) => {
                        tracing::info!("using bwrap container runtime");
                        Some(std::sync::Arc::new(rt))
                    }
                    Err(e2) => {
                        tracing::warn!("bwrap not available: {e2}, containers disabled");
                        None
                    }
                }
            }
        };

    let container_manager = runtime.and_then(|rt| {
        match clawkson_container::ContainerManager::new(
            rt,
            std::path::PathBuf::from(&workspace_root),
        ) {
            Ok(cm) => {
                tracing::info!(%workspace_root, runtime = cm.runtime_name(), "container manager ready");
                Some(std::sync::Arc::new(cm))
            }
            Err(e) => {
                tracing::warn!("container manager init failed: {e}");
                None
            }
        }
    });

    // Clean up orphans from previous runs
    if let Some(cm) = &container_manager {
        if let Err(e) = cm.cleanup_orphans().await {
            tracing::warn!("failed to clean up orphan containers: {e}");
        }
    }

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

    // ── Plugin system ────────────────────────────────────────────
    let plugin_registry = std::sync::Arc::new(clawkson_plugin::PluginRegistry::new());
    tracing::info!("plugin registry initialized");

    // ── WASM runtime ──────────────────────────────────────────────
    let wasm_workspace = std::path::PathBuf::from(
        std::env::var("CLAWKSON_WASM_ROOT")
            .unwrap_or_else(|_| format!("{workspace_root}/wasm-plugins")),
    );
    let wasm_runtime = match clawkson_wasm_runtime::WasmRuntime::new(wasm_workspace.clone()) {
        Ok(rt) => {
            tracing::info!(workspace = %wasm_workspace.display(), "WASM plugin runtime ready");
            std::sync::Arc::new(rt)
        }
        Err(e) => {
            tracing::warn!("WASM runtime init failed: {e}, creating with /tmp fallback");
            std::sync::Arc::new(
                clawkson_wasm_runtime::WasmRuntime::new("/tmp/clawkson-wasm".into())
                    .expect("fallback WASM runtime must succeed"),
            )
        }
    };

    // ── HTTP server ───────────────────────────────────────────────
    let state = clawkson_api::state::AppState::new(db, container_manager.clone(), s3, plugin_registry, wasm_runtime);

    // ── Telegram bot pollers ──────────────────────────────────────
    clawkson_api::telegram::boot_pollers(&state, &state.telegram).await;

    // ── Scheduled task runner ───────────────────────────────────────
    state.scheduler.start(state.clone());
    tracing::info!("scheduled task runner started");

    // ── Stale generation cleanup ────────────────────────────────────
    {
        let gens = state.generations.clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(std::time::Duration::from_secs(300));
            loop {
                interval.tick().await;
                gens.cleanup_stale(std::time::Duration::from_secs(1800));
            }
        });
    }

    let frontend_origin = std::env::var("FRONTEND_ORIGIN")
        .unwrap_or_else(|_| "http://localhost:5173".to_string());
    let cors = CorsLayer::new()
        .allow_origin(frontend_origin.parse::<HeaderValue>().unwrap())
        .allow_methods([Method::GET, Method::POST, Method::PATCH, Method::DELETE, Method::OPTIONS])
        .allow_headers([header::CONTENT_TYPE, header::COOKIE, header::AUTHORIZATION])
        .allow_credentials(true);

    let tg_shutdown = state.telegram.clone();
    let sched_shutdown = state.scheduler.clone();

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
            sched_shutdown.shutdown().await;
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
