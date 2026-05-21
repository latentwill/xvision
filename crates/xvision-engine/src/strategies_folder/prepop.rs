//! Prepopulation logic for `$XVN_HOME/strategies/`.
//!
//! Ships the curated strategy-template library from `docs/strategies/`
//! into the user's strategies folder, with a provenance manifest at
//! `library/.from-docs.json` so re-runs can detect drift and refresh
//! cleanly. The wizard later reads from this surface to quote
//! library content back to the user (see the V2F plan at
//! `docs/superpowers/plans/2026-05-21-v2f-strategies-folder-and-template-refactor.md`).
//!
//! Idempotency contract:
//! - Without `--force`, each library file is rehashed; if it matches
//!   the manifest's `sha256`, the file is refreshed from the embedded
//!   source (no-op if the source is unchanged). If it diverges from
//!   the manifest (user edited the copy), the user's copy is preserved
//!   and a `strategies_library_drift` finding is recorded.
//! - With `--force`, every entry is overwritten regardless of drift.
//! - New embedded sources (not yet in the manifest) are copied in and
//!   the manifest is appended.
//! - Stale manifest entries (no longer present in the embedded source)
//!   are kept on disk but recorded as `strategies_library_stale_source`
//!   findings — operators may have deliberately snapshotted them.
//!
//! Embedded source: the contents of `docs/strategies/templates/**` and
//! `docs/strategies/freqtrade_strategies_playlist.md` are baked into the
//! binary via `include_dir!` so `xvn strategies init` works on hosts
//! that don't ship the docs tree (the deployed container image is the
//! motivating case). Choice of `include_dir!` over a relative-path read
//! keeps the surface single-source — no env-flag toggling between dev
//! and prod, no implicit dependency on the executing directory.

use std::path::{Path, PathBuf};

use chrono::{SecondsFormat, Utc};
use include_dir::{include_dir, Dir, DirEntry};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::api::{ApiError, ApiResult};

use super::reader::{folder_root, SUBFOLDER_ALLOWLIST};

/// Embedded `docs/strategies/` snapshot. Captures the templates tree
/// and the freqtrade playlist at build time so the deployed image
/// doesn't need the docs directory on disk.
static EMBEDDED_DOCS_STRATEGIES: Dir<'_> = include_dir!("$CARGO_MANIFEST_DIR/../../docs/strategies");

/// Manifest filename, relative to `<root>/library/`. Hidden by leading
/// dot so a casual `ls` of the library folder shows just the curated
/// strategies and not the bookkeeping.
pub const MANIFEST_FILENAME: &str = ".from-docs.json";

/// Current schema version of the manifest. Bumped if the on-disk
/// shape changes incompatibly.
pub const MANIFEST_VERSION: u32 = 1;

/// One entry in the provenance manifest.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ManifestEntry {
    /// Path of the copied file relative to `folder_root()` (the
    /// strategies folder), forward-slash separated. Always starts
    /// with `library/`.
    pub rel_path: String,
    /// Path of the source file relative to the workspace root, using
    /// forward slashes. Always starts with `docs/strategies/`.
    pub source: String,
    /// SHA-256 of the source bytes at copy time, lowercase hex. Used
    /// for drift detection on re-runs.
    pub sha256: String,
    /// RFC3339 UTC timestamp of the last copy. Stable across re-runs
    /// for entries whose source bytes haven't changed.
    pub copied_at: String,
}

/// On-disk shape of `<root>/library/.from-docs.json`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Manifest {
    pub version: u32,
    pub entries: Vec<ManifestEntry>,
}

impl Default for Manifest {
    fn default() -> Self {
        Self {
            version: MANIFEST_VERSION,
            entries: Vec::new(),
        }
    }
}

/// Report returned by [`init`]. Callers (CLI surface, tests) inspect
/// this to render the findings to stderr / assert behavior. The CLI
/// formats `drift` and `stale_source` as `strategies_library_drift`
/// and `strategies_library_stale_source` findings respectively.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct InitReport {
    /// Subfolders newly created on this run (e.g. `notes`, `library`).
    /// Empty on a re-run against an already-initialized folder.
    pub created_subfolders: Vec<String>,
    /// rel_paths of files copied for the first time on this run
    /// (manifest entry did not exist).
    pub new_files: Vec<String>,
    /// rel_paths of files refreshed in place (already tracked + copy
    /// matched manifest sha256 / `--force` enabled).
    pub refreshed_files: Vec<String>,
    /// rel_paths of files left untouched because the user-edited copy
    /// diverged from the manifest's sha256 and `--force` was not set.
    pub drift: Vec<String>,
    /// rel_paths of manifest entries whose embedded source no longer
    /// exists. Files are left on disk; only the finding is emitted.
    pub stale_source: Vec<String>,
}

