use std::path::Path;
use std::process::{Command, Output};

use sqlx::sqlite::SqlitePoolOptions;
use sqlx::SqlitePool;
use tempfile::tempdir;

fn xvn(args: &[&str], home: &Path) -> Output {
    Command::new(env!("CARGO_BIN_EXE_xvn"))
        .args(args)
        .env("XVN_HOME", home)
        .output()
        .expect("xvn invocation")
}

fn assert_ok(out: &Output) {
    assert!(
        out.status.success(),
        "xvn failed (exit {:?}): stdout={} stderr={}",
        out.status.code(),
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr),
    );
}

async fn setup_lineage_db(path: &Path) -> SqlitePool {
    let url = format!("sqlite://{}?mode=rwc", path.display());
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect(&url)
        .await
        .expect("create lineage db");
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS lineage_nodes (\
            bundle_hash TEXT PRIMARY KEY, \
            parent_hash TEXT, \
            gate_verdict TEXT NOT NULL, \
            status TEXT NOT NULL, \
            cycle_id TEXT, \
            created_at TEXT NOT NULL, \
            diversity_score REAL\
        )",
    )
    .execute(&pool)
    .await
    .expect("create lineage_nodes");
    pool
}

async fn seed_lineage_node(
    pool: &SqlitePool,
    bundle_hash: &str,
    parent_hash: Option<&str>,
    status: &str,
    gate_verdict: &str,
    cycle_id: Option<&str>,
) {
    sqlx::query(
        "INSERT INTO lineage_nodes \
         (bundle_hash, parent_hash, gate_verdict, status, cycle_id, created_at) \
         VALUES (?, ?, ?, ?, ?, '2026-01-01T00:00:00Z')",
    )
    .bind(bundle_hash)
    .bind(parent_hash)
    .bind(gate_verdict)
    .bind(status)
    .bind(cycle_id)
    .execute(pool)
    .await
    .expect("seed lineage_node");
}

#[tokio::test]
async fn lineage_ls_empty_store() {
    let dir = tempdir().expect("tempdir");
    let db_path = dir.path().join("lineage.db");
    setup_lineage_db(&db_path).await;

    let out = xvn(
        &["optimize", "lineage", "ls", "--db", &db_path.to_string_lossy()],
        dir.path(),
    );
    assert_ok(&out);
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("(no experiments)"),
        "expected empty message, got: {stdout}"
    );
}

#[tokio::test]
async fn lineage_ls_status_filter() {
    let dir = tempdir().expect("tempdir");
    let db_path = dir.path().join("lineage.db");
    let pool = setup_lineage_db(&db_path).await;

    let hash_active = "aa".repeat(32);
    let hash_rejected = "bb".repeat(32);
    seed_lineage_node(&pool, &hash_active, None, "active", "passed", None).await;
    seed_lineage_node(&pool, &hash_rejected, None, "rejected", "rejected", None).await;

    let out = xvn(
        &[
            "optimize",
            "lineage",
            "ls",
            "--db",
            &db_path.to_string_lossy(),
            "--status",
            "rejected",
        ],
        dir.path(),
    );
    assert_ok(&out);
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("bbbbbbbb"),
        "expected rejected hash prefix in output, got: {stdout}"
    );
    assert!(
        !stdout.contains("aaaaaaaa"),
        "expected active hash NOT in filtered output, got: {stdout}"
    );
}

#[tokio::test]
async fn lineage_show_known_hash() {
    let dir = tempdir().expect("tempdir");
    let db_path = dir.path().join("lineage.db");
    let pool = setup_lineage_db(&db_path).await;

    let hash = "cc".repeat(32);
    seed_lineage_node(&pool, &hash, None, "active", "passed", Some("cycle-1")).await;

    let out = xvn(
        &[
            "optimize",
            "lineage",
            "show",
            &hash,
            "--db",
            &db_path.to_string_lossy(),
        ],
        dir.path(),
    );
    assert_ok(&out);
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains(&hash), "expected bundle_hash in output");
    assert!(stdout.contains("active"), "expected status in output");
    assert!(stdout.contains("passed"), "expected gate_verdict in output");

    let bad_hash = "dd".repeat(32);
    let out = xvn(
        &[
            "optimize",
            "lineage",
            "show",
            &bad_hash,
            "--db",
            &db_path.to_string_lossy(),
        ],
        dir.path(),
    );
    assert!(!out.status.success(), "nonexistent hash should exit non-zero");
}
