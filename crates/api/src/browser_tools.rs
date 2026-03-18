use std::sync::Arc;

use clawkson_container::{ContainerManager, ExecRequest};
use denkwerk::{
    functions::FunctionParameter,
    DynKernelFunction, FunctionDefinition,
};
use serde::Deserialize;
use serde_json::Value;
use uuid::Uuid;

/// Interactive browser tool — talks to a persistent Playwright browser session
/// running inside the agent's sandbox container.
///
/// The browser server is started lazily on first invocation and stays alive
/// for the duration of the conversation, allowing the model to navigate pages,
/// click elements, fill forms, and see screenshots between each action.
pub struct BrowserTool {
    agent_id: Uuid,
    conversation_id: Uuid,
    container_manager: Arc<ContainerManager>,
    workspace_root: std::path::PathBuf,
}

impl BrowserTool {
    pub fn new(
        agent_id: Uuid,
        conversation_id: Uuid,
        container_manager: Arc<ContainerManager>,
        workspace_root: std::path::PathBuf,
    ) -> Self {
        Self {
            agent_id,
            conversation_id,
            container_manager,
            workspace_root,
        }
    }

    pub fn into_dyn(self) -> DynKernelFunction {
        Arc::new(self)
    }
}

#[derive(Debug, Deserialize)]
struct BrowserArgs {
    action: String,
    url: Option<String>,
    selector: Option<String>,
    text: Option<String>,
    expression: Option<String>,
    direction: Option<String>,
    amount: Option<u32>,
}

#[async_trait::async_trait]
impl denkwerk::functions::KernelFunction for BrowserTool {
    fn definition(&self) -> FunctionDefinition {
        let mut def = FunctionDefinition::new("browser")
            .with_description(
                "Control a web browser to navigate pages, click elements, fill forms, \
                 take screenshots, and evaluate JavaScript. The browser persists across \
                 calls within this conversation — use sequential calls to interact with \
                 pages step by step. Each action returns a screenshot plus a description \
                 of the page's interactive elements (links, buttons, inputs) so you can \
                 decide what to do next. Always save important results to /workspace/outputs/.",
            );

        def.add_parameter(
            FunctionParameter::new(
                "action",
                serde_json::json!({
                    "type": "string",
                    "enum": ["navigate", "click", "type", "screenshot", "evaluate", "scroll", "back"]
                }),
            )
            .with_description(
                "The browser action: 'navigate' to visit a URL, 'click' to click an element, \
                 'type' to fill a text field, 'screenshot' to capture current page, \
                 'evaluate' to run JavaScript, 'scroll' to scroll the page, 'back' to go back",
            ),
        );

        def.add_parameter(
            FunctionParameter::new("url", serde_json::json!({ "type": "string" }))
                .with_description("URL to navigate to (required for 'navigate')"),
        );

        def.add_parameter(
            FunctionParameter::new("selector", serde_json::json!({ "type": "string" }))
                .with_description(
                    "CSS selector or Playwright selector for the target element \
                     (required for 'click' and 'type'). Examples: '#login-btn', \
                     '[name=\"email\"]', 'text=Submit', 'role=button[name=\"Sign in\"]'",
                ),
        );

        def.add_parameter(
            FunctionParameter::new("text", serde_json::json!({ "type": "string" }))
                .with_description("Text to type into the element (required for 'type')"),
        );

        def.add_parameter(
            FunctionParameter::new("expression", serde_json::json!({ "type": "string" }))
                .with_description("JavaScript expression to evaluate in the page (required for 'evaluate')"),
        );

        def.add_parameter(
            FunctionParameter::new(
                "direction",
                serde_json::json!({ "type": "string", "enum": ["up", "down"] }),
            )
            .with_description("Scroll direction (for 'scroll', defaults to 'down')"),
        );

        def
    }

