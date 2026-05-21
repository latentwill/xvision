//! Summary-sidecar extractors for imported files.
//!
//! Two extractors live here:
//! - PDF: shells out to `pdftotext <src> -` (writes text to stdout). If
//!   the binary is missing, returns `SummaryOutcome::ExtractorUnavailable`
//!   so the caller can emit a soft finding without failing the import.
//! - CSV: builds a markdown table from the header row + first 50 rows.
//!
//! `.md`, `.txt`, `.json` have no sidecar (the body is already
//! human-readable; the wizard will read the raw file via the track-1
//! `read` API).
//!
//! Spec: V2F wave-2 leaf `strategies-folder-import` (see
//! `team/contracts/strategies-folder-import.md`).

use std::path::Path;
use std::process::Command;

/// Maximum rows (excluding header) shown in a CSV summary.
pub const CSV_SUMMARY_MAX_ROWS: usize = 50;

/// Result of a summary attempt for one imported source file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SummaryOutcome {
    /// A `<basename>.summary.md` was written next to the source.
    Written {
        /// Relative-to-folder path of the sidecar (e.g. `docs/foo.summary.md`).
        rel_path: String,
        /// Bytes written to the sidecar.
        bytes: u64,
    },
    /// File kind does not need a sidecar (md/txt/json).
    NotApplicable,
    /// Tried to write a sidecar but the system extractor (`pdftotext`)
    /// is not on PATH. Caller surfaces this as a `summary_extractor_unavailable`
    /// finding — the original file still imports.
    ExtractorUnavailable,
    /// Extractor ran but failed (non-zero exit, garbled output, etc.).
    /// The reason is surfaced to the caller for logging; import still
    /// completes for the original file.
    ExtractorFailed(String),
}

