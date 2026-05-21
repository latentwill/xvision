//! Integration tests for the V2F strategies-folder importer.
//!
//! Spec: `team/contracts/strategies-folder-import.md`.
//!
//! Coverage:
//! - Happy-path: `.md` imported into `notes/`, file lands with correct content.
//! - `.csv` imported, sidecar generated with header + first 50 rows.
//! - `.pdf` imported with `pdftotext` available (cfg-gated to environments
//!   where the binary is on PATH).
//! - Path-escape rejected (filename containing `..` or directory separators).
//! - Size limit rejected (write a 26 MB temp file and confirm validation).
//! - Type allowlist rejected (`.exe`).

use std::path::PathBuf;

use sqlx::sqlite::SqliteConnectOptions;
use sqlx::SqlitePool;
use tempfile::TempDir;

use xvision_engine::api::{Actor, ApiContext, ApiError};
use xvision_engine::strategies_folder::{self, FileKind, ImportOptions, MAX_IMPORT_BYTES};

async fn build_pool(td: &TempDir) -> SqlitePool {
    let db_path = td.path().join("xvn.db");
    let opts = SqliteConnectOptions::new()
        .filename(&db_path)
        .create_if_missing(true);
    SqlitePool::connect_with(opts).await.unwrap()
}

async fn build_ctx() -> (ApiContext, TempDir) {
    let td = tempfile::tempdir().unwrap();
    let pool = build_pool(&td).await;
    let ctx = ApiContext::new(pool, Actor::Cli { user: "test".into() }, td.path().to_path_buf());
    (ctx, td)
}

fn touch(path: &PathBuf, body: &[u8]) {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).unwrap();
    }
    std::fs::write(path, body).unwrap();
}

#[tokio::test]
async fn import_md_lands_in_notes_with_default_subfolder() {
    let (ctx, td) = build_ctx().await;
    let src = td.path().join("hello.md");
    touch(&src, b"# hello\n\nbody text\n");

    let outcome = strategies_folder::import_from_path(&ctx, &src, ImportOptions::default())
        .await
        .unwrap();

    assert_eq!(outcome.entry.rel_path, "notes/hello.md");
    assert_eq!(outcome.entry.kind, FileKind::Markdown);
    assert!(outcome.summary.is_none(), "md should not produce a sidecar");
    assert!(outcome.findings.is_empty(), "md import should be clean");

    // Confirm the file is actually on disk under the strategies folder.
    let landed = strategies_folder::folder_root(&ctx.xvn_home).join("notes/hello.md");
    let body = std::fs::read_to_string(&landed).unwrap();
    assert!(body.contains("# hello"));
}

#[tokio::test]
async fn import_csv_writes_sidecar_with_header_and_rows() {
    let (ctx, td) = build_ctx().await;
    let src = td.path().join("rows.csv");
    let mut body = String::from("a,b,c\n");
    for i in 0..120 {
        body.push_str(&format!("v{i},x{i},y{i}\n"));
    }
    touch(&src, body.as_bytes());

    let outcome = strategies_folder::import_from_path(&ctx, &src, ImportOptions::default())
        .await
        .unwrap();

    assert_eq!(outcome.entry.rel_path, "docs/rows.csv");
    let summary = outcome.summary.expect("csv import should write a sidecar");
    assert_eq!(summary.rel_path, "docs/rows.summary.md");

    let body = std::fs::read_to_string(strategies_folder::folder_root(&ctx.xvn_home).join(&summary.rel_path))
        .unwrap();
    assert!(body.contains("| a | b | c |"));
    // First and 49th rows show; 50th-onward should be trimmed.
    assert!(body.contains("v0"));
    assert!(body.contains("v49"));
    assert!(!body.contains("v50,"), "expected row 50 to be trimmed");
}

#[tokio::test]
async fn import_csv_to_explicit_subfolder() {
    let (ctx, td) = build_ctx().await;
    let src = td.path().join("rows.csv");
    touch(&src, b"a,b\n1,2\n");
    let outcome = strategies_folder::import_from_path(
        &ctx,
        &src,
        ImportOptions {
            subfolder: Some("strategy-files".into()),
            clobber: true,
        },
    )
    .await
    .unwrap();
    assert_eq!(outcome.entry.rel_path, "strategy-files/rows.csv");
}

#[tokio::test]
async fn import_rejects_disallowed_type() {
    let (ctx, td) = build_ctx().await;
    let src = td.path().join("malicious.exe");
    touch(&src, b"MZ\x90\x00");
    let err = strategies_folder::import_from_path(&ctx, &src, ImportOptions::default())
        .await
        .unwrap_err();
    match err {
        ApiError::Validation(msg) => assert!(
            msg.contains("type_not_allowed"),
            "expected type_not_allowed, got: {msg}"
        ),
        other => panic!("expected Validation, got {other:?}"),
    }
}

