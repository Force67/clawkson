/// Host functions exposed to WASM plugins.
///
/// These are the capabilities that plugins can call:
/// - log(level, message)
/// - fetch_url(url) -> Result<string>
/// - read_file(path) -> Result<string>
/// - write_file(path, content) -> Result<()>
/// - list_dir(path) -> Result<Vec<string>>
/// - get_config(key) -> Option<string>
use std::collections::HashMap;
use std::path::PathBuf;

/// Per-plugin host state. Each loaded plugin gets its own instance.
#[derive(Clone)]
pub struct PluginHostState {
    /// Plugin name (for logging).
    pub plugin_name: String,
    /// Workspace directory for this plugin (sandboxed).
    pub workspace: PathBuf,
    /// Configuration key-value pairs from plugin settings.
    pub config: HashMap<String, String>,
    /// Whether network access is allowed.
    pub network_enabled: bool,
}

impl PluginHostState {
    pub fn new(
        plugin_name: String,
        workspace: PathBuf,
        config: HashMap<String, String>,
        network_enabled: bool,
    ) -> Self {
        Self {
            plugin_name,
            workspace,
            config,
            network_enabled,
        }
    }

    /// Log a message from the plugin.
    pub fn log(&self, level: &str, message: &str) {
        match level {
            "debug" => tracing::debug!(plugin = %self.plugin_name, "{message}"),
            "info" => tracing::info!(plugin = %self.plugin_name, "{message}"),
            "warn" => tracing::warn!(plugin = %self.plugin_name, "{message}"),
            "error" => tracing::error!(plugin = %self.plugin_name, "{message}"),
            _ => tracing::info!(plugin = %self.plugin_name, level, "{message}"),
        }
    }

    /// Fetch a URL (if network access is enabled).
    pub async fn fetch_url(&self, url: &str) -> Result<String, String> {
        if !self.network_enabled {
            return Err("network access not enabled for this plugin".to_string());
        }

        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .user_agent(format!("Clawkson-Plugin/{}", self.plugin_name))
            .build()
            .map_err(|e| format!("http client error: {e}"))?;

        let response = client
            .get(url)
            .send()
            .await
            .map_err(|e| format!("fetch failed: {e}"))?;

        if !response.status().is_success() {
            return Err(format!("HTTP {}", response.status().as_u16()));
        }

        response.text().await.map_err(|e| format!("read body: {e}"))
    }

    /// Read a file from the plugin workspace.
    pub fn read_file(&self, path: &str) -> Result<String, String> {
        let full_path = self.workspace.join(path);
        // Prevent path traversal
        if !full_path.starts_with(&self.workspace) {
            return Err("path traversal not allowed".to_string());
        }
        std::fs::read_to_string(&full_path).map_err(|e| format!("read: {e}"))
    }

    /// Write a file to the plugin workspace.
    pub fn write_file(&self, path: &str, content: &str) -> Result<(), String> {
        let full_path = self.workspace.join(path);
        if !full_path.starts_with(&self.workspace) {
            return Err("path traversal not allowed".to_string());
        }
        // Create parent directories
        if let Some(parent) = full_path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| format!("mkdir: {e}"))?;
        }
        std::fs::write(&full_path, content).map_err(|e| format!("write: {e}"))
    }

    /// List files in a workspace directory.
    pub fn list_dir(&self, path: &str) -> Result<Vec<String>, String> {
        let full_path = self.workspace.join(path);
        if !full_path.starts_with(&self.workspace) {
            return Err("path traversal not allowed".to_string());
        }
        let entries = std::fs::read_dir(&full_path).map_err(|e| format!("readdir: {e}"))?;
        let mut names = Vec::new();
        for entry in entries {
            let entry = entry.map_err(|e| format!("entry: {e}"))?;
            if let Some(name) = entry.file_name().to_str() {
                names.push(name.to_string());
            }
        }
        Ok(names)
    }

    /// Get a config value.
    pub fn get_config(&self, key: &str) -> Option<String> {
        self.config.get(key).cloned()
    }
}
