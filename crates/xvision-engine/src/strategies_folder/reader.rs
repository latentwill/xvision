//! Read-only enumeration + body fetch for `$XVN_HOME/strategies/`.
//!
//! Path-safety contract: every `rel_path` resolves under the canonical
//! `folder_root` or is rejected. Symlink escapes (a symlink under the
//! folder pointing at `/etc/passwd`) are rejected because we canonicalize
//! the target and re-check the prefix. `..` traversals are rejected for
//! the same reason.

use std::path::{Component, Path, PathBuf};

use chrono::{DateTime, SecondsFormat, Utc};

use crate::api::{ApiContext, ApiError, ApiResult};

use super::types::{FileContent, FileKind, FolderEntry};

/// Allowlisted subfolder names. Passing anything else to `list` returns
/// `ApiError::Validation` (the closest existing variant to "BadRequest").
pub const SUBFOLDER_ALLOWLIST: &[&str] = &["notes", "docs", "strategy-files", "evals", "library"];

/// Maximum body size returned by `read`. Files larger than this are
/// truncated and the response sets `truncated: true`.
pub const MAX_FILE_BYTES: usize = 256 * 1024;

/// Resolve the strategies-folder root for a given `xvn_home`. The folder
/// itself may not exist — `list` treats missing as empty rather than an
/// error.
pub fn folder_root(xvn_home: &Path) -> PathBuf {
    xvn_home.join("strategies")
}

/// Enumerate entries under `folder_root(ctx.xvn_home)`, optionally
/// restricted to one allowlisted subfolder.
///
/// - `subfolder = None` → recursive scan of the whole strategies folder.
/// - `subfolder = Some("notes")` → recursive scan of `<root>/notes/`.
///
/// Missing folder returns `Ok(vec![])`. Anything outside
/// [`SUBFOLDER_ALLOWLIST`] returns `ApiError::Validation`.
pub async fn list(ctx: &ApiContext, subfolder: Option<&str>) -> ApiResult<Vec<FolderEntry>> {
    let root = folder_root(&ctx.xvn_home);

    let scan_root = match subfolder {
        None => root.clone(),
        Some(name) => {
            if !SUBFOLDER_ALLOWLIST.contains(&name) {
                return Err(ApiError::Validation(format!(
                    "subfolder_not_allowed: '{name}' is not in the strategies-folder allowlist ({})",
                    SUBFOLDER_ALLOWLIST.join(", ")
                )));
            }
            root.join(name)
        }
    };

    // Missing folder is the empty-result case, not an error. Both
    // "never initialized" (`xvn strategies init` not yet run) and
    // "subfolder doesn't exist yet" route through here.
    if !tokio::fs::try_exists(&scan_root).await.unwrap_or(false) {
        return Ok(Vec::new());
    }

    // Canonicalize the root once so the path-safety check inside the
    // walker can reuse it. If canonicalize fails (root just disappeared,
    // permissions blip), fall back to empty rather than panicking.
    let canonical_root = match tokio::fs::canonicalize(&root).await {
        Ok(p) => p,
        Err(_) => return Ok(Vec::new()),
    };

    let mut entries: Vec<FolderEntry> = Vec::new();
    walk_dir(&scan_root, &canonical_root, &mut entries).await?;

    // Stable order for deterministic test assertions + readable output.
    entries.sort_by(|a, b| a.rel_path.cmp(&b.rel_path));
    Ok(entries)
}

/// Read one file's body. `rel_path` is interpreted relative to
/// `folder_root(ctx.xvn_home)`; absolute paths, `..` traversal, and
/// symlinks that escape the root are rejected. Missing files return
/// `ApiError::NotFound`. Bodies larger than [`MAX_FILE_BYTES`] are
/// truncated and `truncated` is set on the response.
pub async fn read(ctx: &ApiContext, rel_path: &str) -> ApiResult<FileContent> {
    let root = folder_root(&ctx.xvn_home);
    let canonical_root = tokio::fs::canonicalize(&root).await.map_err(|_| {
        ApiError::NotFound(format!(
            "strategies folder not initialized at {}",
            root.display()
        ))
    })?;

    let target = resolve_under_root(&canonical_root, &root, rel_path).await?;

    let metadata = tokio::fs::metadata(&target).await.map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            ApiError::NotFound(format!("strategies file '{rel_path}' not found"))
        } else {
            ApiError::Internal(format!("stat {}: {e}", target.display()))
        }
    })?;

    if !metadata.is_file() {
        return Err(ApiError::Validation(format!(
            "rel_path '{rel_path}' is not a regular file"
        )));
    }

    let kind = FileKind::from_extension(target.extension().and_then(|s| s.to_str()));
    let _ = metadata.len();
    let bytes = tokio::fs::read(&target)
        .await
        .map_err(|e| ApiError::Internal(format!("read {}: {e}", target.display())))?;
    let (truncated, slice) = if bytes.len() > MAX_FILE_BYTES {
        (true, &bytes[..MAX_FILE_BYTES])
    } else {
        (false, &bytes[..])
    };
    let content = String::from_utf8_lossy(slice).into_owned();

    Ok(FileContent {
        rel_path: rel_path.to_string(),
        kind,
        content,
        truncated,
    })
}

