//! Phase 3.D Task 10 — run-set comparison.
//!
//! `compare_runs` loads N runs from the store and returns a `ComparisonReport`
//! ready for the dashboard's chart code: per-run summary (with full metrics),
//! per-run equity curve, and the union of all extracted findings.
//!
//! Stays in the `eval` module so callers can compose it directly without
//! going through `api::eval` (e.g., the autoresearcher's lineage gate
//! reuses `compare_runs` in-process).
//!
//! # Manifest mismatch refusal (V2E, migration 027)
//!
//! `compare_runs` refuses to render two or more runs together when their
//! `manifest_canonical` values differ, unless `allow_manifest_mismatch: true`
//! is passed in `CompareOptions`. Refusal returns a `ManifestMismatch` error
//! that the API layer surfaces as a 409 Conflict-shaped error.
//!
//! Rationale: two runs that share bar data but differ in feed (`iex` vs
//! `sip`), adjustment mode, or session filter are not directly comparable —
//! they saw different market views. Silently plotting them side-by-side
//! produces misleading conclusions.

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::eval::behavior::{derive_behavior_summary, BehaviorSummary};
use crate::eval::findings::Finding;
use crate::eval::report::{aggregate_run_token_totals, wall_clock_ms};
use crate::eval::run::{MetricsSummary, RunMode, RunStatus};
use crate::eval::store::RunStore;

// ── Error type ────────────────────────────────────────────────────────────────

/// Returned by `compare_runs` when the runs' `manifest_canonical` values
/// disagree and `allow_manifest_mismatch` is `false`.
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts-export",
    ts(export, export_to = "../../../frontend/web/src/api/types.gen/")
)]
#[derive(Debug, Clone, Serialize, Deserialize, Error)]
#[error("manifest mismatch: run {run_a} and run {run_b} have different manifest_canonical values (differing fields: {diff_fields:?})")]
pub struct ManifestMismatch {
    pub run_a: String,
    pub run_b: String,
    /// Human-readable list of differing manifest fields. Derived from the
    /// `bars_manifest` JSON blobs; empty when blobs aren't present (i.e. the
    /// only signal is the hash mismatch).
    pub diff_fields: Vec<String>,
}

// ── Options ───────────────────────────────────────────────────────────────────

/// Options for `compare_runs`.
#[derive(Debug, Clone, Default)]
pub struct CompareOptions {
    /// If `true`, skip the manifest-canonical consistency check and render
    /// the comparison even when runs have different manifest_canonical values.
    /// This is the `--allow-manifest-mismatch` flag.
    pub allow_manifest_mismatch: bool,
}

// ── Report types ──────────────────────────────────────────────────────────────

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
    /// Derived action-distribution + behaviour summary for this run.
    /// Populated by `compare_runs`; `None` only when the decision store
    /// query fails for this run (treated as best-effort, not fatal).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub behavior: Option<BehaviorSummary>,
    /// SHA-256 hex digest of the Parquet bytes used for this run.
    /// `None` for pre-migration rows.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bars_content_hash: Option<String>,
    /// SHA-256 hex digest of the canonical DataManifest for this run.
    /// `None` for pre-migration rows.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub manifest_canonical: Option<String>,
    /// Net return after deducting LLM inference cost. `None` for old runs
    /// without pricing data or when the model isn't in the pricing catalog.
    /// Mirror of `MetricsSummary::net_return_pct` hoisted to the comparison
    /// arm so the compare view can render a Net column without deep-reading
    /// the full metrics blob.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub net_return_pct: Option<f64>,
    /// Sum of `model_calls.input_token_count` for this run. `None` when
    /// the run produced no model_calls (legacy / pre-observability rows)
    /// or none of them recorded token counts. Appended 2026-05-22 for
    /// `cli-report-actions-and-tokens`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input_tokens: Option<u64>,
    /// Sum of `model_calls.output_token_count`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_tokens: Option<u64>,
    /// Sum of `model_calls.cost_usd`. Not a recomputation — purely a
    /// rollup of the populated column. `None` when every contributing
    /// row had `cost_usd = NULL` or there were no contributing rows.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cost_usd_estimate: Option<f64>,
    /// `true` iff every model_call contributing to `cost_usd_estimate`
    /// had a non-null `cost_usd`. When `false`, the estimate is a strict
    /// lower bound. Defaults to `true` for legacy payloads where no
    /// model_calls were aggregated — but in that case the cost itself is
    /// `None`, so the flag carries no false signal.
    #[serde(default = "default_cost_complete")]
    pub cost_estimate_complete: bool,
    /// Wall-clock duration (`completed_at - started_at`) in milliseconds.
    /// `None` for runs that haven't terminated, or pre-migration rows.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub wall_clock_ms: Option<u64>,
}

