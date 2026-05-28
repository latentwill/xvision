use std::path::Path;
use std::process::{Command, Output};

use tempfile::tempdir;
use xvision_engine::autoresearch::content_hash::ContentHash;

fn xvn(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_xvn"))
        .args(args)
        .output()
        .expect("xvn invocation failed")
}

fn exit_code(out: &Output) -> i32 {
    out.status.code().expect("child terminated by signal")
}

fn parent_strategy_json() -> serde_json::Value {
    serde_json::json!({
        "manifest": {
            "id": "01HTEST00AAAAAAAAAAAAAAAA",
            "display_name": "AR1 Test Strategy",
            "plain_summary": "Autoresearcher AR-1 smoke test",
            "creator": "@test",
            "template": "custom",
            "regime_fit": [],
            "asset_universe": ["BTC/USD"],
            "decision_cadence_minutes": 60,
            "attested_with": [],
            "required_tools": [],
            "risk_preset_or_config": "balanced",
            "published_at": null
        },
        "risk": {
            "risk_pct_per_trade": 0.015,
            "max_concurrent_positions": 2,
            "max_leverage": 3.0,
            "stop_loss_atr_multiple": 2.0,
            "daily_loss_kill_pct": 0.05
        },
        "mechanical_params": {"rsi_period": 14}
    })
}

fn write_parent_blob(blob_dir: &Path) -> String {
    std::fs::create_dir_all(blob_dir).unwrap();
    let strategy = parent_strategy_json();
    let hash = ContentHash::of_json(&strategy);
    let hash_hex = hash.to_hex();
    let blob_path = blob_dir.join(format!("{hash_hex}.json"));
    std::fs::write(&blob_path, serde_json::to_vec_pretty(&strategy).unwrap()).unwrap();
    hash_hex
}

fn write_config(dir: &Path, min_improvement: f64) -> std::path::PathBuf {
    let config_path = dir.join("autoresearch.toml");
    std::fs::write(
        &config_path,
        format!("[gate]\nmin_improvement = {min_improvement}\n"),
    )
    .unwrap();
    config_path
}

fn write_session(dir: &Path, config_path: &Path, key_path: &Path) -> std::path::PathBuf {
    let session_path = dir.join("session.json");
    let out = xvn(&[
        "autoresearch",
        "session-init",
        "--config",
        config_path.to_str().unwrap(),
        "--out",
        session_path.to_str().unwrap(),
        "--key-path",
        key_path.to_str().unwrap(),
    ]);
    assert_eq!(
        exit_code(&out),
        0,
        "session-init failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    session_path
}

fn mutate_once_cmd<'a>(
    hash: &'a str,
    config: &'a Path,
    session: &'a Path,
    db: &'a Path,
    blob_dir: &'a Path,
    key: &'a Path,
    cycle_id: &'a str,
    extra: &[&'a str],
) -> Vec<&'a str> {
    let mut args = vec![
        "autoresearch",
        "mutate-once",
        hash,
        "--config",
        config.to_str().unwrap(),
        "--session",
        session.to_str().unwrap(),
        "--db",
        db.to_str().unwrap(),
        "--blob-dir",
        blob_dir.to_str().unwrap(),
        "--key-path",
        key.to_str().unwrap(),
        "--cycle-id",
        cycle_id,
        "--mock",
    ];
    args.extend_from_slice(extra);
    args
}

#[test]
fn mutate_once_gate_pass_creates_active_node_and_seal() {
    let dir = tempdir().unwrap();
    let blob_dir = dir.path().join("blobs");
    let hash = write_parent_blob(&blob_dir);
    let config = write_config(dir.path(), 0.1); // mock delta=0.2 > 0.1 → PASS
    let key_path = dir.path().join("op.ed25519");
    let session = write_session(dir.path(), &config, &key_path);
    let db = dir.path().join("lineage.db");

    let out = xvn(&mutate_once_cmd(
        &hash, &config, &session, &db, &blob_dir, &key_path, "cycle-pass-01", &[],
    ));
    assert_eq!(
        exit_code(&out),
        0,
        "stdout={}\nstderr={}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr),
    );

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    rt.block_on(async {
        let pool = sqlx::SqlitePool::connect(&format!("sqlite://{}", db.display()))
            .await
            .unwrap();
        let status: String =
            sqlx::query_scalar("SELECT status FROM lineage_nodes WHERE cycle_id = ?")
                .bind("cycle-pass-01")
                .fetch_one(&pool)
                .await
                .expect("lineage node must exist");
        assert_eq!(status, "active", "gate-pass must produce active node");

        let seal_count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM cycle_seals WHERE cycle_id = ?")
                .bind("cycle-pass-01")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(seal_count, 1, "gate-pass must produce a cycle seal (evening summary)");
    });
}

