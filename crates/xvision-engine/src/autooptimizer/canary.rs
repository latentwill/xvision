//! Honesty-check (canary) injection for the autooptimizer.
//!
//! Each research cycle sabotages a copy of the base strategy and verifies
//! that the numeric gate correctly rejects it. If the gate passes the
//! sabotaged mutation the gate is too lax — the operator sees this as a
//! failed "honesty check".
//!
//! Developer-surface types use precise names (`HonestyCheckResult`,
//! `run_honesty_check`). Operator-surface strings say "honesty check".
//!
use anyhow::Result;
use serde::{Deserialize, Serialize};

use crate::autooptimizer::config::AutoOptimizerConfig;
use crate::autooptimizer::content_hash::ContentHash;
use crate::autooptimizer::eval_adapter::PaperTestRunner;
use crate::autooptimizer::gate::{evaluate, GateInput, GateVerdict};
use crate::autooptimizer::mutator::Mutator;
use crate::eval::run::MetricsSummary;
use crate::eval::scenario::Scenario;
use crate::strategies::Strategy;

/// Which deterministic sabotage was applied to the canary's base strategy.
///
/// F9 (2026-06-04): surfaced on [`HonestyCheckResult`] and threaded into the
/// paper-test executor so broker-rule rejections produced *by design* (e.g.
/// the `KillTrades` variant zero-sizes every order, tripping the venue minimum
/// notional) are relabeled as expected honesty-check noise rather than logged
/// as bare `WARN min_order_size_violation` indistinguishable from a real fault.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SabotageVariant {
    /// `risk_pct_per_trade = 0.0` → every order is $0 notional → all rejected.
    KillTrades,
    /// `daily_loss_kill_pct = 1.0` → the daily-loss kill switch never fires.
    RemoveLossLimit,
    /// `decision_cadence_minutes = 999_999` → the trader never gets to decide.
    AbsurdCadence,
}

impl SabotageVariant {
    /// Stable operator-surface label (matches the kebab-case serde rename).
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::KillTrades => "kill-trades",
            Self::RemoveLossLimit => "remove-loss-limit",
            Self::AbsurdCadence => "absurd-cadence",
        }
    }

    /// Short human-readable description of the deliberate breakage.
    pub fn describe(&self) -> &'static str {
        match self {
            Self::KillTrades => "zeroed position sizing",
            Self::RemoveLossLimit => "removed the daily-loss kill switch",
            Self::AbsurdCadence => "absurd decision cadence (never fires)",
        }
    }

    fn from_seed(sabotage_seed: u64) -> Self {
        match sabotage_seed % 3 {
            0 => Self::KillTrades,
            1 => Self::RemoveLossLimit,
            _ => Self::AbsurdCadence,
        }
    }
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
    /// Which sabotage was injected this cycle (operator-surface label).
    pub sabotage_variant: String,
    /// Human-readable summary the CLI prints and the optimizer panel renders.
    pub message: String,
}

/// Build a deterministically sabotaged copy of `base`, returning the variant
/// applied so callers can label the run.
///
/// The sabotage variant is selected by `sabotage_seed % 3`:
/// - 0 → zero position sizing (kills all trades)
/// - 1 → disable daily loss kill (removes stop)
/// - 2 → absurd decision cadence (never fires in a normal backtest)
///
/// Same seed always produces the same sabotage.
pub fn build_sabotaged_strategy(base: &Strategy, sabotage_seed: u64) -> (Strategy, SabotageVariant) {
    let mut s = base.clone();
    let variant = SabotageVariant::from_seed(sabotage_seed);
    match variant {
        SabotageVariant::KillTrades => apply_sabotage_kill_trades(&mut s),
        SabotageVariant::RemoveLossLimit => apply_sabotage_remove_loss_limit(&mut s),
        SabotageVariant::AbsurdCadence => apply_sabotage_absurd_cadence(&mut s),
    }
    (s, variant)
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
    _config: &AutoOptimizerConfig,
    sabotage_seed: u64,
) -> Result<HonestyCheckResult> {
    let (sabotaged, variant) = build_sabotaged_strategy(base, sabotage_seed);
    let variant_label = variant.as_str();

    // The base (parent) runs are legitimate — they must stay unlabeled so a
    // genuine broker fault on them still WARNs. Only the SABOTAGED (child) runs
    // are labeled, so their by-design broker rejections are demoted to expected
    // honesty-check noise.
    let parent_day = paper_tester.run(base, day_scenario).await?;
    let child_day = paper_tester
        .run_canary(&sabotaged, day_scenario, variant_label)
        .await?;
    let parent_untouched = paper_tester.run(base, baseline_scenario).await?;
    let child_untouched = paper_tester
        .run_canary(&sabotaged, baseline_scenario, variant_label)
        .await?;

    let gate_in = gate_input_builder(&parent_day, &child_day, &parent_untouched, &child_untouched);
    let gate_verdict = evaluate(&gate_in);
    let passed_check = matches!(gate_verdict, GateVerdict::Fail { .. });

    let parent_hash = ContentHash::of_json(&serde_json::to_value(base)?);
    let message = if passed_check {
        format!(
            "Honesty check passed: sabotaged variant `{}` ({}) was correctly rejected by the gate.",
            variant_label,
            variant.describe()
        )
    } else {
        format!(
            "Honesty check FAILED: sabotaged variant `{}` ({}) was NOT rejected — the gate may be too lax.",
            variant_label,
            variant.describe()
        )
    };

    Ok(HonestyCheckResult {
        parent_hash,
        gate_verdict,
        passed_check,
        sabotage_variant: variant_label.to_string(),
        message,
    })
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
