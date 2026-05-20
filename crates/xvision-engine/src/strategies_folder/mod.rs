//! Read-only surface over `$XVN_HOME/strategies/` — the per-user folder where
//! operators drop notes, reference docs, imported PDFs/CSVs, and a curated
//! library of strategy ideas. The wizard reads from this surface when
//! authoring strategies so it can quote the user's own material back to them.
//!
//! V2F foundation track — see
//! `docs/superpowers/plans/2026-05-21-v2f-strategies-folder-and-template-refactor.md`
//! and the contract at `team/contracts/strategies-folder-surface.md`.
//!
//! Public surface:
//! - [`folder_root`] — returns `<xvn_home>/strategies`.
//! - [`list`] — enumerate entries, optionally scoped to one allowlisted subfolder.
//! - [`read`] — read one file's content (truncated at 256 KB).
//!
//! Path safety: every `rel_path` is resolved via `folder_root.join(rel_path)`
//! then canonicalized; the canonical path is required to be under the
//! canonical `folder_root`. Symlink escapes and `..` traversals are rejected.
//!
//! Missing folder semantics: `list` returns `Ok(vec![])` when the folder
//! has never been initialized (no `xvn strategies init` has run). `read`
//! on a missing file returns `ApiError::NotFound`.
//!
//! v1 is read-only. Writing (`xvn strategies init`, `xvn strategies import`)
//! ships in wave-2 tracks.

pub mod import;
pub mod prepop;
pub mod reader;
pub mod summary;
pub mod types;

pub use import::{
    import_bytes, import_from_path, ImportFinding, ImportOptions, ImportOutcome, ACCEPTED_EXTENSIONS,
    MAX_IMPORT_BYTES,
};
pub use reader::{folder_root, list, read, MAX_FILE_BYTES, SUBFOLDER_ALLOWLIST};
pub use summary::{summarize_csv, summarize_pdf, SummaryOutcome};
pub use types::{FileContent, FileKind, FolderEntry};
