//! Persistence helpers for AR-3 evidence tables:
//! - `autooptimizer_findings` — judge qualitative findings per bundle_hash
//! - `autooptimizer_gate_records` — numeric gate scores + verdict per bundle_hash
//!
//! Both tables are provisioned by migration 058 (`migrate_autooptimizer_evidence`
//! in `api/mod.rs`), which runs at `ApiContext::open` time. All writes are
//! best-effort and log-warn on failure; they must never abort a cycle.

use anyhow::Result;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;

use crate::autooptimizer::judge::{Finding, FindingSeverity};

// ---------------------------------------------------------------------------
// Finding persistence
// ---------------------------------------------------------------------------

/// Insert one judge finding into `autooptimizer_findings`. Best-effort: the
/// caller wraps the return value in a `tracing::warn!` on `Err` and never
/// propagates.
pub async fn persist_finding(
    pool: &SqlitePool,
    bundle_hash: &str,
    finding: &Finding,
    model: Option<&str>,
) -> Result<()> {
    let severity = match finding.severity {
        FindingSeverity::Info => "info",
        FindingSeverity::Warn => "warn",
        FindingSeverity::Risk => "risk",
    };
    let created_at = Utc::now().to_rfc3339();
    sqlx::query(
        "INSERT INTO autooptimizer_findings \
         (bundle_hash, severity, code, summary, detail, model, created_at) \
         VALUES (?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(bundle_hash)
    .bind(severity)
    .bind(&finding.code)
    .bind(&finding.summary)
    .bind(finding.detail.as_deref())
    .bind(model)
    .bind(created_at)
    .execute(pool)
    .await?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Gate record persistence
// ---------------------------------------------------------------------------

/// All the numeric inputs + output of the gate for one candidate. Written
/// once per bundle_hash at gate-verdict time. `bundle_hash` is the PRIMARY KEY
/// so `INSERT OR REPLACE` is idempotent if a cycle re-derives the same candidate.
pub struct GateRecord<'a> {
    pub bundle_hash: &'a str,
    pub parent_day_score: Option<f64>,
    pub child_day_score: Option<f64>,
    pub parent_holdout_score: Option<f64>,
    pub child_holdout_score: Option<f64>,
    pub gate_epsilon: Option<f64>,
    pub holdout_epsilon: Option<f64>,
    pub delta_day: Option<f64>,
    pub delta_holdout: Option<f64>,
    pub drawdown_ratio: Option<f64>,
    /// Gate verdict string: "passed" | "rejected:<reason>"
    pub verdict: &'a str,
    /// Human-readable reason (None = pass).
    pub reason: Option<&'a str>,
    /// Experiment writer's rationale (from `MutationDiff.rationale`).
    pub rationale: Option<&'a str>,
    /// Edge metrics vs a fixed-seed random baseline (informational, never
    /// gating). `None` when the baseline run was unavailable for this cycle.
    /// `edge_over_random = child_day_score - random_baseline_score`.
    pub edge_over_random: Option<f64>,
    /// `parent_edge = parent_day_score - random_baseline_score`.
    pub parent_edge: Option<f64>,
    /// `edge_delta = edge_over_random - parent_edge`.
    pub edge_delta: Option<f64>,
    /// Parent fill-leg count (from the gate's trade-count dimension).
    pub parent_n_trades: Option<u32>,
    /// Child fill-leg count (from the gate's trade-count dimension).
    pub child_n_trades: Option<u32>,
    /// Minimum trade retention ratio applied during this gate evaluation.
    pub min_trade_retention_ratio: Option<f64>,
    pub parent_realized_return_ratio: Option<f64>,
    pub child_realized_return_ratio: Option<f64>,
    pub gate_min_realized_return_ratio: Option<f64>,
}

/// Insert or replace a gate record in `autooptimizer_gate_records`.
pub async fn persist_gate_record(pool: &SqlitePool, rec: GateRecord<'_>) -> Result<()> {
    let created_at = Utc::now().to_rfc3339();
    sqlx::query(
        "INSERT OR REPLACE INTO autooptimizer_gate_records \
         (bundle_hash, parent_day_score, child_day_score, \
          parent_holdout_score, child_holdout_score, \
          gate_epsilon, holdout_epsilon, delta_day, delta_holdout, drawdown_ratio, \
          verdict, reason, rationale, \
          edge_over_random, parent_edge, edge_delta, \
          parent_n_trades, child_n_trades, min_trade_retention_ratio, \
          parent_realized_return_ratio, child_realized_return_ratio, gate_min_realized_return_ratio, created_at) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(rec.bundle_hash)
    .bind(rec.parent_day_score)
    .bind(rec.child_day_score)
    .bind(rec.parent_holdout_score)
    .bind(rec.child_holdout_score)
    .bind(rec.gate_epsilon)
    .bind(rec.holdout_epsilon)
    .bind(rec.delta_day)
    .bind(rec.delta_holdout)
    .bind(rec.drawdown_ratio)
    .bind(rec.verdict)
    .bind(rec.reason)
    .bind(rec.rationale)
    .bind(rec.edge_over_random)
    .bind(rec.parent_edge)
    .bind(rec.edge_delta)
    .bind(rec.parent_n_trades)
    .bind(rec.child_n_trades)
    .bind(rec.min_trade_retention_ratio)
    .bind(rec.parent_realized_return_ratio)
    .bind(rec.child_realized_return_ratio)
    .bind(rec.gate_min_realized_return_ratio)
    .bind(created_at)
    .execute(pool)
    .await?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Query helpers (used by dashboard routes)
// ---------------------------------------------------------------------------

/// One row from `autooptimizer_findings` — returned by the
/// `GET /api/autooptimizer/findings/:bundle_hash` endpoint.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FindingRow {
    pub id: i64,
    pub bundle_hash: String,
    pub severity: String,
    pub code: String,
    pub summary: String,
    pub detail: Option<String>,
    pub model: Option<String>,
    pub created_at: String,
}

/// One row from `autooptimizer_gate_records`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GateRecordRow {
    pub bundle_hash: String,
    pub parent_day_score: Option<f64>,
    pub child_day_score: Option<f64>,
    pub parent_holdout_score: Option<f64>,
    pub child_holdout_score: Option<f64>,
    pub gate_epsilon: Option<f64>,
    pub holdout_epsilon: Option<f64>,
    pub delta_day: Option<f64>,
    pub delta_holdout: Option<f64>,
    pub drawdown_ratio: Option<f64>,
    pub verdict: String,
    pub reason: Option<String>,
    pub rationale: Option<String>,
    pub edge_over_random: Option<f64>,
    pub parent_edge: Option<f64>,
    pub edge_delta: Option<f64>,
    pub parent_n_trades: Option<u32>,
    pub child_n_trades: Option<u32>,
    pub min_trade_retention_ratio: Option<f64>,
    pub parent_realized_return_ratio: Option<f64>,
    pub child_realized_return_ratio: Option<f64>,
    pub gate_min_realized_return_ratio: Option<f64>,
    pub created_at: String,
}

/// Load all findings for a bundle_hash, ordered by `created_at` ascending.
pub async fn load_findings(pool: &SqlitePool, bundle_hash: &str) -> Result<Vec<FindingRow>> {
    let rows = sqlx::query(
        "SELECT id, bundle_hash, severity, code, summary, detail, model, created_at \
         FROM autooptimizer_findings WHERE bundle_hash = ? ORDER BY created_at ASC",
    )
    .bind(bundle_hash)
    .fetch_all(pool)
    .await?;

    rows.into_iter()
        .map(|row| {
            use sqlx::Row;
            Ok(FindingRow {
                id: row.try_get("id")?,
                bundle_hash: row.try_get("bundle_hash")?,
                severity: row.try_get("severity")?,
                code: row.try_get("code")?,
                summary: row.try_get("summary")?,
                detail: row.try_get("detail")?,
                model: row.try_get("model")?,
                created_at: row.try_get("created_at")?,
            })
        })
        .collect()
}
/// Load the gate record for a bundle_hash (None if not yet persisted).
pub async fn load_gate_record(pool: &SqlitePool, bundle_hash: &str) -> Result<Option<GateRecordRow>> {
    let row = sqlx::query(
        "SELECT bundle_hash, parent_day_score, child_day_score, \
         parent_holdout_score, child_holdout_score, \
         gate_epsilon, holdout_epsilon, delta_day, delta_holdout, drawdown_ratio, \
         verdict, reason, rationale, \
         edge_over_random, parent_edge, edge_delta, \
         parent_n_trades, child_n_trades, min_trade_retention_ratio, \
         parent_realized_return_ratio, child_realized_return_ratio, gate_min_realized_return_ratio, created_at \
         FROM autooptimizer_gate_records WHERE bundle_hash = ?",
    )
    .bind(bundle_hash)
    .fetch_optional(pool)
    .await?;

    let Some(row) = row else {
        return Ok(None);
    };
    use sqlx::Row;
    Ok(Some(GateRecordRow {
        bundle_hash: row.try_get("bundle_hash")?,
        parent_day_score: row.try_get("parent_day_score")?,
        child_day_score: row.try_get("child_day_score")?,
        parent_holdout_score: row.try_get("parent_holdout_score")?,
        child_holdout_score: row.try_get("child_holdout_score")?,
        gate_epsilon: row.try_get("gate_epsilon")?,
        holdout_epsilon: row.try_get("holdout_epsilon")?,
        delta_day: row.try_get("delta_day")?,
        delta_holdout: row.try_get("delta_holdout")?,
        drawdown_ratio: row.try_get("drawdown_ratio")?,
        verdict: row.try_get("verdict")?,
        reason: row.try_get("reason")?,
        rationale: row.try_get("rationale")?,
        edge_over_random: row.try_get("edge_over_random")?,
        parent_n_trades: row.try_get("parent_n_trades").ok(),
        child_n_trades: row.try_get("child_n_trades").ok(),
        min_trade_retention_ratio: row.try_get("min_trade_retention_ratio").ok(),
        parent_edge: row.try_get("parent_edge")?,
        edge_delta: row.try_get("edge_delta")?,
        created_at: row.try_get("created_at")?,
        parent_realized_return_ratio: row.try_get("parent_realized_return_ratio").ok(),
        child_realized_return_ratio: row.try_get("child_realized_return_ratio").ok(),
        gate_min_realized_return_ratio: row.try_get("gate_min_realized_return_ratio").ok(),
    }))
}

// ---------------------------------------------------------------------------
// Schema helper (used by tests that don't go through ApiContext::open)
// ---------------------------------------------------------------------------

/// Provision the evidence tables from additive evidence migrations. Idempotent
/// for fresh test databases because each migration creates or extends the table
/// once in order. Called in tests that don't go through `ApiContext::open`.
pub async fn ensure_evidence_schema(pool: &SqlitePool) -> Result<()> {
    let sql = include_str!("../../migrations/058_autooptimizer_evidence.sql");
    // The file contains multiple statements separated by semicolons; split and
    // execute each one individually (SQLite can't batch multi-statement queries
    // via plain `sqlx::query`).
    for stmt in sql.split(';').map(str::trim).filter(|s| !s.is_empty()) {
        sqlx::query(stmt).execute(pool).await?;
    }
    // Migration 061: additive edge-metric columns on the gate-records table.
    // Strip `--` comment lines before splitting so leading comments don't get
    // glued onto the first ALTER and dropped.
    let baseline_sql = include_str!("../../migrations/061_autooptimizer_random_baseline.sql");
    let baseline_sql: String = baseline_sql
        .lines()
        .filter(|l| !l.trim_start().starts_with("--"))
        .collect::<Vec<_>>()
        .join("\n");
    for stmt in baseline_sql.split(';').map(str::trim).filter(|s| !s.is_empty()) {
        sqlx::query(stmt).execute(pool).await?;
    }
    for migration_sql in [
        include_str!("../../migrations/075_autooptimizer_gate_trade_counts.sql"),
        include_str!("../../migrations/076_autooptimizer_gate_realized_return.sql"),
    ] {
        let migration_sql: String = migration_sql
            .lines()
            .filter(|l| !l.trim_start().starts_with("--"))
            .collect::<Vec<_>>()
            .join("\n");
        for stmt in migration_sql.split(';').map(str::trim).filter(|s| !s.is_empty()) {
            sqlx::query(stmt).execute(pool).await?;
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::autooptimizer::judge::{Finding, FindingSeverity};
    use crate::autooptimizer::lineage::ensure_lineage_schema;

    async fn open_pool() -> SqlitePool {
        let pool = sqlx::SqlitePool::connect("sqlite::memory:")
            .await
            .expect("open in-memory pool");
        ensure_lineage_schema(&pool).await.expect("ensure_lineage_schema");
        ensure_evidence_schema(&pool)
            .await
            .expect("ensure_evidence_schema");
        pool
    }

    // ─── test_finding_persisted ───────────────────────────────────────────────

    /// After persisting a Finding, SELECT from autooptimizer_findings returns
    /// the row with matching code, severity, summary and detail.
    #[tokio::test]
    async fn test_finding_persisted() {
        let pool = open_pool().await;
        let hash = "deadbeef01234567deadbeef01234567deadbeef01234567deadbeef01234567";

        let finding = Finding {
            code: "over_leveraged".into(),
            severity: FindingSeverity::Risk,
            summary: "Leverage exceeds 10x".into(),
            detail: Some("Detected leverage of 12x vs recommended ceiling 10x".into()),
        };

        persist_finding(&pool, hash, &finding, Some("anthropic/claude-3-5-sonnet"))
            .await
            .expect("persist_finding must not error");

        let rows = load_findings(&pool, hash).await.expect("load_findings");
        assert_eq!(rows.len(), 1, "expected exactly 1 finding row");
        let row = &rows[0];
        assert_eq!(row.bundle_hash, hash);
        assert_eq!(row.severity, "risk");
        assert_eq!(row.code, "over_leveraged");
        assert_eq!(row.summary, "Leverage exceeds 10x");
        assert_eq!(
            row.detail.as_deref(),
            Some("Detected leverage of 12x vs recommended ceiling 10x")
        );
        assert_eq!(row.model.as_deref(), Some("anthropic/claude-3-5-sonnet"));
    }

    /// Multiple findings for the same bundle_hash all appear, ordered by
    /// created_at. (Findings use AUTOINCREMENT id, not INSERT OR REPLACE, so
    /// each call appends a new row.)
    #[tokio::test]
    async fn test_multiple_findings_persisted() {
        let pool = open_pool().await;
        let hash = "aaaa0000aaaa0000aaaa0000aaaa0000aaaa0000aaaa0000aaaa0000aaaa0000";

        for (code, sev) in [("f1", FindingSeverity::Info), ("f2", FindingSeverity::Warn)] {
            persist_finding(
                &pool,
                hash,
                &Finding {
                    code: code.into(),
                    severity: sev,
                    summary: format!("summary for {code}"),
                    detail: None,
                },
                None,
            )
            .await
            .expect("persist_finding");
        }

        let rows = load_findings(&pool, hash).await.expect("load_findings");
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].code, "f1");
        assert_eq!(rows[1].code, "f2");
    }

    /// Empty result when no findings were written for a hash.
    #[tokio::test]
    async fn test_findings_empty_for_unknown_hash() {
        let pool = open_pool().await;
        let rows = load_findings(&pool, "unknown_hash").await.expect("load_findings");
        assert!(rows.is_empty());
    }

    // ─── test_gate_record_persisted ──────────────────────────────────────────

    /// After persisting a GateRecord, SELECT from autooptimizer_gate_records
    /// returns the row with matching scores and rationale.
    #[tokio::test]
    async fn test_gate_record_persisted() {
        let pool = open_pool().await;
        let hash = "bbbb1111bbbb1111bbbb1111bbbb1111bbbb1111bbbb1111bbbb1111bbbb1111";

        persist_gate_record(
            &pool,
            GateRecord {
                bundle_hash: hash,
                parent_day_score: Some(1.5),
                child_day_score: Some(1.8),
                parent_holdout_score: Some(0.9),
                child_holdout_score: Some(1.1),
                gate_epsilon: Some(0.05),
                holdout_epsilon: Some(0.005),
                delta_day: Some(0.3),
                delta_holdout: Some(0.2),
                drawdown_ratio: Some(1.1),
                verdict: "passed",
                reason: None,
                rationale: Some("Adjusted ADX threshold from 25 to 30 for stronger trend filter"),
                edge_over_random: Some(0.4),
                parent_edge: Some(0.1),
                edge_delta: Some(0.3),
                parent_n_trades: Some(12),
                child_n_trades: Some(8),
                min_trade_retention_ratio: Some(0.5),
                parent_realized_return_ratio: Some(0.6),
                child_realized_return_ratio: Some(0.4),
                gate_min_realized_return_ratio: Some(0.25),
            },
        )
        .await
        .expect("persist_gate_record must not error");

        let rec = load_gate_record(&pool, hash)
            .await
            .expect("load_gate_record")
            .expect("record should exist");

        assert_eq!(rec.bundle_hash, hash);
        assert!((rec.parent_day_score.unwrap() - 1.5).abs() < 1e-9);
        assert!((rec.child_day_score.unwrap() - 1.8).abs() < 1e-9);
        assert!((rec.delta_day.unwrap() - 0.3).abs() < 1e-9);
        assert!((rec.delta_holdout.unwrap() - 0.2).abs() < 1e-9);
        assert!((rec.holdout_epsilon.unwrap() - 0.005).abs() < 1e-9);
        assert_eq!(rec.verdict, "passed");
        assert!(rec.reason.is_none());
        assert_eq!(
            rec.rationale.as_deref(),
            Some("Adjusted ADX threshold from 25 to 30 for stronger trend filter")
        );
        // Random-baseline edge metrics round-trip through migration 061's columns.
        assert!((rec.edge_over_random.unwrap() - 0.4).abs() < 1e-9);
        assert!((rec.parent_edge.unwrap() - 0.1).abs() < 1e-9);
        assert!((rec.edge_delta.unwrap() - 0.3).abs() < 1e-9);
        // Gate-dimension evidence round-trips through migrations 075/076.
        assert_eq!(rec.parent_n_trades, Some(12));
        assert_eq!(rec.child_n_trades, Some(8));
        assert!((rec.min_trade_retention_ratio.unwrap() - 0.5).abs() < 1e-9);
        assert!((rec.parent_realized_return_ratio.unwrap() - 0.6).abs() < 1e-9);
        assert!((rec.child_realized_return_ratio.unwrap() - 0.4).abs() < 1e-9);
        assert!((rec.gate_min_realized_return_ratio.unwrap() - 0.25).abs() < 1e-9);
    }

    /// INSERT OR REPLACE: re-persisting the same bundle_hash with a new verdict
    /// overwrites the prior row (idempotent, no dup-pk error).
    #[tokio::test]
    async fn test_gate_record_replace_is_idempotent() {
        let pool = open_pool().await;
        let hash = "cccc2222cccc2222cccc2222cccc2222cccc2222cccc2222cccc2222cccc2222";

        for verdict in ["passed", "rejected:delta below threshold"] {
            persist_gate_record(
                &pool,
                GateRecord {
                    bundle_hash: hash,
                    parent_day_score: None,
                    child_day_score: None,
                    parent_holdout_score: None,
                    child_holdout_score: None,
                    gate_epsilon: None,
                    holdout_epsilon: None,
                    delta_day: None,
                    delta_holdout: None,
                    drawdown_ratio: None,
                    verdict,
                    reason: None,
                    rationale: None,
                    edge_over_random: None,
                    parent_edge: None,
                    edge_delta: None,
                    parent_n_trades: None,
                    child_n_trades: None,
                    min_trade_retention_ratio: None,
                    parent_realized_return_ratio: None,
                    child_realized_return_ratio: None,
                    gate_min_realized_return_ratio: None,
                },
            )
            .await
            .expect("persist_gate_record");
        }

        let rec = load_gate_record(&pool, hash)
            .await
            .expect("load_gate_record")
            .expect("record should exist");
        // The last write wins.
        assert!(rec.verdict.starts_with("rejected"));
    }

    /// None when no gate record exists for a hash.
    #[tokio::test]
    async fn test_gate_record_none_for_unknown_hash() {
        let pool = open_pool().await;
        let rec = load_gate_record(&pool, "nonexistent_hash")
            .await
            .expect("load_gate_record");
        assert!(rec.is_none());
    }
}
