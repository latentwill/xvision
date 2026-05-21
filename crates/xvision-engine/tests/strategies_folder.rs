//! Integration tests for the V2F strategies-folder read surface.
//!
//! Spec: `team/contracts/strategies-folder-surface.md`.
//!
//! Coverage:
//! - One per file kind (md / json / csv / pdf / txt) — list + read shapes.
//! - Missing folder returns empty list (not an error).
//! - Path-escape rejection (`../etc/passwd`).
//! - Symlink-escape rejection (cfg-gated to unix where symlinks are supported).
//! - Subfolder allowlist rejection (`secrets` → Validation error).

use std::path::PathBuf;

use sqlx::sqlite::SqliteConnectOptions;
use sqlx::SqlitePool;
use tempfile::TempDir;

use xvision_engine::api::{Actor, ApiContext, ApiError};
use xvision_engine::strategies_folder::{self, FileKind};

/// Build an in-memory-ish SqlitePool against a tempdir file. The
/// strategies-folder reader doesn't touch the DB but `ApiContext::new`
/// still requires a real pool, so we open one without running any
/// migrations.
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
async fn list_empty_when_folder_missing() {
    let (ctx, _td) = build_ctx().await;
    // `xvn strategies init` has never run — `$XVN_HOME/strategies/` does
    // not exist. `list` must return empty rather than error.
    let entries = strategies_folder::list(&ctx, None).await.unwrap();
    assert!(entries.is_empty(), "expected empty list, got {entries:?}");

    // Subfolder filter on a non-existent root also yields empty (the
    // subfolder is allowlisted, the folder just doesn't exist yet).
    let entries = strategies_folder::list(&ctx, Some("notes")).await.unwrap();
    assert!(entries.is_empty());
}

#[tokio::test]
async fn list_and_read_markdown_file() {
    let (ctx, td) = build_ctx().await;
    let root = strategies_folder::folder_root(&ctx.xvn_home);
    let file = root.join("notes").join("hello.md");
    touch(&file, b"# hello\n\nbody text\n");
    let _ = td;

    let entries = strategies_folder::list(&ctx, Some("notes")).await.unwrap();
    assert_eq!(entries.len(), 1);
    let entry = &entries[0];
    assert_eq!(entry.rel_path, "notes/hello.md");
    assert_eq!(entry.kind, FileKind::Markdown);
    assert!(entry.size_bytes > 0);

    let body = strategies_folder::read(&ctx, "notes/hello.md").await.unwrap();
    assert_eq!(body.rel_path, "notes/hello.md");
    assert_eq!(body.kind, FileKind::Markdown);
    assert!(body.content.contains("# hello"));
    assert!(!body.truncated);
}

#[tokio::test]
async fn list_and_read_json_file() {
    let (ctx, td) = build_ctx().await;
    let file = strategies_folder::folder_root(&ctx.xvn_home)
        .join("strategy-files")
        .join("recipe.json");
    touch(&file, br#"{"name":"recipe"}"#);
    let _ = td;

    let entries = strategies_folder::list(&ctx, Some("strategy-files"))
        .await
        .unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].kind, FileKind::Json);

    let body = strategies_folder::read(&ctx, "strategy-files/recipe.json")
        .await
        .unwrap();
    assert_eq!(body.kind, FileKind::Json);
    assert!(body.content.contains("recipe"));
}

#[tokio::test]
async fn list_and_read_csv_file() {
    let (ctx, td) = build_ctx().await;
    let file = strategies_folder::folder_root(&ctx.xvn_home)
        .join("docs")
        .join("rows.csv");
    touch(&file, b"a,b\n1,2\n");
    let _ = td;

    let entries = strategies_folder::list(&ctx, Some("docs")).await.unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].kind, FileKind::Csv);

    let body = strategies_folder::read(&ctx, "docs/rows.csv").await.unwrap();
    assert_eq!(body.kind, FileKind::Csv);
    assert!(body.content.starts_with("a,b"));
}

#[tokio::test]
async fn list_and_read_pdf_file() {
    // A "pdf" by extension only — kind detection is extension-based;
    // text-extraction lives in a wave-2 track.
    let (ctx, td) = build_ctx().await;
    let file = strategies_folder::folder_root(&ctx.xvn_home)
        .join("docs")
        .join("manual.pdf");
    touch(&file, b"%PDF-1.4\nnot really a pdf");
    let _ = td;

    let entries = strategies_folder::list(&ctx, Some("docs")).await.unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].kind, FileKind::Pdf);

    let body = strategies_folder::read(&ctx, "docs/manual.pdf").await.unwrap();
    assert_eq!(body.kind, FileKind::Pdf);
}

#[tokio::test]
async fn list_and_read_txt_file() {
    let (ctx, td) = build_ctx().await;
    let file = strategies_folder::folder_root(&ctx.xvn_home)
        .join("notes")
        .join("scratch.txt");
    touch(&file, b"some plain text\n");
    let _ = td;

    let entries = strategies_folder::list(&ctx, Some("notes")).await.unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].kind, FileKind::Text);

    let body = strategies_folder::read(&ctx, "notes/scratch.txt").await.unwrap();
    assert_eq!(body.kind, FileKind::Text);
    assert_eq!(body.content, "some plain text\n");
}

