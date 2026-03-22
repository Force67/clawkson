/// Agent tool: install_wasm_plugin
///
/// Lets an agent compile and install a WASM plugin at runtime.
/// The agent provides the source code, which is compiled to WASM
/// in the sandbox container, then loaded into the runtime.
use std::sync::Arc;

use denkwerk::functions::{FunctionDefinition, FunctionParameter, KernelFunction};
use denkwerk::DynKernelFunction;
use serde_json::{json, Value};

use crate::runtime::WasmRuntime;

/// Tool that lets agents install WASM plugins from source code.
pub struct InstallWasmPluginTool {
    runtime: Arc<WasmRuntime>,
}

impl InstallWasmPluginTool {
    pub fn new(runtime: Arc<WasmRuntime>) -> Self {
        Self { runtime }
    }

    pub fn into_dyn(self) -> DynKernelFunction {
        Arc::new(self)
    }
}

#[async_trait::async_trait]
impl KernelFunction for InstallWasmPluginTool {
    fn definition(&self) -> FunctionDefinition {
        let mut def = FunctionDefinition::new("install_wasm_plugin")
            .with_description(
                "Install a WASM plugin from a .wasm file path in the workspace, or from raw \
                 base64-encoded WASM bytes. The plugin will be loaded and its tools will become \
                 available to you. Use this to extend your capabilities at runtime."
            );

        def.add_parameter(
            FunctionParameter::new("wasm_path", json!({"type": "string"}))
                .with_description("Path to a .wasm file in the workspace (relative to /workspace)")
                .optional(),
        );
        def.add_parameter(
            FunctionParameter::new("wasm_base64", json!({"type": "string"}))
                .with_description("Base64-encoded .wasm bytes (alternative to wasm_path)")
                .optional(),
        );
        def.add_parameter(
            FunctionParameter::new("config", json!({"type": "object"}))
                .with_description("Configuration key-value pairs for the plugin")
                .optional(),
        );
        def.add_parameter(
            FunctionParameter::new("network_enabled", json!({"type": "boolean"}))
                .with_description("Whether to allow network access (default: false)")
                .optional(),
        );
        def.add_parameter(
            FunctionParameter::new("source_code", json!({"type": "string"}))
                .with_description("Original source code that produced the .wasm (preserved for future reference/editing)")
                .optional(),
        );
        def.add_parameter(
            FunctionParameter::new("source_filename", json!({"type": "string"}))
                .with_description("Filename for the source code (e.g. 'plugin.wat', 'plugin.rs')")
                .optional(),
        );

        def
    }

    async fn invoke(&self, arguments: &Value) -> Result<Value, denkwerk::LLMError> {
        let wasm_path = arguments.get("wasm_path").and_then(|v| v.as_str());
        let wasm_base64 = arguments.get("wasm_base64").and_then(|v| v.as_str());
        let network_enabled = arguments
            .get("network_enabled")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let source_code = arguments.get("source_code").and_then(|v| v.as_str());
        let source_filename = arguments.get("source_filename").and_then(|v| v.as_str());

        // Parse config
        let config: std::collections::HashMap<String, String> = arguments
            .get("config")
            .and_then(|v| v.as_object())
            .map(|obj| {
                obj.iter()
                    .filter_map(|(k, v)| v.as_str().map(|s| (k.clone(), s.to_string())))
                    .collect()
            })
            .unwrap_or_default();

        let info = if let Some(path) = wasm_path {
            // Load from workspace file
            let full_path = std::path::PathBuf::from("/workspace").join(path);
            match self.runtime.load_plugin(&full_path, config, network_enabled).await {
                Ok(info) => info,
                Err(e) => return Ok(json!({"error": format!("load failed: {e}"), "path": path})),
            }
        } else if let Some(b64) = wasm_base64 {
            // Decode base64
            let bytes = match base64_decode(b64) {
                Ok(b) => b,
                Err(e) => return Ok(json!({"error": format!("base64 decode: {e}")})),
            };
            match self.runtime.load_plugin_bytes_with_source(
                &bytes,
                "<base64>".to_string(),
                config,
                network_enabled,
                source_code,
                source_filename,
            ).await {
                Ok(info) => info,
                Err(e) => return Ok(json!({"error": format!("load failed: {e}")})),
            }
        } else {
            return Ok(json!({"error": "provide either wasm_path or wasm_base64"}));
        };

        Ok(json!({
            "status": "ok",
            "plugin": info.name,
            "description": info.description,
            "version": info.version,
            "tools_installed": info.tools.iter().map(|t| &t.name).collect::<Vec<_>>(),
            "tool_count": info.tools.len(),
        }))
    }
}

/// Simple base64 decoder (avoids adding another dep).
fn base64_decode(input: &str) -> Result<Vec<u8>, String> {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

    let input = input.trim().replace(['\n', '\r', ' '], "");
    let mut output = Vec::with_capacity(input.len() * 3 / 4);
    let mut buf = 0u32;
    let mut bits = 0u32;

    for ch in input.bytes() {
        let val = if ch == b'=' {
            break;
        } else if let Some(pos) = TABLE.iter().position(|&b| b == ch) {
            pos as u32
        } else {
            return Err(format!("invalid base64 char: {}", ch as char));
        };

        buf = (buf << 6) | val;
        bits += 6;

        if bits >= 8 {
            bits -= 8;
            output.push((buf >> bits) as u8);
            buf &= (1 << bits) - 1;
        }
    }

    Ok(output)
}
