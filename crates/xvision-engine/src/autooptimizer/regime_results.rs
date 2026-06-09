//! Storage helpers for `autooptimizer_regime_results` (Phase 2 regime matrix).
//!
//! One row per `(bundle_hash, regime_label)` captures the per-regime
//! evaluation outcome for an optimizer candidate.  The table is provisioned by
//! [`super::lineage::ensure_lineage_schema`] which is called at `ApiContext::open`.

use anyhow::{Context, Result};
use sqlx::SqlitePool;

use crate::autooptimizer::config::RegimeSide;
use crate::eval::MetricsSummary;

/// One row in `autooptimizer_regime_results`.
#[derive(Debug, Clone)]
pub struct RegimeResultRow {
    pub regime_label: String,
    pub side: RegimeSide,
    pub metrics_day: MetricsSummary,
    pub metrics_untouched: MetricsSummary,
    pub delta_sharpe: f64,
    pub verdict: String,
}

/// Insert (or replace) a slice of [`RegimeResultRow`]s for a given
/// `bundle_hash`.  Safe to call more than once — uses `INSERT OR REPLACE`.
///
/// Accepts any sqlx executor so it can be called inside a transaction:
/// pass `pool` for a standalone call, or `&mut *tx` to participate in an
/// existing transaction.
pub async fn insert_regime_results(
    executor: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    bundle_hash: &str,
    rows: &[RegimeResultRow],
    created_at: &str,
) -> Result<()> {
    for r in rows {
        let side_json = serde_json::to_string(&r.side).context("serialize side")?;
        let metrics_day_json = serde_json::to_string(&r.metrics_day).context("serialize metrics_day")?;
        let metrics_untouched_json =
            serde_json::to_string(&r.metrics_untouched).context("serialize metrics_untouched")?;

        sqlx::query(
            "INSERT OR REPLACE INTO autooptimizer_regime_results \
             (bundle_hash, regime_label, side, metrics_day_json, metrics_untouched_json, \
              delta_sharpe, verdict, created_at) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(bundle_hash)
        .bind(&r.regime_label)
        .bind(&side_json)
        .bind(&metrics_day_json)
        .bind(&metrics_untouched_json)
        .bind(r.delta_sharpe)
        .bind(&r.verdict)
        .bind(created_at)
        .execute(&mut **executor)
        .await
        .with_context(|| format!("insert regime_result {} {}", bundle_hash, r.regime_label))?;
    }
    Ok(())
}

/// Convenience wrapper: begin a transaction on `pool`, insert all regime rows,
/// then commit.  Used by callers that are not already in a transaction.
pub async fn insert_regime_results_standalone(
    pool: &SqlitePool,
    bundle_hash: &str,
    rows: &[RegimeResultRow],
    created_at: &str,
) -> Result<()> {
    let mut tx = pool.begin().await.context("begin regime_results tx")?;
    insert_regime_results(&mut tx, bundle_hash, rows, created_at).await?;
    tx.commit().await.context("commit regime_results tx")?;
    Ok(())
}

/// Load all [`RegimeResultRow`]s for a given `bundle_hash`, ordered by
/// `regime_label`.
pub async fn load_regime_results(pool: &SqlitePool, bundle_hash: &str) -> Result<Vec<RegimeResultRow>> {
    let rows = sqlx::query(
        "SELECT regime_label, side, metrics_day_json, metrics_untouched_json, \
                delta_sharpe, verdict \
         FROM autooptimizer_regime_results \
         WHERE bundle_hash = ? \
         ORDER BY regime_label",
    )
    .bind(bundle_hash)
    .fetch_all(pool)
    .await
    .with_context(|| format!("load regime_results for {bundle_hash}"))?;

    rows.into_iter()
        .map(|row| {
            use sqlx::Row;
            let regime_label: String = row.try_get("regime_label").context("regime_label")?;
            let side_json: String = row.try_get("side").context("side")?;
            let metrics_day_json: String = row.try_get("metrics_day_json").context("metrics_day_json")?;
            let metrics_untouched_json: String = row
                .try_get("metrics_untouched_json")
                .context("metrics_untouched_json")?;
            let delta_sharpe: f64 = row.try_get("delta_sharpe").context("delta_sharpe")?;
            let verdict: String = row.try_get("verdict").context("verdict")?;

            let side: RegimeSide = serde_json::from_str(&side_json).context("deserialize side")?;
            let metrics_day: MetricsSummary =
                serde_json::from_str(&metrics_day_json).context("deserialize metrics_day")?;
            let metrics_untouched: MetricsSummary =
                serde_json::from_str(&metrics_untouched_json).context("deserialize metrics_untouched")?;

            Ok(RegimeResultRow {
                regime_label,
                side,
                metrics_day,
                metrics_untouched,
                delta_sharpe,
                verdict,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::autooptimizer::lineage::ensure_lineage_schema;

    /// Provision an in-memory SQLite pool with the full lineage schema
    /// (including `autooptimizer_regime_results`), then round-trip two rows.
    #[tokio::test]
    async fn regime_results_round_trip() {
        let pool = sqlx::SqlitePool::connect("sqlite::memory:")
            .await
            .expect("open in-memory pool");

        // Provision the full lineage schema (includes autooptimizer_regime_results).
        ensure_lineage_schema(&pool).await.expect("ensure_lineage_schema");

        // Insert a parent row in lineage_nodes so the FK is satisfied.
        sqlx::query(
            "INSERT INTO lineage_nodes \
             (bundle_hash, parent_hash, gate_verdict, status, cycle_id, created_at) \
             VALUES ('hash-abc', NULL, 'pass', 'active', NULL, '2026-01-01T00:00:00Z')",
        )
        .execute(&pool)
        .await
        .expect("insert lineage_nodes row");

        let rows = vec![
            RegimeResultRow {
                regime_label: "bull_2024".to_string(),
                side: RegimeSide::Bull,
                metrics_day: MetricsSummary {
                    sharpe: 1.5,
                    total_return_pct: 12.0,
                    ..Default::default()
                },
                metrics_untouched: MetricsSummary {
                    sharpe: 0.8,
                    total_return_pct: 5.0,
                    ..Default::default()
                },
                delta_sharpe: 0.7,
                verdict: "pass".to_string(),
            },
            RegimeResultRow {
                regime_label: "bear_2022".to_string(),
                side: RegimeSide::BearOrShock,
                metrics_day: MetricsSummary {
                    sharpe: -0.3,
                    total_return_pct: -4.0,
                    ..Default::default()
                },
                metrics_untouched: MetricsSummary {
                    sharpe: 0.1,
                    total_return_pct: 0.5,
                    ..Default::default()
                },
                delta_sharpe: -0.4,
                verdict: "fail".to_string(),
            },
        ];

        insert_regime_results_standalone(&pool, "hash-abc", &rows, "2026-01-01T00:00:00Z")
            .await
            .expect("insert_regime_results");

        let loaded = load_regime_results(&pool, "hash-abc")
            .await
            .expect("load_regime_results");

        assert_eq!(loaded.len(), 2, "expected 2 rows back");

        // Ordered by regime_label alphabetically: bear_2022 < bull_2024
        assert_eq!(loaded[0].regime_label, "bear_2022");
        assert!(matches!(loaded[0].side, RegimeSide::BearOrShock));
        assert_eq!(loaded[0].verdict, "fail");
        assert!((loaded[0].delta_sharpe - (-0.4)).abs() < 1e-9);

        assert_eq!(loaded[1].regime_label, "bull_2024");
        assert!(matches!(loaded[1].side, RegimeSide::Bull));
        assert_eq!(loaded[1].verdict, "pass");
        assert!((loaded[1].metrics_day.sharpe - 1.5).abs() < 1e-9);
    }
}