#[tokio::test]
async fn subfolder_allowlist_rejects_secrets() {
    let (ctx, _td) = build_ctx().await;
    // Ensure the root exists so we can be sure the rejection is hitting
    // the allowlist check, not the missing-folder short-circuit.
    std::fs::create_dir_all(strategies_folder::folder_root(&ctx.xvn_home)).unwrap();

    let err = strategies_folder::list(&ctx, Some("secrets")).await.unwrap_err();
    match err {
        ApiError::Validation(msg) => {
            assert!(
                msg.contains("subfolder_not_allowed"),
                "expected subfolder_not_allowed prefix, got: {msg}"
            );
        }
        other => panic!("expected Validation, got {other:?}"),
    }
}

#[tokio::test]
async fn path_escape_dotdot_rejected() {
    let (ctx, _td) = build_ctx().await;
    // Materialize the folder so canonicalize works against `root`.
    std::fs::create_dir_all(strategies_folder::folder_root(&ctx.xvn_home)).unwrap();

    let err = strategies_folder::read(&ctx, "../etc/passwd").await.unwrap_err();
    match err {
        ApiError::Validation(msg) => {
            assert!(msg.contains("path_escape"), "expected path_escape, got: {msg}");
        }
        other => panic!("expected Validation, got {other:?}"),
    }
}

#[tokio::test]
async fn absolute_path_rejected() {
    let (ctx, _td) = build_ctx().await;
    std::fs::create_dir_all(strategies_folder::folder_root(&ctx.xvn_home)).unwrap();

    let err = strategies_folder::read(&ctx, "/etc/passwd").await.unwrap_err();
    match err {
        ApiError::Validation(msg) => {
            assert!(msg.contains("path_escape"), "expected path_escape, got: {msg}");
        }
        other => panic!("expected Validation, got {other:?}"),
    }
}

#[cfg(unix)]
#[tokio::test]
async fn symlink_escape_rejected() {
    use std::os::unix::fs::symlink;

    let (ctx, td) = build_ctx().await;
    let root = strategies_folder::folder_root(&ctx.xvn_home);
    std::fs::create_dir_all(&root).unwrap();

    // Drop a target file *outside* the strategies folder.
    let outside = td.path().join("secret.md");
    std::fs::write(&outside, b"top secret\n").unwrap();

    // Symlink under `notes/` pointing at the outside file.
    std::fs::create_dir_all(root.join("notes")).unwrap();
    let link = root.join("notes").join("link.md");
    symlink(&outside, &link).unwrap();

    // list() must not enumerate the symlinked file (canonicalize escapes
    // the root) — the result should be empty.
    let entries = strategies_folder::list(&ctx, Some("notes")).await.unwrap();
    assert!(
        entries.is_empty(),
        "symlink escape must not be enumerated, got {entries:?}"
    );

    // read() must reject the same symlink target.
    let err = strategies_folder::read(&ctx, "notes/link.md").await.unwrap_err();
    match err {
        ApiError::Validation(msg) => {
            assert!(msg.contains("path_escape"), "expected path_escape, got: {msg}");
        }
        other => panic!("expected Validation, got {other:?}"),
    }
}

#[tokio::test]
async fn read_missing_file_returns_not_found() {
    let (ctx, _td) = build_ctx().await;
    std::fs::create_dir_all(strategies_folder::folder_root(&ctx.xvn_home)).unwrap();

    let err = strategies_folder::read(&ctx, "notes/missing.md")
        .await
        .unwrap_err();
    assert!(matches!(err, ApiError::NotFound(_)), "got {err:?}");
}

#[tokio::test]
async fn list_recurses_into_nested_directories() {
    let (ctx, _td) = build_ctx().await;
    let root = strategies_folder::folder_root(&ctx.xvn_home);
    touch(&root.join("library").join("ema").join("a.json"), b"{}");
    touch(&root.join("library").join("bollinger").join("b.json"), b"{}");

    let entries = strategies_folder::list(&ctx, Some("library")).await.unwrap();
    assert_eq!(entries.len(), 2);
    // Stable sort: alphabetical.
    assert_eq!(entries[0].rel_path, "library/bollinger/b.json");
    assert_eq!(entries[1].rel_path, "library/ema/a.json");
}

#[tokio::test]
async fn read_truncates_oversize_files() {
    let (ctx, _td) = build_ctx().await;
    let file = strategies_folder::folder_root(&ctx.xvn_home)
        .join("notes")
        .join("big.md");
    let body = vec![b'x'; strategies_folder::MAX_FILE_BYTES + 4096];
    touch(&file, &body);

    let content = strategies_folder::read(&ctx, "notes/big.md").await.unwrap();
    assert!(content.truncated, "expected truncated flag");
    assert_eq!(content.content.len(), strategies_folder::MAX_FILE_BYTES);
}
