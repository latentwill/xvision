//! Checkpoint promotion gate + trained_models row insertion.
//!
//! Promotion gate: `val_acc - current_best > epsilon AND val_acc >= floor AND
//! holdout_samples >= min_holdout AND val_acc IS NOT NULL`.
//!
//! `promoted = 1` means the checkpoint appears in the strategy-builder picker
//! as a candidate. `live_approved` starts at 0 and requires an explicit
//! operator action (`POST /api/nanochat/checkpoints/:id/approve`) after a
//! backtest-vs-baseline comparison.

use anyhow::{Context, Result};
use chrono::Utc;
use sqlx::SqlitePool;

/// Configurable thresholds for the promotion gate. Read from the config store
/// at run-start time and captured into `RunConfig` so they don't change during
/// a long run.
#[derive(Debug, Clone)]
pub struct PromotionGate {
    /// Minimum improvement over current best val_acc to promote.
    pub epsilon: f64,
    /// Absolute minimum val_acc to promote (prevents promoting at 51% accuracy
    /// just because it's the best seen).
    pub acc_floor: f64,
    /// Minimum number of held-out samples backing the val_acc measurement.
    pub min_holdout_samples: u32,
}

impl PromotionGate {
    /// Evaluate whether `val_acc` / `holdout_samples` meet the gate.
    ///
    /// `current_best`: the best `val_acc` across all previously promoted
    /// checkpoints for the same `source_strategy_id`. `None` = no prior
    /// checkpoint (treated as a 0.0 baseline).
    ///
    /// Delegates to the single canonical gate `nanochat::validate::
    /// evaluate_promotion_gate` (s3ph.17 — promotion logic lives in ONE place).
    /// For sane configs (`epsilon << acc_floor`) this is identical to the prior
    /// behavior: with no prior checkpoint, any `val_acc ≥ acc_floor` also clears
    /// the epsilon-over-zero check (since `acc_floor > epsilon`). The two impls
    /// previously diverged only in the pathological `epsilon ≥ acc_floor` case.
    pub fn should_promote(&self, val_acc: f64, holdout_samples: u32, current_best: Option<f64>) -> bool {
        crate::nanochat::validate::evaluate_promotion_gate(
            Some(val_acc),
            current_best,
            holdout_samples as i64,
            crate::nanochat::validate::PromotionGateCfg {
                epsilon: self.epsilon,
                acc_floor: self.acc_floor,
                min_holdout: self.min_holdout_samples as i64,
            },
        )
    }

    /// Variant accepting `Option<f64>` for val_acc — `None` (crash) always
    /// returns `false`.
    pub fn should_promote_opt(
        &self,
        val_acc: Option<f64>,
        holdout_samples: u32,
        current_best: Option<f64>,
    ) -> bool {
        match val_acc {
            Some(v) => self.should_promote(v, holdout_samples, current_best),
            None => false,
        }
    }
}

/// Request payload for inserting a new `trained_models` row.
/// `promoted` is set to 1 by the insertion function (caller only writes
/// promoted checkpoints here); `live_approved` always starts at 0.
pub struct NewTrainedModel {
    pub model_id: String,
    pub display_name: String,
    pub source_strategy_id: Option<String>,
    pub source_strategy_name: Option<String>,
    pub run_tag: String,
    pub checkpoint_path: String,
    pub weights_sha256: String,
    pub input_spec: String,
    pub label_strategy: String,
    pub label_config: String,
    pub best_acc: Option<f64>,
    pub best_loss: Option<f64>,
    pub holdout_samples: Option<u32>,
    pub autoresearch_run_id: Option<String>,
}

