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

/// A single cycle plus every lineage node it produced.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CycleRunDetail {
    #[serde(flatten)]
    pub summary: CycleRunSummary,
    pub nodes: Vec<LineageNode>,
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
    Ok(Some(CycleRunDetail { summary, nodes }))
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
