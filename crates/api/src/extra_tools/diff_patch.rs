/// Diff/patch tools for code editing: apply_diff, search_and_replace, edit_file.
use std::sync::Arc;

use denkwerk::functions::{FunctionDefinition, FunctionParameter, KernelFunction};
use denkwerk::DynKernelFunction;
use serde_json::{json, Value};

/// Tool that applies a search-and-replace diff to a file.
pub struct ApplyDiffTool {
    workspace_path: std::path::PathBuf,
}

impl ApplyDiffTool {
    pub fn new(workspace_path: std::path::PathBuf) -> Self { Self { workspace_path } }
    pub fn into_dyn(self) -> DynKernelFunction { Arc::new(self) }
}

#[async_trait::async_trait]
impl KernelFunction for ApplyDiffTool {
    fn definition(&self) -> FunctionDefinition {
        let mut def = FunctionDefinition::new("apply_diff")
            .with_description("Apply a search-and-replace diff to a file. Finds the exact old_text and replaces it with new_text.");
        def.add_parameter(FunctionParameter::new("path", json!({"type": "string"})).with_description("Relative path to file"));
        def.add_parameter(FunctionParameter::new("old_text", json!({"type": "string"})).with_description("Exact text to find"));
        def.add_parameter(FunctionParameter::new("new_text", json!({"type": "string"})).with_description("Replacement text"));
        def
    }

    async fn invoke(&self, arguments: &Value) -> Result<Value, denkwerk::LLMError> {
        let path = arguments.get("path").and_then(|v| v.as_str())
            .ok_or_else(|| denkwerk::LLMError::InvalidFunctionArguments("path required".into()))?;
        let old_text = arguments.get("old_text").and_then(|v| v.as_str())
            .ok_or_else(|| denkwerk::LLMError::InvalidFunctionArguments("old_text required".into()))?;
        let new_text = arguments.get("new_text").and_then(|v| v.as_str())
            .ok_or_else(|| denkwerk::LLMError::InvalidFunctionArguments("new_text required".into()))?;

        let file_path = self.workspace_path.join(path);
        if !file_path.starts_with(&self.workspace_path) {
            return Ok(json!({"error": "path traversal not allowed"}));
        }

        let content = match tokio::fs::read_to_string(&file_path).await {
            Ok(c) => c,
            Err(e) => return Ok(json!({"error": format!("read: {e}"), "path": path})),
        };

        let count = content.matches(old_text).count();
        if count == 0 {
            return Ok(json!({"error": "old_text not found in file", "path": path}));
        }

        let new_content = content.replacen(old_text, new_text, 1);
        if let Err(e) = tokio::fs::write(&file_path, &new_content).await {
            return Ok(json!({"error": format!("write: {e}"), "path": path}));
        }

        Ok(json!({"path": path, "replacements": 1, "occurrences_found": count, "status": "ok"}))
    }
}

/// Tool that edits specific line ranges.
pub struct EditFileTool {
    workspace_path: std::path::PathBuf,
}

impl EditFileTool {
    pub fn new(workspace_path: std::path::PathBuf) -> Self { Self { workspace_path } }
    pub fn into_dyn(self) -> DynKernelFunction { Arc::new(self) }
}

#[async_trait::async_trait]
impl KernelFunction for EditFileTool {
    fn definition(&self) -> FunctionDefinition {
        let mut def = FunctionDefinition::new("edit_file")
            .with_description("Edit specific line ranges in a file (1-indexed, inclusive).");
        def.add_parameter(FunctionParameter::new("path", json!({"type": "string"})).with_description("Relative path to file"));
        def.add_parameter(FunctionParameter::new("start_line", json!({"type": "integer"})).with_description("First line (1-indexed)"));
        def.add_parameter(FunctionParameter::new("end_line", json!({"type": "integer"})).with_description("Last line (inclusive)"));
        def.add_parameter(FunctionParameter::new("new_content", json!({"type": "string"})).with_description("New content to insert"));
        def
    }

