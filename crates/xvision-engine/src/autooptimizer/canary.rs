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
    //
    // R2: the four canary eval calls are wrapped so ANY eval/trader error
    // (provider outage, malformed output, etc.) degrades to a NEUTRAL
    // failed-canary result instead of propagating — no canary decision may be
    // fatal. This overrides the earlier B28 narrowing (which propagated genuine
    // canary errors): a canary INFRA failure must not kill the optimizer
    // session. It is surfaced via a WARN + the `errored` sabotage_variant +
    // a Fail verdict, not hidden. (A zero-trade *completion* is still handled by
    // `neutralize_zero_trade_canary` on the success path.)
    let eval = async {
        let parent_day = paper_tester.run(base, day_scenario).await?;
        let child_day = neutralize_zero_trade_canary(
            paper_tester
                .run_canary(&sabotaged, day_scenario, variant_label)
                .await?,
            variant_label,
            day_scenario.id.as_str(),
        );
        let parent_untouched = paper_tester.run(base, baseline_scenario).await?;
        let child_untouched = neutralize_zero_trade_canary(
            paper_tester
                .run_canary(&sabotaged, baseline_scenario, variant_label)
                .await?,
            variant_label,
            baseline_scenario.id.as_str(),
        );
        Ok::<_, anyhow::Error>((parent_day, child_day, parent_untouched, child_untouched))
    }
    .await;

    let (parent_day, child_day, parent_untouched, child_untouched) = match eval {
        Ok(metrics) => metrics,
        Err(e) => {
            tracing::warn!(
                target: "xvision::autooptimizer",
                error = %e,
                sabotage_variant = variant_label,
                "honesty-check canary errored; recording a neutral failed-canary result and continuing (R2)"
            );
            return Ok(HonestyCheckResult {
                parent_hash: ContentHash::of_json(&serde_json::to_value(base)?),
                gate_verdict: GateVerdict::Fail {
                    reason: format!("canary errored: {e:#}"),
                },
                // A canary that could not RUN is not evidence the gate is too
                // lax, so the honesty check is treated as a non-event (passed),
                // distinguished by the `errored` variant + the verdict reason.
                passed_check: true,
                sabotage_variant: "errored".to_string(),
                message: format!(
                    "Honesty check skipped: canary evaluation errored ({e:#}); recorded as neutral and the cycle continued."
                ),
            });
        }
    };

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

/// B28: map a zero-trade canary (sabotage) backtest to a NEUTRAL
/// (no-improvement) [`MetricsSummary`] the gate will reject.
///
/// A deliberately-sabotaged strategy frequently completes **zero trades** — the
/// `kill-trades` variant zero-sizes every order, so every order is rejected by
/// broker-rule validation and no fill is recorded. Such a run still completes
/// successfully (`RunStatus::Completed`) with `n_trades == 0` and all-zero
/// metrics; it does NOT error and does NOT leave `metrics_json` NULL. We map
/// that specific zero-trade case to an explicit neutral sentinel so the gate
/// reliably rejects the sabotage and the honesty check PASSES — independent of
/// objective (0.0 is the no-improvement sentinel for return / sharpe / win-rate).
///
/// **Narrow by design (B28 follow-up).** ONLY the zero-trade *completion* case
/// is neutralised here. A genuine canary backtest *error* (provider outage,
/// panic, malformed scenario) is a separate concern handled one level up: R2
/// makes `run_honesty_check` degrade such an error to a NEUTRAL failed-canary
/// result (`sabotage_variant = "errored"`) so a real infrastructure fault is
/// surfaced (WARN + `errored` marker) without killing the cycle or the
/// optimizer session. This function deliberately stays scoped to the
/// successful-but-zero-trade case only.
fn neutralize_zero_trade_canary(
    metrics: MetricsSummary,
    variant_label: &str,
    scenario_id: &str,
) -> MetricsSummary {
    if metrics.n_trades == 0 {
        tracing::info!(
            target: "xvision::autooptimizer",
            sabotage_variant = variant_label,
            scenario_id,
            "honesty-check canary completed zero trades; scoring it as a neutral \
             no-improvement result so the gate rejects it and the cycle completes (B28)"
        );
        return MetricsSummary::default();
    }
    metrics
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

/// Minimum acceptable prompt length in characters. Shorter prompts are likely
/// truncated or empty — the optimizer can't meaningfully improve from them.
const MIN_PROMPT_LENGTH: usize = 200;

/// Check the semantic structure of an agent prompt for basic integrity.
///
/// Returns `Ok(())` when the prompt passes all checks. Returns `Err(reasons)`
/// when one or more checks fail, with each reason describing the specific
/// violation. Used by the honesty gate to detect clearly broken mutations
/// before they waste evaluation cycles.
///
/// Checks:
/// 1. Minimum length (≥ 200 chars — truncated/empty prompts are useless)
/// 2. Required trading decision fields (stop_loss, take_profit, or action spec)
/// 3. No zero/negative position sizing language
/// 4. No obviously reversed signal logic patterns
pub fn validate_prompt_semantics(prompt: &str) -> Result<(), Vec<String>> {
    let mut failures: Vec<String> = Vec::new();

    // 1. Minimum length
    if prompt.trim().len() < MIN_PROMPT_LENGTH {
        failures.push(format!(
            "prompt is too short ({} chars, minimum {}) — likely truncated or empty",
            prompt.trim().len(),
            MIN_PROMPT_LENGTH
        ));
    }

    let lower = prompt.to_ascii_lowercase();

    // 2. Required trading decision fields
    let required_terms = ["stop_loss", "stop loss", "take_profit", "take profit", "action", "position"];
    let has_decision_fields = required_terms.iter().any(|term| lower.contains(term));
    if !has_decision_fields {
        failures.push(
            "prompt missing required trading decision fields (no stop_loss, take_profit, or action spec)"
                .to_string(),
        );
    }

    // 3. Zero/negative position sizing
    let zero_size_patterns = [
        "position size 0",
        "position size: 0",
        "size 0%",
        "size: 0%",
        "risk 0%",
        "risk: 0",
        "allocate 0",
        "zero position",
        "no position",
        "do not trade",
    ];
    for pat in &zero_size_patterns {
        if lower.contains(pat) {
            failures.push(format!(
                "prompt contains zero/negative position sizing language: \"{pat}\""
            ));
            break; // one is enough
        }
    }

    // 4. Obviously reversed signal direction (mean-reversion context)
    // "go long when RSI > 70" in a mean-reversion strategy is backwards —
    // mean-reversion shorts overbought and buys oversold.
    let reversed_signal_patterns = [
        ("buy when rsi > 70", "reversed signal: buying overbought is anti-mean-reversion"),
        ("go long when rsi > 70", "reversed signal: going long at overbought is anti-mean-reversion"),
        ("sell when rsi < 30", "reversed signal: selling oversold is anti-mean-reversion"),
        ("short when rsi < 30", "reversed signal: shorting oversold is anti-mean-reversion"),
    ];
    for (pat, reason) in &reversed_signal_patterns {
        if lower.contains(pat) {
            failures.push(format!("{reason} (\"{pat}\")"));
            break; // one is enough to flag
        }
    }

    if failures.is_empty() {
        Ok(())
    } else {
        Err(failures)
    }
}
