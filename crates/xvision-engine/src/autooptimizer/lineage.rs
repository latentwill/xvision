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
    pub gate_verdict: GateVerdict,
    pub status: LineageStatus,
    pub cycle_id: Option<String>,
    pub created_at: DateTime<Utc>,
    pub diversity_score: Option<f64>,
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
             (bundle_hash, parent_hash, gate_verdict, status, cycle_id, created_at) \
             VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind(node.bundle_hash.to_hex())
        .bind(node.parent_hash.as_ref().map(|h| h.to_hex()))
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
            "SELECT bundle_hash, parent_hash, gate_verdict, status, cycle_id, created_at, diversity_score \
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
            "SELECT bundle_hash, parent_hash, gate_verdict, status, cycle_id, created_at, diversity_score \
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
}

const SELECT_COLS: &str = "SELECT bundle_hash, parent_hash, gate_verdict, status, cycle_id, \
     created_at, diversity_score FROM lineage_nodes WHERE bundle_hash = ?";

fn row_to_node(row: SqliteRow) -> Result<LineageNode> {
    let bundle_hex: String = row.try_get("bundle_hash").context("bundle_hash")?;
    let parent_hex: Option<String> = row.try_get("parent_hash").context("parent_hash")?;
    let gate_str: String = row.try_get("gate_verdict").context("gate_verdict")?;
    let status_str: String = row.try_get("status").context("status")?;
    let cycle_id: Option<String> = row.try_get("cycle_id").context("cycle_id")?;
    let created_str: String = row.try_get("created_at").context("created_at")?;
    let diversity_score: Option<f64> = row.try_get("diversity_score").context("diversity_score")?;

    Ok(LineageNode {
        bundle_hash: ContentHash::from_hex(&bundle_hex).context("bundle_hash hex")?,
        parent_hash: parent_hex
            .map(|h| ContentHash::from_hex(&h))
            .transpose()
            .context("parent_hash hex")?,
        gate_verdict: GateVerdict::from_str(&gate_str)?,
        status: LineageStatus::from_str(&status_str)?,
        cycle_id,
        created_at: DateTime::parse_from_rfc3339(&created_str)
            .context("created_at parse")?
            .with_timezone(&Utc),
        diversity_score,
    })
}