/// Options for [`init`].
#[derive(Debug, Default, Clone, Copy)]
pub struct InitOptions {
    /// When `true`, overwrite even files that diverge from the
    /// manifest's sha256 (i.e. user-edited copies are clobbered).
    pub force: bool,
}

/// Initialize (or refresh) `<xvn_home>/strategies/` from the embedded
/// docs snapshot. Creates the five allowlisted subfolders if missing
/// and writes the library + manifest. Idempotent — safe to call on
/// an already-populated folder.
pub async fn init(xvn_home: &Path, opts: InitOptions) -> ApiResult<InitReport> {
    let root = folder_root(xvn_home);
    let mut report = InitReport::default();

    // Step 1: create root + the five allowlisted subfolders. Track
    // which ones we materialize for the first time so the CLI can
    // surface a "wrote N subfolders" line.
    tokio::fs::create_dir_all(&root)
        .await
        .map_err(|e| ApiError::Internal(format!("create strategies root {}: {e}", root.display())))?;
    for name in SUBFOLDER_ALLOWLIST {
        let sub = root.join(name);
        let existed = tokio::fs::try_exists(&sub).await.unwrap_or(false);
        if !existed {
            tokio::fs::create_dir_all(&sub)
                .await
                .map_err(|e| ApiError::Internal(format!("create subfolder {}: {e}", sub.display())))?;
            report.created_subfolders.push((*name).to_string());
        }
    }

    // Step 2: load the existing manifest (if any). Missing → fresh.
    let library_root = root.join("library");
    let manifest_path = library_root.join(MANIFEST_FILENAME);
    let mut manifest = read_manifest(&manifest_path).await?;

    // Step 3: enumerate embedded sources. Build a map source → bytes
    // so we can both copy and lookup-by-source for stale detection.
    let embedded = collect_embedded_sources();

    // Step 4: process each embedded source. Either it matches an
    // existing manifest entry (refresh / drift-skip) or it's new
    // (copy + append).
    let mut keep_indices: Vec<bool> = vec![false; manifest.entries.len()];
    let now = Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true);

    for (source_rel, bytes) in &embedded {
        let rel_path = library_rel_path_for(source_rel);
        let target_abs = root.join(&rel_path);
        let source_str = format!("docs/strategies/{}", source_rel);
        let source_hash = sha256_hex(bytes);

        // Find an existing manifest entry by rel_path.
        let existing_idx = manifest.entries.iter().position(|e| e.rel_path == rel_path);

        if let Some(idx) = existing_idx {
            keep_indices[idx] = true;
            // Decide refresh vs preserve.
            let entry = &manifest.entries[idx];
            let on_disk_hash = match read_file_hash(&target_abs).await? {
                Some(h) => h,
                None => {
                    // File was deleted underneath us — recopy from
                    // source. Update manifest's sha256 to the new
                    // source hash and copied_at to now.
                    write_file(&target_abs, bytes).await?;
                    manifest.entries[idx] = ManifestEntry {
                        rel_path: rel_path.clone(),
                        source: source_str,
                        sha256: source_hash,
                        copied_at: now.clone(),
                    };
                    report.refreshed_files.push(rel_path);
                    continue;
                }
            };

            if on_disk_hash != entry.sha256 && !opts.force {
                // User edited the copy — preserve and surface drift.
                report.drift.push(rel_path);
                continue;
            }

            // Either matches (clean refresh) or force-overwrite.
            // Only touch the file + bump copied_at if the source has
            // actually changed; this keeps the idempotency contract
            // ("identical filesystem state modulo copied_at" — and
            // copied_at itself is stable for unchanged entries).
            if on_disk_hash == source_hash {
                // Already in sync. Update source/sha256/copied_at
                // only if they differ (e.g. source path renamed but
                // bytes identical — unusual but cheap to handle).
                if entry.source != source_str || entry.sha256 != source_hash {
                    manifest.entries[idx] = ManifestEntry {
                        rel_path: rel_path.clone(),
                        source: source_str,
                        sha256: source_hash,
                        copied_at: entry.copied_at.clone(),
                    };
                }
                continue;
            }

            write_file(&target_abs, bytes).await?;
            manifest.entries[idx] = ManifestEntry {
                rel_path: rel_path.clone(),
                source: source_str,
                sha256: source_hash,
                copied_at: now.clone(),
            };
            report.refreshed_files.push(rel_path);
        } else {
            // New source: copy in and append to manifest.
            write_file(&target_abs, bytes).await?;
            manifest.entries.push(ManifestEntry {
                rel_path: rel_path.clone(),
                source: source_str,
                sha256: source_hash,
                copied_at: now.clone(),
            });
            keep_indices.push(true);
            report.new_files.push(rel_path);
        }
    }

    // Step 5: stale-source pass. Any manifest entry not visited above
    // points at a source that no longer exists. Leave the on-disk copy
    // in place (operator may want the snapshot) but record the finding.
    // The entry itself stays in the manifest so a future restoration of
    // the source can pick it back up.
    for (idx, kept) in keep_indices.iter().enumerate() {
        if !kept {
            report.stale_source.push(manifest.entries[idx].rel_path.clone());
        }
    }

    // Step 6: sort manifest entries by rel_path for deterministic
    // serialization. Order changes otherwise as the embedded-dir
    // walker's order isn't guaranteed across platforms.
    manifest.entries.sort_by(|a, b| a.rel_path.cmp(&b.rel_path));
    report.new_files.sort();
    report.refreshed_files.sort();
    report.drift.sort();
    report.stale_source.sort();
    report.created_subfolders.sort();

    // Step 7: write manifest. Pretty-printed so operators can read it.
    write_manifest(&manifest_path, &manifest).await?;

    Ok(report)
}

