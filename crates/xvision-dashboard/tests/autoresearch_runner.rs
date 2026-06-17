//! Integration tests for `autoresearch_runner::execute_training_run`.
//!
//! TDD RED phase: these tests reference `xvision_dashboard::autoresearch_runner`
//! which does not yet exist. Run first to confirm they compile-fail / fail.
//!
//! Test environment: TempDir with stub Python scripts that `uv run` hands off
//! to the system `python3` interpreter (no `uv` environment needed for stubs
//! that use only stdlib). Each test sets up:
//!   - nanochat/ subdir inside TempDir
//!   - stub xvision_prepare.py
//!   - stub xvision_train.py
//!   - In-memory SQLite pool with autoresearch_runs + autoresearch_experiments
//!   - One 'running' run row
//!   - A broadcast::Sender for AutoresearchStdoutLine

use std::path::PathBuf;
use std::time::Duration;

use sqlx::SqlitePool;
use tempfile::TempDir;
use tokio::sync::broadcast;

use xvision_dashboard::autoresearch_runner::execute_training_run;
use xvision_dashboard::sse::autoresearch_sse::AutoresearchStdoutLine;

// ─── DDL helpers ──────────────────────────────────────────────────────────────

async fn make_pool() -> SqlitePool {
    let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();

    sqlx::query(
        "CREATE TABLE autoresearch_runs (
            run_id             TEXT PRIMARY KEY,
            run_tag            TEXT NOT NULL,
            source_strategy_id TEXT,
            label_strategy     TEXT NOT NULL,
            label_config       TEXT NOT NULL,
            git_branch         TEXT NOT NULL,
            worktree_path      TEXT NOT NULL,
            status             TEXT NOT NULL,
            started_at         TEXT NOT NULL,
            stopped_at         TEXT,
            experiments        INTEGER NOT NULL DEFAULT 0,
            best_acc           REAL,
            best_model_id      TEXT
        )",
    )
    .execute(&pool)
    .await
    .unwrap();

    sqlx::query(
        "CREATE UNIQUE INDEX idx_autoresearch_single_running
         ON autoresearch_runs (status) WHERE status = 'running'",
    )
    .execute(&pool)
    .await
    .unwrap();

    sqlx::query(
        "CREATE TABLE autoresearch_experiments (
            experiment_id    TEXT PRIMARY KEY,
            run_id           TEXT NOT NULL,
            git_commit       TEXT NOT NULL,
            val_acc          REAL,
            val_loss         REAL,
            peak_vram_mb     REAL,
            training_seconds REAL,
            status           TEXT NOT NULL,
            description      TEXT NOT NULL,
            created_at       TEXT NOT NULL
        )",
    )
    .execute(&pool)
    .await
    .unwrap();

    pool
}

async fn insert_running_run(pool: &SqlitePool, run_id: &str) {
    sqlx::query(
        "INSERT INTO autoresearch_runs
         (run_id, run_tag, label_strategy, label_config, git_branch,
          worktree_path, status, started_at)
         VALUES (?, 'testrun', 'price_forward', '{}',
                 'autoresearch/testrun', '.worktrees/testrun',
                 'running', '2026-06-14T00:00:00Z')",
    )
    .bind(run_id)
    .execute(pool)
    .await
    .unwrap();
}

