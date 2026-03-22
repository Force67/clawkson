/// MCP (Model Context Protocol) Bridge plugin for Clawkson.
/// Manages connections to external MCP servers and exposes their tools.
use std::collections::HashSet;
use std::sync::Arc;

use clawkson_plugin::{
    ClawksonPlugin, PluginCapability, PluginContext, PluginManifest,
    ToolContext, ToolProvider,
};
use denkwerk::DynKernelFunction;
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;

/// Configuration for an individual MCP server connection.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpServerConfig {
    /// Human-readable name for this MCP server.
    pub name: String,
    /// Command to launch the MCP server process.
    pub command: String,
    /// Arguments to pass to the command.
    #[serde(default)]
    pub args: Vec<String>,
    /// Environment variables to set for the process.
    #[serde(default)]
    pub env: std::collections::HashMap<String, String>,
}

/// Top-level config for the MCP bridge plugin.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpBridgeConfig {
    /// List of MCP servers to connect to.
    #[serde(default)]
    pub mcp_servers: Vec<McpServerConfig>,
}

/// Represents a connected MCP server and its discovered tools.
#[derive(Debug)]
struct McpConnection {
    config: McpServerConfig,
    /// Process handle (if running).
    _process: Option<tokio::process::Child>,
}

#[derive(Debug)]
pub struct McpBridgePlugin {
    manifest: PluginManifest,
    connections: Arc<RwLock<Vec<McpConnection>>>,
}

impl McpBridgePlugin {
    pub fn new() -> Self {
        let mut caps = HashSet::new();
        caps.insert(PluginCapability::Tools);

        Self {
            manifest: PluginManifest {
                name: "mcp-bridge".to_string(),
                display_name: "MCP Bridge".to_string(),
                description: "Bridge to Model Context Protocol servers, exposing their tools to agents.".to_string(),
                version: "0.1.0".to_string(),
                dependencies: vec![],
                capabilities: caps,
                frontend: None,
            },
            connections: Arc::new(RwLock::new(Vec::new())),
        }
    }

    /// Connect to all configured MCP servers.
    pub async fn connect_servers(&self, config: &McpBridgeConfig) -> anyhow::Result<()> {
        let mut conns = self.connections.write().await;
        for server in &config.mcp_servers {
            tracing::info!(
                name = %server.name,
                command = %server.command,
                "MCP Bridge: would start MCP server process (stub)"
            );
            conns.push(McpConnection {
                config: server.clone(),
                _process: None,
            });
        }
        Ok(())
    }
}

impl Default for McpBridgePlugin {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl ClawksonPlugin for McpBridgePlugin {
    fn manifest(&self) -> &PluginManifest {
        &self.manifest
    }

    async fn init(&self, _ctx: &PluginContext) -> anyhow::Result<()> {
        tracing::info!("MCP Bridge plugin initialized");
        Ok(())
    }

    async fn shutdown(&self) -> anyhow::Result<()> {
        let mut conns = self.connections.write().await;
        for conn in conns.drain(..) {
            tracing::info!(name = %conn.config.name, "MCP Bridge: disconnecting server");
            if let Some(mut process) = conn._process {
                let _ = process.kill().await;
            }
        }
        tracing::info!("MCP Bridge plugin shut down");
        Ok(())
    }
}

#[async_trait::async_trait]
impl ToolProvider for McpBridgePlugin {
    async fn tools(&self, _ctx: &ToolContext) -> Vec<DynKernelFunction> {
        let conns = self.connections.read().await;
        tracing::info!(
            server_count = conns.len(),
            "MCP Bridge: listing tools from connected servers (stub)"
        );

        // In a full implementation, this would:
        // 1. Send `tools/list` JSON-RPC request to each connected MCP server
        // 2. Convert each MCP tool definition to a DynKernelFunction
        // 3. Route tool calls back to the originating MCP server via `tools/call`
        Vec::new()
    }
}
