/// WASM plugin runtime: loads .wasm modules, links host functions, executes tool calls.
///
/// Uses wasmtime with WASI for sandboxed filesystem access.
/// Each plugin gets its own Store (isolated memory) and workspace directory.
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;
use wasmtime::*;

use crate::host::PluginHostState;

/// Metadata about a loaded WASM plugin.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WasmPluginInfo {
    pub name: String,
    pub description: String,
    pub version: String,
    pub tools: Vec<WasmToolDef>,
    /// Path to the .wasm file.
    pub wasm_path: String,
}

/// A tool definition exported by a WASM plugin.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WasmToolDef {
    pub name: String,
    pub description: String,
    pub parameters_schema: serde_json::Value,
}

/// A loaded and ready-to-execute WASM plugin instance.
struct LoadedPlugin {
    info: WasmPluginInfo,
    module: Module,
    host_state: PluginHostState,
}

/// The WASM plugin runtime manages all loaded WASM plugins.
pub struct WasmRuntime {
    engine: Engine,
    /// Root directory for plugin workspaces.
    workspace_root: PathBuf,
    /// Loaded plugins indexed by name.
    plugins: RwLock<HashMap<String, Arc<LoadedPlugin>>>,
}

impl WasmRuntime {
    /// Create a new WASM runtime.
    pub fn new(workspace_root: PathBuf) -> Result<Self> {
        let mut config = Config::new();
        // Don't enable async_support — our host functions are sync.
        // Fuel limits prevent infinite loops (1 billion instructions).
        config.consume_fuel(true);

        let engine = Engine::new(&config)?;

        // Ensure workspace root exists
        std::fs::create_dir_all(&workspace_root)
            .context("create wasm workspace root")?;

        Ok(Self {
            engine,
            workspace_root,
            plugins: RwLock::new(HashMap::new()),
        })
    }

    /// Load a WASM plugin from a file path.
    pub async fn load_plugin(
        &self,
        wasm_path: &Path,
        config: HashMap<String, String>,
        network_enabled: bool,
    ) -> Result<WasmPluginInfo> {
        let wasm_bytes = tokio::fs::read(wasm_path)
            .await
            .context("read wasm file")?;

        self.load_plugin_bytes(&wasm_bytes, wasm_path.to_string_lossy().to_string(), config, network_enabled)
            .await
    }

    /// Load a WASM plugin from raw bytes.
    pub async fn load_plugin_bytes(
        &self,
        wasm_bytes: &[u8],
        source_path: String,
        config: HashMap<String, String>,
        network_enabled: bool,
    ) -> Result<WasmPluginInfo> {
        // Compile the module
        let module = Module::new(&self.engine, wasm_bytes)
            .context("compile wasm module")?;

        // Create a temporary store to call metadata functions
        let temp_host = PluginHostState::new(
            "loading".to_string(),
            self.workspace_root.join("_tmp"),
            config.clone(),
            false,
        );

        let (name, description, version, tools) =
            self.query_plugin_metadata(&module, &temp_host).await?;

        // Create the plugin workspace
        let workspace = self.workspace_root.join(&name);
        std::fs::create_dir_all(&workspace)
            .context("create plugin workspace")?;

        let host_state = PluginHostState::new(
            name.clone(),
            workspace,
            config,
            network_enabled,
        );

        let info = WasmPluginInfo {
            name: name.clone(),
            description,
            version,
            tools,
            wasm_path: source_path,
        };

        let loaded = Arc::new(LoadedPlugin {
            info: info.clone(),
            module,
            host_state,
        });

        self.plugins.write().await.insert(name.clone(), loaded);

        tracing::info!(
            plugin = %name,
            tools = info.tools.len(),
            "WASM plugin loaded"
        );

        Ok(info)
    }

