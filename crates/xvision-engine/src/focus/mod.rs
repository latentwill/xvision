//! Focus chain (Phase 2.4) — per-scope `focus.md` files.
//!
//! Each [`ContextScope`](crate::chat_session::ContextScope) the chat rail can
//! attach to gets a single durable focus file the operator edits and the
//! WizardLoop injects into the system prompt. Focus is **filesystem-backed**,
//! not a DB row: it lives at
//!
//! ```text
//! $XVN_HOME/scopes/<scope_kind>/<scope_id>/focus.md
//! ```
//!
//! where `<scope_id>` is the scope's id, or the literal `_` when the scope
//! names none (e.g. the workspace scope). The `chat_sessions.focus_path`
//! column (migration 041) stores the resolved path so a resumed session
//! re-loads the same file.
//!
//! ## Path safety (hard requirement)
//!
//! Both `scope_kind` and `scope_id` reach this module from user-driven HTTP
//! query/body params. A malicious or buggy caller must never be able to read
//! or write outside `$XVN_HOME/scopes/`. [`focus_path`] therefore rejects any
//! scope component that:
//!
//! - is empty,
//! - equals `.` or `..`,
//! - contains a path separator (`/` or `\`) or a NUL,
//! - is an absolute path,
//! - or contains any other path component (`Component::ParentDir`,
//!   `Component::RootDir`, etc.).
//!
//! The check is whitelist-shaped: after stripping, the only legal form is a
//! single `Component::Normal` that is byte-for-byte the input. Anything else
//! is an error, so traversal (`../../etc`), absolute escapes (`/etc/passwd`),
//! and separator smuggling (`a/b`) all fail before any IO happens.

use std::path::{Component, Path, PathBuf};

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::chat_session::ContextScope;

/// Sentinel `scope_id` directory segment for scopes that name no id (e.g.
/// the workspace scope). Chosen so it can never collide with a real ULID /
/// route id (those never equal a bare underscore) and so the on-disk layout
/// stays a uniform `<kind>/<id>/focus.md`.
pub const NO_SCOPE_ID: &str = "_";

/// The focus filename within a scope directory.
pub const FOCUS_FILENAME: &str = "focus.md";

/// A loaded or freshly-saved focus document: its resolved on-disk path, the
/// UTF-8 content, and the content-addressed hash used to detect drift and to
/// stamp [`FocusEvent`](xvision_observability::FocusEvent)s.
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts-export",
    ts(export, export_to = "../../../frontend/web/src/api/types.gen/")
)]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FocusDoc {
    /// Absolute path to the focus file (`…/scopes/<kind>/<id>/focus.md`).
    pub path: String,
    /// Full UTF-8 content of the focus file.
    pub content: String,
    /// Lowercase hex `sha256(content)`. Stable for identical content; changes
    /// on any edit. Mirrors the `content_hash` carried on `FocusEvent`.
    pub content_hash: String,
}

/// Lowercase-hex `sha256` of the given content. Shared by [`save`] and
/// callers that need to compare a hash without re-reading the file.
pub fn content_hash(content: &str) -> String {
    format!("{:x}", Sha256::digest(content.as_bytes()))
}

/// Root directory holding every scope's focus tree: `$XVN_HOME/scopes/`.
pub fn scopes_root(xvn_home: &Path) -> PathBuf {
    xvn_home.join("scopes")
}

