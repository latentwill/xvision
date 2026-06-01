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
            diff_hash TEXT, \
            metrics_day_hash TEXT, \
            metrics_untouched_hash TEXT, \
            gate_verdict TEXT NOT NULL, \
            status TEXT NOT NULL, \
            cycle_id TEXT, \
            created_at TEXT NOT NULL\
        )",
    )
    .execute(&pool)
    .await
    .expect("create lineage_nodes");
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS cycle_seals (\
            seal_id TEXT PRIMARY KEY, \
            cycle_id TEXT NOT NULL, \
            merkle_root TEXT NOT NULL, \
            operator_signature TEXT NOT NULL, \
            sealed_at TEXT NOT NULL\
        )",
    )
    .execute(&pool)
    .await
    .expect("create cycle_seals");
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

async fn seed_cycle_seal(
    pool: &SqlitePool,
    seal_id: &str,
    cycle_id: &str,
    merkle_root: &str,
    operator_signature: &str,
) {
    sqlx::query(
        "INSERT INTO cycle_seals \
         (seal_id, cycle_id, merkle_root, operator_signature, sealed_at) \
         VALUES (?, ?, ?, ?, '2026-01-01T00:00:00Z')",
    )
    .bind(seal_id)
    .bind(cycle_id)
    .bind(merkle_root)
    .bind(operator_signature)
    .execute(pool)
    .await
    .expect("seed cycle_seal");
}

#[tokio::test]
async fn lineage_ls_empty_store() {
    let dir = tempdir().expect("tempdir");
    let db_path = dir.path().join("lineage.db");
    setup_lineage_db(&db_path).await;

    let out = xvn(
        &["autooptimizer", "lineage", "ls", "--db", &db_path.to_string_lossy()],
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
            "autooptimizer", "lineage", "ls",
            "--db", &db_path.to_string_lossy(),
            "--status", "rejected",
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
        &["autooptimizer", "lineage", "show", &hash, "--db", &db_path.to_string_lossy()],
        dir.path(),
    );
    assert_ok(&out);
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains(&hash), "expected bundle_hash in output");
    assert!(stdout.contains("active"), "expected status in output");
    assert!(stdout.contains("passed"), "expected gate_verdict in output");

    let bad_hash = "dd".repeat(32);
    let out = xvn(
        &["autooptimizer", "lineage", "show", &bad_hash, "--db", &db_path.to_string_lossy()],
        dir.path(),
    );
    assert!(
        !out.status.success(),
        "nonexistent hash should exit non-zero"
    );
}

#[tokio::test]
async fn seal_show_known_seal_id() {
    let dir = tempdir().expect("tempdir");
    let db_path = dir.path().join("lineage.db");
    let pool = setup_lineage_db(&db_path).await;

    let bundle_hash = "ee".repeat(32);
    seed_lineage_node(&pool, &bundle_hash, None, "active", "passed", Some("cycle-test")).await;

    let merkle = "ff".repeat(32);
    let sig = "0123456789abcdef".repeat(4);
    seed_cycle_seal(&pool, "seal-01", "cycle-test", &merkle, &sig).await;

    let out = xvn(
        &["autooptimizer", "seal", "show", "seal-01", "--db", &db_path.to_string_lossy()],
        dir.path(),
    );
    assert_ok(&out);
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("Evening summary"), "expected header in output");
    assert!(stdout.contains("cycle-test"), "expected cycle_id in output");
    assert!(stdout.contains('1'), "expected node_count=1 in output");
}