#[test]
fn mutate_once_gate_fail_creates_rejected_node_no_seal() {
    let dir = tempdir().unwrap();
    let blob_dir = dir.path().join("blobs");
    let hash = write_parent_blob(&blob_dir);
    let config = write_config(dir.path(), 0.5); // mock delta=0.2 < 0.5 → FAIL
    let key_path = dir.path().join("op.ed25519");
    let session = write_session(dir.path(), &config, &key_path);
    let db = dir.path().join("lineage.db");

    let out = xvn(&mutate_once_cmd(
        &hash, &config, &session, &db, &blob_dir, &key_path, "cycle-fail-01", &[],
    ));
    assert_eq!(
        exit_code(&out),
        0,
        "stdout={}\nstderr={}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr),
    );

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    rt.block_on(async {
        let pool = sqlx::SqlitePool::connect(&format!("sqlite://{}", db.display()))
            .await
            .unwrap();
        let status: String =
            sqlx::query_scalar("SELECT status FROM lineage_nodes WHERE cycle_id = ?")
                .bind("cycle-fail-01")
                .fetch_one(&pool)
                .await
                .expect("lineage node must exist on gate fail");
        assert_eq!(status, "rejected", "gate-fail must produce rejected node");

        let seal_count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM cycle_seals WHERE cycle_id = ?")
                .bind("cycle-fail-01")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(seal_count, 0, "gate-fail must not produce a cycle seal");
    });
}

#[test]
fn mutate_once_dry_run_no_db_writes() {
    let dir = tempdir().unwrap();
    let blob_dir = dir.path().join("blobs");
    let hash = write_parent_blob(&blob_dir);
    let config = write_config(dir.path(), 0.1);
    let key_path = dir.path().join("op.ed25519");
    let session = write_session(dir.path(), &config, &key_path);
    let db = dir.path().join("lineage.db");

    let out = xvn(&mutate_once_cmd(
        &hash,
        &config,
        &session,
        &db,
        &blob_dir,
        &key_path,
        "cycle-dry-01",
        &["--dry-run"],
    ));
    assert_eq!(
        exit_code(&out),
        0,
        "stdout={}\nstderr={}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr),
    );

    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("verdict:"),
        "dry-run must print verdict, got: {stdout}"
    );
    assert!(!db.exists(), "dry-run must not create the lineage database");
}

#[test]
fn mutate_once_unknown_hash_returns_not_found() {
    let dir = tempdir().unwrap();
    let blob_dir = dir.path().join("blobs");
    std::fs::create_dir_all(&blob_dir).unwrap();
    let config = write_config(dir.path(), 0.1);
    let key_path = dir.path().join("op.ed25519");
    let session = write_session(dir.path(), &config, &key_path);
    let db = dir.path().join("lineage.db");
    let unknown = "a".repeat(64);

    let out = xvn(&mutate_once_cmd(
        &unknown, &config, &session, &db, &blob_dir, &key_path, "cycle-nf-01", &[],
    ));
    assert_eq!(exit_code(&out), 4, "unknown hash must exit NotFound(4)");
}

#[test]
fn mutate_once_child_hash_is_deterministic() {
    let dir = tempdir().unwrap();
    let blob_dir = dir.path().join("blobs");
    let hash = write_parent_blob(&blob_dir);
    let config = write_config(dir.path(), 0.1);
    let key_path = dir.path().join("op.ed25519");
    let session = write_session(dir.path(), &config, &key_path);
    let db = dir.path().join("lineage.db");

    let run1 = xvn(&mutate_once_cmd(
        &hash, &config, &session, &db, &blob_dir, &key_path, "cycle-det-01", &[],
    ));
    assert_eq!(
        exit_code(&run1),
        0,
        "run1 failed: {}",
        String::from_utf8_lossy(&run1.stderr)
    );

    let run2 = xvn(&mutate_once_cmd(
        &hash, &config, &session, &db, &blob_dir, &key_path, "cycle-det-01", &[],
    ));
    assert_eq!(
        exit_code(&run2),
        0,
        "run2 failed: {}",
        String::from_utf8_lossy(&run2.stderr)
    );

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    rt.block_on(async {
        let pool = sqlx::SqlitePool::connect(&format!("sqlite://{}", db.display()))
            .await
            .unwrap();
        let rows: Vec<String> =
            sqlx::query_scalar("SELECT bundle_hash FROM lineage_nodes WHERE cycle_id = ?")
                .bind("cycle-det-01")
                .fetch_all(&pool)
                .await
                .unwrap();
        assert_eq!(
            rows.len(),
            1,
            "INSERT OR REPLACE must yield exactly one row for the same cycle and same parent"
        );
    });
}
