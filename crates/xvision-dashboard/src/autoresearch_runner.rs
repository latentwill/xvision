//! Detached training executor for autoresearch runs.
//!
//! `execute_training_run` is called from `start_run` via `tokio::spawn` so the
//! HTTP response is returned immediately (201) while training proceeds in the
//! background. The function owns the full run lifecycle:
//!
//! 1. Data prep  — `uv run xvision_prepare.py <config_path>`
//! 2. Training   — `uv run xvision_train.py <config_path>` (wall-clock bounded)
//! 3. Experiment row insert  (on success)
//! 4. Run status update  — always transitions from 'running' to 'completed' or 'failed'
//!
//! No error is returned: all failure modes are recorded via the DB status column
//! and broadcast via the stdout channel so the operator can observe them in the
//! SSE stream.

use std::path::PathBuf;
use std::time::Duration;

use chrono::Utc;
use sqlx::SqlitePool;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tokio::sync::broadcast;
use ulid::Ulid;

use xvision_engine::autoresearch::experiment::parse_xvn_result;

use crate::sse::autoresearch_sse::AutoresearchStdoutLine;

/// Main entry point. Called detached via `tokio::spawn`.
///
/// Parameters:
/// - `pool`           — shared SQLite pool (must already have `autoresearch_runs` +
///                      `autoresearch_experiments` tables).
/// - `stdout_tx`      — broadcast channel; each stdout line is emitted as an
///                      `AutoresearchStdoutLine { run_id, line }`.
/// - `run_id`         — ULID string of the run row (already 'running').
/// - `worktree_root`  — root of the autoresearch git worktree; `nanochat/` is
///                      resolved as a sub-directory of this path.
/// - `run_config_path`— absolute path to the `run_config.json` already written
///                      by `start_run`; passed verbatim to both Python scripts.
/// - `wall_clock`     — maximum wall-clock duration for the train step.
pub async fn execute_training_run(
    pool: SqlitePool,
    stdout_tx: broadcast::Sender<AutoresearchStdoutLine>,
    run_id: String,
    worktree_root: PathBuf,
    run_config_path: PathBuf,
    wall_clock: Duration,
) {
    // Emit a single line on the broadcast channel (ignores send errors — no
    // receivers is fine for test/offline scenarios).
    let emit = |line: String| {
        let _ = stdout_tx.send(AutoresearchStdoutLine {
            run_id: run_id.clone(),
            line,
        });
    };

    let nanochat_dir = worktree_root.join("nanochat");

    // ── Step 1: data prep ────────────────────────────────────────────────────

    emit("[autoresearch] preparing training data…".to_string());

    let prep_ok = run_subprocess_streaming(
        "uv",
        &[
            "run",
            "xvision_prepare.py",
            run_config_path.to_str().unwrap_or(""),
        ],
        &nanochat_dir,
        &emit,
    )
    .await;

    if !prep_ok {
        emit("[autoresearch] failed: data preparation exited non-zero".to_string());
        mark_run_failed(&pool, &run_id, None).await;
        return;
    }

    // ── Step 2: train ────────────────────────────────────────────────────────

    emit("[autoresearch] training…".to_string());

    let train_lines = run_subprocess_collecting(
        "uv",
        &["run", "xvision_train.py", run_config_path.to_str().unwrap_or("")],
        &nanochat_dir,
        &emit,
        wall_clock,
    )
    .await;

    let train_lines = match train_lines {
        Some(lines) => lines,
        None => {
            emit("[autoresearch] failed: training timed out or failed to spawn".to_string());
            mark_run_failed(&pool, &run_id, None).await;
            return;
        }
    };

    // ── Step 3: parse result + record ────────────────────────────────────────

    let full_stdout = train_lines.join("\n");
    let xvn_result = parse_xvn_result(&full_stdout);

    let val_acc = match xvn_result.as_ref() {
        Some(r) => r.val_acc,
        None => {
            emit("[autoresearch] failed: XVN_RESULT not found in training output".to_string());
            mark_run_failed(&pool, &run_id, None).await;
            return;
        }
    };

    let val_loss = xvn_result.as_ref().and_then(|r| r.val_loss);

    // Check whether the training subprocess exited 0 (it must for a success).
    // We detect non-zero exit via the collecting helper returning None for the
    // wait_with_output error branch; here we also verify if we got all lines
    // (the lines vector is populated only when exit was 0 or we timed out).
    //
    // In the collecting helper, a non-zero exit code still returns Some(lines)
    // but we need to distinguish it. We embed the exit code in the return value
    // by returning None on non-zero exit.

    // Insert the experiment row.
    let experiment_id = Ulid::new().to_string();
    let created_at = Utc::now().to_rfc3339();

    let insert_result = sqlx::query(
        "INSERT INTO autoresearch_experiments
            (experiment_id, run_id, git_commit, val_acc, val_loss, status,
             description, created_at)
         VALUES (?, ?, '', ?, ?, 'keep', 'autonomous baseline run', ?)",
    )
    .bind(&experiment_id)
    .bind(&run_id)
    .bind(val_acc)
    .bind(val_loss)
    .bind(&created_at)
    .execute(&pool)
    .await;

    if let Err(e) = insert_result {
        emit(format!(
            "[autoresearch] failed: could not insert experiment row: {e}"
        ));
        mark_run_failed(&pool, &run_id, None).await;
        return;
    }

    // ── Step 4: transition run to completed ──────────────────────────────────

    let stopped_at = Utc::now().to_rfc3339();
    let update_result = sqlx::query(
        "UPDATE autoresearch_runs
         SET status = 'completed', stopped_at = ?, experiments = experiments + 1, best_acc = ?
         WHERE run_id = ?",
    )
    .bind(&stopped_at)
    .bind(val_acc)
    .bind(&run_id)
    .execute(&pool)
    .await;

    match update_result {
        Ok(_) => {
            emit("[autoresearch] completed".to_string());
        }
        Err(e) => {
            emit(format!("[autoresearch] failed: DB update error: {e}"));
        }
    }
}