    /// Invoke a tool on a loaded plugin.
    pub async fn invoke_tool(
        &self,
        plugin_name: &str,
        tool_name: &str,
        arguments_json: &str,
    ) -> Result<serde_json::Value> {
        let plugins = self.plugins.read().await;
        let plugin = plugins
            .get(plugin_name)
            .ok_or_else(|| anyhow::anyhow!("plugin '{}' not loaded", plugin_name))?
            .clone();
        drop(plugins); // Release read lock

        let mut store = self.create_store(&plugin.host_state)?;

        // Link host functions and instantiate
        let instance = self.instantiate(&mut store, &plugin.module)?;

        // Call invoke_tool(tool_name, arguments_json) -> (output_ptr, output_len, success, error_ptr, error_len)
        let invoke_fn = instance
            .get_typed_func::<(i32, i32, i32, i32), (i32, i32, i32, i32, i32)>(&mut store, "invoke_tool")
            .or_else(|_| {
                // Fallback: try simpler ABI
                Err(anyhow::anyhow!("invoke_tool export not found"))
            })?;

        let memory = instance
            .get_memory(&mut store, "memory")
            .ok_or_else(|| anyhow::anyhow!("no memory export"))?;

        // Write tool_name to WASM memory
        let alloc = instance
            .get_typed_func::<i32, i32>(&mut store, "alloc")
            .context("alloc export not found")?;

        let tool_name_ptr = alloc.call(&mut store, tool_name.len() as i32)?;
        memory.write(&mut store, tool_name_ptr as usize, tool_name.as_bytes())?;

        let args_ptr = alloc.call(&mut store, arguments_json.len() as i32)?;
        memory.write(&mut store, args_ptr as usize, arguments_json.as_bytes())?;

        let (out_ptr, out_len, success, err_ptr, err_len) = invoke_fn.call(
            &mut store,
            (tool_name_ptr, tool_name.len() as i32, args_ptr, arguments_json.len() as i32),
        )?;

        if success != 0 {
            let output = read_wasm_string(&store, &memory, out_ptr, out_len)?;
            let value: serde_json::Value = serde_json::from_str(&output)
                .unwrap_or(serde_json::Value::String(output));
            Ok(value)
        } else {
            let error = read_wasm_string(&store, &memory, err_ptr, err_len)?;
            Err(anyhow::anyhow!("plugin tool error: {}", error))
        }
    }

    /// List all loaded plugins.
    pub async fn list_plugins(&self) -> Vec<WasmPluginInfo> {
        self.plugins
            .read()
            .await
            .values()
            .map(|p| p.info.clone())
            .collect()
    }

    /// Unload a plugin by name.
    pub async fn unload_plugin(&self, name: &str) -> bool {
        self.plugins.write().await.remove(name).is_some()
    }

    /// Get tools from a specific plugin.
    pub async fn get_plugin_tools(&self, plugin_name: &str) -> Option<Vec<WasmToolDef>> {
        self.plugins
            .read()
            .await
            .get(plugin_name)
            .map(|p| p.info.tools.clone())
    }

    /// Get all tools from all loaded plugins.
    pub async fn all_tools(&self) -> Vec<(String, WasmToolDef)> {
        let plugins = self.plugins.read().await;
        let mut tools = Vec::new();
        for (name, plugin) in plugins.iter() {
            for tool in &plugin.info.tools {
                tools.push((name.clone(), tool.clone()));
            }
        }
        tools
    }

    // ── Internal helpers ──────────────────────────────────────────

    fn create_store(&self, host_state: &PluginHostState) -> Result<Store<PluginHostState>> {
        let mut store = Store::new(&self.engine, host_state.clone());
        // Give each invocation 1 billion fuel units
        store.set_fuel(1_000_000_000)?;
        Ok(store)
    }

    fn instantiate(
        &self,
        store: &mut Store<PluginHostState>,
        module: &Module,
    ) -> Result<Instance> {
        let mut linker = Linker::new(&self.engine);

        // Link WASI (for filesystem access)
        // Note: in production, use wasmtime_wasi::add_to_linker
        // For now, link our custom host functions

        // host.log(level_ptr, level_len, msg_ptr, msg_len)
        linker.func_wrap(
            "host",
            "log",
            |mut caller: Caller<'_, PluginHostState>, level_ptr: i32, level_len: i32, msg_ptr: i32, msg_len: i32| {
                let memory = caller.get_export("memory").and_then(|e| e.into_memory());
                if let Some(mem) = memory {
                    let level = read_str_from_memory(&caller, &mem, level_ptr, level_len).unwrap_or_default();
                    let msg = read_str_from_memory(&caller, &mem, msg_ptr, msg_len).unwrap_or_default();
                    caller.data().log(&level, &msg);
                }
            },
        )?;

