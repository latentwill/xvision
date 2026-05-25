//! Holdout discipline + overfit detection (Phase 4.4).
//!
//! Two concerns live here:
//!
//! 1. **The holdout store** — durable persistence of one paired
//!    `(train_metric_value, holdout_metric_value)` result per optimization
//!    snapshot, in `optimization_holdout_results` (migration 046). The values
//!    are scalars produced by the eval harness (CLI side); the engine persists
//!    + reads them and does NOT depend on `xvision-dspy`.
//!
//! 2. **The accept gate** — pure functions that decide whether a snapshot may be
//!    accepted (promoted into a child agent) and whether overfit blocks a
//!    marketplace mint. The DISCIPLINE is the product: a candidate cannot be
//!    accepted WITHOUT a holdout result unless a documented `override_reason` is
//!    recorded; an overfit warning (train ≫ holdout beyond a threshold) blocks
//!    marketplace minting unless waived with a recorded reason.
//!
//! Overfit detection is a single relative statistic — `(train - holdout) /
//! |train|` — compared against [`DEFAULT_OVERFIT_THRESHOLD`]. It is intentionally
//! direction-aware: a holdout result *better* than train never trips the warning.

use chrono::Utc;
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use thiserror::Error;

use super::metrics::{missing_metrics, required_metrics};

/// Default relative-drop threshold above which a train/holdout gap is flagged as
/// overfit. `0.30` ⇒ a holdout metric more than 30% below the train metric (as a
/// fraction of the train magnitude) trips the warning. Deterministic + tunable
/// by the caller via [`OverfitConfig`].
pub const DEFAULT_OVERFIT_THRESHOLD: f64 = 0.30;

/// Configuration for overfit detection. `threshold` is the relative drop above
/// which the warning trips. `Default` uses [`DEFAULT_OVERFIT_THRESHOLD`].
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct OverfitConfig {
    pub threshold: f64,
}

impl Default for OverfitConfig {
    fn default() -> Self {
        Self {
            threshold: DEFAULT_OVERFIT_THRESHOLD,
        }
    }
}

/// A persisted holdout result for one snapshot.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
// NB: no `Eq` — `f64` fields don't implement it.
pub struct HoldoutResult {
    pub snapshot_id: String,
    pub run_id: String,
    /// Metric name measured (mirrors the run's objective metric).
    pub metric: String,
    pub train_metric_value: f64,
    pub holdout_metric_value: f64,
    /// `true` ⇒ train ≫ holdout beyond the configured threshold.
    pub overfit_warning: bool,
    /// The detection statistic `(train - holdout) / |train|`; `None` when train
    /// is zero (ratio undefined — no warning).
    pub overfit_ratio: Option<f64>,
    /// Recorded rationale lifting the overfit mint-block; `None` ⇒ not waived.
    pub overfit_waiver_reason: Option<String>,
    pub created_at: String,
}

/// Request to record a holdout result. The overfit verdict is computed by the
/// store from the paired values + [`OverfitConfig`] — the caller never asserts
/// the warning directly.
#[derive(Clone, Debug)]
pub struct NewHoldoutResult {
    pub snapshot_id: String,
    pub run_id: String,
    pub metric: String,
    pub train_metric_value: f64,
    pub holdout_metric_value: f64,
}

