//! Phase 3.D Task 10 — run-set comparison.
//!
//! `compare_runs` loads N runs from the store and returns a `ComparisonReport`
//! ready for the dashboard's chart code: per-run summary (with full metrics),
//! per-run equity curve, and the union of all extracted findings.
//!
//! Stays in the `eval` module so callers can compose it directly without
//! going through `api::eval` (e.g., the autoresearcher's lineage gate
//! reuses `compare_runs` in-process).

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::eval::findings::Finding;
use crate::eval::run::{MetricsSummary, RunMode, RunStatus};
use crate::eval::store::RunStore;

#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts-export",
    ts(export, export_to = "../../../frontend/web/src/api/types.gen/")
)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComparisonReport {
    pub runs: Vec<ComparisonRunSummary>,
    pub equity_curves: Vec<ComparisonEquityCurve>,
    pub findings: Vec<Finding>,
}

#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts-export",
    ts(export, export_to = "../../../frontend/web/src/api/types.gen/")
)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComparisonRunSummary {
    pub id: String,
    pub agent_id: String,
    pub scenario_id: String,
    pub mode: RunMode,
    pub status: RunStatus,
    #[cfg_attr(feature = "ts-export", ts(type = "string"))]
    pub started_at: DateTime<Utc>,
    #[cfg_attr(feature = "ts-export", ts(type = "string | null"))]
    pub completed_at: Option<DateTime<Utc>>,
    pub metrics: Option<MetricsSummary>,
    pub error: Option<String>,
}

#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts-export",
    ts(export, export_to = "../../../frontend/web/src/api/types.gen/")
)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComparisonEquityCurve {
    pub run_id: String,
    pub samples: Vec<ComparisonEquitySample>,
}

#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts-export",
    ts(export, export_to = "../../../frontend/web/src/api/types.gen/")
)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComparisonEquitySample {
    #[cfg_attr(feature = "ts-export", ts(type = "string"))]
    pub timestamp: DateTime<Utc>,
    pub equity_usd: f64,
}

/// Loads each run + equity curve + findings from the store and packages
/// them into a `ComparisonReport`. Output ordering mirrors `run_ids` so
/// the dashboard / CLI can render side-by-side without re-sorting.
///
/// Errors propagate as `anyhow::Error` from the store. The api layer
/// (`api::eval::compare`) maps a missing-run error to typed `NotFound`
/// so the wire surface returns a 404-shaped error.
pub async fn compare_runs(run_ids: &[String], store: &RunStore) -> Result<ComparisonReport> {
    let mut runs = Vec::with_capacity(run_ids.len());
    let mut curves = Vec::with_capacity(run_ids.len());
    let mut findings = Vec::new();
    for id in run_ids {
        let run = store
            .get(id)
            .await
            .with_context(|| format!("compare_runs: load run {id}"))?;
        let curve = store
            .read_equity_curve(id)
            .await
            .with_context(|| format!("compare_runs: equity curve for {id}"))?;
        let run_findings = store
            .read_findings(id)
            .await
            .with_context(|| format!("compare_runs: findings for {id}"))?;
        runs.push(ComparisonRunSummary {
            id: run.id.clone(),
            agent_id: run.agent_id.clone(),
            scenario_id: run.scenario_id.clone(),
            mode: run.mode,
            status: run.status,
            started_at: run.started_at,
            completed_at: run.completed_at,
            metrics: run.metrics.clone(),
            error: run.error.clone(),
        });
        curves.push(ComparisonEquityCurve {
            run_id: run.id,
            samples: curve
                .into_iter()
                .map(|(ts, equity_usd)| ComparisonEquitySample {
                    timestamp: ts,
                    equity_usd,
                })
                .collect(),
        });
        findings.extend(run_findings);
    }
    Ok(ComparisonReport {
        runs,
        equity_curves: curves,
        findings,
    })
}
