//! `BatchStore` — sqlx-backed persistence for eval batches.
//!
//! Owned data: `eval_batches` table (created by migration 020). The store
//! also reads `eval_runs.batch_id` to load the runs belonging to a batch,
//! but write access to `eval_runs` itself stays with `RunStore`.

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use ulid::Ulid;

/// Row shape mirroring `eval_batches`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Batch {
    pub batch_id: String,
    pub strategy_id: String,
    /// Agent profile id supplied via `--review-with`. `None` when the batch
    /// was launched without per-run review.
    pub review_with: Option<String>,
    #[serde(with = "chrono::serde::ts_seconds")]
    pub created_at: DateTime<Utc>,
    #[serde(default, with = "chrono::serde::ts_seconds_option")]
    pub completed_at: Option<DateTime<Utc>>,
    /// `pending` | `running` | `completed` | `partial` | `failed`
    pub status: String,
}

#[derive(Debug, Clone)]
pub struct BatchStore {
    pool: SqlitePool,
}

impl BatchStore {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    /// Insert a new batch row. `status` is set to `'running'` immediately
    /// (the batch id is generated before any run is launched; there is no
    /// observable `pending` window).
    pub async fn create(&self, strategy_id: &str, review_with: Option<&str>) -> Result<Batch> {
        let batch_id = format!("batch_{}", Ulid::new());
        let created_at = Utc::now();
        sqlx::query(
            "INSERT INTO eval_batches \
             (batch_id, strategy_id, review_with, created_at, completed_at, status) \
             VALUES (?, ?, ?, ?, NULL, 'running')",
        )
        .bind(&batch_id)
        .bind(strategy_id)
        .bind(review_with)
        .bind(created_at.to_rfc3339())
        .execute(&self.pool)
        .await
        .with_context(|| format!("insert eval_batches batch_id={batch_id}"))?;

        Ok(Batch {
            batch_id,
            strategy_id: strategy_id.to_string(),
            review_with: review_with.map(str::to_string),
            created_at,
            completed_at: None,
            status: "running".to_string(),
        })
    }

    /// Load a single batch row. Returns `None` when the batch_id is not found.
    pub async fn get(&self, batch_id: &str) -> Result<Option<Batch>> {
        let row: Option<(String, String, Option<String>, String, Option<String>, String)> =
            sqlx::query_as(
                "SELECT batch_id, strategy_id, review_with, created_at, completed_at, status \
                 FROM eval_batches WHERE batch_id = ?",
            )
            .bind(batch_id)
            .fetch_optional(&self.pool)
            .await
            .with_context(|| format!("get eval_batches batch_id={batch_id}"))?;

        match row {
            None => Ok(None),
            Some((bid, sid, rw, ca, comp, status)) => Ok(Some(Batch {
                batch_id: bid,
                strategy_id: sid,
                review_with: rw,
                created_at: ca.parse().context("parse created_at")?,
                completed_at: comp.map(|s| s.parse()).transpose().context("parse completed_at")?,
                status,
            })),
        }
    }

    /// List batches, most-recent first. Optionally filter by strategy_id.
    pub async fn list(&self, strategy_id: Option<&str>) -> Result<Vec<Batch>> {
        let rows: Vec<(String, String, Option<String>, String, Option<String>, String)> =
            match strategy_id {
                Some(sid) => sqlx::query_as(
                    "SELECT batch_id, strategy_id, review_with, created_at, completed_at, status \
                     FROM eval_batches WHERE strategy_id = ? \
                     ORDER BY created_at DESC",
                )
                .bind(sid)
                .fetch_all(&self.pool)
                .await
                .context("list eval_batches filtered")?,
                None => sqlx::query_as(
                    "SELECT batch_id, strategy_id, review_with, created_at, completed_at, status \
                     FROM eval_batches ORDER BY created_at DESC",
                )
                .fetch_all(&self.pool)
                .await
                .context("list eval_batches all")?,
            };

        rows.into_iter()
            .map(|(bid, sid, rw, ca, comp, status)| {
                Ok(Batch {
                    batch_id: bid,
                    strategy_id: sid,
                    review_with: rw,
                    created_at: ca.parse().context("parse created_at")?,
                    completed_at: comp
                        .map(|s| s.parse())
                        .transpose()
                        .context("parse completed_at")?,
                    status,
                })
            })
            .collect()
    }