/// Typed holdout errors. Distinct from `ApiError` so the gate logic stays pure
/// and testable; the dashboard maps these to HTTP.
#[derive(Debug, Error)]
pub enum HoldoutError {
    #[error("holdout result not found for snapshot {0}")]
    NotFound(String),
    #[error("database: {0}")]
    Db(#[from] sqlx::Error),
}

/// Compute the overfit statistic + verdict from a paired train/holdout value.
///
/// Returns `(overfit_warning, overfit_ratio)`. The ratio is `(train - holdout) /
/// |train|`; it is `None` when `train == 0.0` (undefined — never a warning). A
/// holdout value at-or-above train yields a non-positive ratio and never trips.
pub fn detect_overfit(train: f64, holdout: f64, cfg: OverfitConfig) -> (bool, Option<f64>) {
    if train == 0.0 {
        return (false, None);
    }
    let ratio = (train - holdout) / train.abs();
    (ratio > cfg.threshold, Some(ratio))
}

/// CRUD over `optimization_holdout_results`. Thin wrapper around a `SqlitePool`.
#[derive(Clone)]
pub struct HoldoutStore {
    pool: SqlitePool,
    cfg: OverfitConfig,
}

impl HoldoutStore {
    /// Build a store with the default overfit threshold.
    pub fn new(pool: SqlitePool) -> Self {
        Self {
            pool,
            cfg: OverfitConfig::default(),
        }
    }

    /// Build a store with an explicit overfit threshold.
    pub fn with_config(pool: SqlitePool, cfg: OverfitConfig) -> Self {
        Self { pool, cfg }
    }

    /// Record (or replace) the holdout result for a snapshot. The overfit verdict
    /// is computed here from the paired values. `INSERT OR REPLACE` so re-running
    /// the eval against a snapshot updates the row (resetting any prior waiver —
    /// a fresh measurement must be re-waived).
    pub async fn record(&self, req: NewHoldoutResult) -> Result<HoldoutResult, HoldoutError> {
        let (overfit_warning, overfit_ratio) =
            detect_overfit(req.train_metric_value, req.holdout_metric_value, self.cfg);
        let created_at = Utc::now().to_rfc3339();
        sqlx::query(
            "INSERT OR REPLACE INTO optimization_holdout_results \
             (snapshot_id, run_id, metric, train_metric_value, holdout_metric_value, \
              overfit_warning, overfit_ratio, overfit_waiver_reason, created_at) \
             VALUES (?, ?, ?, ?, ?, ?, ?, NULL, ?)",
        )
        .bind(&req.snapshot_id)
        .bind(&req.run_id)
        .bind(&req.metric)
        .bind(req.train_metric_value)
        .bind(req.holdout_metric_value)
        .bind(overfit_warning as i64)
        .bind(overfit_ratio)
        .bind(&created_at)
        .execute(&self.pool)
        .await?;
        Ok(HoldoutResult {
            snapshot_id: req.snapshot_id,
            run_id: req.run_id,
            metric: req.metric,
            train_metric_value: req.train_metric_value,
            holdout_metric_value: req.holdout_metric_value,
            overfit_warning,
            overfit_ratio,
            overfit_waiver_reason: None,
            created_at,
        })
    }

    /// Fetch a snapshot's holdout result, if one was recorded.
    pub async fn get(&self, snapshot_id: &str) -> Result<Option<HoldoutResult>, HoldoutError> {
        let row: Option<HoldoutRow> = sqlx::query_as(
            "SELECT snapshot_id, run_id, metric, train_metric_value, holdout_metric_value, \
                    overfit_warning, overfit_ratio, overfit_waiver_reason, created_at \
             FROM optimization_holdout_results WHERE snapshot_id = ?",
        )
        .bind(snapshot_id)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(Into::into))
    }

    /// Record a waiver reason lifting the overfit mint-block for a snapshot.
    /// `NotFound` if no holdout result exists. The reason must be non-empty —
    /// the caller is responsible for that check (the gate enforces it).
    pub async fn waive_overfit(
        &self,
        snapshot_id: &str,
        reason: &str,
    ) -> Result<HoldoutResult, HoldoutError> {
        let res = sqlx::query(
            "UPDATE optimization_holdout_results SET overfit_waiver_reason = ? \
             WHERE snapshot_id = ?",
        )
        .bind(reason)
        .bind(snapshot_id)
        .execute(&self.pool)
        .await?;
        if res.rows_affected() == 0 {
            return Err(HoldoutError::NotFound(snapshot_id.to_string()));
        }
        self.get(snapshot_id)
            .await?
            .ok_or_else(|| HoldoutError::NotFound(snapshot_id.to_string()))
    }