/// Insert a promoted checkpoint row.
///
/// `promoted = 1` is set unconditionally (the caller must have already passed
/// the gate). `live_approved = 0` is always set so the operator must
/// explicitly approve before attachment.
pub async fn insert_trained_model(pool: &SqlitePool, req: &NewTrainedModel) -> Result<()> {
    let created_at = Utc::now().to_rfc3339();
    sqlx::query(
        "INSERT INTO trained_models
            (model_id, display_name, source_strategy_id, source_strategy_name,
             run_tag, checkpoint_path, weights_format, weights_sha256, input_spec,
             base_model, label_strategy, label_config, best_acc, best_loss,
             holdout_samples, promoted, live_approved, created_at, autoresearch_run_id)
         VALUES (?, ?, ?, ?, ?, ?, 'safetensors', ?, ?, 'gpt2-nanochat',
                 ?, ?, ?, ?, ?, 1, 0, ?, ?)",
    )
    .bind(&req.model_id)
    .bind(&req.display_name)
    .bind(&req.source_strategy_id)
    .bind(&req.source_strategy_name)
    .bind(&req.run_tag)
    .bind(&req.checkpoint_path)
    .bind(&req.weights_sha256)
    .bind(&req.input_spec)
    .bind(&req.label_strategy)
    .bind(&req.label_config)
    .bind(req.best_acc)
    .bind(req.best_loss)
    .bind(req.holdout_samples.map(|v| v as i64))
    .bind(&created_at)
    .bind(&req.autoresearch_run_id)
    .execute(pool)
    .await
    .context("insert trained_models row")?;
    Ok(())
}