    /// Compute rollup status from a set of per-run status strings and write
    /// `completed_at` + `status`. Idempotent: if the batch already has a
    /// terminal status the call is a no-op and returns the stored batch.
    ///
    /// Rollup rules:
    /// - All runs completed   → `completed`
    /// - All runs failed      → `failed`
    /// - Mix of completed + failed (or any partial) → `partial`
    pub async fn finalize(&self, batch_id: &str, run_statuses: &[&str]) -> Result<Batch> {
        // Load current; return early if already terminal.
        let batch = self
            .get(batch_id)
            .await?
            .with_context(|| format!("batch not found: {batch_id}"))?;

        if matches!(
            batch.status.as_str(),
            "completed" | "partial" | "failed"
        ) {
            return Ok(batch);
        }

        let rollup = compute_rollup_status(run_statuses);
        let now = Utc::now();
        sqlx::query(
            "UPDATE eval_batches \
             SET status = ?, completed_at = ? \
             WHERE batch_id = ?",
        )
        .bind(&rollup)
        .bind(now.to_rfc3339())
        .bind(batch_id)
        .execute(&self.pool)
        .await
        .with_context(|| format!("finalize eval_batches batch_id={batch_id}"))?;

        Ok(Batch {
            status: rollup,
            completed_at: Some(now),
            ..batch
        })
    }

    /// Attach a batch_id to an existing eval_run row. Called by the CLI
    /// immediately after each run completes so the run is linkable to its
    /// batch.
    pub async fn attach_run(&self, run_id: &str, batch_id: &str) -> Result<()> {
        sqlx::query("UPDATE eval_runs SET batch_id = ? WHERE id = ?")
            .bind(batch_id)
            .bind(run_id)
            .execute(&self.pool)
            .await
            .with_context(|| format!("attach run {run_id} to batch {batch_id}"))?;
        Ok(())
    }

    /// Return all run_ids belonging to the given batch.
    pub async fn run_ids_for_batch(&self, batch_id: &str) -> Result<Vec<String>> {
        let rows: Vec<(String,)> =
            sqlx::query_as("SELECT id FROM eval_runs WHERE batch_id = ? ORDER BY started_at ASC")
                .bind(batch_id)
                .fetch_all(&self.pool)
                .await
                .with_context(|| format!("run_ids_for_batch {batch_id}"))?;
        Ok(rows.into_iter().map(|(id,)| id).collect())
    }
}

