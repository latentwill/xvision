//! Per-iteration experiment loop for autoresearch training runs.
//!
//! Each call to `run_one_experiment` commits the current `xvision_train.py`
//! into the run's worktree, spawns `uv run xvision_train.py` with a
//! wall-clock timeout, parses the mandatory `XVN_RESULT` last line, writes
//! an `autoresearch_experiments` row, and either keeps or resets the commit
//! based on val_acc improvement.

use std::path::Path;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use tokio::process::Command;
use tokio::time::timeout;
use ulid::Ulid;

/// Per-line byte cap for SSE streaming. Lines beyond this are truncated so a
/// runaway log line cannot exhaust the SSE buffer (Task 6.4).
pub const SSE_LINE_BYTE_CAP: usize = 4096;

/// The result produced by `XVN_RESULT {"val_acc": ..., "val_loss": ...}`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct XvnResult {
    pub val_acc: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub val_loss: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub peak_vram_mb: Option<f64>,
}

/// Outcome status written to the `autoresearch_experiments` row.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ExperimentStatus {
    /// val_acc improved → commit kept.
    Keep,
    /// val_acc did not improve → `git reset` applied.
    Discard,
    /// Subprocess completed without printing `XVN_RESULT`, or timed out.
    Crash,
}

impl ExperimentStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Keep => "keep",
            Self::Discard => "discard",
            Self::Crash => "crash",
        }
    }
}

/// Summary row returned to the caller. Mirrors `autoresearch_experiments`.
#[derive(Debug, Clone)]
pub struct ExperimentOutcome {
    pub experiment_id: String,
    pub git_commit: String,
    pub val_acc: Option<f64>,
    pub val_loss: Option<f64>,
    pub peak_vram_mb: Option<f64>,
    pub training_seconds: f64,
    pub status: ExperimentStatus,
}

/// Mutable state threaded through the experiment loop by the caller.
#[derive(Debug)]
pub struct ExperimentLoopState {
    pub run_id: String,
    /// Current best `val_acc` across all kept experiments in this run.
    /// `None` = no kept experiment yet.
    pub best_val_acc: Option<f64>,
}

impl ExperimentLoopState {
    pub fn new(run_id: String, best_val_acc: Option<f64>) -> Self {
        Self { run_id, best_val_acc }
    }
}