/// Update `autoresearch_runs.best_acc` and `best_model_id` when a new
/// checkpoint promotes.
pub async fn update_run_best(
    pool: &SqlitePool,
    run_id: &str,
    val_acc: f64,
    model_id: &str,
) -> Result<()> {
    sqlx::query(
        "UPDATE autoresearch_runs SET best_acc = ?, best_model_id = ?
         WHERE run_id = ?",
    )
    .bind(val_acc)
    .bind(model_id)
    .bind(run_id)
    .execute(pool)
    .await
    .context("update autoresearch_runs best")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::SqlitePool;

    async fn in_memory_pool() -> SqlitePool {
        let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();
        sqlx::query(
            "CREATE TABLE trained_models (
                model_id            TEXT PRIMARY KEY,
                display_name        TEXT NOT NULL,
                source_strategy_id  TEXT,
                source_strategy_name TEXT,
                run_tag             TEXT NOT NULL,
                checkpoint_path     TEXT NOT NULL,
                weights_format      TEXT NOT NULL DEFAULT 'safetensors',
                weights_sha256      TEXT NOT NULL,
                input_spec          TEXT NOT NULL,
                base_model          TEXT NOT NULL DEFAULT 'gpt2-nanochat',
                label_strategy      TEXT NOT NULL,
                label_config        TEXT NOT NULL,
                best_acc            REAL,
                best_loss           REAL,
                holdout_samples     INTEGER,
                promoted            INTEGER NOT NULL DEFAULT 0,
                live_approved       INTEGER NOT NULL DEFAULT 0,
                created_at          TEXT NOT NULL,
                autoresearch_run_id TEXT
            )",
        )
        .execute(&pool)
        .await
        .unwrap();

        sqlx::query(
            "CREATE TABLE autoresearch_runs (
                run_id          TEXT PRIMARY KEY,
                run_tag         TEXT NOT NULL,
                source_strategy_id TEXT,
                label_strategy  TEXT NOT NULL,
                label_config    TEXT NOT NULL,
                git_branch      TEXT NOT NULL,
                worktree_path   TEXT NOT NULL,
                status          TEXT NOT NULL,
                started_at      TEXT NOT NULL,
                stopped_at      TEXT,
                experiments     INTEGER NOT NULL DEFAULT 0,
                best_acc        REAL,
                best_model_id   TEXT
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

        pool
    }

    fn gate() -> PromotionGate {
        PromotionGate {
            epsilon: 0.01,
            acc_floor: 0.52,
            min_holdout_samples: 200,
        }
    }

    #[test]
    fn promotes_when_all_conditions_met() {
        let gate = gate();
        assert!(gate.should_promote(0.75, 250, None));
        assert!(gate.should_promote(0.75, 250, Some(0.60))); // beats current best by > epsilon
    }

    #[test]
    fn no_promotion_below_floor() {
        let gate = gate();
        assert!(!gate.should_promote(0.51, 250, None));
    }

    #[test]
    fn no_promotion_when_epsilon_not_exceeded() {
        let gate = gate();
        // current best = 0.75, new = 0.755 — delta = 0.005 < epsilon 0.01
        assert!(!gate.should_promote(0.755, 250, Some(0.75)));
    }

    #[test]
    fn no_promotion_with_insufficient_holdout() {
        let gate = gate();
        assert!(!gate.should_promote(0.80, 199, None));
    }

    #[test]
    fn crash_experiment_never_promotes() {
        let gate = gate();
        // val_acc = None (crash) → should_promote_opt returns false.
        assert!(!gate.should_promote_opt(None, 300, None));
    }

    #[test]
    fn unified_with_evaluate_promotion_gate_no_divergence_on_none_baseline() {
        // s3ph.17: PromotionGate now DELEGATES to evaluate_promotion_gate, so the
        // two impls must agree even in the pathological epsilon >= acc_floor case
        // where they historically diverged (old PromotionGate had `None => true`,
        // skipping epsilon). Here epsilon (0.55) > acc_floor (0.52); val_acc 0.54
        // clears the floor but NOT epsilon-over-zero, so it must NOT promote.
        let strict_gate = PromotionGate {
            epsilon: 0.55,
            acc_floor: 0.52,
            min_holdout_samples: 200,
        };
        let pg = strict_gate.should_promote(0.54, 300, None);
        let canon = crate::nanochat::validate::evaluate_promotion_gate(
            Some(0.54),
            None,
            300,
            crate::nanochat::validate::PromotionGateCfg {
                epsilon: 0.55,
                acc_floor: 0.52,
                min_holdout: 200,
            },
        );
        assert_eq!(pg, canon, "PromotionGate must match the canonical gate");
        assert!(!pg, "0.54 must NOT promote when epsilon=0.55 (no None-skip)");

        // And the common sane case still promotes (no behavior change there).
        assert!(gate().should_promote(0.75, 250, None));
    }

    #[tokio::test]
    async fn write_trained_model_row_sets_live_approved_zero() {
        let pool = in_memory_pool().await;
        let req = NewTrainedModel {
            model_id: "model-01".to_string(),
            display_name: "Strat — jun12a — 0.75".to_string(),
            source_strategy_id: Some("strat-01".to_string()),
            source_strategy_name: Some("My Strategy".to_string()),
            run_tag: "jun12a".to_string(),
            checkpoint_path: "/models/jun12a/checkpoint".to_string(),
            weights_sha256: "abc123".to_string(),
            input_spec: "{\"window_bars\":64,\"indicators\":[],\"normalization\":\"zscore\"}".to_string(),
            label_strategy: "price_forward".to_string(),
            label_config: "{}".to_string(),
            best_acc: Some(0.75),
            best_loss: Some(0.3),
            holdout_samples: Some(250),
            autoresearch_run_id: Some("run-01".to_string()),
        };
        insert_trained_model(&pool, &req).await.unwrap();

        let (promoted, live_approved): (i64, i64) = sqlx::query_as(
            "SELECT promoted, live_approved FROM trained_models WHERE model_id = 'model-01'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();

        assert_eq!(promoted, 1, "should be promoted");
        assert_eq!(live_approved, 0, "live_approved must start at 0");
    }

    #[tokio::test]
    async fn concurrency_guard_rejects_second_running_run() {
        let pool = in_memory_pool().await;
        // Insert first running run.
        sqlx::query(
            "INSERT INTO autoresearch_runs
             (run_id, run_tag, label_strategy, label_config, git_branch,
              worktree_path, status, started_at)
             VALUES ('r1', 'jun12a', 'price_forward', '{}',
                     'autoresearch/jun12a', '.worktrees/autoresearch-jun12a',
                     'running', '2026-06-14T00:00:00Z')",
        )
        .execute(&pool)
        .await
        .unwrap();

        // A second 'running' row must fail due to the partial unique index.
        let err = sqlx::query(
            "INSERT INTO autoresearch_runs
             (run_id, run_tag, label_strategy, label_config, git_branch,
              worktree_path, status, started_at)
             VALUES ('r2', 'jun13a', 'price_forward', '{}',
                     'autoresearch/jun13a', '.worktrees/autoresearch-jun13a',
                     'running', '2026-06-14T00:01:00Z')",
        )
        .execute(&pool)
        .await;

        assert!(err.is_err(), "expected UNIQUE constraint violation");
    }
}
