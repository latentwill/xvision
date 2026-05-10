//! Phase 10.2 — Markdown report generator.
//!
//! Reads a `BacktestResult`, computes the headline Δ-Sharpe (treatment vs.
//! a designated baseline arm) via paired bootstrap, runs the anti-overfit
//! gate by regime, and renders Markdown with:
//! 1. Headline Δ-Sharpe + 95% CI per treatment arm
//! 2. Per-arm dashboard (Sharpe, MDD, PF, WR, total PnL)
//! 3. Regime-stratified Δ-Sharpe + gate verdict
//! 4. Notes calling out inferential vs descriptive metrics
//!
//! Inferential vs descriptive distinction is stated explicitly (Tier 3
//! review item from implementation-plan.md).

use std::fmt::Write;

use crate::bootstrap::paired_bootstrap_sharpe_delta;
use crate::gate::{anti_overfit_verdict, GateVerdict};
use crate::metrics::{
    compute_pre_committed, max_drawdown_pct, profit_factor, sharpe_annualized, win_rate,
};
use crate::result::BacktestResult;

#[derive(Debug, Clone)]
pub struct ReportConfig {
    /// Arm name to treat as the baseline (Δ-Sharpe is treatment − baseline).
    pub baseline_arm: String,
    pub n_bootstrap_resamples: usize,
    pub block_size: Option<usize>,
    /// Periods per year for the Sharpe annualisation. Hourly returns → 8760;
    /// daily → 252; per-setup with horizon=24h → 8760/24 = 365.
    pub periods_per_year: f32,
    pub seed: u64,
}

impl Default for ReportConfig {
    fn default() -> Self {
        Self {
            baseline_arm: "buy_and_hold".into(),
            n_bootstrap_resamples: 1000,
            block_size: None,
            // Setup-cadence default. Reports may override per-asset / per-cadence.
            periods_per_year: 365.0,
            seed: 0xdeadbeef_cafef00d,
        }
    }
}

