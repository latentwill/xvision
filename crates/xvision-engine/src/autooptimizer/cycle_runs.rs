//! Cycle runs — a first-class "historic run" view over the lineage graph.
//!
//! F13/F19 (QA 2026-06-04): a completed `run-cycle` writes its candidates to
//! `lineage_nodes` (keyed by `cycle_id`) but never to the memory-distillation
//! `autooptimizer_runs` ledger that `xvn optimizer ls`/`inspect` and
//! `GET /api/autooptimizer` read. So after a real cycle those run-oriented
//! surfaces were empty/404 even though the genealogy surface showed the cycle.
//!
//! Rather than overload the distillation ledger (a genuine semantic mismatch —
//! see commit c162135a), this module derives the run list/detail directly from
//! the lineage nodes a cycle produced: one [`CycleRunSummary`] per distinct
//! `cycle_id`, with per-cycle node counts and time bounds, and a
//! [`CycleRunDetail`] carrying every node (gate verdict, status, parent/child
//! hash, diversity) so a panel or the CLI can open a cycle as a historic run.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use sqlx::{Row, SqlitePool};

use super::lineage::{row_to_node, LineageNode, SELECT_COLS_PREFIX};
use crate::eval::run::MetricsSummary;

/// One completed (or in-progress) optimizer cycle, aggregated from the lineage
/// nodes that share its `cycle_id`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CycleRunSummary {
    pub cycle_id: String,
    /// Total lineage nodes recorded for this cycle (candidates gated).
    pub node_count: i64,
    /// Nodes that passed the gate (kept).
    pub active_count: i64,
    /// Nodes that failed the gate (dropped).
    pub rejected_count: i64,
    /// RFC-3339 timestamp of the earliest node in the cycle.
    pub first_created_at: String,
    /// RFC-3339 timestamp of the latest node in the cycle.
    pub last_created_at: String,
}

/// Mutator provenance for a candidate (from `mutator_attribution`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeProvenance {
    pub provider: String,
    pub model: String,
    pub prompt_version: String,
    pub delta_sharpe: Option<f64>,
}

/// One lineage node enriched with the per-candidate detail a historic-run view
/// needs: backtest metrics on both windows and mutator provenance (F13). The
/// candidate strategy itself is fetched via `GET /api/autooptimizer/blob/:hash`
/// keyed on the node's `bundle_hash` (its parent via `parent_hash`), which is
/// how the run-detail surfaces the candidate diff.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CycleNodeDetail {
    #[serde(flatten)]
    pub node: LineageNode,
    pub metrics_day: Option<MetricsSummary>,
    pub metrics_untouched: Option<MetricsSummary>,
    pub provenance: Option<NodeProvenance>,
}

/// The per-cycle honesty-check (canary) outcome (from `cycle_honesty_checks`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HonestyCheckRecord {
    pub passed: bool,
    pub sabotage_variant: String,
    pub message: String,
    pub gate_verdict: String,
    pub parent_hash: String,
    pub created_at: String,
}

/// A single cycle plus every candidate it produced (with metrics + provenance)
/// and its honesty-check outcome.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CycleRunDetail {
    #[serde(flatten)]
    pub summary: CycleRunSummary,
    pub nodes: Vec<CycleNodeDetail>,
    pub honesty_check: Option<HonestyCheckRecord>,
}

/// List completed cycles, most-recent first, paginated. Cycles with a NULL
/// `cycle_id` (seeded root strategies that were never run) are excluded.
pub async fn list_cycle_runs(pool: &SqlitePool, limit: i64, offset: i64) -> Result<Vec<CycleRunSummary>> {
    let rows = sqlx::query(
        "SELECT cycle_id, \
                COUNT(*) AS node_count, \
                SUM(CASE WHEN status = 'active' THEN 1 ELSE 0 END) AS active_count, \
                SUM(CASE WHEN status = 'rejected' THEN 1 ELSE 0 END) AS rejected_count, \
                MIN(created_at) AS first_created_at, \
                MAX(created_at) AS last_created_at \
         FROM lineage_nodes \
         WHERE cycle_id IS NOT NULL \
         GROUP BY cycle_id \
         ORDER BY last_created_at DESC \
         LIMIT ? OFFSET ?",
    )
    .bind(limit)
    .bind(offset)
    .fetch_all(pool)
    .await
    .context("list_cycle_runs query")?;
    rows.into_iter().map(row_to_cycle_summary).collect()
}

