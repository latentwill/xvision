//! Public types for the strategies-folder read surface.
//!
//! ts-rs derives are gated on `feature = "ts-export"` so the dashboard's
//! TypeScript generation picks them up automatically.

use serde::{Deserialize, Serialize};

/// File-kind classification driven purely by extension. Anything not in
/// the allowlist falls into `Other` — the wizard tool surfaces it as an
/// opaque entry the agent can still see in the `list` result.
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts-export",
    ts(export, export_to = "../../../frontend/web/src/api/types.gen/")
)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FileKind {
    Markdown,
    Json,
    Csv,
    Pdf,
    Text,
    Other,
}

impl FileKind {
    /// Detect by lowercased file extension. Unknown extensions → `Other`.
    /// Files without an extension also fall to `Other`.
    pub fn from_extension(ext: Option<&str>) -> Self {
        match ext.map(|e| e.to_ascii_lowercase()).as_deref() {
            Some("md") => FileKind::Markdown,
            Some("json") => FileKind::Json,
            Some("csv") => FileKind::Csv,
            Some("pdf") => FileKind::Pdf,
            Some("txt") => FileKind::Text,
            _ => FileKind::Other,
        }
    }
}

/// One enumerated entry under `$XVN_HOME/strategies/`. `rel_path` is the
/// path relative to `folder_root()` using forward slashes, so the wizard
/// can quote it verbatim when calling `read_strategies_file`.
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts-export",
    ts(export, export_to = "../../../frontend/web/src/api/types.gen/")
)]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FolderEntry {
    /// Path relative to `folder_root()`, forward-slash separated.
    pub rel_path: String,
    pub kind: FileKind,
    pub size_bytes: u64,
    /// RFC3339 UTC timestamp of the file's last-modified mtime. Empty
    /// string when the mtime is unavailable on the host filesystem.
    pub modified_at: String,
}

/// One file's body, returned by `read`. Truncated at 256 KB with
/// `truncated: true` set on the response so the wizard can detect partial
/// reads and either ask for a smaller scope or call the (future) chunked
/// reader.
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts-export",
    ts(export, export_to = "../../../frontend/web/src/api/types.gen/")
)]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FileContent {
    pub rel_path: String,
    pub kind: FileKind,
    /// File body, decoded as UTF-8 with lossy replacement for invalid
    /// sequences (PDFs etc. are still returned — agents can decide whether
    /// to ignore the binary blob or call a future summary extractor).
    pub content: String,
    /// `true` when the file is larger than the 256 KB cap and `content`
    /// has been truncated to the cap.
    pub truncated: bool,
}
