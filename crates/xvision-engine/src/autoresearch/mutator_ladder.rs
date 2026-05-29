use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{Row, SqlitePool};

use super::content_hash::ContentHash;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MutatorScore {
    pub provider: String,
    pub model: String,
    pub prompt_version: String,
    pub proposals: u32,
    pub accepted: u32,
    pub rejected_overfit: u32,
    pub avg_delta_sharpe: f64,
}

impl MutatorScore {
    pub fn acceptance_rate(&self) -> f64 {
        if self.proposals == 0 {
            return 0.0;
        }
        self.accepted as f64 / self.proposals as f64
    }
}

pub async fn record_proposal(
    pool: &SqlitePool,
    bundle_hash: &ContentHash,
    provider: &str,
    model: &str,
    prompt_version: &str,
) -> Result<()> {
    let now = Utc::now().to_rfc3339();
    sqlx::query(
        "INSERT OR IGNORE INTO mutator_attribution \
         (bundle_hash, provider, model, prompt_version, proposed_at) \
         VALUES (?, ?, ?, ?, ?)",
    )
    .bind(bundle_hash.to_hex())
    .bind(provider)
    .bind(model)
    .bind(prompt_version)
    .bind(&now)
    .execute(pool)
    .await
    .context("record_proposal insert")?;
    Ok(())
}

/// Records the Δ-Sharpe outcome for a bundle that passed the numeric gate.
/// Called post-gate so compute_ladder can include it in avg_delta_sharpe.
pub async fn record_outcome(
    pool: &SqlitePool,
    bundle_hash: &ContentHash,
    delta_sharpe: f64,
) -> Result<()> {
    sqlx::query(
        "UPDATE mutator_attribution SET delta_sharpe = ? WHERE bundle_hash = ?",
    )
    .bind(delta_sharpe)
    .bind(bundle_hash.to_hex())
    .execute(pool)
    .await
    .context("record_outcome update")?;
    Ok(())
}

const LADDER_SQL: &str = "
    SELECT
        ma.provider,
        ma.model,
        ma.prompt_version,
        CAST(COUNT(*) AS INTEGER) AS proposals,
        CAST(SUM(CASE WHEN ln.gate_verdict = 'passed' THEN 1 ELSE 0 END) AS INTEGER) AS accepted,
        CAST(SUM(CASE WHEN ln.gate_verdict = 'rejected' OR ln.gate_verdict LIKE 'rejected:%' THEN 1 ELSE 0 END) AS INTEGER) AS rejected_overfit,
        COALESCE(AVG(CASE WHEN ln.gate_verdict = 'passed' THEN ma.delta_sharpe END), 0.0) AS avg_delta_sharpe
    FROM mutator_attribution ma
    LEFT JOIN lineage_nodes ln ON ln.bundle_hash = ma.bundle_hash
    WHERE ma.proposed_at >= ?
    GROUP BY ma.provider, ma.model, ma.prompt_version
    ORDER BY avg_delta_sharpe DESC
";

pub async fn compute_ladder(
    pool: &SqlitePool,
    since: DateTime<Utc>,
) -> Result<Vec<MutatorScore>> {
    let rows = sqlx::query(LADDER_SQL)
        .bind(since.to_rfc3339())
        .fetch_all(pool)
        .await
        .context("compute_ladder query")?;
    rows.into_iter()
        .map(|row| -> Result<MutatorScore> {
            Ok(MutatorScore {
                provider: row.try_get("provider").context("provider")?,
                model: row.try_get("model").context("model")?,
                prompt_version: row.try_get("prompt_version").context("prompt_version")?,
                proposals: row.try_get::<i64, _>("proposals").context("proposals")? as u32,
                accepted: row.try_get::<i64, _>("accepted").context("accepted")? as u32,
                rejected_overfit: row
                    .try_get::<i64, _>("rejected_overfit")
                    .context("rejected_overfit")? as u32,
                avg_delta_sharpe: row
                    .try_get("avg_delta_sharpe")
                    .context("avg_delta_sharpe")?,
            })
        })
        .collect()
}
