//! Permission guard: wraps any `KernelFunction` to enforce permission
//! checks and audit logging before delegating to the inner tool.
//!
//! There are two flavours:
//!   1. `GuardedHttpTool` — wraps `AuthenticatedHttpTool`, checks `ConnectorPolicy`
//!      rules against the HTTP method + URL path before forwarding.
//!   2. `GuardedBuiltinTool` — wraps built-in tools (code_execution, workspace_*,
//!      knowledge_*) and checks `TaskPermissionOverride` flags.
//!
//! Both record a `ToolAuditEntry` for every invocation (allowed or denied).

use std::sync::Arc;
use std::time::Instant;

use clawkson_core::models::{ConnectorPolicy, TaskPermissionOverride};
use clawkson_db::Db;
use denkwerk::functions::KernelFunction;
use denkwerk::{DynKernelFunction, FunctionDefinition, LLMError};
use serde_json::Value;
use uuid::Uuid;

use crate::proxy::{evaluate_request, extract_url_path, parse_http_method, PolicyVerdict};

/// Context needed by guards to evaluate permissions and write audit entries.
#[derive(Clone)]
pub struct GuardContext {
    pub db: Db,
    pub conversation_id: Uuid,
    pub agent_id: Uuid,
    pub user_id: Uuid,
    /// Connector policies configured on the agent (deserialized from JSONB).
    pub connector_policies: Vec<ConnectorPolicy>,
    /// Optional per-conversation permission override (can only narrow).
    pub task_override: Option<TaskPermissionOverride>,
    /// Map from connector name (lowercased) → connector UUID.
    /// Populated from the connectors available to the agent.
    pub connector_name_to_id: std::collections::HashMap<String, Uuid>,
}

// ── GuardedHttpTool ─────────────────────────────────────────────────

/// Wraps `AuthenticatedHttpTool` and enforces `ConnectorPolicy` rules.
pub struct GuardedHttpTool {
    inner: DynKernelFunction,
    ctx: GuardContext,
}

impl GuardedHttpTool {
    pub fn new(inner: DynKernelFunction, ctx: GuardContext) -> Self {
        Self { inner, ctx }
    }

    pub fn into_dyn(self) -> DynKernelFunction {
        Arc::new(self)
    }
}

#[async_trait::async_trait]
impl KernelFunction for GuardedHttpTool {
    fn definition(&self) -> FunctionDefinition {
        // Pass through; the tool's schema doesn't change.
        self.inner.definition()
    }

    async fn invoke(&self, arguments: &Value) -> Result<Value, LLMError> {
        let start = Instant::now();

        // Extract method, url, and connector from the arguments
        let method_str = arguments
            .get("method")
            .and_then(|v| v.as_str())
            .unwrap_or("GET");
        let url = arguments
            .get("url")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let connector_name = arguments
            .get("connector")
            .and_then(|v| v.as_str())
            .unwrap_or("");

        let method = match parse_http_method(method_str) {
            Some(m) => m,
            None => {
                let _ = self.log_audit("authenticated_http", Some(method_str), Some(url), None, "denied", Some(&format!("Unknown HTTP method: {method_str}")), None).await;
                return Ok(serde_json::json!({
                    "error": format!("Unknown HTTP method: {method_str}")
                }));
            }
        };

        // Resolve connector name → ID
        let connector_id = self
            .ctx
            .connector_name_to_id
            .get(&connector_name.to_lowercase());

        // Check task-level method override
        if let Some(ref ovr) = self.ctx.task_override {
            if let Some(ref allowed_methods) = ovr.allowed_methods {
                if !allowed_methods.contains(&method) {
                    let reason = format!(
                        "Task override restricts methods to {:?}, but {} was requested",
                        allowed_methods, method
                    );
                    let _ = self.log_audit("authenticated_http", Some(method_str), Some(url), connector_id.copied(), "denied", Some(&reason), None).await;
                    return Ok(serde_json::json!({ "error": reason }));
                }
            }
            if let Some(ref allowed_ids) = ovr.allowed_connector_ids {
                if let Some(cid) = connector_id {
                    if !allowed_ids.contains(cid) {
                        let reason = format!(
                            "Task override does not allow connector '{}'",
                            connector_name
                        );
                        let _ = self.log_audit("authenticated_http", Some(method_str), Some(url), Some(*cid), "denied", Some(&reason), None).await;
                        return Ok(serde_json::json!({ "error": reason }));
                    }
                }
            }
        }

        // Extract URL path for policy matching
        let url_path = extract_url_path(url).unwrap_or_default();

        // Evaluate connector policy
        if let Some(cid) = connector_id {
            let verdict =
                evaluate_request(&self.ctx.connector_policies, cid, &method, &url_path);
            match verdict {
                PolicyVerdict::Allowed => {
                    // Proceed to inner tool
                }
                PolicyVerdict::Denied(reason) => {
                    let _ = self.log_audit("authenticated_http", Some(method_str), Some(&url_path), Some(*cid), "denied", Some(&reason), None).await;
                    return Ok(serde_json::json!({ "error": reason }));
                }
            }
        }
        // If connector_id is unknown, the inner tool will return an error
        // about the unknown connector name — that's fine.

        // Allowed — delegate to inner tool
        let result = self.inner.invoke(arguments).await;
        let duration = start.elapsed().as_millis() as i64;

        let _ = self.log_audit(
            "authenticated_http",
            Some(method_str),
            Some(&url_path),
            connector_id.copied(),
            "allowed",
            None,
            Some(duration),
        ).await;

        result
    }
}