/// Execute one experiment iteration in `worktree_path`:
///
/// 1. `git add xvision_train.py && git commit -m "experiment: {description}"`
/// 2. `uv run xvision_train.py` with `wall_clock` timeout.
/// 3. Parse `XVN_RESULT` from the last stdout line; missing ⇒ crash.
/// 4. Write `autoresearch_experiments` row.
/// 5. Keep commit if val_acc improved; else `git reset HEAD~1`.
///
/// The caller is responsible for having already modified `xvision_train.py`
/// before invoking this function (the autoresearcher agent step).
pub async fn run_one_experiment(
    pool: &SqlitePool,
    worktree_path: &Path,
    run_id: &str,
    experiment_id: &str,
    description: &str,
    state: &mut ExperimentLoopState,
    wall_clock: Duration,
) -> Result<ExperimentOutcome> {
    // 1. Stage and commit xvision_train.py.
    let git_commit = {
        let add_status = Command::new("git")
            .args(["add", "xvision_train.py"])
            .current_dir(worktree_path)
            .status()
            .await
            .context("git add xvision_train.py")?;
        if !add_status.success() {
            anyhow::bail!("git add failed in {}", worktree_path.display());
        }

        let commit_msg = format!("experiment: {description}");
        let commit_status = Command::new("git")
            .args([
                "-c", "user.email=autoresearch@xvision",
                "-c", "user.name=Autoresearch Harness",
                "commit", "-m", &commit_msg,
                "--allow-empty",
            ])
            .current_dir(worktree_path)
            .status()
            .await
            .context("git commit")?;
        if !commit_status.success() {
            anyhow::bail!("git commit failed in {}", worktree_path.display());
        }

        // Read short hash.
        let output = Command::new("git")
            .args(["rev-parse", "--short", "HEAD"])
            .current_dir(worktree_path)
            .output()
            .await
            .context("git rev-parse HEAD")?;
        String::from_utf8_lossy(&output.stdout).trim().to_string()
    };

    // 2. Spawn training subprocess with timeout.
    let start = Instant::now();
    let train_result: Option<XvnResult> = {
        let child = Command::new("uv")
            .args(["run", "xvision_train.py"])
            .current_dir(worktree_path)
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::inherit())
            .spawn()
            .context("spawn uv run xvision_train.py")?;

        let output_result = timeout(wall_clock, child.wait_with_output()).await;

        match output_result {
            Ok(Ok(output)) => {
                // 3. Parse XVN_RESULT from the last stdout line.
                let stdout = String::from_utf8_lossy(&output.stdout);
                parse_xvn_result(&stdout)
            }
            Ok(Err(e)) => {
                tracing::warn!(error = %e, "training subprocess I/O error");
                None
            }
            Err(_) => {
                tracing::warn!("training subprocess timed out after {:?}", wall_clock);
                None
            }
        }
    };
    let training_seconds = start.elapsed().as_secs_f64();

    // 4. Determine status and update loop state.
    let status = match &train_result {
        Some(r) => {
            if state.best_val_acc.map_or(true, |best| r.val_acc > best) {
                state.best_val_acc = Some(r.val_acc);
                ExperimentStatus::Keep
            } else {
                ExperimentStatus::Discard
            }
        }
        None => ExperimentStatus::Crash,
    };

    // 5. Git reset if not keeping.
    if status != ExperimentStatus::Keep {
        let _ = Command::new("git")
            .args(["reset", "HEAD~1"])
            .current_dir(worktree_path)
            .status()
            .await;
    }

    let val_acc = train_result.as_ref().map(|r| r.val_acc);
    let val_loss = train_result.as_ref().and_then(|r| r.val_loss);
    let peak_vram_mb = train_result.as_ref().and_then(|r| r.peak_vram_mb);

    // 6. Write experiment row.
    let created_at = Utc::now().to_rfc3339();
    sqlx::query(
        "INSERT INTO autoresearch_experiments
            (experiment_id, run_id, git_commit, val_acc, val_loss, peak_vram_mb,
             training_seconds, status, description, created_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(experiment_id)
    .bind(run_id)
    .bind(&git_commit)
    .bind(val_acc)
    .bind(val_loss)
    .bind(peak_vram_mb)
    .bind(training_seconds)
    .bind(status.as_str())
    .bind(description)
    .bind(&created_at)
    .execute(pool)
    .await
    .context("insert autoresearch_experiments row")?;

    Ok(ExperimentOutcome {
        experiment_id: experiment_id.to_string(),
        git_commit,
        val_acc,
        val_loss,
        peak_vram_mb,
        training_seconds,
        status,
    })
}

/// Parse the `XVN_RESULT <json>` last line from subprocess stdout.
/// Returns `None` if the line is absent or malformed.
pub fn parse_xvn_result(stdout: &str) -> Option<XvnResult> {
    // Scan from the end for the XVN_RESULT line.
    for line in stdout.lines().rev() {
        let line = line.trim();
        if let Some(json_part) = line.strip_prefix("XVN_RESULT ") {
            return serde_json::from_str::<XvnResult>(json_part).ok();
        }
    }
    None
}

/// Generate a fresh ULID for a new experiment.
pub fn new_experiment_id() -> String {
    Ulid::new().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;
    use sqlx::SqlitePool;

    /// Write a deterministic stub `xvision_train.py` into `dir`.
    /// `outcome` controls what the script emits as its last line:
    ///   - `Ok(f64)` → prints `XVN_RESULT {"val_acc": <v>, "val_loss": 0.1}`
    ///   - `Err(())` → exits 0 but prints NO `XVN_RESULT` line (crash case)
    fn write_stub_train(dir: &std::path::Path, outcome: Result<f64, ()>) {
        let script = match outcome {
            Ok(val_acc) => format!(
                r#"#!/usr/bin/env python3
import sys
print("training epoch 1/1")
print("XVN_RESULT {{\"val_acc\": {val_acc}, \"val_loss\": 0.1}}")
sys.exit(0)
"#
            ),
            Err(()) => r#"#!/usr/bin/env python3
import sys
print("something went wrong")
sys.exit(0)
"#
            .to_string(),
        };
        let path = dir.join("xvision_train.py");
        std::fs::write(&path, script).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
    }

    async fn in_memory_pool() -> SqlitePool {
        let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();
        // Create the autoresearch_experiments table.
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

    fn init_git_repo(dir: &std::path::Path) {
        let git = |args: &[&str]| {
            std::process::Command::new("git")
                .args(args)
                .current_dir(dir)
                .status()
                .unwrap();
        };
        git(&["init", "-b", "main"]);
        git(&["config", "user.email", "t@t.com"]);
        git(&["config", "user.name", "T"]);
        std::fs::write(dir.join(".gitkeep"), b"").unwrap();
        git(&["add", ".gitkeep"]);
        git(&["commit", "-m", "init"]);
    }

    #[tokio::test]
    async fn keep_commit_when_val_acc_improves() {
        let tmp = TempDir::new().unwrap();
        let pool = in_memory_pool().await;
        init_git_repo(tmp.path());
        write_stub_train(tmp.path(), Ok(0.75));

        let mut loop_state = ExperimentLoopState::new("run-01".to_string(), None);
        let result = run_one_experiment(
            &pool,
            tmp.path(),
            "run-01",
            "experiment-01",
            "baseline OHLCV model",
            &mut loop_state,
            std::time::Duration::from_secs(30),
        )
        .await
        .unwrap();

        assert_eq!(result.status, ExperimentStatus::Keep);
        assert_eq!(result.val_acc, Some(0.75));
        assert_eq!(loop_state.best_val_acc, Some(0.75));

        // Row should be in the DB.
        let count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM autoresearch_experiments WHERE run_id = 'run-01'")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(count, 1);
    }

    #[tokio::test]
    async fn discard_commit_when_val_acc_does_not_improve() {
        let tmp = TempDir::new().unwrap();
        let pool = in_memory_pool().await;
        init_git_repo(tmp.path());
        write_stub_train(tmp.path(), Ok(0.60));

        // Seed best_val_acc with a value higher than 0.60.
        let mut loop_state = ExperimentLoopState::new("run-02".to_string(), Some(0.70));

        let result = run_one_experiment(
            &pool,
            tmp.path(),
            "run-02",
            "experiment-02",
            "wider window attempt",
            &mut loop_state,
            std::time::Duration::from_secs(30),
        )
        .await
        .unwrap();

        assert_eq!(result.status, ExperimentStatus::Discard);
        // best_val_acc must NOT have been updated.
        assert_eq!(loop_state.best_val_acc, Some(0.70));

        let status: String = sqlx::query_scalar(
            "SELECT status FROM autoresearch_experiments WHERE experiment_id = 'experiment-02'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(status, "discard");
    }

    #[tokio::test]
    async fn crash_when_xvn_result_missing() {
        let tmp = TempDir::new().unwrap();
        let pool = in_memory_pool().await;
        init_git_repo(tmp.path());
        write_stub_train(tmp.path(), Err(()));

        let mut loop_state = ExperimentLoopState::new("run-03".to_string(), None);

        let result = run_one_experiment(
            &pool,
            tmp.path(),
            "run-03",
            "experiment-03",
            "crash scenario",
            &mut loop_state,
            std::time::Duration::from_secs(30),
        )
        .await
        .unwrap();

        assert_eq!(result.status, ExperimentStatus::Crash);
        assert_eq!(result.val_acc, None);
        // best_val_acc unchanged.
        assert_eq!(loop_state.best_val_acc, None);

        let status: String = sqlx::query_scalar(
            "SELECT status FROM autoresearch_experiments WHERE experiment_id = 'experiment-03'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(status, "crash");
    }
}