    async fn invoke(&self, arguments: &Value) -> Result<Value, denkwerk::LLMError> {
        let path = arguments.get("path").and_then(|v| v.as_str())
            .ok_or_else(|| denkwerk::LLMError::InvalidFunctionArguments("path required".into()))?;
        let start = arguments.get("start_line").and_then(|v| v.as_u64())
            .ok_or_else(|| denkwerk::LLMError::InvalidFunctionArguments("start_line required".into()))? as usize;
        let end = arguments.get("end_line").and_then(|v| v.as_u64())
            .ok_or_else(|| denkwerk::LLMError::InvalidFunctionArguments("end_line required".into()))? as usize;
        let new_content = arguments.get("new_content").and_then(|v| v.as_str())
            .ok_or_else(|| denkwerk::LLMError::InvalidFunctionArguments("new_content required".into()))?;

        if start == 0 || end == 0 || start > end {
            return Ok(json!({"error": "invalid line range (1-indexed, start <= end)"}));
        }

        let file_path = self.workspace_path.join(path);
        if !file_path.starts_with(&self.workspace_path) {
            return Ok(json!({"error": "path traversal not allowed"}));
        }

        let content = match tokio::fs::read_to_string(&file_path).await {
            Ok(c) => c,
            Err(e) => return Ok(json!({"error": format!("read: {e}"), "path": path})),
        };

        let lines: Vec<&str> = content.lines().collect();
        if start > lines.len() {
            return Ok(json!({"error": format!("start_line {} exceeds file length {}", start, lines.len())}));
        }
        let actual_end = end.min(lines.len());

        let mut result = Vec::new();
        result.extend_from_slice(&lines[..start - 1]);
        for line in new_content.lines() { result.push(line); }
        if actual_end < lines.len() { result.extend_from_slice(&lines[actual_end..]); }

        if let Err(e) = tokio::fs::write(&file_path, result.join("\n") + "\n").await {
            return Ok(json!({"error": format!("write: {e}"), "path": path}));
        }

        Ok(json!({"path": path, "lines_replaced": actual_end - start + 1, "new_lines": new_content.lines().count(), "status": "ok"}))
    }
}

/// Tool for global search-and-replace.
pub struct SearchAndReplaceTool {
    workspace_path: std::path::PathBuf,
}

impl SearchAndReplaceTool {
    pub fn new(workspace_path: std::path::PathBuf) -> Self { Self { workspace_path } }
    pub fn into_dyn(self) -> DynKernelFunction { Arc::new(self) }
}

#[async_trait::async_trait]
impl KernelFunction for SearchAndReplaceTool {
    fn definition(&self) -> FunctionDefinition {
        let mut def = FunctionDefinition::new("search_and_replace")
            .with_description("Search and replace all occurrences of a pattern in a file.");
        def.add_parameter(FunctionParameter::new("path", json!({"type": "string"})).with_description("Relative path to file"));
        def.add_parameter(FunctionParameter::new("search", json!({"type": "string"})).with_description("Text to search for"));
        def.add_parameter(FunctionParameter::new("replace", json!({"type": "string"})).with_description("Replacement text"));
        def
    }

    async fn invoke(&self, arguments: &Value) -> Result<Value, denkwerk::LLMError> {
        let path = arguments.get("path").and_then(|v| v.as_str())
            .ok_or_else(|| denkwerk::LLMError::InvalidFunctionArguments("path required".into()))?;
        let search = arguments.get("search").and_then(|v| v.as_str())
            .ok_or_else(|| denkwerk::LLMError::InvalidFunctionArguments("search required".into()))?;
        let replace = arguments.get("replace").and_then(|v| v.as_str())
            .ok_or_else(|| denkwerk::LLMError::InvalidFunctionArguments("replace required".into()))?;

        let file_path = self.workspace_path.join(path);
        if !file_path.starts_with(&self.workspace_path) {
            return Ok(json!({"error": "path traversal not allowed"}));
        }

        let content = match tokio::fs::read_to_string(&file_path).await {
            Ok(c) => c,
            Err(e) => return Ok(json!({"error": format!("read: {e}"), "path": path})),
        };

        let count = content.matches(search).count();
        if count == 0 {
            return Ok(json!({"error": "search text not found", "path": path}));
        }

        if let Err(e) = tokio::fs::write(&file_path, content.replace(search, replace)).await {
            return Ok(json!({"error": format!("write: {e}"), "path": path}));
        }

        Ok(json!({"path": path, "replacements": count, "status": "ok"}))
    }
}