/// Compute the rollup `status` string from a slice of individual run statuses.
///
/// - All `"completed"` → `"completed"`
/// - All `"failed"` (or empty) → `"failed"`
/// - Otherwise → `"partial"`
pub(crate) fn compute_rollup_status(run_statuses: &[&str]) -> String {
    if run_statuses.is_empty() {
        return "failed".to_string();
    }
    let all_completed = run_statuses.iter().all(|s| *s == "completed");
    let all_failed = run_statuses.iter().all(|s| *s == "failed");
    if all_completed {
        "completed".to_string()
    } else if all_failed {
        "failed".to_string()
    } else {
        "partial".to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::{Actor, ApiContext};

    /// Open a fresh in-memory ApiContext (with all migrations applied).
    async fn fresh_ctx() -> (ApiContext, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        let ctx = ApiContext::open(dir.path(), Actor::Cli { user: "test".into() })
            .await
            .unwrap();
        (ctx, dir)
    }

    // ── compute_rollup_status unit tests ─────────────────────────────────

    #[test]
    fn rollup_all_completed() {
        assert_eq!(
            compute_rollup_status(&["completed", "completed"]),
            "completed"
        );
    }

    #[test]
    fn rollup_all_failed() {
        assert_eq!(
            compute_rollup_status(&["failed", "failed"]),
            "failed"
        );
    }

    #[test]
    fn rollup_mixed_is_partial() {
        assert_eq!(
            compute_rollup_status(&["completed", "failed"]),
            "partial"
        );
    }

    #[test]
    fn rollup_empty_is_failed() {
        assert_eq!(compute_rollup_status(&[]), "failed");
    }

    // ── integration tests exercising BatchStore against a real SQLite ─────

    #[tokio::test]
    async fn create_and_get_batch() {
        let (ctx, _dir) = fresh_ctx().await;
        let store = BatchStore::new(ctx.db.clone());

        let batch = store.create("strat-01", None).await.unwrap();
        assert!(batch.batch_id.starts_with("batch_"));
        assert_eq!(batch.strategy_id, "strat-01");
        assert_eq!(batch.status, "running");
        assert!(batch.completed_at.is_none());
        assert!(batch.review_with.is_none());

        // Round-trip via get
        let fetched = store.get(&batch.batch_id).await.unwrap().unwrap();
        assert_eq!(fetched.batch_id, batch.batch_id);
        assert_eq!(fetched.status, "running");
    }

    #[tokio::test]
    async fn create_batch_with_review_with() {
        let (ctx, _dir) = fresh_ctx().await;
        let store = BatchStore::new(ctx.db.clone());

        let batch = store.create("strat-02", Some("profile-xyz")).await.unwrap();
        assert_eq!(batch.review_with.as_deref(), Some("profile-xyz"));

        let fetched = store.get(&batch.batch_id).await.unwrap().unwrap();
        assert_eq!(fetched.review_with.as_deref(), Some("profile-xyz"));
    }

    #[tokio::test]
    async fn finalize_batch_all_completed() {
        let (ctx, _dir) = fresh_ctx().await;
        let store = BatchStore::new(ctx.db.clone());

        let batch = store.create("strat-03", None).await.unwrap();
        let finalized = store
            .finalize(&batch.batch_id, &["completed", "completed"])
            .await
            .unwrap();

        assert_eq!(finalized.status, "completed");
        assert!(finalized.completed_at.is_some());

        // Idempotent: calling again returns the same terminal status.
        let again = store
            .finalize(&batch.batch_id, &["failed"])
            .await
            .unwrap();
        assert_eq!(again.status, "completed");
    }

    #[tokio::test]
    async fn finalize_batch_partial() {
        let (ctx, _dir) = fresh_ctx().await;
        let store = BatchStore::new(ctx.db.clone());

        let batch = store.create("strat-04", None).await.unwrap();
        let finalized = store
            .finalize(&batch.batch_id, &["completed", "failed"])
            .await
            .unwrap();
        assert_eq!(finalized.status, "partial");
    }

    #[tokio::test]
    async fn finalize_batch_all_failed() {
        let (ctx, _dir) = fresh_ctx().await;
        let store = BatchStore::new(ctx.db.clone());

        let batch = store.create("strat-05", None).await.unwrap();
        let finalized = store
            .finalize(&batch.batch_id, &["failed", "failed"])
            .await
            .unwrap();
        assert_eq!(finalized.status, "failed");
    }

    #[tokio::test]
    async fn list_batches_filtered_by_strategy() {
        let (ctx, _dir) = fresh_ctx().await;
        let store = BatchStore::new(ctx.db.clone());

        let _a = store.create("strat-alpha", None).await.unwrap();
        let _b = store.create("strat-beta", None).await.unwrap();
        let _c = store.create("strat-alpha", None).await.unwrap();

        let alpha = store.list(Some("strat-alpha")).await.unwrap();
        assert_eq!(alpha.len(), 2);
        assert!(alpha.iter().all(|b| b.strategy_id == "strat-alpha"));

        let all = store.list(None).await.unwrap();
        assert_eq!(all.len(), 3);
    }

    #[tokio::test]
    async fn full_lifecycle_create_attach_finalize() {
        use crate::eval::{
            run::{Run, RunMode},
            store::RunStore,
        };

        let (ctx, _dir) = fresh_ctx().await;
        let batch_store = BatchStore::new(ctx.db.clone());
        let run_store = RunStore::new(ctx.db.clone());

        // Create batch
        let batch = batch_store.create("strat-lifecycle", None).await.unwrap();

        // Seed the two scenarios that the eval_runs trigger-FK needs.
        // The trigger checks the `scenarios` table (not `eval_scenarios`).
        // We upsert minimal rows so the FK is satisfied without depending on
        // the canonical seed. Schema: id, source, display_name, description,
        // body_json, created_at, created_by (see migration 011_scenarios.sql).
        let now = chrono::Utc::now().to_rfc3339();
        for (sid, name) in [
            ("test-sc-lifecycle-1", "Lifecycle Test Scenario 1"),
            ("test-sc-lifecycle-2", "Lifecycle Test Scenario 2"),
        ] {
            sqlx::query(
                "INSERT OR IGNORE INTO scenarios \
                 (id, source, display_name, description, body_json, created_at, created_by) \
                 VALUES (?, 'canonical', ?, '', '{}', ?, 'test')",
            )
            .bind(sid)
            .bind(name)
            .bind(&now)
            .execute(&ctx.db)
            .await
            .unwrap();
        }

        // Insert two fake runs
        let mut run1 = Run::new_queued(
            "strat-lifecycle".into(),
            "test-sc-lifecycle-1".into(),
            RunMode::Backtest,
        );
        let mut run2 = Run::new_queued(
            "strat-lifecycle".into(),
            "test-sc-lifecycle-2".into(),
            RunMode::Backtest,
        );
        run_store.create(&run1).await.unwrap();
        run_store.create(&run2).await.unwrap();

        // Attach runs to batch
        batch_store.attach_run(&run1.id, &batch.batch_id).await.unwrap();
        batch_store.attach_run(&run2.id, &batch.batch_id).await.unwrap();

        // Verify run_ids_for_batch
        let ids = batch_store.run_ids_for_batch(&batch.batch_id).await.unwrap();
        assert_eq!(ids.len(), 2);
        assert!(ids.contains(&run1.id));
        assert!(ids.contains(&run2.id));

        // Finalize runs (simulate both completed)
        run_store.begin_running(&run1.id).await.unwrap();
        run_store.begin_running(&run2.id).await.unwrap();
        let metrics = crate::eval::run::MetricsSummary {
            total_return_pct: 5.0,
            sharpe: 1.2,
            max_drawdown_pct: 8.0,
            win_rate: 0.6,
            n_trades: 10,
            n_decisions: 20,
            baselines: None,
        };
        run_store.finalize(&run1.id, &metrics).await.unwrap();
        run_store.finalize(&run2.id, &metrics).await.unwrap();
        run1.status = crate::eval::run::RunStatus::Completed;
        run2.status = crate::eval::run::RunStatus::Completed;

        // Finalize batch
        let statuses: Vec<&str> = [&run1.status, &run2.status]
            .iter()
            .map(|s| s.as_str())
            .collect();
        let done = batch_store
            .finalize(&batch.batch_id, &statuses)
            .await
            .unwrap();
        assert_eq!(done.status, "completed");
        assert!(done.completed_at.is_some());

        // get_batch returns runs via run_ids_for_batch
        let run_ids = batch_store.run_ids_for_batch(&batch.batch_id).await.unwrap();
        assert_eq!(run_ids.len(), 2);
    }
}