/// Render the full Markdown report.
pub fn render(result: &BacktestResult, cfg: &ReportConfig) -> anyhow::Result<String> {
    let mut s = String::with_capacity(4096);
    writeln!(s, "# XIANVEC backtest report")?;
    writeln!(s)?;
    writeln!(s, "- cycles evaluated: {}", result.cycles_evaluated)?;
    writeln!(s, "- initial NAV: ${:.2}", result.initial_nav_usd)?;
    writeln!(s, "- started: {}", result.started_at)?;
    writeln!(s, "- finished: {}", result.finished_at)?;
    writeln!(s, "- baseline arm: `{}`", cfg.baseline_arm)?;
    writeln!(s)?;

    // -- Headline Δ-Sharpe per treatment arm -----------------------------
    writeln!(s, "## Headline Δ-Sharpe (95% CI)")?;
    writeln!(s)?;
    let baseline = result.arms.get(&cfg.baseline_arm).ok_or_else(|| {
        anyhow::anyhow!(
            "baseline arm `{}` not present in BacktestResult; available: {}",
            cfg.baseline_arm,
            result
                .arms
                .keys()
                .cloned()
                .collect::<Vec<_>>()
                .join(", ")
        )
    })?;

    writeln!(s, "| arm | Δ-Sharpe (vs baseline) | 95% CI |")?;
    writeln!(s, "|-----|-----------------------:|--------|")?;
    for (name, arm) in &result.arms {
        if name == &cfg.baseline_arm {
            continue;
        }
        if arm.returns.len() != baseline.returns.len() {
            writeln!(
                s,
                "| `{}` | n/a (length mismatch: {} vs baseline {}) | — |",
                name,
                arm.returns.len(),
                baseline.returns.len()
            )?;
            continue;
        }
        if arm.returns.is_empty() {
            writeln!(s, "| `{}` | n/a (empty returns) | — |", name)?;
            continue;
        }
        let r = paired_bootstrap_sharpe_delta(
            &arm.returns,
            &baseline.returns,
            cfg.n_bootstrap_resamples,
            cfg.block_size,
            cfg.periods_per_year,
            cfg.seed,
        )?;
        writeln!(
            s,
            "| `{}` | {:+.4} | [{:+.4}, {:+.4}] |",
            name, r.point_estimate, r.ci_low, r.ci_high
        )?;
    }
    writeln!(s)?;

    // -- Per-arm dashboard -----------------------------------------------
    writeln!(s, "## Per-arm dashboard")?;
    writeln!(s)?;
    writeln!(s, "| arm | n | Sharpe | MDD% | PF | WR | realized $ |")?;
    writeln!(s, "|-----|--:|-------:|----:|---:|---:|-----------:|")?;
    for (name, arm) in &result.arms {
        let sh = sharpe_annualized(&arm.returns, cfg.periods_per_year);
        let nav: Vec<f64> = arm.equity_curve.iter().map(|p| p.nav_usd).collect();
        let mdd = max_drawdown_pct(&nav);
        let pf = profit_factor(&arm.returns);
        let wr = win_rate(&arm.returns);
        writeln!(
            s,
            "| `{}` | {} | {:.3} | {:.3} | {:.3} | {:.3} | {:.2} |",
            name,
            arm.returns.len(),
            sh,
            mdd,
            pf,
            wr,
            arm.realized_pnl_total_usd
        )?;
    }
    writeln!(s)?;

    // -- Regime stratification + gate ------------------------------------
    writeln!(s, "## Regime-stratified Δ-Sharpe + anti-overfit gate")?;
    writeln!(s)?;
    let mut treatments: Vec<&String> = result
        .arms
        .keys()
        .filter(|name| *name != &cfg.baseline_arm)
        .collect();
    treatments.sort();
    for treatment_name in treatments {
        writeln!(s, "### Treatment `{treatment_name}`")?;
        writeln!(s)?;

        let treatment = &result.arms[treatment_name];
        if treatment.returns.is_empty() || baseline.returns.is_empty() {
            writeln!(s, "_(empty returns — gate skipped)_")?;
            writeln!(s)?;
            continue;
        }
        if treatment.returns.len() != baseline.returns.len() {
            writeln!(s, "_(length mismatch — gate skipped)_")?;
            writeln!(s)?;
            continue;
        }
        let metrics = match compute_pre_committed(
            result,
            treatment_name,
            &cfg.baseline_arm,
            cfg.n_bootstrap_resamples,
            cfg.block_size,
        ) {
            Ok(m) => m,
            Err(e) => {
                writeln!(s, "_(metrics error: {e})_")?;
                writeln!(s)?;
                continue;
            }
        };

        if metrics.regime_stratified.is_empty() {
            writeln!(s, "_(no regimes recorded for this arm)_")?;
        } else {
            writeln!(s, "| regime | n | Δ-Sharpe | 95% CI |")?;
            writeln!(s, "|--------|--:|---------:|--------|")?;
            let mut regs: Vec<_> = metrics.regime_stratified.iter().collect();
            regs.sort_by_key(|(r, _)| format!("{r:?}"));
            for (regime, rm) in regs {
                writeln!(
                    s,
                    "| {regime:?} | {} | {:+.4} | [{:+.4}, {:+.4}] |",
                    rm.n_cycles,
                    rm.delta_sharpe.point_estimate,
                    rm.delta_sharpe.ci_low,
                    rm.delta_sharpe.ci_high
                )?;
            }
        }
        writeln!(s)?;

        let verdict = anti_overfit_verdict(&metrics);
        match verdict {
            GateVerdict::PassesBothRegimes => {
                writeln!(s, "**Gate: PassesBothRegimes** — Δ-Sharpe CI > 0 in every recorded regime.")?
            }
            GateVerdict::SingleRegimeEvidence {
                winning_regime,
                losing_regime,
            } => writeln!(
                s,
                "**Gate: SingleRegimeEvidence** — wins in {winning_regime:?}, loses in {losing_regime:?}."
            )?,
            GateVerdict::Fails { regimes } => writeln!(
                s,
                "**Gate: Fails** — no regime cleared the positive-CI criterion ({} stratum/strata).",
                regimes.len()
            )?,
        }
        writeln!(s)?;
    }

    // -- Notes -----------------------------------------------------------
    writeln!(s, "## Notes")?;
    writeln!(s)?;
    writeln!(
        s,
        "- Δ-Sharpe is the only inferential metric. MDD, PF, WR are descriptive and not multiple-comparisons-corrected."
    )?;
    writeln!(
        s,
        "- Bootstrap: paired, n_resamples = {}, block_size = {:?}, seed = {}, periods_per_year = {}.",
        cfg.n_bootstrap_resamples, cfg.block_size, cfg.seed, cfg.periods_per_year
    )?;
    writeln!(
        s,
        "- Anti-overfit gate is reportable but not blocking in v1 (see `decisions/0005-lookahead-audit.md`)."
    )?;
    Ok(s)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    use chrono::Utc;
    use xianvec_core::trading::Regime;

    use crate::result::{ArmResult, BacktestResult};

    fn fixture_two_arms() -> BacktestResult {
        BacktestResult {
            arms: BTreeMap::from([
                (
                    "buy_and_hold".into(),
                    ArmResult {
                        name: "buy_and_hold".into(),
                        equity_curve: vec![],
                        fills: vec![],
                        decisions: vec![],
                        risk_outcomes: vec![],
                        returns: vec![0.001, -0.002, 0.003, -0.001, 0.002],
                        realized_pnl_total_usd: 100.0,
                        regimes: vec![
                            Regime::Bull,
                            Regime::Bull,
                            Regime::Bear,
                            Regime::Chop,
                            Regime::Bull,
                        ],
                    },
                ),
                (
                    "trader_arm".into(),
                    ArmResult {
                        name: "trader_arm".into(),
                        equity_curve: vec![],
                        fills: vec![],
                        decisions: vec![],
                        risk_outcomes: vec![],
                        returns: vec![0.002, -0.001, 0.004, 0.0, 0.003],
                        realized_pnl_total_usd: 500.0,
                        regimes: vec![
                            Regime::Bull,
                            Regime::Bull,
                            Regime::Bear,
                            Regime::Chop,
                            Regime::Bull,
                        ],
                    },
                ),
            ]),
            cycles_evaluated: 5,
            initial_nav_usd: 100_000.0,
            started_at: Utc::now(),
            finished_at: Utc::now(),
        }
    }

    #[test]
    fn render_emits_headline_table() {
        let result = fixture_two_arms();
        let cfg = ReportConfig {
            n_bootstrap_resamples: 50,
            ..Default::default()
        };
        let md = render(&result, &cfg).expect("render must succeed");
        assert!(md.contains("Headline Δ-Sharpe"), "headline section");
        assert!(md.contains("trader_arm"), "treatment arm row");
        assert!(md.contains("Per-arm dashboard"), "dashboard section");
        assert!(md.contains("Gate:"), "gate verdict line");
    }

    #[test]
    fn render_errors_on_missing_baseline() {
        let result = fixture_two_arms();
        let cfg = ReportConfig {
            baseline_arm: "nonexistent_arm".into(),
            ..Default::default()
        };
        assert!(render(&result, &cfg).is_err());
    }

    #[test]
    fn render_handles_empty_result() {
        let r = BacktestResult {
            arms: BTreeMap::new(),
            cycles_evaluated: 0,
            initial_nav_usd: 0.0,
            started_at: Utc::now(),
            finished_at: Utc::now(),
        };
        // No baseline arm in the result → render must error cleanly.
        assert!(render(&r, &ReportConfig::default()).is_err());
    }
}