    async fn invoke(&self, arguments: &Value) -> Result<Value, denkwerk::LLMError> {
        let args: BrowserArgs = serde_json::from_value(arguments.clone()).map_err(|e| {
            denkwerk::LLMError::InvalidFunctionArguments(format!(
                "Invalid arguments for browser: {e}"
            ))
        })?;

        // Validate required params per action
        match args.action.as_str() {
            "navigate" if args.url.is_none() => {
                return Ok(serde_json::json!({ "error": "Missing required 'url' for navigate action" }));
            }
            "click" if args.selector.is_none() => {
                return Ok(serde_json::json!({ "error": "Missing required 'selector' for click action" }));
            }
            "type" if args.selector.is_none() || args.text.is_none() => {
                return Ok(serde_json::json!({ "error": "Missing required 'selector' and 'text' for type action" }));
            }
            "evaluate" if args.expression.is_none() => {
                return Ok(serde_json::json!({ "error": "Missing required 'expression' for evaluate action" }));
            }
            _ => {}
        }

        // Build params JSON for the browser client
        let params = serde_json::json!({
            "url": args.url,
            "selector": args.selector,
            "text": args.text,
            "expression": args.expression,
            "direction": args.direction.as_deref().unwrap_or("down"),
            "amount": args.amount.unwrap_or(500),
            "wait_until": "load",
        });

        // Write params to a temp file in the workspace
        let cmd_id = Uuid::new_v4().as_simple().to_string();
        let workspace = self
            .workspace_root
            .join(self.agent_id.to_string())
            .join(self.conversation_id.to_string());
        let params_file = workspace.join(format!(".browser_params_{cmd_id}.json"));

        if let Err(e) = tokio::fs::write(&params_file, serde_json::to_string(&params).unwrap_or_default()).await {
            return Ok(serde_json::json!({ "error": format!("Failed to write params: {e}") }));
        }

        // Execute the browser client script in the container
        let command = format!(
            "python3 /usr/lib/clawkson/browser_client.py {} /workspace/.browser_params_{}.json",
            args.action, cmd_id,
        );

        let request = ExecRequest {
            command,
            timeout: Some(90), // generous timeout for first call (browser startup)
            output_dir: Some("outputs".to_string()),
        };

        let exec_result = match self
            .container_manager
            .exec(self.agent_id, self.conversation_id, &request)
            .await
        {
            Err(clawkson_container::ContainerError::NotFound(_)) => {
                tracing::info!(
                    agent_id = %self.agent_id,
                    conversation_id = %self.conversation_id,
                    "container not found for browser tool, attempting auto-restart",
                );
                let config = clawkson_container::ContainerConfig::default();
                if let Err(e) = self
                    .container_manager
                    .start_container(self.agent_id, self.conversation_id, &config)
                    .await
                {
                    return Ok(serde_json::json!({
                        "error": format!("Container lost and restart failed: {e}"),
                    }));
                }
                self.container_manager
                    .exec(self.agent_id, self.conversation_id, &request)
                    .await
            }
            other => other,
        };

        // Clean up params file (best-effort)
        let _ = tokio::fs::remove_file(&params_file).await;

        match exec_result {
            Ok(result) => {
                if result.exit_code != 0 || result.stdout.trim().is_empty() {
                    return Ok(serde_json::json!({
                        "error": format!(
                            "Browser command failed (exit {}): {}",
                            result.exit_code,
                            if result.stderr.is_empty() { &result.stdout } else { &result.stderr }
                        ),
                    }));
                }

                // Parse the JSON response from the browser client
                match serde_json::from_str::<Value>(result.stdout.trim()) {
                    Ok(mut response) => {
                        // Merge output_files from exec result if present
                        if response.get("output_files").is_none() {
                            if let Some(output_files) = &result.output_files {
                                let files: Vec<Value> = output_files
                                    .iter()
                                    .map(|f| {
                                        serde_json::json!({
                                            "path": f.path,
                                            "size": f.size,
                                        })
                                    })
                                    .collect();
                                response["output_files"] = Value::Array(files);
                            }
                        }
                        Ok(response)
                    }
                    Err(_) => Ok(serde_json::json!({
                        "error": format!("Invalid response from browser: {}", &result.stdout[..result.stdout.len().min(200)]),
                    })),
                }
            }
            Err(e) => Ok(serde_json::json!({ "error": e.to_string() })),
        }
    }
}