/// Validate one scope path component (kind or id) and return it unchanged on
/// success. Rejects anything that is not a single, normal, separator-free
/// path segment — the only shape that cannot escape the scopes root.
fn safe_component(label: &str, value: &str) -> Result<String> {
    if value.is_empty() {
        bail!("focus {label} is empty");
    }
    if value == "." || value == ".." {
        bail!("focus {label} `{value}` is a relative path component");
    }
    if value.contains('\0') {
        bail!("focus {label} `{value}` contains a NUL byte");
    }
    if value.contains('/') || value.contains('\\') {
        bail!("focus {label} `{value}` contains a path separator");
    }
    // Defense in depth: the byte checks above already exclude separators and
    // dot segments, but parse the component set too so any platform-specific
    // path quirk (drive prefixes, UNC roots) is also rejected.
    let path = Path::new(value);
    let mut comps = path.components();
    match (comps.next(), comps.next()) {
        (Some(Component::Normal(c)), None) if c == std::ffi::OsStr::new(value) => Ok(value.to_string()),
        _ => bail!("focus {label} `{value}` is not a single normal path component"),
    }
}

/// Resolve the focus file path for an explicit `(scope_kind, scope_id)` pair.
///
/// `scope_id` is `None` for scopes that name none; it resolves to the
/// [`NO_SCOPE_ID`] sentinel directory. Returns an error (no IO performed) if
/// either component would let the path escape `$XVN_HOME/scopes/`.
pub fn focus_path(xvn_home: &Path, scope_kind: &str, scope_id: Option<&str>) -> Result<PathBuf> {
    let kind = safe_component("scope_kind", scope_kind)?;
    let id = match scope_id {
        Some(raw) => safe_component("scope_id", raw)?,
        None => NO_SCOPE_ID.to_string(),
    };
    Ok(scopes_root(xvn_home).join(kind).join(id).join(FOCUS_FILENAME))
}

/// Map a [`ContextScope`] to its `(scope_kind, scope_id)` addressing pair,
/// mirroring the snake_case discriminant + scoped id the unified event model
/// uses. The `kind` matches `ContextScope`'s serde tag.
pub fn scope_address(scope: &ContextScope) -> (String, Option<String>) {
    match scope {
        ContextScope::Workspace => ("workspace".into(), None),
        ContextScope::Route { route } => ("route".into(), Some(route.clone())),
        ContextScope::Run { run_id } => ("run".into(), Some(run_id.clone())),
        ContextScope::Strategy { draft_id } => ("strategy".into(), Some(draft_id.clone())),
        ContextScope::Deployment { deployment_id } => {
            ("deployment".into(), Some(deployment_id.clone()))
        }
        // Set/list scopes have no single stable id; they share the no-id
        // sentinel so the workspace-level focus applies. The injector can
        // still load these; the route layer never derives an unsafe segment
        // because the sentinel is used.
        ContextScope::Compare { .. } => ("compare".into(), None),
        ContextScope::JournalFilter { .. } => ("journal_filter".into(), None),
        ContextScope::Selection { .. } => ("selection".into(), None),
        ContextScope::Seed { seed_id } => ("seed".into(), Some(seed_id.clone())),
    }
}

/// Resolve the focus path for a [`ContextScope`] directly.
pub fn focus_path_for_scope(xvn_home: &Path, scope: &ContextScope) -> Result<PathBuf> {
    let (kind, id) = scope_address(scope);
    focus_path(xvn_home, &kind, id.as_deref())
}

/// Load the focus document for a scope. `Ok(None)` when no focus file exists
/// yet; `Err` for unsafe scope components, non-UTF-8 content, or IO errors.
pub async fn load(xvn_home: &Path, scope: &ContextScope) -> Result<Option<FocusDoc>> {
    let (kind, id) = scope_address(scope);
    load_by_kind(xvn_home, &kind, id.as_deref()).await
}

/// Load by explicit `(scope_kind, scope_id)` — the form the HTTP route uses.
pub async fn load_by_kind(
    xvn_home: &Path,
    scope_kind: &str,
    scope_id: Option<&str>,
) -> Result<Option<FocusDoc>> {
    let path = focus_path(xvn_home, scope_kind, scope_id)?;
    match tokio::fs::read(&path).await {
        Ok(bytes) => {
            let content = String::from_utf8(bytes)
                .with_context(|| format!("focus file {} is not valid UTF-8", path.display()))?;
            let hash = content_hash(&content);
            Ok(Some(FocusDoc {
                path: path.to_string_lossy().into_owned(),
                content,
                content_hash: hash,
            }))
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e).with_context(|| format!("read focus file {}", path.display())),
    }
}

