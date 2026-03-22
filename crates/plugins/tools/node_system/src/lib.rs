/// Node System plugin for Clawkson.
/// Provides tools for interacting with device capabilities such as camera,
/// screen capture, clipboard, location, and filesystem browsing.
/// Each capability requires explicit user approval before use.
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use clawkson_plugin::{
    ClawksonPlugin, PluginCapability, PluginContext, PluginManifest,
    ToolContext, ToolProvider,
};
use denkwerk::DynKernelFunction;
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;
use uuid::Uuid;

// ── Configuration ───────────────────────────────────────────────

/// Configuration for the node system plugin.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeSystemConfig {
    /// List of capabilities that are enabled (e.g. "camera", "screen_capture",
    /// "clipboard_read", "clipboard_write", "location", "filesystem_browse").
    #[serde(default)]
    pub allowed_capabilities: Vec<String>,
    /// Whether to require explicit user approval for each capability invocation.
    #[serde(default = "default_require_approval")]
    pub require_user_approval: bool,
}

fn default_require_approval() -> bool {
    true
}

impl Default for NodeSystemConfig {
    fn default() -> Self {
        Self {
            allowed_capabilities: vec![
                "camera".to_string(),
                "screen_capture".to_string(),
                "clipboard_read".to_string(),
                "clipboard_write".to_string(),
                "location".to_string(),
                "filesystem_browse".to_string(),
            ],
            require_user_approval: true,
        }
    }
}

// ── Capability Approval Tracking ────────────────────────────────

/// Tracks per-user, per-capability approval status.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapabilityApproval {
    pub user_id: Uuid,
    pub capability: String,
    pub approved: bool,
    pub approved_at: Option<String>,
}

// ── Plugin ──────────────────────────────────────────────────────

#[derive(Debug)]
pub struct NodeSystemPlugin {
    manifest: PluginManifest,
    config: RwLock<NodeSystemConfig>,
    /// Per-user capability approvals: user_id -> (capability -> approved).
    approvals: Arc<RwLock<HashMap<Uuid, HashMap<String, bool>>>>,
}

impl NodeSystemPlugin {
    pub fn new() -> Self {
        let mut caps = HashSet::new();
        caps.insert(PluginCapability::Tools);

        Self {
            manifest: PluginManifest {
                name: "node-system".to_string(),
                display_name: "Node System".to_string(),
                description: "Device capability tools: camera, screen capture, clipboard, location, and filesystem browsing.".to_string(),
                version: "0.1.0".to_string(),
                dependencies: vec![],
                capabilities: caps,
                frontend: None,
            },
            config: RwLock::new(NodeSystemConfig::default()),
            approvals: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Update the plugin configuration.
    pub async fn set_config(&self, config: NodeSystemConfig) {
        tracing::info!(
            allowed = ?config.allowed_capabilities,
            require_approval = config.require_user_approval,
            "NodeSystem: config updated"
        );
        *self.config.write().await = config;
    }

    /// Check whether a capability is allowed by config.
    pub async fn is_capability_allowed(&self, capability: &str) -> bool {
        let config = self.config.read().await;
        config.allowed_capabilities.iter().any(|c| c == capability)
    }

    /// Check whether a user has approved a capability.
    pub async fn is_approved(&self, user_id: Uuid, capability: &str) -> bool {
        let approvals = self.approvals.read().await;
        approvals
            .get(&user_id)
            .and_then(|caps| caps.get(capability))
            .copied()
            .unwrap_or(false)
    }

    /// Record a user's approval for a capability.
    pub async fn approve_capability(&self, user_id: Uuid, capability: &str) {
        tracing::info!(
            user_id = %user_id,
            capability = %capability,
            "NodeSystem: user approved capability"
        );
        let mut approvals = self.approvals.write().await;
        approvals
            .entry(user_id)
            .or_default()
            .insert(capability.to_string(), true);
    }

    /// Revoke a user's approval for a capability.
    pub async fn revoke_capability(&self, user_id: Uuid, capability: &str) {
        tracing::info!(
            user_id = %user_id,
            capability = %capability,
            "NodeSystem: user revoked capability"
        );
        let mut approvals = self.approvals.write().await;
        if let Some(caps) = approvals.get_mut(&user_id) {
            caps.insert(capability.to_string(), false);
        }
    }
}

impl Default for NodeSystemPlugin {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl ClawksonPlugin for NodeSystemPlugin {
    fn manifest(&self) -> &PluginManifest {
        &self.manifest
    }

    async fn init(&self, _ctx: &PluginContext) -> anyhow::Result<()> {
        tracing::info!("Node System plugin initialized");
        Ok(())
    }

    async fn shutdown(&self) -> anyhow::Result<()> {
        tracing::info!("Node System plugin shut down");
        Ok(())
    }

    fn migrations(&self) -> Vec<&str> {
        vec![
            "CREATE TABLE IF NOT EXISTS node_capability_approvals (
                id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
                user_id UUID NOT NULL,
                capability TEXT NOT NULL,
                approved BOOLEAN NOT NULL DEFAULT FALSE,
                approved_at TIMESTAMPTZ,
                revoked_at TIMESTAMPTZ,
                created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
                UNIQUE (user_id, capability)
            )",
        ]
    }
}

#[async_trait::async_trait]
impl ToolProvider for NodeSystemPlugin {
    async fn tools(&self, _ctx: &ToolContext) -> Vec<DynKernelFunction> {
        tracing::info!("NodeSystem: listing tools (stub)");

        // In a full implementation, this would return up to six DynKernelFunctions
        // depending on which capabilities are enabled in the config:
        //
        // 1. device_camera(action: String) -> Object
        //    Captures a photo or video from the device camera.
        //    action: "photo" | "video" | "list_devices"
        //    Requires "camera" capability approval.
        //
        // 2. screen_capture(region?: Object) -> Object
        //    Captures a screenshot of the screen or a specific region.
        //    region: { x, y, width, height } (optional, captures full screen if omitted)
        //    Requires "screen_capture" capability approval.
        //
        // 3. clipboard_read() -> Object
        //    Reads the current contents of the system clipboard.
        //    Returns { text?: String, has_image: bool, formats: [String] }.
        //    Requires "clipboard_read" capability approval.
        //
        // 4. clipboard_write(content: String, format?: String) -> Object
        //    Writes content to the system clipboard.
        //    format: "text" (default) | "html" | "rtf"
        //    Requires "clipboard_write" capability approval.
        //
        // 5. get_location(high_accuracy?: bool) -> Object
        //    Gets the device's current geographic location.
        //    Returns { latitude, longitude, accuracy, altitude?, speed? }.
        //    Requires "location" capability approval.
        //
        // 6. filesystem_browse(path: String, recursive?: bool) -> Object
        //    Lists files and directories at the given path.
        //    Returns { entries: [{ name, path, is_dir, size, modified }] }.
        //    Requires "filesystem_browse" capability approval.
        //
        // Each tool checks:
        // - self.is_capability_allowed(cap) to verify the config permits it
        // - self.is_approved(user_id, cap) to verify user consent
        // - If require_user_approval is true and not approved, returns an approval prompt
        Vec::new()
    }
}