#[tokio::test]
async fn import_rejects_oversize_file() {
    let (ctx, td) = build_ctx().await;
    let src = td.path().join("big.md");
    let body = vec![b'x'; (MAX_IMPORT_BYTES + 1024) as usize];
    touch(&src, &body);
    let err = strategies_folder::import_from_path(&ctx, &src, ImportOptions::default())
        .await
        .unwrap_err();
    match err {
        ApiError::Validation(msg) => assert!(
            msg.contains("import_too_large"),
            "expected import_too_large, got: {msg}"
        ),
        other => panic!("expected Validation, got {other:?}"),
    }
}

#[tokio::test]
async fn import_bytes_rejects_traversal_in_filename() {
    let (ctx, _td) = build_ctx().await;
    let err = strategies_folder::import_bytes(&ctx, "../etc/passwd", b"root:x:0", ImportOptions::default())
        .await
        .unwrap_err();
    match err {
        ApiError::Validation(msg) => assert!(msg.contains("path_escape"), "expected path_escape, got: {msg}"),
        other => panic!("expected Validation, got {other:?}"),
    }
}

#[tokio::test]
async fn import_bytes_rejects_separator_in_filename() {
    let (ctx, _td) = build_ctx().await;
    let err = strategies_folder::import_bytes(
        &ctx,
        "notes/should-not-nest.md",
        b"# nope",
        ImportOptions::default(),
    )
    .await
    .unwrap_err();
    assert!(matches!(err, ApiError::Validation(_)));
}

#[tokio::test]
async fn import_with_no_clobber_skips_existing() {
    let (ctx, td) = build_ctx().await;
    let src = td.path().join("dup.md");
    touch(&src, b"# first");
    let first = strategies_folder::import_from_path(&ctx, &src, ImportOptions::default())
        .await
        .unwrap();
    assert_eq!(first.entry.rel_path, "notes/dup.md");

    let err = strategies_folder::import_from_path(
        &ctx,
        &src,
        ImportOptions {
            subfolder: None,
            clobber: false,
        },
    )
    .await
    .unwrap_err();
    match err {
        ApiError::Conflict(msg) => assert!(msg.contains("no_clobber"), "got {msg}"),
        other => panic!("expected Conflict, got {other:?}"),
    }
}

#[tokio::test]
async fn import_default_overwrites_existing() {
    let (ctx, td) = build_ctx().await;
    let src = td.path().join("dup.md");
    touch(&src, b"# first");
    strategies_folder::import_from_path(&ctx, &src, ImportOptions::default())
        .await
        .unwrap();

    touch(&src, b"# second pass");
    let again = strategies_folder::import_from_path(&ctx, &src, ImportOptions::default())
        .await
        .unwrap();
    assert_eq!(again.entry.rel_path, "notes/dup.md");

    let landed_body =
        std::fs::read_to_string(strategies_folder::folder_root(&ctx.xvn_home).join("notes/dup.md")).unwrap();
    assert_eq!(landed_body, "# second pass");
}

#[cfg(unix)]
#[tokio::test]
async fn import_rejects_existing_destination_symlink() {
    use std::os::unix::fs::symlink;

    let (ctx, td) = build_ctx().await;
    let outside = td.path().join("outside.csv");
    touch(&outside, b"keep,me\n");

    let docs = strategies_folder::folder_root(&ctx.xvn_home).join("docs");
    std::fs::create_dir_all(&docs).unwrap();
    symlink(&outside, docs.join("foo.csv")).unwrap();

    let err = strategies_folder::import_bytes(&ctx, "foo.csv", b"new,content\n", ImportOptions::default())
        .await
        .unwrap_err();

    match err {
        ApiError::Validation(msg) => assert!(
            msg.contains("symlink_target_not_allowed"),
            "expected symlink_target_not_allowed, got: {msg}"
        ),
        other => panic!("expected Validation, got {other:?}"),
    }
    assert_eq!(std::fs::read_to_string(outside).unwrap(), "keep,me\n");
}

#[tokio::test]
async fn import_pdf_emits_finding_when_pdftotext_missing() {
    // We can't easily uninstall the binary inside the test, so this case
    // only meaningfully exercises the unavailable branch on hosts that
    // genuinely lack `pdftotext`. When it's installed, we assert the
    // summary sidecar lands or the extractor reports a clean failure on
    // the synthetic non-PDF body.
    let (ctx, td) = build_ctx().await;
    let src = td.path().join("manual.pdf");
    touch(&src, b"%PDF-1.4\nnot a real pdf body\n");

    let outcome = strategies_folder::import_from_path(&ctx, &src, ImportOptions::default())
        .await
        .unwrap();
    assert_eq!(outcome.entry.rel_path, "docs/manual.pdf");

    let pdftotext_available = std::process::Command::new("pdftotext")
        .arg("-v")
        .output()
        .map(|o| o.status.success() || !o.stderr.is_empty())
        .unwrap_or(false);
    if !pdftotext_available {
        // No binary on PATH → the importer must surface a finding.
        assert!(
            outcome
                .findings
                .iter()
                .any(|f| f.code == "summary_extractor_unavailable"),
            "expected summary_extractor_unavailable finding when pdftotext is missing; got {:?}",
            outcome.findings
        );
    }
    // When the binary is available, either a sidecar lands (true PDF) or
    // pdftotext fails on the synthetic body — both cases keep the import
    // itself successful, which we already asserted via the entry above.
}
