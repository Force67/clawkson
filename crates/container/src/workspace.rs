use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

use crate::error::ContainerError;

// ── Models ────────────────────────────────────────────────────────

/// A single entry in a workspace directory listing.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceEntry {
    /// Entry name (file or directory name only, not full path).
    pub name: String,
    /// Path relative to the workspace root (e.g. "outputs/result.csv").
    pub path: String,
    pub is_dir: bool,
    pub size: u64,
    pub modified_at: Option<DateTime<Utc>>,
}

/// Response for a workspace directory listing.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceListing {
    /// The directory path that was listed (relative to workspace root).
    pub path: String,
    pub entries: Vec<WorkspaceEntry>,
}

/// An output file discovered after exec.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OutputFile {
    /// Path relative to the workspace root.
    pub path: String,
    pub size: u64,
}

// ── Path sandbox ─────────────────────────────────────────────────

/// Resolve `rel` relative to `workspace_root`, rejecting any path
/// that would escape the workspace root (path traversal).
///
/// Returns the absolute, canonicalized-equivalent path. The
/// workspace root itself does NOT need to exist on disk yet;
/// we use a lexical normalization rather than `canonicalize`
/// so callers can supply paths before writing files.
pub fn sandbox_path(workspace_root: &Path, rel: &str) -> Result<PathBuf, ContainerError> {
    // Strip leading slashes / dots so the user-supplied path is
    // always treated as relative.
    let stripped = rel.trim_start_matches('/').trim_start_matches('\\');

    let joined = if stripped.is_empty() {
        workspace_root.to_path_buf()
    } else {
        workspace_root.join(stripped)
    };

    // Lexical normalization: resolve `.` and `..` components without
    // hitting the filesystem. We walk the components and build the
    // canonical path segment by segment.
    let mut resolved = PathBuf::new();
    for component in joined.components() {
        match component {
            std::path::Component::ParentDir => {
                // If we can't pop (already at workspace root or above),
                // reject the path.
                if !resolved.pop() {
                    return Err(ContainerError::PathEscape(rel.to_string()));
                }
            }
            std::path::Component::CurDir => {}
            other => resolved.push(other),
        }
    }

    // Final safety check: resolved path must start with workspace_root.
    if !resolved.starts_with(workspace_root) {
        return Err(ContainerError::PathEscape(rel.to_string()));
    }

    Ok(resolved)
}

// ── Workspace I/O helpers ────────────────────────────────────────

/// List the contents of a workspace directory.
pub fn list_workspace(
    workspace_root: &Path,
    rel: &str,
) -> Result<WorkspaceListing, ContainerError> {
    let dir = sandbox_path(workspace_root, rel)?;

    if !dir.exists() {
        return Ok(WorkspaceListing {
            path: rel.trim_start_matches('/').to_string(),
            entries: vec![],
        });
    }

    if !dir.is_dir() {
        return Err(ContainerError::Io(std::io::Error::new(
            std::io::ErrorKind::NotADirectory,
            format!("{rel} is not a directory"),
        )));
    }

    let mut entries = Vec::new();
    for entry in std::fs::read_dir(&dir)? {
        let entry = entry?;
        let meta = entry.metadata()?;
        let name = entry.file_name().to_string_lossy().to_string();

        // Build path relative to workspace root
        let abs = entry.path();
        let rel_path = abs
            .strip_prefix(workspace_root)
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_else(|_| name.clone());

        let modified_at = meta.modified().ok().map(|t| DateTime::<Utc>::from(t));

        entries.push(WorkspaceEntry {
            name,
            path: rel_path,
            is_dir: meta.is_dir(),
            size: if meta.is_dir() { 0 } else { meta.len() },
            modified_at,
        });
    }

    entries.sort_by(|a, b| {
        // Directories first, then alphabetical.
        match (a.is_dir, b.is_dir) {
            (true, false) => std::cmp::Ordering::Less,
            (false, true) => std::cmp::Ordering::Greater,
            _ => a.name.cmp(&b.name),
        }
    });

    Ok(WorkspaceListing {
        path: rel.trim_start_matches('/').to_string(),
        entries,
    })
}