// ─── Helpers ─────────────────────────────────────────────────────────────────

/// Spawn a subprocess, stream each stdout line via `emit`, wait for exit.
/// Returns `true` if the process exited with status 0, `false` otherwise
/// (spawn error or non-zero exit).
async fn run_subprocess_streaming(
    program: &str,
    args: &[&str],
    cwd: &PathBuf,
    emit: &impl Fn(String),
) -> bool {
    let child = Command::new(program)
        .args(args)
        .current_dir(cwd)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::inherit())
        .spawn();

    let mut child = match child {
        Ok(c) => c,
        Err(e) => {
            emit(format!("[autoresearch] spawn error: {e}"));
            return false;
        }
    };

    // Stream stdout lines.
    if let Some(stdout) = child.stdout.take() {
        let mut reader = BufReader::new(stdout).lines();
        while let Ok(Some(line)) = reader.next_line().await {
            emit(line);
        }
    }

    match child.wait().await {
        Ok(status) => status.success(),
        Err(_) => false,
    }
}

/// Spawn a subprocess under a wall-clock timeout, stream + collect stdout
/// lines via `emit`, wait for exit.
///
/// Returns:
/// - `Some(lines)` if the process exited **0** within the timeout.
/// - `None` if the process timed out, failed to spawn, exited non-zero, or
///   had an I/O error.
async fn run_subprocess_collecting(
    program: &str,
    args: &[&str],
    cwd: &PathBuf,
    emit: &impl Fn(String),
    wall_clock: Duration,
) -> Option<Vec<String>> {
    let child = Command::new(program)
        .args(args)
        .current_dir(cwd)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::inherit())
        .spawn();

    let mut child = match child {
        Ok(c) => c,
        Err(e) => {
            emit(format!("[autoresearch] spawn error: {e}"));
            return None;
        }
    };

    // Collect lines from stdout while streaming them to the broadcast channel.
    let stdout = child.stdout.take()?;
    let mut reader = BufReader::new(stdout).lines();
    let mut collected: Vec<String> = Vec::new();

    // Run the read loop + wait under a single timeout.
    let result = tokio::time::timeout(wall_clock, async {
        loop {
            match reader.next_line().await {
                Ok(Some(line)) => {
                    emit(line.clone());
                    collected.push(line);
                }
                Ok(None) => break, // EOF
                Err(_) => break,   // I/O error
            }
        }
        child.wait().await
    })
    .await;

    match result {
        Ok(Ok(status)) if status.success() => Some(collected),
        Ok(Ok(_)) => {
            // Non-zero exit — still emit nothing extra; caller will handle
            None
        }
        Ok(Err(_)) => None, // wait() error
        Err(_) => {
            // Timeout — kill the child
            let _ = child.kill().await;
            None
        }
    }
}

/// Mark a run as 'failed', always. Best-effort: DB errors are logged to
/// stderr but not propagated (we have no error surface from a detached task).
async fn mark_run_failed(pool: &SqlitePool, run_id: &str, _reason: Option<&str>) {
    let stopped_at = Utc::now().to_rfc3339();
    if let Err(e) =
        sqlx::query("UPDATE autoresearch_runs SET status = 'failed', stopped_at = ? WHERE run_id = ?")
            .bind(&stopped_at)
            .bind(run_id)
            .execute(pool)
            .await
    {
        eprintln!("[autoresearch_runner] failed to mark run {run_id} as failed: {e}");
    }
}
