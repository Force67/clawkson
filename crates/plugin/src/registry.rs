use std::collections::HashMap;
use std::sync::Arc;

use axum::Router;
use denkwerk::{DynKernelFunction, LLMProvider};
use serde_json::Value;
use tokio::sync::RwLock;
use tracing;

use crate::event_bus::EventBus;
use crate::traits::*;

/// Shared plugin context provided to all plugins during init.
#[derive(Clone)]
pub struct PluginContext {
    pub db_pool: sqlx::PgPool,
    pub event_bus: EventBus,
    pub config: Value,
    pub data_dir: std::path::PathBuf,
}

/// Central registry holding all loaded plugins and typed sub-registries.
pub struct PluginRegistry {
    /// All registered plugins by name.
    plugins: RwLock<HashMap<String, Arc<dyn ClawksonPlugin>>>,
    /// Plugins that provide tools.
    tool_providers: RwLock<Vec<Arc<dyn ToolProvider>>>,
    /// Plugins that provide messaging channels.
    channel_providers: RwLock<HashMap<String, Arc<dyn ChannelProvider>>>,
    /// Plugins that provide LLM backends.
    llm_factories: RwLock<HashMap<String, Arc<dyn LlmProviderFactory>>>,
    /// Plugins that provide API routes.
    route_providers: RwLock<Vec<Arc<dyn RouteProvider>>>,
    /// Plugins that provide web search.
    search_providers: RwLock<HashMap<String, Arc<dyn SearchProvider>>>,
    /// Plugins that hook into the context engine.
    context_plugins: RwLock<Vec<Arc<dyn ContextEnginePlugin>>>,
    /// The shared event bus.
    pub event_bus: EventBus,
}

impl PluginRegistry {
    pub fn new() -> Self {
        Self {
            plugins: RwLock::new(HashMap::new()),
            tool_providers: RwLock::new(Vec::new()),
            channel_providers: RwLock::new(HashMap::new()),
            llm_factories: RwLock::new(HashMap::new()),
            route_providers: RwLock::new(Vec::new()),
            search_providers: RwLock::new(HashMap::new()),
            context_plugins: RwLock::new(Vec::new()),
            event_bus: EventBus::new(),
        }
    }

    /// Register and initialize a plugin.
    pub async fn register_plugin(
        &self,
        plugin: Arc<dyn ClawksonPlugin>,
        ctx: &PluginContext,
    ) -> anyhow::Result<()> {
        let manifest = plugin.manifest();
        let name = manifest.name.clone();
        let caps = &manifest.capabilities;

        tracing::info!(
            plugin = %name,
            version = %manifest.version,
            capabilities = ?caps,
            "registering plugin"
        );

        // Run plugin migrations
        let migrations = plugin.migrations();
        if !migrations.is_empty() {
            self.run_plugin_migrations(&ctx.db_pool, &name, &migrations)
                .await?;
        }

        // Initialize the plugin
        plugin.init(ctx).await?;

        // Store in main registry
        self.plugins.write().await.insert(name.clone(), plugin.clone());

        tracing::info!(plugin = %name, "plugin registered");
        Ok(())
    }

    /// Register a tool provider (typically called by the plugin during init).
    pub async fn register_tool_provider(&self, provider: Arc<dyn ToolProvider>) {
        self.tool_providers.write().await.push(provider);
    }

    /// Register a channel provider.
    pub async fn register_channel_provider(&self, provider: Arc<dyn ChannelProvider>) {
        let name = provider.channel_type().to_string();
        self.channel_providers.write().await.insert(name, provider);
    }

    /// Register an LLM provider factory.
    pub async fn register_llm_factory(&self, factory: Arc<dyn LlmProviderFactory>) {
        let name = factory.provider_type().to_string();
        tracing::info!(provider_type = %name, "registered LLM provider factory");
        self.llm_factories.write().await.insert(name, factory);
    }

    /// Register a route provider.
    pub async fn register_route_provider(&self, provider: Arc<dyn RouteProvider>) {
        self.route_providers.write().await.push(provider);
    }

    /// Register a search provider.
    pub async fn register_search_provider(&self, provider: Arc<dyn SearchProvider>) {
        let name = provider.provider_name().to_string();
        self.search_providers.write().await.insert(name, provider);
    }