/// Scan a workspace subdirectory and return all files (recursive).
/// Used to collect output files after exec.
pub fn collect_output_files(
    workspace_root: &Path,
    output_rel: &str,
) -> Result<Vec<OutputFile>, ContainerError> {
    let dir = sandbox_path(workspace_root, output_rel)?;
    if !dir.exists() {
        return Ok(vec![]);
    }

    let mut files = Vec::new();
    collect_recursive(workspace_root, &dir, &mut files)?;
    Ok(files)
}

fn collect_recursive(
    workspace_root: &Path,
    dir: &Path,
    out: &mut Vec<OutputFile>,
) -> Result<(), ContainerError> {
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let meta = entry.metadata()?;
        let path = entry.path();
        if meta.is_dir() {
            collect_recursive(workspace_root, &path, out)?;
        } else {
            let rel_path = path
                .strip_prefix(workspace_root)
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_default();
            out.push(OutputFile {
                path: rel_path,
                size: meta.len(),
            });
        }
    }
    Ok(())
}

// ── Tests ─────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn root() -> PathBuf {
        PathBuf::from("/workspace/agent-123")
    }

    #[test]
    fn allows_normal_subpath() {
        let p = sandbox_path(&root(), "outputs/result.csv").unwrap();
        assert_eq!(p, PathBuf::from("/workspace/agent-123/outputs/result.csv"));
    }

    #[test]
    fn allows_root() {
        let p = sandbox_path(&root(), "").unwrap();
        assert_eq!(p, root());
    }

    #[test]
    fn allows_leading_slash() {
        let p = sandbox_path(&root(), "/outputs").unwrap();
        assert_eq!(p, PathBuf::from("/workspace/agent-123/outputs"));
    }

    #[test]
    fn rejects_escape_with_dotdot() {
        assert!(sandbox_path(&root(), "../../etc/passwd").is_err());
    }

    #[test]
    fn leading_slash_is_sandboxed_not_absolute() {
        // A leading "/" is stripped so "/etc/passwd" → "etc/passwd" which
        // is safely inside the workspace.
        let p = sandbox_path(&root(), "/etc/passwd").unwrap();
        assert_eq!(p, PathBuf::from("/workspace/agent-123/etc/passwd"));
    }

    #[test]
    fn rejects_dotdot_after_subdir() {
        assert!(sandbox_path(&root(), "outputs/../../etc/passwd").is_err());
    }

    // ── Per-conversation workspace isolation tests ─────────────

    /// Build a workspace root like the ContainerManager would:
    /// {workspace_root}/{agent_id}/{conversation_id}
    fn conv_workspace(base: &Path, agent: &str, conv: &str) -> PathBuf {
        base.join(agent).join(conv)
    }

    #[test]
    fn conversation_workspaces_are_disjoint() {
        let tmp = tempfile::tempdir().unwrap();
        let base = tmp.path();
        let agent = "agent-aaa";

        let ws_a = conv_workspace(base, agent, "conv-111");
        let ws_b = conv_workspace(base, agent, "conv-222");

        // Create both workspaces with outputs dirs
        for ws in [&ws_a, &ws_b] {
            std::fs::create_dir_all(ws.join("outputs")).unwrap();
            std::fs::create_dir_all(ws.join("inputs")).unwrap();
        }

        // Write a file into conv-111's workspace
        std::fs::write(ws_a.join("outputs/secret.txt"), "user-1-data").unwrap();

        // Write a different file into conv-222's workspace
        std::fs::write(ws_b.join("outputs/other.txt"), "user-2-data").unwrap();

        // conv-111 should only see secret.txt
        let listing_a = list_workspace(&ws_a, "outputs").unwrap();
        assert_eq!(listing_a.entries.len(), 1);
        assert_eq!(listing_a.entries[0].name, "secret.txt");

        // conv-222 should only see other.txt
        let listing_b = list_workspace(&ws_b, "outputs").unwrap();
        assert_eq!(listing_b.entries.len(), 1);
        assert_eq!(listing_b.entries[0].name, "other.txt");

        // Reading from conv-222 cannot reach conv-111's files
        let escape = sandbox_path(&ws_b, "../conv-111/outputs/secret.txt");
        assert!(escape.is_err(), "path traversal between conversations must be rejected");
    }

    #[test]
    fn collect_outputs_scoped_to_conversation() {
        let tmp = tempfile::tempdir().unwrap();
        let base = tmp.path();
        let agent = "agent-bbb";

        let ws_a = conv_workspace(base, agent, "conv-aaa");
        let ws_b = conv_workspace(base, agent, "conv-bbb");

        for ws in [&ws_a, &ws_b] {
            std::fs::create_dir_all(ws.join("outputs")).unwrap();
        }

        // Agent produces files in both conversation workspaces
        std::fs::write(ws_a.join("outputs/report.csv"), "a,b,c").unwrap();
        std::fs::write(ws_b.join("outputs/chart.png"), "PNG...").unwrap();
        std::fs::write(ws_b.join("outputs/data.json"), "{}").unwrap();

        // Collecting outputs from ws_a should only see report.csv
        let files_a = collect_output_files(&ws_a, "outputs").unwrap();
        assert_eq!(files_a.len(), 1);
        assert_eq!(files_a[0].path, "outputs/report.csv");

        // Collecting outputs from ws_b should see chart.png and data.json
        let files_b = collect_output_files(&ws_b, "outputs").unwrap();
        assert_eq!(files_b.len(), 2);
        let names: Vec<&str> = files_b.iter().map(|f| f.path.as_str()).collect();
        assert!(names.contains(&"outputs/chart.png"));
        assert!(names.contains(&"outputs/data.json"));
    }

    #[test]
    fn sandbox_prevents_cross_conversation_access() {
        let tmp = tempfile::tempdir().unwrap();
        let base = tmp.path();

        let ws = conv_workspace(base, "agent-x", "conv-target");
        std::fs::create_dir_all(&ws).unwrap();

        // Try various escape patterns — all must fail
        let attacks = [
            "../conv-other/outputs/secret",
            "../../agent-y/conv-z/outputs/secret",
            "../../../etc/passwd",
            "outputs/../../../other-conv/data",
        ];

        for attack in attacks {
            let result = sandbox_path(&ws, attack);
            assert!(
                result.is_err(),
                "sandbox_path should reject '{}' but got {:?}",
                attack,
                result,
            );
        }
    }

    #[test]
    fn write_and_read_within_conversation_workspace() {
        let tmp = tempfile::tempdir().unwrap();
        let base = tmp.path();

        let ws = conv_workspace(base, "agent-rw", "conv-rw");
        std::fs::create_dir_all(ws.join("inputs")).unwrap();
        std::fs::create_dir_all(ws.join("outputs")).unwrap();

        // Write to inputs
        let input_path = sandbox_path(&ws, "inputs/data.csv").unwrap();
        std::fs::write(&input_path, "col1,col2\n1,2\n").unwrap();

        // Read it back
        let content = std::fs::read_to_string(&input_path).unwrap();
        assert_eq!(content, "col1,col2\n1,2\n");

        // Write to outputs
        let output_path = sandbox_path(&ws, "outputs/result.txt").unwrap();
        std::fs::write(&output_path, "done").unwrap();

        // List outputs
        let listing = list_workspace(&ws, "outputs").unwrap();
        assert_eq!(listing.entries.len(), 1);
        assert_eq!(listing.entries[0].name, "result.txt");

        // Confirm the listing path is correct
        assert_eq!(listing.entries[0].path, "outputs/result.txt");
    }
}