/// Convert a path inside the embedded `docs/strategies/` tree into the
/// rel_path used under `<root>/library/`. Forward-slash separated.
///
/// Examples:
/// - `templates/EMA/ema_pullback_bounce.json`
///   → `library/templates/EMA/ema_pullback_bounce.json`
/// - `freqtrade_strategies_playlist.md`
///   → `library/freqtrade_strategies_playlist.md`
fn library_rel_path_for(source_rel: &str) -> String {
    format!("library/{}", source_rel)
}

/// Walk the embedded `docs/strategies/` snapshot and return the
/// (source_rel_path, bytes) pairs the prepopulator copies. Only the
/// templates tree and the freqtrade playlist are eligible; other
/// files in `docs/strategies/` (e.g. the README) are skipped.
fn collect_embedded_sources() -> Vec<(String, Vec<u8>)> {
    let mut out: Vec<(String, Vec<u8>)> = Vec::new();
    collect_recursive(&EMBEDDED_DOCS_STRATEGIES, &mut out);
    out.sort_by(|a, b| a.0.cmp(&b.0));
    out
}

fn collect_recursive(dir: &Dir<'_>, out: &mut Vec<(String, Vec<u8>)>) {
    for entry in dir.entries() {
        match entry {
            DirEntry::Dir(sub) => collect_recursive(sub, out),
            DirEntry::File(f) => {
                let path = f.path();
                let rel = normalize_to_forward_slashes(path);
                if is_eligible_source(&rel) {
                    out.push((rel, f.contents().to_vec()));
                }
            }
        }
    }
}

fn normalize_to_forward_slashes(path: &Path) -> String {
    path.components()
        .filter_map(|c| match c {
            std::path::Component::Normal(s) => s.to_str().map(|s| s.to_string()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("/")
}

/// Eligibility filter. Pulls everything under `templates/` plus the
/// freqtrade playlist; skips top-level READMEs.
fn is_eligible_source(rel: &str) -> bool {
    rel == "freqtrade_strategies_playlist.md" || rel.starts_with("templates/")
}

async fn read_manifest(path: &Path) -> ApiResult<Manifest> {
    match tokio::fs::read_to_string(path).await {
        Ok(text) => serde_json::from_str(&text)
            .map_err(|e| ApiError::Internal(format!("parse manifest {}: {e}", path.display()))),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Manifest::default()),
        Err(e) => Err(ApiError::Internal(format!(
            "read manifest {}: {e}",
            path.display()
        ))),
    }
}

async fn write_manifest(path: &Path, manifest: &Manifest) -> ApiResult<()> {
    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .map_err(|e| ApiError::Internal(format!("create manifest parent {}: {e}", parent.display())))?;
    }
    let body = serde_json::to_string_pretty(manifest)
        .map_err(|e| ApiError::Internal(format!("serialize manifest: {e}")))?;
    tokio::fs::write(path, body)
        .await
        .map_err(|e| ApiError::Internal(format!("write manifest {}: {e}", path.display())))
}

async fn write_file(path: &Path, bytes: &[u8]) -> ApiResult<()> {
    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .map_err(|e| ApiError::Internal(format!("create library parent {}: {e}", parent.display())))?;
    }
    tokio::fs::write(path, bytes)
        .await
        .map_err(|e| ApiError::Internal(format!("write library file {}: {e}", path.display())))
}

/// Hash a file's bytes. Returns `Ok(None)` when the file does not
/// exist (caller will recopy from source).
async fn read_file_hash(path: &Path) -> ApiResult<Option<String>> {
    match tokio::fs::read(path).await {
        Ok(bytes) => Ok(Some(sha256_hex(&bytes))),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(ApiError::Internal(format!("hash {}: {e}", path.display()))),
    }
}

fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hex::encode(hasher.finalize())
}

/// Test-only helper: derive the absolute path to the manifest file
/// for a given xvn_home. Centralized so tests don't hand-build the
/// path and miss a refactor.
pub fn manifest_path(xvn_home: &Path) -> PathBuf {
    folder_root(xvn_home).join("library").join(MANIFEST_FILENAME)
}
