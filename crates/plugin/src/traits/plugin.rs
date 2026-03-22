use std::collections::HashSet;
use std::fmt;

use serde::{Deserialize, Serialize};

use crate::PluginContext;

/// Capabilities a plugin can declare.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PluginCapability {
    /// Provides tools to agents.
    Tools,
    /// Provides a messaging channel (e.g. Discord, Telegram).
    Channel,
    /// Provides an LLM backend.
    LlmProvider,
    /// Provides HTTP API routes.
    Routes,
    /// Provides a web search backend.
    Search,
    /// Hooks into the context engine pipeline.
    ContextEngine,
}

/// Static manifest describing a plugin.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginManifest {
    /// Unique plugin identifier (e.g. "discord-channel", "brave-search").
    pub name: String,
    /// Human-readable display name.
    pub display_name: String,
    /// Short description of what this plugin does.
    pub description: String,
    /// Semver version string.
    pub version: String,
    /// Other plugin names this plugin depends on.
    #[serde(default)]
    pub dependencies: Vec<String>,
    /// Capabilities this plugin declares.
    pub capabilities: HashSet<PluginCapability>,
    /// Optional frontend manifest for dynamic UI.
    #[serde(default)]
    pub frontend: Option<FrontendManifest>,
}

/// Frontend metadata so the UI can render plugin pages/panels.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FrontendManifest {
    /// Sidebar navigation items to inject.
    #[serde(default)]
    pub sidebar_items: Vec<SidebarItem>,
    /// Route definitions for plugin pages.
    #[serde(default)]
    pub routes: Vec<PluginRoute>,
    /// Settings panel definitions.
    #[serde(default)]
    pub settings_panels: Vec<SettingsPanel>,
    /// Connector card definitions.
    #[serde(default)]
    pub connector_cards: Vec<ConnectorCard>,
    /// URL to a JS bundle for custom components (served by the plugin's RouteProvider).
    pub bundle_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SidebarItem {
    /// Label shown in the sidebar.
    pub label: String,
    /// Route path (e.g. "/canvas").
    pub path: String,
    /// Lucide icon name (e.g. "palette", "radio").
    pub icon: String,
    /// Which nav group to place in (e.g. "overview", "resources", "infrastructure").
    #[serde(default = "default_group")]
    pub group: String,
}

fn default_group() -> String {
    "resources".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginRoute {
    /// URL path (e.g. "/canvas").
    pub path: String,
    /// Component name to load from the bundle.
    pub component: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SettingsPanel {
    /// Tab label in Settings.
    pub label: String,
    /// Component name to load from the bundle.
    pub component: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectorCard {
    /// Connector type name this card handles.
    pub connector_type: String,
    /// Component name to load from the bundle.
    pub component: String,
    /// Display name for the connector type.
    pub display_name: String,
    /// Lucide icon name.
    pub icon: String,
}

/// Base trait all plugins implement.
#[async_trait::async_trait]
pub trait ClawksonPlugin: Send + Sync + fmt::Debug {
    /// Return the static manifest for this plugin.
    fn manifest(&self) -> &PluginManifest;

    /// Initialize the plugin with the given context.
    /// Called once at startup after all dependencies are registered.
    async fn init(&self, ctx: &PluginContext) -> anyhow::Result<()>;

    /// Graceful shutdown. Called on server shutdown.
    async fn shutdown(&self) -> anyhow::Result<()> {
        Ok(())
    }

    /// SQL migration statements to run (in order) for this plugin.
    /// Each entry is a single SQL statement. The registry tracks which have been applied.
    fn migrations(&self) -> Vec<&str> {
        Vec::new()
    }
}