/// Save focus content for a scope, creating the scope directory tree as
/// needed. Returns the resulting [`FocusDoc`] (path + content + fresh hash).
pub async fn save(xvn_home: &Path, scope: &ContextScope, content: &str) -> Result<FocusDoc> {
    let (kind, id) = scope_address(scope);
    save_by_kind(xvn_home, &kind, id.as_deref(), content).await
}

/// Save by explicit `(scope_kind, scope_id)` — the form the HTTP route uses.
///
/// Writes atomically (tmp file + rename) so a concurrent reader never sees a
/// half-written focus file.
pub async fn save_by_kind(
    xvn_home: &Path,
    scope_kind: &str,
    scope_id: Option<&str>,
    content: &str,
) -> Result<FocusDoc> {
    let path = focus_path(xvn_home, scope_kind, scope_id)?;
    let dir = path
        .parent()
        .ok_or_else(|| anyhow::anyhow!("focus path {} has no parent dir", path.display()))?;
    tokio::fs::create_dir_all(dir)
        .await
        .with_context(|| format!("create focus dir {}", dir.display()))?;

    // Unique temp file in the same dir → atomic rename, no torn reads even
    // when two saves for the same scope race.
    let tmp = unique_tmp_path(dir);
    tokio::fs::write(&tmp, content.as_bytes())
        .await
        .with_context(|| format!("write tmp focus file {}", tmp.display()))?;
    if let Err(e) = tokio::fs::rename(&tmp, &path).await {
        let _ = tokio::fs::remove_file(&tmp).await;
        return Err(e).with_context(|| format!("rename focus file into {}", path.display()));
    }

    Ok(FocusDoc {
        path: path.to_string_lossy().into_owned(),
        content: content.to_string(),
        content_hash: content_hash(content),
    })
}