/// Fetch one cycle's summary + all of its nodes (ordered oldest-first), or
/// `None` when no node carries that `cycle_id`.
pub async fn get_cycle_run(pool: &SqlitePool, cycle_id: &str) -> Result<Option<CycleRunDetail>> {
    let node_rows = sqlx::query(&format!(
        "{SELECT_COLS_PREFIX} WHERE cycle_id = ? ORDER BY created_at ASC"
    ))
    .bind(cycle_id)
    .fetch_all(pool)
    .await
    .context("get_cycle_run nodes query")?;
    if node_rows.is_empty() {
        return Ok(None);
    }
    let nodes: Vec<LineageNode> = node_rows.into_iter().map(row_to_node).collect::<Result<_>>()?;

    let active_count = nodes
        .iter()
        .filter(|n| matches!(n.status, super::lineage::LineageStatus::Active))
        .count() as i64;
    let node_count = nodes.len() as i64;
    let summary = CycleRunSummary {
        cycle_id: cycle_id.to_string(),
        node_count,
        active_count,
        rejected_count: node_count - active_count,
        first_created_at: nodes
            .first()
            .map(|n| n.created_at.to_rfc3339())
            .unwrap_or_default(),
        last_created_at: nodes
            .last()
            .map(|n| n.created_at.to_rfc3339())
            .unwrap_or_default(),
    };

    // Enrich each node with its persisted metrics + mutator provenance
    // (best-effort: a node predating the F13 side tables simply has `None`).
    let mut detailed = Vec::with_capacity(nodes.len());
    for node in nodes {
        let hash = node.bundle_hash.to_hex();
        let (metrics_day, metrics_untouched) = load_node_metrics(pool, &hash).await;
        let provenance = load_node_provenance(pool, &hash).await;
        detailed.push(CycleNodeDetail {
            node,
            metrics_day,
            metrics_untouched,
            provenance,
        });
    }

    let honesty_check = load_honesty_check(pool, cycle_id).await;

    Ok(Some(CycleRunDetail {
        summary,
        nodes: detailed,
        honesty_check,
    }))
}

async fn load_node_metrics(
    pool: &SqlitePool,
    bundle_hash: &str,
) -> (Option<MetricsSummary>, Option<MetricsSummary>) {
    let row = sqlx::query(
        "SELECT metrics_day_json, metrics_untouched_json FROM lineage_node_metrics WHERE bundle_hash = ?",
    )
    .bind(bundle_hash)
    .fetch_optional(pool)
    .await
    .ok()
    .flatten();
    let Some(row) = row else {
        return (None, None);
    };
    let day = row
        .try_get::<String, _>("metrics_day_json")
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok());
    let untouched = row
        .try_get::<String, _>("metrics_untouched_json")
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok());
    (day, untouched)
}

async fn load_node_provenance(pool: &SqlitePool, bundle_hash: &str) -> Option<NodeProvenance> {
    let row = sqlx::query(
        "SELECT provider, model, prompt_version, delta_sharpe \
         FROM mutator_attribution WHERE bundle_hash = ?",
    )
    .bind(bundle_hash)
    .fetch_optional(pool)
    .await
    .ok()
    .flatten()?;
    Some(NodeProvenance {
        provider: row.try_get("provider").ok()?,
        model: row.try_get("model").ok()?,
        prompt_version: row.try_get("prompt_version").ok()?,
        delta_sharpe: row.try_get("delta_sharpe").ok(),
    })
}

async fn load_honesty_check(pool: &SqlitePool, cycle_id: &str) -> Option<HonestyCheckRecord> {
    let row = sqlx::query(
        "SELECT passed, sabotage_variant, message, gate_verdict, parent_hash, created_at \
         FROM cycle_honesty_checks WHERE cycle_id = ?",
    )
    .bind(cycle_id)
    .fetch_optional(pool)
    .await
    .ok()
    .flatten()?;
    Some(HonestyCheckRecord {
        passed: row.try_get::<i64, _>("passed").ok()? != 0,
        sabotage_variant: row.try_get("sabotage_variant").ok()?,
        message: row.try_get("message").ok()?,
        gate_verdict: row.try_get("gate_verdict").ok()?,
        parent_hash: row.try_get("parent_hash").ok()?,
        created_at: row.try_get("created_at").ok()?,
    })
}

fn row_to_cycle_summary(row: sqlx::sqlite::SqliteRow) -> Result<CycleRunSummary> {
    Ok(CycleRunSummary {
        cycle_id: row.try_get("cycle_id").context("cycle_id")?,
        node_count: row.try_get("node_count").context("node_count")?,
        active_count: row.try_get("active_count").context("active_count")?,
        rejected_count: row.try_get("rejected_count").context("rejected_count")?,
        first_created_at: row.try_get("first_created_at").context("first_created_at")?,
        last_created_at: row.try_get("last_created_at").context("last_created_at")?,
    })
}