/// Build the worktree_root (TempDir) with nanochat/ subdir and write stub
/// Python scripts. Returns (tmp, nanochat_dir, config_path).
fn setup_worktree(tmp: &TempDir, prepare_exit: i32, train_body: &str) -> PathBuf {
    let nanochat = tmp.path().join("nanochat");
    std::fs::create_dir_all(&nanochat).unwrap();

    // stub xvision_prepare.py
    let prepare_script = format!(
        "#!/usr/bin/env python3\nimport sys\nprint('[prepare] done')\nsys.exit({})\n",
        prepare_exit
    );
    std::fs::write(nanochat.join("xvision_prepare.py"), prepare_script).unwrap();

    // stub xvision_train.py — caller supplies the body
    std::fs::write(nanochat.join("xvision_train.py"), train_body).unwrap();

    // Write a dummy run_config.json at nanochat/ (path passed to scripts)
    let config_path = tmp.path().join("run_config.json");
    std::fs::write(&config_path, r#"{"dummy": true}"#).unwrap();

    config_path
}

// ─── Happy path ───────────────────────────────────────────────────────────────

#[tokio::test]
async fn happy_path_run_completes_with_experiment_row() {
    let tmp = TempDir::new().unwrap();
    let pool = make_pool().await;
    let run_id = "run-happy-01";
    insert_running_run(&pool, run_id).await;

    let train_body = "#!/usr/bin/env python3\nimport sys\n\
        print('epoch 1/1 loss=0.4')\n\
        print('XVN_RESULT {\"val_acc\": 0.6, \"val_loss\": 0.3}')\n\
        sys.exit(0)\n";
    let config_path = setup_worktree(&tmp, 0, train_body);

    let (tx, mut rx) = broadcast::channel::<AutoresearchStdoutLine>(64);

    execute_training_run(
        pool.clone(),
        tx,
        run_id.to_string(),
        tmp.path().to_path_buf(),
        config_path,
        Duration::from_secs(30),
    )
    .await;

    // Run status must be 'completed'
    let status: String = sqlx::query_scalar(
        "SELECT status FROM autoresearch_runs WHERE run_id = ?",
    )
    .bind(run_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(status, "completed", "run must be marked completed");

    // best_acc must be stored
    let best_acc: Option<f64> = sqlx::query_scalar(
        "SELECT best_acc FROM autoresearch_runs WHERE run_id = ?",
    )
    .bind(run_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(
        best_acc.is_some(),
        "best_acc must be set after a successful run"
    );
    let acc = best_acc.unwrap();
    assert!((acc - 0.6).abs() < 1e-9, "best_acc should be ~0.6, got {acc}");

    // An experiments row must exist
    let exp_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM autoresearch_experiments WHERE run_id = ?",
    )
    .bind(run_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(exp_count >= 1, "at least one experiment row expected");

    // At least one stdout line must have been broadcast
    let mut got_any = false;
    while let Ok(msg) = rx.try_recv() {
        if msg.run_id == run_id {
            got_any = true;
            break;
        }
    }
    assert!(got_any, "at least one AutoresearchStdoutLine must have been broadcast");
}

// ─── Prepare failure path ─────────────────────────────────────────────────────

#[tokio::test]
async fn prepare_failure_marks_run_failed() {
    let tmp = TempDir::new().unwrap();
    let pool = make_pool().await;
    let run_id = "run-prep-fail-01";
    insert_running_run(&pool, run_id).await;

    // prepare exits non-zero
    let train_body = "#!/usr/bin/env python3\nimport sys\n\
        print('XVN_RESULT {\"val_acc\": 0.9}')\nsys.exit(0)\n";
    let config_path = setup_worktree(&tmp, 1, train_body);

    let (tx, _rx) = broadcast::channel::<AutoresearchStdoutLine>(64);

    execute_training_run(
        pool.clone(),
        tx,
        run_id.to_string(),
        tmp.path().to_path_buf(),
        config_path,
        Duration::from_secs(30),
    )
    .await;

    let status: String = sqlx::query_scalar(
        "SELECT status FROM autoresearch_runs WHERE run_id = ?",
    )
    .bind(run_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(status, "failed", "run must be failed when prepare exits non-zero");
}

// ─── Train failure path (exits non-zero / no XVN_RESULT) ─────────────────────

#[tokio::test]
async fn train_failure_marks_run_failed() {
    let tmp = TempDir::new().unwrap();
    let pool = make_pool().await;
    let run_id = "run-train-fail-01";
    insert_running_run(&pool, run_id).await;

    // prepare OK, train exits non-zero with no XVN_RESULT
    let train_body = "#!/usr/bin/env python3\nimport sys\n\
        print('something went wrong')\nsys.exit(1)\n";
    let config_path = setup_worktree(&tmp, 0, train_body);

    let (tx, _rx) = broadcast::channel::<AutoresearchStdoutLine>(64);

    execute_training_run(
        pool.clone(),
        tx,
        run_id.to_string(),
        tmp.path().to_path_buf(),
        config_path,
        Duration::from_secs(30),
    )
    .await;

    let status: String = sqlx::query_scalar(
        "SELECT status FROM autoresearch_runs WHERE run_id = ?",
    )
    .bind(run_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(status, "failed", "run must be failed when train exits non-zero");
}

// ─── Missing XVN_RESULT → failed ─────────────────────────────────────────────

#[tokio::test]
async fn missing_xvn_result_marks_run_failed() {
    let tmp = TempDir::new().unwrap();
    let pool = make_pool().await;
    let run_id = "run-no-result-01";
    insert_running_run(&pool, run_id).await;

    // prepare OK, train exits 0 but does NOT print XVN_RESULT
    let train_body = "#!/usr/bin/env python3\nimport sys\n\
        print('training done')\nsys.exit(0)\n";
    let config_path = setup_worktree(&tmp, 0, train_body);

    let (tx, _rx) = broadcast::channel::<AutoresearchStdoutLine>(64);

    execute_training_run(
        pool.clone(),
        tx,
        run_id.to_string(),
        tmp.path().to_path_buf(),
        config_path,
        Duration::from_secs(30),
    )
    .await;

    let status: String = sqlx::query_scalar(
        "SELECT status FROM autoresearch_runs WHERE run_id = ?",
    )
    .bind(run_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(status, "failed", "run must be failed when XVN_RESULT is missing");
}
