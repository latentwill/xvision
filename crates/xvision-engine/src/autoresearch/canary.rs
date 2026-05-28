//! Honesty-check (canary) injection for the autoresearcher.
//!
//! Each research cycle sabotages a copy of the base strategy and verifies
//! that the numeric gate correctly rejects it. If the gate passes the
//! sabotaged mutation the gate is too lax — the operator sees this as a
//! failed "honesty check".
//!
//! Developer-surface types use precise names (`HonestyCheckResult`,
//! `run_honesty_check`). Operator-surface strings say "honesty check".
//!
//! TODO: when the AR-2 gate.rs update lands, replace the local `GateInput`
//! definition and `evaluate_gate` helper with imports from
//! `crate::autoresearch::gate::{GateInput, evaluate}` and update
//! `passed_check` to use the new `GateVerdict::Fail { .. }` variant.

use anyhow::Result;
use serde::{Deserialize, Serialize};

use crate::autoresearch::config::AutoresearchConfig;
use crate::autoresearch::content_hash::ContentHash;
use crate::autoresearch::gate::GateVerdict;
use crate::autoresearch::mutator::Mutator;
use crate::eval::run::MetricsSummary;
use crate::eval::scenario::Scenario;
use crate::strategies::Strategy;

pub use crate::autoresearch::eval_adapter::PaperTestRunner;

/// Minimal gate-input bundle for the honesty-check path.
///
/// Mirrors the shape pre-written in `tests/autoresearch_gate.rs`.
/// Replace with `use crate::autoresearch::gate::GateInput` once
/// gate.rs defines it (AR-2 gate task).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GateInput {
    pub parent_day_metrics: MetricsSummary,
    pub child_day_metrics: MetricsSummary,
    pub parent_untouched_metrics: MetricsSummary,
    pub child_untouched_metrics: MetricsSummary,
    pub min_improvement: f64,
}

/// Outcome of a single honesty-check run.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HonestyCheckResult {
    /// BLAKE3 hash of the base strategy that was sabotaged.
    pub parent_hash: ContentHash,
    /// The gate's verdict on the sabotaged mutation.
    pub gate_verdict: GateVerdict,
    /// `true` when the honesty check WORKED — i.e., the gate correctly
    /// rejected the sabotaged mutation.
    pub passed_check: bool,
}

/// Runs a strategy against a scenario and returns performance metrics.
///
/// Implement this trait with your backtest / paper-test engine.
/// The `run` method must be deterministic for a given strategy + scenario pair.
#[async_trait]
pub trait PaperTestRunner: Send + Sync {
    async fn run(&self, strategy: &Strategy, scenario: &Scenario) -> Result<MetricsSummary>;
}

/// Build a deterministically sabotaged copy of `base`.
///
/// The sabotage variant is selected by `sabotage_seed % 3`:
/// - 0 → zero position sizing (kills all trades)
/// - 1 → disable daily loss kill (removes stop)
/// - 2 → absurd decision cadence (never fires in a normal backtest)
///
/// Same seed always produces the same sabotage.
pub fn build_sabotaged_strategy(base: &Strategy, sabotage_seed: u64) -> Strategy {
    let mut s = base.clone();
    match sabotage_seed % 3 {
        0 => apply_sabotage_kill_trades(&mut s),
        1 => apply_sabotage_remove_loss_limit(&mut s),
        _ => apply_sabotage_absurd_cadence(&mut s),
    }
    s
}

/// Run the honesty check: inject a sabotaged mutation and assert the gate rejects it.
///
/// Returns `HonestyCheckResult::passed_check = true` when the gate correctly
/// rejects the sabotaged strategy (the expected outcome — the check "passed").
///
/// `_mutator` and `_config` are reserved for the cycle orchestrator that
/// chains real mutations around the honesty check.
pub async fn run_honesty_check(
    base: &Strategy,
    _mutator: &Mutator,
    paper_tester: &dyn PaperTestRunner,
    gate_input_builder: impl Fn(&MetricsSummary, &MetricsSummary, &MetricsSummary, &MetricsSummary) -> GateInput,
    day_scenario: &Scenario,
    baseline_scenario: &Scenario,
    _config: &AutoresearchConfig,
    sabotage_seed: u64,
) -> Result<HonestyCheckResult> {
    let sabotaged = build_sabotaged_strategy(base, sabotage_seed);

    let parent_day = paper_tester.run(base, day_scenario).await?;
    let child_day = paper_tester.run(&sabotaged, day_scenario).await?;
    let parent_untouched = paper_tester.run(base, baseline_scenario).await?;
    let child_untouched = paper_tester.run(&sabotaged, baseline_scenario).await?;

    let gate_in = gate_input_builder(&parent_day, &child_day, &parent_untouched, &child_untouched);
    let gate_verdict = evaluate_gate(&gate_in);
    let passed_check = matches!(gate_verdict, GateVerdict::Rejected);

    let parent_hash = ContentHash::of_json(&serde_json::to_value(base)?);

    Ok(HonestyCheckResult { parent_hash, gate_verdict, passed_check })
}

fn evaluate_gate(input: &GateInput) -> GateVerdict {
    let delta_day = input.child_day_metrics.sharpe - input.parent_day_metrics.sharpe;
    let delta_untouched =
        input.child_untouched_metrics.sharpe - input.parent_untouched_metrics.sharpe;
    if delta_day >= input.min_improvement && delta_untouched >= input.min_improvement {
        GateVerdict::Passed
    } else {
        GateVerdict::Rejected
    }
}

fn apply_sabotage_kill_trades(s: &mut Strategy) {
    s.risk.risk_pct_per_trade = 0.0;
}

fn apply_sabotage_remove_loss_limit(s: &mut Strategy) {
    s.risk.daily_loss_kill_pct = 1.0;
}

fn apply_sabotage_absurd_cadence(s: &mut Strategy) {
    s.manifest.decision_cadence_minutes = 999_999;
}