    /// Register a context engine plugin.
    pub async fn register_context_plugin(&self, plugin: Arc<dyn ContextEnginePlugin>) {
        self.context_plugins.write().await.push(plugin);
    }

    /// Aggregate tools from all ToolProviders for a given context.
    pub async fn tools_for_context(&self, ctx: &ToolContext) -> Vec<DynKernelFunction> {
        let providers = self.tool_providers.read().await;
        let mut all_tools = Vec::new();
        for provider in providers.iter() {
            let tools = provider.tools(ctx).await;
            all_tools.extend(tools);
        }
        all_tools
    }

    /// Build an LLM provider from a type name and config.
    pub async fn build_llm_provider(
        &self,
        type_name: &str,
        config: Value,
        timeout: std::time::Duration,
    ) -> anyhow::Result<Option<Box<dyn LLMProvider>>> {
        let factories = self.llm_factories.read().await;
        match factories.get(type_name) {
            Some(factory) => Ok(Some(factory.build(config, timeout).await?)),
            None => Ok(None),
        }
    }

    /// Check if a given LLM provider type is supported (either built-in or plugin).
    pub async fn has_llm_factory(&self, type_name: &str) -> bool {
        self.llm_factories.read().await.contains_key(type_name)
    }

    /// Merge all RouteProviders into a single Router.
    pub async fn plugin_routes(&self) -> Router {
        let providers = self.route_providers.read().await;
        let mut router = Router::new();
        for provider in providers.iter() {
            let prefix = provider.prefix();
            let routes = provider.routes();
            router = router.nest(prefix, routes);
        }
        router
    }

    /// Get a channel provider by type name.
    pub async fn get_channel_provider(&self, channel_type: &str) -> Option<Arc<dyn ChannelProvider>> {
        self.channel_providers.read().await.get(channel_type).cloned()
    }

    /// Get a search provider by name.
    pub async fn get_search_provider(&self, name: &str) -> Option<Arc<dyn SearchProvider>> {
        self.search_providers.read().await.get(name).cloned()
    }

    /// Get all context engine plugins.
    pub async fn context_plugins(&self) -> Vec<Arc<dyn ContextEnginePlugin>> {
        self.context_plugins.read().await.clone()
    }

    /// Get all plugin manifests (for the API).
    pub async fn manifests(&self) -> Vec<PluginManifest> {
        self.plugins
            .read()
            .await
            .values()
            .map(|p| p.manifest().clone())
            .collect()
    }

    /// List all registered LLM provider type names.
    pub async fn llm_provider_types(&self) -> Vec<String> {
        self.llm_factories.read().await.keys().cloned().collect()
    }

    /// List all registered search provider names.
    pub async fn search_provider_names(&self) -> Vec<String> {
        self.search_providers.read().await.keys().cloned().collect()
    }

    /// Shutdown all plugins gracefully.
    pub async fn shutdown(&self) {
        let plugins = self.plugins.read().await;
        for (name, plugin) in plugins.iter() {
            if let Err(e) = plugin.shutdown().await {
                tracing::error!(plugin = %name, "plugin shutdown failed: {e}");
            }
        }
    }

    /// Run migrations for a plugin, tracking which have been applied.
    async fn run_plugin_migrations(
        &self,
        pool: &sqlx::PgPool,
        plugin_name: &str,
        migrations: &[&str],
    ) -> anyhow::Result<()> {
        // Get the current migration version for this plugin
        let current_version: i32 = sqlx::query_scalar(
            "SELECT COALESCE(MAX(version), 0) FROM plugin_migrations WHERE plugin_name = $1",
        )
        .bind(plugin_name)
        .fetch_one(pool)
        .await
        .unwrap_or(0);

        for (i, sql) in migrations.iter().enumerate() {
            let version = (i + 1) as i32;
            if version <= current_version {
                continue;
            }
            tracing::info!(plugin = %plugin_name, version, "running plugin migration");
            sqlx::query(sql).execute(pool).await?;
            sqlx::query(
                "INSERT INTO plugin_migrations (plugin_name, version, applied_at) VALUES ($1, $2, now())",
            )
            .bind(plugin_name)
            .bind(version)
            .execute(pool)
            .await?;
        }

        Ok(())
    }
}

impl Default for PluginRegistry {
    fn default() -> Self {
        Self::new()
    }
}
