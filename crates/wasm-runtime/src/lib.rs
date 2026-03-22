//! WASM Plugin Runtime for Clawkson
//!
//! Enables agents to load and execute WASM plugins at runtime.
//! Plugins are sandboxed via WASI with controlled filesystem access,
//! optional network capabilities, and fuel-based execution limits.
//!
//! # Architecture
//!
//! ```text
//! Agent → install_wasm_plugin tool → WasmRuntime.load_plugin()
//!                                         ↓
//!                                    Module compiled
//!                                    Metadata queried
//!                                    Tools registered
//!                                         ↓
//! Agent → tool call → WasmToolBridge → WasmRuntime.invoke_tool()
//!                                         ↓
//!                                    Store created (isolated)
//!                                    Host functions linked
//!                                    Plugin function called
//!                                    Result returned as JSON
//! ```

pub mod bridge;
pub mod host;
pub mod install_tool;
pub mod runtime;

pub use bridge::{tools_for_plugin, WasmToolBridge};
pub use host::PluginHostState;
pub use install_tool::InstallWasmPluginTool;
pub use runtime::{WasmPluginInfo, WasmRuntime, WasmToolDef};
