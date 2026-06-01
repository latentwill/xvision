use anyhow::{bail, Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{sqlite::SqliteRow, Row, SqlitePool};

use super::content_hash::ContentHash;
use super::gate::GateVerdict;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LineageNode {
    pub bundle_hash: ContentHash,
    pub parent_hash: Option<ContentHash>,
    pub diff_hash: Option<ContentHash>,
    pub metrics_day_hash: Option<ContentHash>,
    pub metrics_untouched_hash: Option<ContentHash>,
    pub gate_verdict: GateVerdict,
    pub status: LineageStatus,
    pub cycle_id: Option<String>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LineageStatus {
    Active,
    Rejected,
}

impl LineageStatus {
    fn as_str(&self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Rejected => "rejected",
        }
    }

    fn from_str(s: &str) -> Result<Self> {
        match s {
            "active" => Ok(Self::Active),
            "rejected" => Ok(Self::Rejected),
            _ => bail!("unknown LineageStatus: {s}"),
        }
    }
}

pub struct LineageStore {
    pub pool: SqlitePool,
}

impl LineageStore {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    pub async fn insert(&self, node: &LineageNode) -> Result<()> {
        sqlx::query(
            "INSERT OR REPLACE INTO lineage_nodes \
             (bundle_hash, parent_hash, diff_hash, metrics_day_hash, \
              metrics_untouched_hash, gate_verdict, status, cycle_id, created_at) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(node.bundle_hash.to_hex())
        .bind(node.parent_hash.as_ref().map(|h| h.to_hex()))
        .bind(node.diff_hash.as_ref().map(|h| h.to_hex()))
        .bind(node.metrics_day_hash.as_ref().map(|h| h.to_hex()))
        .bind(node.metrics_untouched_hash.as_ref().map(|h| h.to_hex()))
        .bind(node.gate_verdict.as_str())
        .bind(node.status.as_str())
        .bind(&node.cycle_id)
        .bind(node.created_at.to_rfc3339())
        .execute(&self.pool)
        .await
        .context("insert lineage_node")?;
        Ok(())
    }

    pub async fn get(&self, bundle_hash: &ContentHash) -> Result<Option<LineageNode>> {
        let row = sqlx::query(SELECT_COLS)
            .bind(bundle_hash.to_hex())
            .fetch_optional(&self.pool)
            .await
            .context("get lineage_node")?;
        row.map(row_to_node).transpose()
    }

    pub async fn children_of(&self, parent_hash: &ContentHash) -> Result<Vec<LineageNode>> {
        let rows = sqlx::query(
            "SELECT bundle_hash, parent_hash, diff_hash, metrics_day_hash, \
             metrics_untouched_hash, gate_verdict, status, cycle_id, created_at \
             FROM lineage_nodes WHERE parent_hash = ? ORDER BY created_at",
        )
        .bind(parent_hash.to_hex())
        .fetch_all(&self.pool)
        .await
        .context("children_of lineage_node")?;
        rows.into_iter().map(row_to_node).collect()
    }

    pub async fn active_leaves(&self) -> Result<Vec<LineageNode>> {
        let rows = sqlx::query(
            "SELECT bundle_hash, parent_hash, diff_hash, metrics_day_hash, \
             metrics_untouched_hash, gate_verdict, status, cycle_id, created_at \
             FROM lineage_nodes n \
             WHERE n.status = 'active' \
               AND NOT EXISTS ( \
                 SELECT 1 FROM lineage_nodes c \
                 WHERE c.parent_hash = n.bundle_hash AND c.status = 'active' \
               )",
        )
        .fetch_all(&self.pool)
        .await
        .context("active_leaves")?;
        rows.into_iter().map(row_to_node).collect()
    }

    pub async fn merkle_root_for_cycle(&self, cycle_id: &str) -> Result<ContentHash> {
        let rows = sqlx::query(
            "SELECT bundle_hash, parent_hash, diff_hash, metrics_day_hash, \
             metrics_untouched_hash, gate_verdict, status, cycle_id, created_at \
             FROM lineage_nodes WHERE cycle_id = ? ORDER BY bundle_hash",
        )
        .bind(cycle_id)
        .fetch_all(&self.pool)
        .await
        .context("merkle_root_for_cycle fetch")?;
        if rows.is_empty() {
            return Ok(ContentHash::of_bytes(b""));
        }
        let nodes: Vec<LineageNode> = rows.into_iter().map(row_to_node).collect::<Result<_>>()?;
        compute_merkle_root(&nodes)
    }
}

const SELECT_COLS: &str = "SELECT bundle_hash, parent_hash, diff_hash, metrics_day_hash, \
     metrics_untouched_hash, gate_verdict, status, cycle_id, created_at \
     FROM lineage_nodes WHERE bundle_hash = ?";

fn row_to_node(row: SqliteRow) -> Result<LineageNode> {
    let bundle_hex: String = row.try_get("bundle_hash").context("bundle_hash")?;
    let parent_hex: Option<String> = row.try_get("parent_hash").context("parent_hash")?;
    let diff_hex: Option<String> = row.try_get("diff_hash").context("diff_hash")?;
    let day_hex: Option<String> = row.try_get("metrics_day_hash").context("metrics_day_hash")?;
    let untouched_hex: Option<String> = row
        .try_get("metrics_untouched_hash")
        .context("metrics_untouched_hash")?;
    let gate_str: String = row.try_get("gate_verdict").context("gate_verdict")?;
    let status_str: String = row.try_get("status").context("status")?;
    let cycle_id: Option<String> = row.try_get("cycle_id").context("cycle_id")?;
    let created_str: String = row.try_get("created_at").context("created_at")?;

    Ok(LineageNode {
        bundle_hash: ContentHash::from_hex(&bundle_hex).context("bundle_hash hex")?,
        parent_hash: parent_hex
            .map(|h| ContentHash::from_hex(&h))
            .transpose()
            .context("parent_hash hex")?,
        diff_hash: diff_hex
            .map(|h| ContentHash::from_hex(&h))
            .transpose()
            .context("diff_hash hex")?,
        metrics_day_hash: day_hex
            .map(|h| ContentHash::from_hex(&h))
            .transpose()
            .context("metrics_day_hash hex")?,
        metrics_untouched_hash: untouched_hex
            .map(|h| ContentHash::from_hex(&h))
            .transpose()
            .context("metrics_untouched_hash hex")?,
        gate_verdict: GateVerdict::from_str(&gate_str)?,
        status: LineageStatus::from_str(&status_str)?,
        cycle_id,
        created_at: DateTime::parse_from_rfc3339(&created_str)
            .context("created_at parse")?
            .with_timezone(&Utc),
    })
}

fn node_to_leaf_json(node: &LineageNode) -> serde_json::Value {
    serde_json::json!({
        "bundle_hash": node.bundle_hash.to_hex(),
        "created_at": node.created_at.to_rfc3339(),
        "cycle_id": node.cycle_id,
        "diff_hash": node.diff_hash.as_ref().map(|h| h.to_hex()),
        "gate_verdict": node.gate_verdict.as_str(),
        "metrics_day_hash": node.metrics_day_hash.as_ref().map(|h| h.to_hex()),
        "metrics_untouched_hash": node.metrics_untouched_hash.as_ref().map(|h| h.to_hex()),
        "parent_hash": node.parent_hash.as_ref().map(|h| h.to_hex()),
        "status": node.status.as_str(),
    })
}

fn compute_merkle_root(nodes: &[LineageNode]) -> Result<ContentHash> {
    assert!(!nodes.is_empty(), "merkle root requires at least one node");
    let mut leaves: Vec<ContentHash> = nodes
        .iter()
        .map(|n| ContentHash::of_json(&node_to_leaf_json(n)))
        .collect();
    // Each iteration halves the slice; terminates in ceil(log2(n)) steps.
    while leaves.len() > 1 {
        let mut next: Vec<ContentHash> = Vec::with_capacity((leaves.len() + 1) / 2);
        let mut i = 0;
        while i < leaves.len() {
            let left = leaves[i];
            let right = if i + 1 < leaves.len() {
                leaves[i + 1]
            } else {
                leaves[i]
            };
            let mut combined = [0u8; 64];
            combined[..32].copy_from_slice(left.as_bytes());
            combined[32..].copy_from_slice(right.as_bytes());
            next.push(ContentHash::of_bytes(&combined));
            i += 2;
        }
        leaves = next;
    }
    assert_eq!(leaves.len(), 1, "merkle reduction must yield exactly one root");
    Ok(leaves[0])
}