/// Resolve `rel_path` to a real path under `canonical_root`. Rejects
/// absolute paths, parent traversal, and symlinks that point outside
/// the root.
async fn resolve_under_root(
    canonical_root: &Path,
    root: &Path,
    rel_path: &str,
) -> ApiResult<PathBuf> {
    if rel_path.is_empty() {
        return Err(ApiError::Validation("rel_path is empty".into()));
    }

    let candidate = Path::new(rel_path);
    if candidate.is_absolute() {
        return Err(ApiError::Validation(format!(
            "path_escape: '{rel_path}' must be relative to the strategies folder"
        )));
    }
    // Reject `..` components even before canonicalize so a path that
    // never reaches an existing file still bounces here. canonicalize on
    // a non-existing path would otherwise return an io::Error and we'd
    // lose the specific reason.
    for comp in candidate.components() {
        match comp {
            Component::ParentDir => {
                return Err(ApiError::Validation(format!(
                    "path_escape: '{rel_path}' contains '..'"
                )));
            }
            Component::Prefix(_) | Component::RootDir => {
                return Err(ApiError::Validation(format!(
                    "path_escape: '{rel_path}' is absolute or rooted"
                )));
            }
            _ => {}
        }
    }

    let joined = root.join(candidate);

    // Canonicalize resolves symlinks; if the resolved path is not under
    // the canonical root, the rel_path was pointing at a symlink that
    // escapes (or `..` smuggled through a CurDir component).
    let resolved = tokio::fs::canonicalize(&joined).await.map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            ApiError::NotFound(format!("strategies file '{rel_path}' not found"))
        } else {
            ApiError::Internal(format!("canonicalize {}: {e}", joined.display()))
        }
    })?;

    if !resolved.starts_with(canonical_root) {
        return Err(ApiError::Validation(format!(
            "path_escape: '{rel_path}' resolves outside the strategies folder"
        )));
    }
    Ok(resolved)
}

/// Recursive directory walker. Skips symlinks that escape the canonical
/// root (an attacker who drops a symlink under the folder pointing at
/// `/etc/passwd` doesn't get to enumerate that file).
async fn walk_dir(dir: &Path, canonical_root: &Path, out: &mut Vec<FolderEntry>) -> ApiResult<()> {
    // Plain BFS via a stack so we don't need a recursive async fn.
    let mut stack: Vec<PathBuf> = vec![dir.to_path_buf()];
    while let Some(current) = stack.pop() {
        let mut read_dir = match tokio::fs::read_dir(&current).await {
            Ok(rd) => rd,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
            Err(e) => {
                return Err(ApiError::Internal(format!(
                    "read_dir {}: {e}",
                    current.display()
                )));
            }
        };

        while let Some(entry) = read_dir.next_entry().await.map_err(|e| {
            ApiError::Internal(format!("read_dir next {}: {e}", current.display()))
        })? {
            let path = entry.path();
            // Resolve symlinks at the entry level so an attacker can't
            // smuggle a path outside the folder. We canonicalize here;
            // if it fails or escapes, skip the entry rather than blowing
            // up the entire enumeration.
            let canonical = match tokio::fs::canonicalize(&path).await {
                Ok(p) => p,
                Err(_) => continue,
            };
            if !canonical.starts_with(canonical_root) {
                continue;
            }
            let metadata = match tokio::fs::metadata(&canonical).await {
                Ok(m) => m,
                Err(_) => continue,
            };
            if metadata.is_dir() {
                stack.push(path);
                continue;
            }
            if !metadata.is_file() {
                continue;
            }
            // Compute the rel_path relative to the canonical_root using
            // forward slashes (the on-wire shape we want the wizard to see).
            let rel = match canonical.strip_prefix(canonical_root) {
                Ok(r) => r.to_path_buf(),
                Err(_) => continue,
            };
            let rel_str = rel
                .components()
                .filter_map(|c| match c {
                    Component::Normal(s) => s.to_str().map(|s| s.to_string()),
                    _ => None,
                })
                .collect::<Vec<_>>()
                .join("/");

            let kind = FileKind::from_extension(canonical.extension().and_then(|s| s.to_str()));
            let size_bytes = metadata.len();
            let modified_at = metadata
                .modified()
                .ok()
                .map(|t| {
                    DateTime::<Utc>::from(t).to_rfc3339_opts(SecondsFormat::Secs, true)
                })
                .unwrap_or_default();

            out.push(FolderEntry {
                rel_path: rel_str,
                kind,
                size_bytes,
                modified_at,
            });
        }
    }
    Ok(())
}