fn unique_tmp_path(dir: &Path) -> PathBuf {
    use std::time::SystemTime;
    let pid = std::process::id();
    let nanos = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    dir.join(format!(".{FOCUS_FILENAME}.{pid}.{nanos}.tmp"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    // ── Path safety ─────────────────────────────────────────────────────

    #[test]
    fn rejects_parent_dir_traversal() {
        let home = Path::new("/tmp/xvn");
        assert!(focus_path(home, "..", Some("x")).is_err());
        assert!(focus_path(home, "strategy", Some("..")).is_err());
        assert!(focus_path(home, "strategy", Some("../../etc/passwd")).is_err());
    }

    #[test]
    fn rejects_separator_bearing_components() {
        let home = Path::new("/tmp/xvn");
        assert!(focus_path(home, "a/b", Some("x")).is_err());
        assert!(focus_path(home, "strategy", Some("a/b")).is_err());
        assert!(focus_path(home, "strategy", Some("a\\b")).is_err());
    }

    #[test]
    fn rejects_absolute_components() {
        let home = Path::new("/tmp/xvn");
        assert!(focus_path(home, "/etc", Some("x")).is_err());
        assert!(focus_path(home, "strategy", Some("/etc/passwd")).is_err());
    }

    #[test]
    fn rejects_empty_dot_and_nul() {
        let home = Path::new("/tmp/xvn");
        assert!(focus_path(home, "", Some("x")).is_err());
        assert!(focus_path(home, ".", Some("x")).is_err());
        assert!(focus_path(home, "strategy", Some("")).is_err());
        assert!(focus_path(home, "strategy", Some(".")).is_err());
        assert!(focus_path(home, "strategy", Some("a\0b")).is_err());
    }

    #[test]
    fn accepts_well_formed_components_and_stays_under_scopes_root() {
        let home = Path::new("/tmp/xvn");
        let p = focus_path(home, "strategy", Some("01HABCDEF")).unwrap();
        assert_eq!(
            p,
            Path::new("/tmp/xvn/scopes/strategy/01HABCDEF/focus.md")
        );
        // The resolved path must be inside the scopes root.
        assert!(p.starts_with(scopes_root(home)));
    }

    #[test]
    fn none_scope_id_uses_sentinel() {
        let home = Path::new("/tmp/xvn");
        let p = focus_path(home, "workspace", None).unwrap();
        assert_eq!(p, Path::new("/tmp/xvn/scopes/workspace/_/focus.md"));
    }

    #[test]
    fn scope_address_matches_serde_tags() {
        assert_eq!(scope_address(&ContextScope::Workspace), ("workspace".into(), None));
        assert_eq!(
            scope_address(&ContextScope::Strategy { draft_id: "d1".into() }),
            ("strategy".into(), Some("d1".into()))
        );
        assert_eq!(
            scope_address(&ContextScope::Run { run_id: "r1".into() }),
            ("run".into(), Some("r1".into()))
        );
    }

    // ── Round trip ──────────────────────────────────────────────────────

    #[tokio::test]
    async fn load_absent_is_none() {
        let dir = tempdir().unwrap();
        let scope = ContextScope::Strategy { draft_id: "missing".into() };
        let loaded = load(dir.path(), &scope).await.unwrap();
        assert!(loaded.is_none());
    }

    #[tokio::test]
    async fn save_then_load_round_trips_with_stable_hash() {
        let dir = tempdir().unwrap();
        let scope = ContextScope::Strategy { draft_id: "btc-momentum".into() };
        let content = "# Focus\n\nKeep position sizing conservative.\n";

        let saved = save(dir.path(), &scope, content).await.unwrap();
        assert_eq!(saved.content, content);
        assert_eq!(saved.content_hash, content_hash(content));

        let loaded = load(dir.path(), &scope).await.unwrap().expect("focus exists");
        assert_eq!(loaded.content, content);
        assert_eq!(loaded.content_hash, saved.content_hash, "hash stable across save→load");
        assert_eq!(loaded.path, saved.path);

        // The file actually lives at the safe path.
        let expected = focus_path_for_scope(dir.path(), &scope).unwrap();
        assert_eq!(loaded.path, expected.to_string_lossy());
    }

    #[tokio::test]
    async fn edit_changes_the_hash() {
        let dir = tempdir().unwrap();
        let scope = ContextScope::Run { run_id: "r1".into() };

        let v1 = save(dir.path(), &scope, "first").await.unwrap();
        let v2 = save(dir.path(), &scope, "second").await.unwrap();
        assert_ne!(v1.content_hash, v2.content_hash, "edit must change the hash");

        let loaded = load(dir.path(), &scope).await.unwrap().unwrap();
        assert_eq!(loaded.content, "second");
        assert_eq!(loaded.content_hash, v2.content_hash);
    }

    #[tokio::test]
    async fn workspace_scope_persists_under_sentinel_dir() {
        let dir = tempdir().unwrap();
        let saved = save(dir.path(), &ContextScope::Workspace, "ws focus").await.unwrap();
        assert!(saved.path.ends_with("/scopes/workspace/_/focus.md"));
        let loaded = load(dir.path(), &ContextScope::Workspace).await.unwrap().unwrap();
        assert_eq!(loaded.content, "ws focus");
    }

    #[tokio::test]
    async fn save_by_kind_rejects_traversal_before_io() {
        let dir = tempdir().unwrap();
        let res = save_by_kind(dir.path(), "strategy", Some("../escape"), "x").await;
        assert!(res.is_err(), "traversal must be rejected, no file written");
        // Nothing should have been created outside scopes/.
        let escaped = dir.path().join("escape");
        assert!(!escaped.exists());
    }
}
