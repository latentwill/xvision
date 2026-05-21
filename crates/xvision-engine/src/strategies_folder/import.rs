//! Pure import logic for `$XVN_HOME/strategies/`. Resolves the target
//! subfolder, enforces the type allowlist + size cap, copies/writes the
//! source bytes into place, and runs a summary extractor for PDFs/CSVs.
//!
//! Two entry points:
//! - [`import_from_path`] — used by `xvn strategies import <path>`
//!   (CLI). Reads bytes from a host filesystem path.
//! - [`import_bytes`] — used by the dashboard POST upload route. Takes a
//!   raw byte buffer + a logical filename (the multipart `filename`).
//!
//! Both routes produce a [`FolderEntry`] for the saved file and a
//! [`Vec<ImportFinding>`] capturing soft warnings (e.g. missing
//! `pdftotext`).
//!
//! Spec: V2F wave-2 leaf — see `team/contracts/strategies-folder-import.md`.

use std::path::{Component, Path};

use chrono::{DateTime, SecondsFormat, Utc};

use crate::api::{ApiContext, ApiError, ApiResult};

use super::reader::{folder_root, SUBFOLDER_ALLOWLIST};
use super::summary::{summarize_csv, summarize_pdf, SummaryOutcome};
use super::types::{FileKind, FolderEntry};

/// Hard size cap per imported file. 25 MB matches the contract.
pub const MAX_IMPORT_BYTES: u64 = 25 * 1024 * 1024;

/// File extensions accepted by the importer. Anything not in this set is
/// rejected with [`ApiError::Validation`] (mapped to 400 by the dashboard).
pub const ACCEPTED_EXTENSIONS: &[&str] = &["md", "txt", "csv", "pdf", "json"];

/// Soft warning surfaced alongside a successful import. The `code` is a
/// stable enum-string the dashboard surfaces to operators verbatim
/// (e.g. `summary_extractor_unavailable`). `detail` is human-readable.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ImportFinding {
    pub code: String,
    pub detail: String,
}

/// Full import result. Always carries the new entry; findings list is
/// empty when nothing soft-failed (which is the happy path for md/txt/json).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ImportOutcome {
    pub entry: FolderEntry,
    pub summary: Option<FolderEntry>,
    pub findings: Vec<ImportFinding>,
}

/// Caller-side options. `subfolder = None` triggers the per-extension
/// default; `clobber = false` skips when the target exists.
#[derive(Debug, Clone)]
pub struct ImportOptions {
    pub subfolder: Option<String>,
    pub clobber: bool,
}

impl Default for ImportOptions {
    fn default() -> Self {
        Self {
            subfolder: None,
            clobber: true,
        }
    }
}

/// Read the file at `src` from disk and import it. Validates size before
/// reading the body — a 100 MB file is rejected with a Validation error
/// after a single `stat`, not after a full read.
pub async fn import_from_path(ctx: &ApiContext, src: &Path, opts: ImportOptions) -> ApiResult<ImportOutcome> {
    let metadata = tokio::fs::metadata(src).await.map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            ApiError::NotFound(format!("import source {} not found", src.display()))
        } else {
            ApiError::Internal(format!("stat {}: {e}", src.display()))
        }
    })?;
    if !metadata.is_file() {
        return Err(ApiError::Validation(format!(
            "import source {} is not a regular file",
            src.display()
        )));
    }
    if metadata.len() > MAX_IMPORT_BYTES {
        return Err(ApiError::Validation(format!(
            "import_too_large: {} is {} bytes; max is {} bytes",
            src.display(),
            metadata.len(),
            MAX_IMPORT_BYTES
        )));
    }

    let filename = src
        .file_name()
        .and_then(|s| s.to_str())
        .ok_or_else(|| {
            ApiError::Validation(format!(
                "import source {} has no readable filename",
                src.display()
            ))
        })?
        .to_string();

    let bytes = tokio::fs::read(src)
        .await
        .map_err(|e| ApiError::Internal(format!("read {}: {e}", src.display())))?;

    import_bytes(ctx, &filename, &bytes, opts).await
}