        // host.read_file(path_ptr, path_len) -> (result_ptr, result_len, is_ok)
        linker.func_wrap(
            "host",
            "read_file",
            |mut caller: Caller<'_, PluginHostState>, path_ptr: i32, path_len: i32| -> (i32, i32, i32) {
                let memory = match caller.get_export("memory").and_then(|e| e.into_memory()) {
                    Some(m) => m,
                    None => return (0, 0, 0),
                };
                let path = match read_str_from_memory(&caller, &memory, path_ptr, path_len) {
                    Ok(p) => p,
                    Err(_) => return (0, 0, 0),
                };
                let result = caller.data().read_file(&path);
                match result {
                    Ok(content) => {
                        let len = content.len() as i32;
                        let mem_size = memory.data_size(&caller);
                        let offset = mem_size.saturating_sub(content.len() + 1024);
                        match memory.write(&mut caller, offset, content.as_bytes()) {
                            Ok(()) => (offset as i32, len, 1),
                            Err(_) => (0, 0, 0),
                        }
                    }
                    Err(_) => (0, 0, 0),
                }
            },
        )?;

        // host.get_config(key_ptr, key_len) -> (val_ptr, val_len, found)
        linker.func_wrap(
            "host",
            "get_config",
            |mut caller: Caller<'_, PluginHostState>, key_ptr: i32, key_len: i32| -> (i32, i32, i32) {
                let memory = match caller.get_export("memory").and_then(|e| e.into_memory()) {
                    Some(m) => m,
                    None => return (0, 0, 0),
                };
                let key = match read_str_from_memory(&caller, &memory, key_ptr, key_len) {
                    Ok(k) => k,
                    Err(_) => return (0, 0, 0),
                };
                match caller.data().get_config(&key) {
                    Some(_val) => (0, 0, 1), // Simplified — full impl would write to shared memory
                    None => (0, 0, 0),
                }
            },
        )?;

        let instance = linker.instantiate(store, module)?;
        Ok(instance)
    }

    /// Query plugin metadata by calling its exported functions.
    async fn query_plugin_metadata(
        &self,
        module: &Module,
        host_state: &PluginHostState,
    ) -> Result<(String, String, String, Vec<WasmToolDef>)> {
        let mut store = self.create_store(host_state)?;
        let instance = self.instantiate(&mut store, module)?;

        // Try to call get_name, get_description, get_version
        let name = self.call_string_export(&mut store, &instance, "get_name")
            .unwrap_or_else(|_| "unnamed-plugin".to_string());
        let desc = self.call_string_export(&mut store, &instance, "get_description")
            .unwrap_or_else(|_| "A WASM plugin".to_string());
        let version = self.call_string_export(&mut store, &instance, "get_version")
            .unwrap_or_else(|_| "0.0.0".to_string());

        // Try to call list_tools -> JSON string
        let tools_json = match self.call_string_export(&mut store, &instance, "list_tools") {
            Ok(json) => {
                tracing::debug!(json_len = json.len(), json_preview = &json[..json.len().min(200)], "list_tools returned");
                json
            }
            Err(e) => {
                tracing::warn!(error = %e, "list_tools export failed, plugin will have no tools");
                "[]".to_string()
            }
        };

        let tools: Vec<WasmToolDef> = match serde_json::from_str(&tools_json) {
            Ok(t) => t,
            Err(e) => {
                tracing::warn!(error = %e, raw = &tools_json[..tools_json.len().min(300)], "failed to parse list_tools JSON");
                Vec::new()
            }
        };

        Ok((name, desc, version, tools))
    }

    fn call_string_export(
        &self,
        store: &mut Store<PluginHostState>,
        instance: &Instance,
        name: &str,
    ) -> Result<String> {
        let func = instance
            .get_typed_func::<(), (i32, i32)>(&mut *store, name)
            .context(format!("{name} export not found"))?;

        let (ptr, len) = func.call(&mut *store, ())?;

        let memory = instance
            .get_memory(&mut *store, "memory")
            .ok_or_else(|| anyhow::anyhow!("no memory export"))?;

        let data = memory.data(&*store);
        let start = ptr as usize;
        let end = start + len as usize;
        if end > data.len() {
            anyhow::bail!("string out of bounds: {start}..{end} (mem size: {})", data.len());
        }
        Ok(String::from_utf8_lossy(&data[start..end]).to_string())
    }
}

/// Read a string from WASM memory (for use inside Caller closures).
fn read_str_from_memory(
    caller: &Caller<'_, PluginHostState>,
    memory: &Memory,
    ptr: i32,
    len: i32,
) -> Result<String, ()> {
    let data = memory.data(caller);
    let start = ptr as usize;
    let end = start + len as usize;
    if end > data.len() {
        return Err(());
    }
    String::from_utf8(data[start..end].to_vec()).map_err(|_| ())
}

/// Read a string from WASM memory (for use with &Store).
fn read_wasm_string(
    store: &Store<PluginHostState>,
    memory: &Memory,
    ptr: i32,
    len: i32,
) -> Result<String> {
    let data = memory.data(store);
    let start = ptr as usize;
    let end = start + len as usize;
    if end > data.len() {
        anyhow::bail!("string out of bounds: {start}..{end} (mem size: {})", data.len());
    }
    Ok(String::from_utf8_lossy(&data[start..end]).to_string())
}
