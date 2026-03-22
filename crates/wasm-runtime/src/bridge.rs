/// Bridge: adapts WASM plugins into denkwerk KernelFunction tools.
///
/// Each WASM tool becomes a DynKernelFunction that the agent can call.
use std::sync::Arc;

use denkwerk::functions::{FunctionDefinition, FunctionParameter, KernelFunction};
use denkwerk::{DynKernelFunction, LLMError};
use serde_json::{json, Value};

use crate::runtime::{WasmRuntime, WasmToolDef};

/// A denkwerk KernelFunction backed by a WASM plugin tool.
pub struct WasmToolBridge {
    runtime: Arc<WasmRuntime>,
    plugin_name: String,
    tool: WasmToolDef,
}

impl WasmToolBridge {
    pub fn new(runtime: Arc<WasmRuntime>, plugin_name: String, tool: WasmToolDef) -> Self {
        Self {
            runtime,
            plugin_name,
            tool,
        }
    }

    pub fn into_dyn(self) -> DynKernelFunction {
        Arc::new(self)
    }
}

#[async_trait::async_trait]
impl KernelFunction for WasmToolBridge {
    fn definition(&self) -> FunctionDefinition {
        let mut def = FunctionDefinition::new(&self.tool.name)
            .with_description(&self.tool.description);

        // Parse the JSON schema and convert to FunctionParameters
        if let Some(props) = self.tool.parameters_schema.get("properties").and_then(|p| p.as_object()) {
            let required: Vec<String> = self.tool.parameters_schema
                .get("required")
                .and_then(|r| r.as_array())
                .map(|arr| arr.iter().filter_map(|v| v.as_str().map(String::from)).collect())
                .unwrap_or_default();

            for (name, schema) in props {
                let desc = schema
                    .get("description")
                    .and_then(|d| d.as_str())
                    .unwrap_or("");

                let mut param = FunctionParameter::new(name, schema.clone())
                    .with_description(desc);

                if !required.contains(name) {
                    param = param.optional();
                }

                def.add_parameter(param);
            }
        }

        def
    }

    async fn invoke(&self, arguments: &Value) -> Result<Value, LLMError> {
        let args_json = serde_json::to_string(arguments)
            .map_err(|e| LLMError::InvalidFunctionArguments(format!("serialize args: {e}")))?;

        match self.runtime.invoke_tool(&self.plugin_name, &self.tool.name, &args_json).await {
            Ok(value) => Ok(value),
            Err(e) => {
                // Return errors as JSON rather than propagating,
                // so the LLM can see what went wrong.
                Ok(json!({
                    "error": e.to_string(),
                    "plugin": self.plugin_name,
                    "tool": self.tool.name,
                }))
            }
        }
    }
}

/// Create DynKernelFunction instances for all tools in a WASM plugin.
pub fn tools_for_plugin(
    runtime: Arc<WasmRuntime>,
    plugin_name: &str,
    tools: &[WasmToolDef],
) -> Vec<DynKernelFunction> {
    tools
        .iter()
        .map(|tool| {
            WasmToolBridge::new(
                runtime.clone(),
                plugin_name.to_string(),
                tool.clone(),
            )
            .into_dyn()
        })
        .collect()
}
