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
}