/// Run pdftotext against `src` and capture stdout. Returns `Ok(None)` when
/// the binary is missing; `Ok(Some(text))` on success; `Err(reason)` on
/// non-zero exit so the caller can decide whether to log/skip.
fn run_pdftotext(src: &Path) -> Result<Option<String>, String> {
    let output = Command::new("pdftotext").arg(src).arg("-").output();
    match output {
        Ok(out) => {
            if out.status.success() {
                Ok(Some(String::from_utf8_lossy(&out.stdout).into_owned()))
            } else {
                Err(format!(
                    "pdftotext exited with status {}: {}",
                    out.status,
                    String::from_utf8_lossy(&out.stderr).trim()
                ))
            }
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(format!("spawn pdftotext: {e}")),
    }
}

/// Build a markdown summary for a PDF source file. Returns
/// [`SummaryOutcome`] reflecting whether the sidecar was written.
///
/// Sidecar path is `<src parent>/<src stem>.summary.md`. The caller
/// resolves that path relative to the strategies folder for the response.
pub async fn summarize_pdf(src: &Path) -> SummaryOutcome {
    let src_owned = src.to_path_buf();
    let extraction = tokio::task::spawn_blocking(move || run_pdftotext(&src_owned)).await;

    let text = match extraction {
        Ok(Ok(Some(t))) => t,
        Ok(Ok(None)) => return SummaryOutcome::ExtractorUnavailable,
        Ok(Err(reason)) => return SummaryOutcome::ExtractorFailed(reason),
        Err(join_err) => return SummaryOutcome::ExtractorFailed(format!("join error: {join_err}")),
    };

    let sidecar_path = match sidecar_path_for(src) {
        Some(p) => p,
        None => {
            return SummaryOutcome::ExtractorFailed(format!(
                "could not derive sidecar path from {}",
                src.display()
            ))
        }
    };

    let body = format!(
        "# Summary of {}\n\n_Extracted with `pdftotext`._\n\n```\n{}\n```\n",
        src.file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("<unknown>"),
        text.trim_end()
    );

    match tokio::fs::write(&sidecar_path, body.as_bytes()).await {
        Ok(()) => SummaryOutcome::Written {
            rel_path: sidecar_path
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or_default()
                .to_string(),
            bytes: body.len() as u64,
        },
        Err(e) => SummaryOutcome::ExtractorFailed(format!(
            "write sidecar {}: {e}",
            sidecar_path.display()
        )),
    }
}

/// Build a markdown summary for a CSV source file. Reads the header row
/// + first [`CSV_SUMMARY_MAX_ROWS`] data rows and emits a markdown table.
/// Empty / unreadable CSV returns [`SummaryOutcome::ExtractorFailed`].
pub async fn summarize_csv(src: &Path) -> SummaryOutcome {
    let text = match tokio::fs::read_to_string(src).await {
        Ok(t) => t,
        Err(e) => {
            return SummaryOutcome::ExtractorFailed(format!("read csv {}: {e}", src.display()))
        }
    };

    let mut lines = text.lines();
    let header = match lines.next() {
        Some(h) => h,
        None => {
            return SummaryOutcome::ExtractorFailed(format!(
                "csv {} is empty (no header row)",
                src.display()
            ))
        }
    };

    let header_cols: Vec<&str> = header.split(',').collect();
    let mut body = String::new();
    body.push_str(&format!(
        "# Summary of {}\n\n_First {} rows shown._\n\n",
        src.file_name().and_then(|s| s.to_str()).unwrap_or("<unknown>"),
        CSV_SUMMARY_MAX_ROWS,
    ));
    body.push_str("| ");
    body.push_str(&header_cols.join(" | "));
    body.push_str(" |\n| ");
    body.push_str(
        &(0..header_cols.len())
            .map(|_| "---")
            .collect::<Vec<_>>()
            .join(" | "),
    );
    body.push_str(" |\n");

    let mut count = 0usize;
    for line in lines {
        if count >= CSV_SUMMARY_MAX_ROWS {
            break;
        }
        let cells: Vec<&str> = line.split(',').collect();
        body.push_str("| ");
        body.push_str(&cells.join(" | "));
        body.push_str(" |\n");
        count += 1;
    }

    let sidecar_path = match sidecar_path_for(src) {
        Some(p) => p,
        None => {
            return SummaryOutcome::ExtractorFailed(format!(
                "could not derive sidecar path from {}",
                src.display()
            ))
        }
    };

    match tokio::fs::write(&sidecar_path, body.as_bytes()).await {
        Ok(()) => SummaryOutcome::Written {
            rel_path: sidecar_path
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or_default()
                .to_string(),
            bytes: body.len() as u64,
        },
        Err(e) => SummaryOutcome::ExtractorFailed(format!(
            "write sidecar {}: {e}",
            sidecar_path.display()
        )),
    }
}

/// `<src parent>/<stem>.summary.md`. Returns `None` when the source path
/// has no usable file stem.
pub fn sidecar_path_for(src: &Path) -> Option<std::path::PathBuf> {
    let stem = src.file_stem()?.to_str()?;
    let parent = src.parent()?;
    Some(parent.join(format!("{stem}.summary.md")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn csv_summary_writes_markdown_table() {
        let td = tempfile::tempdir().unwrap();
        let src = td.path().join("rows.csv");
        std::fs::write(&src, "a,b,c\n1,2,3\n4,5,6\n").unwrap();
        let outcome = summarize_csv(&src).await;
        match outcome {
            SummaryOutcome::Written { .. } => {}
            other => panic!("expected Written, got {other:?}"),
        }
        let sidecar = src.parent().unwrap().join("rows.summary.md");
        let body = std::fs::read_to_string(&sidecar).unwrap();
        assert!(body.contains("| a | b | c |"));
        assert!(body.contains("| 1 | 2 | 3 |"));
        assert!(body.contains("| 4 | 5 | 6 |"));
    }

    #[tokio::test]
    async fn csv_summary_caps_at_50_rows() {
        let td = tempfile::tempdir().unwrap();
        let src = td.path().join("big.csv");
        let mut body = String::from("col\n");
        for i in 0..120 {
            body.push_str(&format!("row-{i}\n"));
        }
        std::fs::write(&src, body).unwrap();
        let outcome = summarize_csv(&src).await;
        assert!(matches!(outcome, SummaryOutcome::Written { .. }));

        let sidecar_body =
            std::fs::read_to_string(src.parent().unwrap().join("big.summary.md")).unwrap();
        // 50 data rows + header table line + table-divider line should
        // be present; row 50 should appear, row 51+ should not.
        assert!(sidecar_body.contains("row-49"));
        assert!(!sidecar_body.contains("row-50\n"), "expected row-50 to be trimmed");
    }

    #[test]
    fn sidecar_path_replaces_extension() {
        let path = sidecar_path_for(Path::new("/tmp/foo/bar.pdf")).unwrap();
        assert_eq!(path, Path::new("/tmp/foo/bar.summary.md"));
    }
}