/// Import a raw byte buffer with the supplied logical filename. Used by
/// the dashboard multipart upload route after it has collected each part.
pub async fn import_bytes(
    ctx: &ApiContext,
    filename: &str,
    bytes: &[u8],
    opts: ImportOptions,
) -> ApiResult<ImportOutcome> {
    let safe_name = sanitize_filename(filename)?;
    if (bytes.len() as u64) > MAX_IMPORT_BYTES {
        return Err(ApiError::Validation(format!(
            "import_too_large: {safe_name} is {} bytes; max is {MAX_IMPORT_BYTES} bytes",
            bytes.len()
        )));
    }
    let ext = extension(&safe_name).ok_or_else(|| {
        ApiError::Validation(format!(
            "type_not_allowed: '{safe_name}' has no extension; accepted: {}",
            ACCEPTED_EXTENSIONS.join(", ")
        ))
    })?;
    if !ACCEPTED_EXTENSIONS.contains(&ext.as_str()) {
        return Err(ApiError::Validation(format!(
            "type_not_allowed: '.{ext}' is not in the importer allowlist; accepted: {}",
            ACCEPTED_EXTENSIONS.join(", ")
        )));
    }

    let subfolder = resolve_subfolder(opts.subfolder.as_deref(), &ext)?;
    let root = folder_root(&ctx.xvn_home);
    let target_dir = root.join(&subfolder);
    tokio::fs::create_dir_all(&target_dir)
        .await
        .map_err(|e| ApiError::Internal(format!("create {}: {e}", target_dir.display())))?;
    let target_path = target_dir.join(&safe_name);

    // Path-safety: make sure target stays inside the canonical folder
    // root. Canonicalize the *parent* (which we just created) — the file
    // itself may not exist yet.
    let canonical_root = tokio::fs::canonicalize(&root)
        .await
        .map_err(|e| ApiError::Internal(format!("canonicalize {}: {e}", root.display())))?;
    let canonical_target_dir = tokio::fs::canonicalize(&target_dir)
        .await
        .map_err(|e| ApiError::Internal(format!("canonicalize {}: {e}", target_dir.display())))?;
    if !canonical_target_dir.starts_with(&canonical_root) {
        return Err(ApiError::Validation(format!(
            "path_escape: target {} resolves outside the strategies folder",
            target_path.display()
        )));
    }

    if !opts.clobber && tokio::fs::try_exists(&target_path).await.unwrap_or(false) {
        return Err(ApiError::Conflict(format!(
            "no_clobber: {} already exists",
            relative_to_root(&canonical_root, &target_path)
        )));
    }

    tokio::fs::write(&target_path, bytes)
        .await
        .map_err(|e| ApiError::Internal(format!("write {}: {e}", target_path.display())))?;

    let kind = FileKind::from_extension(Some(ext.as_str()));
    let entry = build_entry(&canonical_root, &target_path, kind).await?;

    let mut findings = Vec::new();
    let mut summary_entry = None;
    match kind {
        FileKind::Pdf => match summarize_pdf(&target_path).await {
            SummaryOutcome::Written { .. } => {
                let sidecar = match super::summary::sidecar_path_for(&target_path) {
                    Some(p) => p,
                    None => target_path.clone(),
                };
                summary_entry = Some(build_entry(&canonical_root, &sidecar, FileKind::Markdown).await?);
            }
            SummaryOutcome::ExtractorUnavailable => {
                findings.push(ImportFinding {
                    code: "summary_extractor_unavailable".into(),
                    detail: "pdftotext not on PATH; original PDF imported without summary".into(),
                });
            }
            SummaryOutcome::ExtractorFailed(detail) => {
                findings.push(ImportFinding {
                    code: "summary_extractor_failed".into(),
                    detail,
                });
            }
            SummaryOutcome::NotApplicable => {}
        },
        FileKind::Csv => match summarize_csv(&target_path).await {
            SummaryOutcome::Written { .. } => {
                let sidecar = match super::summary::sidecar_path_for(&target_path) {
                    Some(p) => p,
                    None => target_path.clone(),
                };
                summary_entry = Some(build_entry(&canonical_root, &sidecar, FileKind::Markdown).await?);
            }
            SummaryOutcome::ExtractorUnavailable => {
                findings.push(ImportFinding {
                    code: "summary_extractor_unavailable".into(),
                    detail: "csv summary extractor unavailable".into(),
                });
            }
            SummaryOutcome::ExtractorFailed(detail) => {
                findings.push(ImportFinding {
                    code: "summary_extractor_failed".into(),
                    detail,
                });
            }
            SummaryOutcome::NotApplicable => {}
        },
        FileKind::Markdown | FileKind::Json | FileKind::Text | FileKind::Other => {}
    }

    Ok(ImportOutcome {
        entry,
        summary: summary_entry,
        findings,
    })
}