    /// The overfit threshold this store applies.
    pub fn config(&self) -> OverfitConfig {
        self.cfg
    }
}

/// Convenience: the required-metric gap for a capability against a proof's
/// metric set, re-exported from [`super::metrics`] so callers can stay in the
/// `holdout` module.
pub fn metric_coverage_gap(capability: &str, provided: &[String]) -> Vec<&'static str> {
    missing_metrics(capability, provided)
}

/// The required-metric set for a capability (re-export convenience).
pub fn capability_required_metrics(capability: &str) -> &'static [&'static str] {
    required_metrics(capability)
}

// ── sqlx row shim ───────────────────────────────────────────────────────────

#[derive(sqlx::FromRow)]
struct HoldoutRow {
    snapshot_id: String,
    run_id: String,
    metric: String,
    train_metric_value: f64,
    holdout_metric_value: f64,
    overfit_warning: i64,
    overfit_ratio: Option<f64>,
    overfit_waiver_reason: Option<String>,
    created_at: String,
}

impl From<HoldoutRow> for HoldoutResult {
    fn from(r: HoldoutRow) -> Self {
        HoldoutResult {
            snapshot_id: r.snapshot_id,
            run_id: r.run_id,
            metric: r.metric,
            train_metric_value: r.train_metric_value,
            holdout_metric_value: r.holdout_metric_value,
            overfit_warning: r.overfit_warning != 0,
            overfit_ratio: r.overfit_ratio,
            overfit_waiver_reason: r.overfit_waiver_reason,
            created_at: r.created_at,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::SqlitePool;

    /// Apply migrations 045 (optimization store) then 046 (holdout) to a fresh
    /// in-memory pool, mirroring the `ApiContext::open` ordering. Statement-split
    /// the same way the runtime migrate helpers do.
    async fn fresh_pool() -> SqlitePool {
        let pool = SqlitePool::connect(":memory:").await.unwrap();
        for sql in [
            include_str!("../../migrations/045_optimization_store.sql"),
            include_str!("../../migrations/046_holdout.sql"),
        ] {
            for stmt in split_statements(sql) {
                sqlx::query(&stmt).execute(&pool).await.unwrap();
            }
        }
        pool
    }

    fn split_statements(sql: &str) -> Vec<String> {
        let without_comments: String = sql
            .lines()
            .map(|line| match line.find("--") {
                Some(idx) => &line[..idx],
                None => line,
            })
            .collect::<Vec<_>>()
            .join("\n");
        without_comments
            .split(';')
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string())
            .collect()
    }

    /// Seed a run + snapshot so the holdout result's FKs resolve.
    async fn seed_snapshot(pool: &SqlitePool, run_id: &str, snapshot_id: &str) {
        sqlx::query(
            "INSERT INTO optimization_runs \
             (id, agent_id, slot_name, capability, optimizer, metric, corpus_query, rng_seed, status, created_at) \
             VALUES (?, 'agentP', 'trader', 'trader', 'mipro', 'sharpe', 'q', 42, 'completed', '2026-05-24T00:00:00Z')",
        )
        .bind(run_id)
        .execute(pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO optimization_snapshots \
             (id, run_id, snapshot_json, signature_hash, accepted, created_at) \
             VALUES (?, ?, '{}', 'sig', 0, '2026-05-24T00:00:00Z')",
        )
        .bind(snapshot_id)
        .bind(run_id)
        .execute(pool)
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn record_persists_and_computes_overfit() {
        let pool = fresh_pool().await;
        seed_snapshot(&pool, "run1", "snap1").await;
        let store = HoldoutStore::new(pool);

        let res = store
            .record(NewHoldoutResult {
                snapshot_id: "snap1".into(),
                run_id: "run1".into(),
                metric: "sharpe".into(),
                train_metric_value: 1.0,
                holdout_metric_value: 0.4, // ratio 0.6 > 0.30 → overfit
            })
            .await
            .unwrap();
        assert!(res.overfit_warning);
        assert!(res.overfit_waiver_reason.is_none());

        // Round-trips out of the DB.
        let got = store.get("snap1").await.unwrap().unwrap();
        assert_eq!(got, res);
        assert!(store.get("nope").await.unwrap().is_none());
    }

    #[tokio::test]
    async fn waive_overfit_records_reason() {
        let pool = fresh_pool().await;
        seed_snapshot(&pool, "run1", "snap1").await;
        let store = HoldoutStore::new(pool);
        store
            .record(NewHoldoutResult {
                snapshot_id: "snap1".into(),
                run_id: "run1".into(),
                metric: "sharpe".into(),
                train_metric_value: 1.0,
                holdout_metric_value: 0.4,
            })
            .await
            .unwrap();

        let waived = store
            .waive_overfit("snap1", "reviewed by quant lead; acceptable")
            .await
            .unwrap();
        assert_eq!(
            waived.overfit_waiver_reason.as_deref(),
            Some("reviewed by quant lead; acceptable")
        );

        // Waiving an unknown snapshot is typed NotFound.
        let err = store.waive_overfit("ghost", "x").await.unwrap_err();
        matches!(err, HoldoutError::NotFound(_));
    }

    #[tokio::test]
    async fn record_replaces_and_resets_waiver() {
        let pool = fresh_pool().await;
        seed_snapshot(&pool, "run1", "snap1").await;
        let store = HoldoutStore::new(pool);
        store
            .record(NewHoldoutResult {
                snapshot_id: "snap1".into(),
                run_id: "run1".into(),
                metric: "sharpe".into(),
                train_metric_value: 1.0,
                holdout_metric_value: 0.4,
            })
            .await
            .unwrap();
        store.waive_overfit("snap1", "ok").await.unwrap();

        // Re-recording a fresh measurement resets the waiver — it must be
        // re-justified against the new numbers.
        let re = store
            .record(NewHoldoutResult {
                snapshot_id: "snap1".into(),
                run_id: "run1".into(),
                metric: "sharpe".into(),
                train_metric_value: 1.0,
                holdout_metric_value: 0.95, // now in-sample-aligned → no overfit
            })
            .await
            .unwrap();
        assert!(!re.overfit_warning);
        assert!(re.overfit_waiver_reason.is_none());
    }

    #[test]
    fn detect_overfit_trips_above_threshold() {
        let cfg = OverfitConfig::default(); // 0.30
                                            // train 1.0, holdout 0.5 → ratio 0.5 > 0.30 → warning.
        let (warn, ratio) = detect_overfit(1.0, 0.5, cfg);
        assert!(warn);
        assert!((ratio.unwrap() - 0.5).abs() < 1e-9);
    }

    #[test]
    fn detect_overfit_silent_within_threshold() {
        let cfg = OverfitConfig::default();
        // train 1.0, holdout 0.8 → ratio 0.2 < 0.30 → no warning.
        let (warn, ratio) = detect_overfit(1.0, 0.8, cfg);
        assert!(!warn);
        assert!((ratio.unwrap() - 0.2).abs() < 1e-9);
    }

    #[test]
    fn detect_overfit_holdout_better_never_trips() {
        let cfg = OverfitConfig::default();
        let (warn, ratio) = detect_overfit(1.0, 1.5, cfg);
        assert!(!warn);
        assert!(ratio.unwrap() < 0.0);
    }

    #[test]
    fn detect_overfit_zero_train_is_undefined() {
        let (warn, ratio) = detect_overfit(0.0, -5.0, OverfitConfig::default());
        assert!(!warn);
        assert!(ratio.is_none());
    }

    #[test]
    fn detect_overfit_custom_threshold() {
        let cfg = OverfitConfig { threshold: 0.10 };
        // ratio 0.2 > 0.10 → warning under the stricter threshold.
        let (warn, _) = detect_overfit(1.0, 0.8, cfg);
        assert!(warn);
    }
}