impl GuardedHttpTool {
    async fn log_audit(
        &self,
        tool_name: &str,
        http_method: Option<&str>,
        target_path: Option<&str>,
        connector_id: Option<Uuid>,
        decision: &str,
        denial_reason: Option<&str>,
        duration_ms: Option<i64>,
    ) -> Result<(), ()> {
        clawkson_db::tool_audit::insert(
            &self.ctx.db,
            self.ctx.conversation_id,
            self.ctx.agent_id,
            self.ctx.user_id,
            tool_name,
            http_method,
            target_path,
            connector_id,
            decision,
            denial_reason,
            duration_ms,
        )
        .await
        .map(|_| ())
        .map_err(|e| {
            tracing::error!("Failed to write audit log: {e}");
        })
    }
}

// ── GuardedBuiltinTool ──────────────────────────────────────────────

/// Wraps a built-in tool (code_execution, workspace_*, knowledge_*) and
/// checks `TaskPermissionOverride` flags before delegation.
pub struct GuardedBuiltinTool {
    inner: DynKernelFunction,
    tool_name: String,
    ctx: GuardContext,
}

impl GuardedBuiltinTool {
    pub fn new(inner: DynKernelFunction, tool_name: String, ctx: GuardContext) -> Self {
        Self {
            inner,
            tool_name,
            ctx,
        }
    }

    pub fn into_dyn(self) -> DynKernelFunction {
        Arc::new(self)
    }
}

#[async_trait::async_trait]
impl KernelFunction for GuardedBuiltinTool {
    fn definition(&self) -> FunctionDefinition {
        self.inner.definition()
    }

    async fn invoke(&self, arguments: &Value) -> Result<Value, LLMError> {
        let start = Instant::now();

        // Check task-level overrides for built-in tools
        if let Some(ref ovr) = self.ctx.task_override {
            let blocked = match self.tool_name.as_str() {
                "code_execution" if ovr.disable_code_execution => {
                    Some("Task override disables code execution")
                }
                "workspace_write" if ovr.disable_workspace_write => {
                    Some("Task override disables workspace writes")
                }
                "knowledge_list" | "knowledge_search" if ovr.disable_knowledge_access => {
                    Some("Task override disables knowledge base access")
                }
                _ => None,
            };

            if let Some(reason) = blocked {
                let _ = self.log_audit("denied", Some(reason), None).await;
                return Ok(serde_json::json!({ "error": reason }));
            }
        }

        // Allowed — delegate
        let result = self.inner.invoke(arguments).await;
        let duration = start.elapsed().as_millis() as i64;
        let _ = self.log_audit("allowed", None, Some(duration)).await;
        result
    }
}

impl GuardedBuiltinTool {
    async fn log_audit(
        &self,
        decision: &str,
        denial_reason: Option<&str>,
        duration_ms: Option<i64>,
    ) -> Result<(), ()> {
        clawkson_db::tool_audit::insert(
            &self.ctx.db,
            self.ctx.conversation_id,
            self.ctx.agent_id,
            self.ctx.user_id,
            &self.tool_name,
            None,
            None,
            None,
            decision,
            denial_reason,
            duration_ms,
        )
        .await
        .map(|_| ())
        .map_err(|e| {
            tracing::error!("Failed to write audit log: {e}");
        })
    }
}