/// Build a `FolderEntry` for a freshly-written file. Mirrors the shape
/// `reader::list` emits so the dashboard can append the new row without
/// re-listing.
async fn build_entry(canonical_root: &Path, target_path: &Path, kind: FileKind) -> ApiResult<FolderEntry> {
    let metadata = tokio::fs::metadata(target_path)
        .await
        .map_err(|e| ApiError::Internal(format!("stat {}: {e}", target_path.display())))?;
    let canonical = tokio::fs::canonicalize(target_path)
        .await
        .map_err(|e| ApiError::Internal(format!("canonicalize {}: {e}", target_path.display())))?;
    let rel_path = relative_to_root(canonical_root, &canonical);
    let modified_at = metadata
        .modified()
        .ok()
        .map(|t| DateTime::<Utc>::from(t).to_rfc3339_opts(SecondsFormat::Secs, true))
        .unwrap_or_default();
    Ok(FolderEntry {
        rel_path,
        kind,
        size_bytes: metadata.len(),
        modified_at,
    })
}

fn relative_to_root(canonical_root: &Path, path: &Path) -> String {
    match path.strip_prefix(canonical_root) {
        Ok(rel) => rel
            .components()
            .filter_map(|c| match c {
                Component::Normal(s) => s.to_str().map(|s| s.to_string()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("/"),
        Err(_) => path.display().to_string(),
    }
}

fn extension(name: &str) -> Option<String> {
    Path::new(name)
        .extension()
        .and_then(|s| s.to_str())
        .map(|s| s.to_ascii_lowercase())
}

/// Sanitise a caller-supplied filename. Rejects parent traversal, drive
/// letters, and anything that resolves to more than one path component.
fn sanitize_filename(name: &str) -> ApiResult<String> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Err(ApiError::Validation("filename is empty".into()));
    }
    let path = Path::new(trimmed);
    let mut components = path.components();
    let only = components.next();
    if components.next().is_some() {
        return Err(ApiError::Validation(format!(
            "path_escape: filename '{trimmed}' contains directory separators"
        )));
    }
    match only {
        Some(Component::Normal(s)) => {
            let raw = s
                .to_str()
                .ok_or_else(|| ApiError::Validation("filename is not utf-8".into()))?;
            if raw.contains('\\') || raw.contains('/') {
                return Err(ApiError::Validation(format!(
                    "path_escape: filename '{raw}' contains separators"
                )));
            }
            Ok(raw.to_string())
        }
        _ => Err(ApiError::Validation(format!(
            "path_escape: filename '{trimmed}' is not a plain name"
        ))),
    }
}

/// Resolve the destination subfolder. Caller `--to` overrides; missing
/// override defaults by extension per the contract.
fn resolve_subfolder(requested: Option<&str>, ext: &str) -> ApiResult<String> {
    if let Some(name) = requested {
        if !SUBFOLDER_ALLOWLIST.contains(&name) {
            return Err(ApiError::Validation(format!(
                "subfolder_not_allowed: '{name}' is not in the strategies-folder allowlist ({})",
                SUBFOLDER_ALLOWLIST.join(", ")
            )));
        }
        return Ok(name.to_string());
    }
    Ok(default_subfolder_for(ext).to_string())
}

/// Per-extension default destination. Documented in the contract:
/// `.md` / `.txt` → notes/, `.pdf` → docs/, `.csv` → docs/,
/// `.json` → strategy-files/.
pub fn default_subfolder_for(ext: &str) -> &'static str {
    match ext {
        "md" | "txt" => "notes",
        "pdf" | "csv" => "docs",
        "json" => "strategy-files",
        _ => "notes",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_subfolder_routes_extensions() {
        assert_eq!(default_subfolder_for("md"), "notes");
        assert_eq!(default_subfolder_for("txt"), "notes");
        assert_eq!(default_subfolder_for("pdf"), "docs");
        assert_eq!(default_subfolder_for("csv"), "docs");
        assert_eq!(default_subfolder_for("json"), "strategy-files");
    }

    #[test]
    fn sanitize_filename_rejects_traversal() {
        assert!(sanitize_filename("../etc/passwd").is_err());
        assert!(sanitize_filename("foo/bar.md").is_err());
        assert!(sanitize_filename("/abs/bar.md").is_err());
        assert!(sanitize_filename("").is_err());
        assert!(sanitize_filename("  ").is_err());
        assert!(sanitize_filename("foo.md").is_ok());
    }

    #[test]
    fn resolve_subfolder_uses_allowlist() {
        assert_eq!(resolve_subfolder(None, "md").unwrap(), "notes");
        assert_eq!(resolve_subfolder(Some("docs"), "md").unwrap(), "docs");
        assert!(resolve_subfolder(Some("secrets"), "md").is_err());
    }
}