/// Default `cost_estimate_complete` for serde deserialization of legacy
/// payloads. We default to `true` because a payload that didn't include
/// the field also didn't include `cost_usd_estimate` (it's all-`None`),
/// so the flag is operationally meaningless on legacy reads.
fn default_cost_complete() -> bool {
    true
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

// ── Main entry point ──────────────────────────────────────────────────────────

/// Loads each run + equity curve + findings from the store and packages
/// them into a `ComparisonReport`. Output ordering mirrors `run_ids` so
/// the dashboard / CLI can render side-by-side without re-sorting.
///
/// Returns `Err` wrapping a `ManifestMismatch` when runs have incompatible
/// data manifests and `options.allow_manifest_mismatch` is `false`.
///
/// Other errors propagate as `anyhow::Error` from the store. The api layer
/// (`api::eval::compare`) maps a missing-run error to typed `NotFound`
/// so the wire surface returns a 404-shaped error.
pub async fn compare_runs(
    run_ids: &[String],
    store: &RunStore,
    options: &CompareOptions,
) -> Result<ComparisonReport> {
    let mut loaded_runs = Vec::with_capacity(run_ids.len());

    for id in run_ids {
        let run = store
            .get(id)
            .await
            .with_context(|| format!("compare_runs: load run {id}"))?;
        loaded_runs.push(run);
    }

    // ── Manifest-canonical consistency check ──────────────────────────────
    if !options.allow_manifest_mismatch {
        check_manifest_consistency(&loaded_runs)?;
    }

    let mut runs = Vec::with_capacity(loaded_runs.len());
    let mut curves = Vec::with_capacity(loaded_runs.len());
    let mut findings = Vec::new();

    for run in &loaded_runs {
        let id = &run.id;
        let curve = store
            .read_equity_curve(id)
            .await
            .with_context(|| format!("compare_runs: equity curve for {id}"))?;
        let run_findings = store
            .read_findings(id)
            .await
            .with_context(|| format!("compare_runs: findings for {id}"))?;
        // Action distribution + behaviour summary — best-effort.
        let behavior = store
            .read_decisions(id)
            .await
            .ok()
            .map(|rows| derive_behavior_summary(&rows));

        // Token / cost / wall-clock rollup. Aggregator is defensive: when
        // the model_calls join yields zero rows (e.g. very old runs, or
        // baselines that didn't touch the observability bus), every
        // optional field stays `None` and `cost_estimate_complete` is the
        // safe default.
        let totals = aggregate_run_token_totals(store.pool(), &run.id).await;
        let net_return_pct = run.metrics.as_ref().and_then(|m| m.net_return_pct);

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
            behavior,
            bars_content_hash: run.bars_content_hash.clone(),
            manifest_canonical: run.manifest_canonical.clone(),
            net_return_pct,
            input_tokens: totals.input_tokens,
            output_tokens: totals.output_tokens,
            cost_usd_estimate: totals.cost_usd_estimate,
            cost_estimate_complete: totals.cost_estimate_complete,
            wall_clock_ms: wall_clock_ms(run.started_at, run.completed_at),
        });
        curves.push(ComparisonEquityCurve {
            run_id: run.id.clone(),
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

/// Backward-compatible wrapper: calls `compare_runs` with no manifest
/// mismatch override (the default-safe path). Preserved for callers that
/// don't yet pass `CompareOptions`.
pub async fn compare_runs_default(run_ids: &[String], store: &RunStore) -> Result<ComparisonReport> {
    compare_runs(run_ids, store, &CompareOptions::default()).await
}

// ── Manifest consistency check ────────────────────────────────────────────────

/// Check that all loaded runs share the same `manifest_canonical`, if any
/// have one set. Runs with `manifest_canonical = None` (pre-migration rows)
/// are excluded from the check — they're allowed to compare with any run.
fn check_manifest_consistency(runs: &[crate::eval::run::Run]) -> Result<()> {
    // Collect runs that have a manifest hash.
    let manifest_runs: Vec<_> = runs
        .iter()
        .filter_map(|r| r.manifest_canonical.as_deref().map(|h| (&r.id, h)))
        .collect();

    if manifest_runs.len() < 2 {
        // Zero or one run has a manifest — nothing to compare.
        return Ok(());
    }

    let (first_id, first_hash) = &manifest_runs[0];
    for (other_id, other_hash) in &manifest_runs[1..] {
        if other_hash != first_hash {
            // Try to compute which manifest fields differ.
            let diff_fields = diff_manifest_fields(
                runs.iter().find(|r| &r.id == *first_id),
                runs.iter().find(|r| &r.id == *other_id),
            );
            return Err(anyhow::Error::from(ManifestMismatch {
                run_a: first_id.to_string(),
                run_b: other_id.to_string(),
                diff_fields,
            }));
        }
    }
    Ok(())
}

/// Compute a human-readable list of manifest field names that differ between
/// two runs' `bars_manifest` JSON blobs.
fn diff_manifest_fields(
    run_a: Option<&crate::eval::run::Run>,
    run_b: Option<&crate::eval::run::Run>,
) -> Vec<String> {
    let (Some(a), Some(b)) = (run_a, run_b) else {
        return vec![];
    };
    let (Some(ma), Some(mb)) = (a.bars_manifest.as_ref(), b.bars_manifest.as_ref()) else {
        return vec![];
    };
    let manifest_fields = [
        "feed",
        "adjustment",
        "timeframe",
        "session_filter",
        "calendar",
        "timezone",
    ];
    manifest_fields
        .iter()
        .filter(|&field| ma.get(*field) != mb.get(*field))
        .map(|f| f.to_string())
        .collect()
}
