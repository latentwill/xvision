//! `xvn eval probe-lookahead` — two-pass lookahead-bias prober for a
//! completed eval run.
//!
//! Runs the `LookaheadProber` post-hoc against a specified baseline algorithm,
//! using the same scenario bars as the original run (loaded from the bar
//! cache). Output: pretty-printed list of `LookaheadFinding`s.
//!
//! ## Performance note
//!
//! The prober is 2× the cost of a normal baseline run (two passes). It is
//! opt-in via this CLI subcommand and is not run by default on every eval.

use clap::Args;
use uuid::Uuid;
use xvision_core::market::{IndicatorPanel, MarketSnapshot, Ohlcv, OnchainPanel};
use xvision_core::trading::{AssetSymbol, Regime};
use xvision_data::fixtures::load_ohlcv_fixture;
use xvision_engine::api::eval;
use xvision_engine::api::scenario as api_scenario;
use xvision_engine::eval::findings::{Finding, KIND_LOOKAHEAD_SUSPECTED};
use xvision_eval::baselines::{AlwaysLong, MaCrossover, MacdMomentum, RsiMeanReversion};
use xvision_eval::prober::{LookaheadFinding, LookaheadProber, ProberConfig};

use super::open_ctx;
use crate::exit::{CliResult, ResultExt, XvnExit};

// ---------------------------------------------------------------------------
// Clap args
// ---------------------------------------------------------------------------

#[derive(Args, Debug)]
pub struct ProbeLookaheadArgs {
    /// Run id (ULID) of a completed eval run. The prober uses the run's
    /// scenario to re-load the same bars.
    #[arg(long)]
    pub run: String,

    /// Which baseline to probe.
    /// One of: `always_long`, `ma_crossover`, `macd_momentum`,
    /// `rsi_mean_reversion`, or `all` (default).
    ///
    /// `always_long` is an unconditional-signal baseline; the prober will
    /// report findings for every bar since it reads no bar data. Use
    /// `--skip-always-signal` to suppress.
    #[arg(long, default_value = "all")]
    pub baseline: String,

    /// Skip unconditional-signal baselines (those that fire on every bar
    /// regardless of data). Suppresses expected-but-uninformative findings
    /// from `always_long`.
    #[arg(long)]
    pub skip_always_signal: bool,

    /// Override the xvn home directory.
    #[arg(long)]
    pub xvn_home: Option<std::path::PathBuf>,

    /// Emit findings as JSON (default: human-readable).
    #[arg(long)]
    pub json: bool,

    /// Number of warmup bars to prepend before the scenario window so
    /// stateful algorithms (MaCrossover, MacdMomentum) can reach stable
    /// state before the first signal bar. Default: 0 (bare scenario window).
    ///
    /// Note: only used when bars are loaded from the legacy fixture path.
    /// When bars are cached in the DB, the scenario's `warmup_bars` setting
    /// applies automatically.
    #[arg(long, default_value_t = 0)]
    pub warmup_bars: usize,
}

// ---------------------------------------------------------------------------
// Runner
// ---------------------------------------------------------------------------

pub async fn run_probe_lookahead(args: ProbeLookaheadArgs) -> CliResult<()> {
    let ctx = open_ctx(args.xvn_home.clone())
        .await
        .exit_with(XvnExit::Upstream)?;

    // 1. Load the run to validate it exists and get scenario_id.
    let run = eval::get(&ctx, &args.run)
        .await
        .map_err(|e| super::api_to_cli("probe-lookahead get run", e))?;

    // 2. Load the scenario to get the bar cache key.
    let scenario = api_scenario::get(&ctx, &run.scenario_id)
        .await
        .map_err(|e| super::api_to_cli("probe-lookahead get scenario", e))?;

    let cache_key = &scenario.bar_cache_policy.cache_key;
    // Scenarios are asset-free. The bar fixture is keyed by `cache_key` alone
    // (the symbol arg to `load_ohlcv_fixture` is unused), and this symbol only
    // labels the synthetic snapshots best-effort, so a placeholder suffices.
    let asset_venue_symbol = "BTC/USD";

    eprintln!(
        "probe-lookahead: run={} scenario={} cache_key={}",
        run.id, run.scenario_id, cache_key
    );

    // 3. Load OHLCV bars from the cached fixture.
    //    Uses the legacy load_ohlcv_fixture path (the same one the eval engine
    //    uses for canonical scenarios).  For DB-cached bars, this falls back
    //    gracefully — if the fixture is missing, the prober surfaces a clear error.
    let raw_bars =
        load_ohlcv_fixture(cache_key, asset_venue_symbol, usize::MAX).map_err(|e| crate::exit::CliError {
            exit: XvnExit::Upstream,
            source: anyhow::anyhow!(
                "probe-lookahead: bars not found for scenario '{}' (cache_key='{}'). \
                     Fetch bars with `xvn bars fetch` first: {e}",
                run.scenario_id,
                cache_key,
            ),
        })?;

    if raw_bars.is_empty() {
        eprintln!("probe-lookahead: no bars found — scenario window may be empty. Exiting.");
        return Ok(());
    }

    eprintln!("probe-lookahead: loaded {} bars", raw_bars.len());

    // 4. Convert raw bars to MarketSnapshots.
    //    Each snapshot carries recent_bars = bars[..=t] (the current bar
    //    and its history up to the scenario's window start).
    //    We use a sliding window of the last 200 bars as the `recent_bars`
    //    slice — matching the default `warmup_bars` setting.
    let context_window = (args.warmup_bars + 200).max(200);
    let snapshots = bars_to_snapshots(&raw_bars, context_window, asset_venue_symbol);

    eprintln!(
        "probe-lookahead: constructed {} decision snapshots",
        snapshots.len()
    );

    if snapshots.is_empty() {
        eprintln!("probe-lookahead: no snapshots — not enough bars for warmup. Exiting.");
        return Ok(());
    }

    // 5. Determine which baselines to probe.
    let baselines: Vec<&str> = match args.baseline.as_str() {
        "all" => vec![
            "always_long",
            "ma_crossover",
            "macd_momentum",
            "rsi_mean_reversion",
        ],
        b => vec![b],
    };

    // 6. Run the prober for each baseline.
    let mut all_findings: Vec<(String, Vec<LookaheadFinding>)> = Vec::new();

    for baseline_name in &baselines {
        eprintln!("probe-lookahead: probing baseline '{baseline_name}'…");

        let cfg = ProberConfig {
            algorithm_name: Some(baseline_name.to_string()),
            skip_always_signal: args.skip_always_signal,
        };
        let prober = LookaheadProber::new(cfg);

        let findings = match *baseline_name {
            "always_long" => prober
                .probe(|| Box::new(AlwaysLong), &snapshots)
                .await
                .map_err(|e| crate::exit::CliError {
                    exit: XvnExit::Upstream,
                    source: anyhow::anyhow!("probe-lookahead always_long: {e}"),
                })?,
            "ma_crossover" => prober
                .probe(|| Box::new(MaCrossover::new(30, 90)), &snapshots)
                .await
                .map_err(|e| crate::exit::CliError {
                    exit: XvnExit::Upstream,
                    source: anyhow::anyhow!("probe-lookahead ma_crossover: {e}"),
                })?,
            "macd_momentum" => prober
                .probe(|| Box::new(MacdMomentum::new()), &snapshots)
                .await
                .map_err(|e| crate::exit::CliError {
                    exit: XvnExit::Upstream,
                    source: anyhow::anyhow!("probe-lookahead macd_momentum: {e}"),
                })?,
            "rsi_mean_reversion" => prober
                .probe(|| Box::new(RsiMeanReversion::new()), &snapshots)
                .await
                .map_err(|e| crate::exit::CliError {
                    exit: XvnExit::Upstream,
                    source: anyhow::anyhow!("probe-lookahead rsi_mean_reversion: {e}"),
                })?,
            unknown => {
                return Err(crate::exit::CliError {
                    exit: XvnExit::Usage,
                    source: anyhow::anyhow!(
                        "unknown baseline '{unknown}'. Choose from: always_long, \
                         ma_crossover, macd_momentum, rsi_mean_reversion, all"
                    ),
                });
            }
        };

        eprintln!(
            "probe-lookahead: baseline '{}' → {} finding(s)",
            baseline_name,
            findings.len()
        );
        all_findings.push((baseline_name.to_string(), findings));
    }

    // 7. Build engine Finding objects from LookaheadFindings.
    let run_id_for_findings = run.id.clone();
    let engine_findings: Vec<Finding> = all_findings
        .iter()
        .flat_map(|(baseline_name, findings)| {
            let run_id = run_id_for_findings.clone();
            findings.iter().map(move |f| {
                Finding::lookahead_suspected(
                    &run_id,
                    &f.cycle_id.to_string(),
                    Some(baseline_name.as_str()),
                    &f.pass_1_action,
                    f.pass_2_action.as_deref(),
                    f.snapshot_index,
                )
            })
        })
        .collect();

    // 8. Output.
    if args.json {
        crate::io::print_json(&engine_findings)?;
        return Ok(());
    }

    // Human-readable output.
    if engine_findings.is_empty() {
        println!("probe-lookahead: no lookahead-bias findings detected.");
        println!("  run_id   {}", run.id);
        println!("  scenario {}", run.scenario_id);
        println!("  baselines {}", baselines.join(", "));
        return Ok(());
    }

    println!("probe-lookahead: {} finding(s)", engine_findings.len());
    println!("  run_id   {}", run.id);
    println!("  scenario {}", run.scenario_id);
    println!();

    for f in &engine_findings {
        let indicator = f
            .evidence
            .get("indicator_name")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown");
        let snap_idx = f
            .evidence
            .get("snapshot_index")
            .and_then(|v| v.as_u64())
            .unwrap_or(0);
        let pass1 = f
            .evidence
            .get("pass_1_action")
            .and_then(|v| v.as_str())
            .unwrap_or("?");
        let pass2 = f
            .evidence
            .get("pass_2_action")
            .map(|v| v.as_str().unwrap_or("none"))
            .unwrap_or("none");
        let cycle_id = f.evidence.get("cycle_id").and_then(|v| v.as_str()).unwrap_or("?");

        println!(
            "[{}] cycle={} snapshot={} indicator={} pass1={} pass2={}",
            f.severity.as_str(),
            cycle_id,
            snap_idx,
            indicator,
            pass1,
            pass2
        );
        println!("      {}", f.summary);
        println!();
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Convert a flat OHLCV bar sequence to a sequence of `MarketSnapshot`s.
///
/// Each snapshot at index `t` receives `recent_bars = bars[t.saturating_sub(window)..=t]`
/// (a sliding window of at most `window` bars ending at the current bar).
///
/// Indicators are left as `None` — the four audited baselines use either
/// `recent_bars` (MaCrossover) or IndicatorPanel fields (RSI, MACD, Bollinger).
/// The prober is designed for raw-bar baselines; indicator-based baselines
/// (RSI, MACD) will never fire because their `IndicatorPanel` fields are `None`.
/// This is documented behaviour: the prober catches ~90% of raw-bar-based
/// lookahead; indicator-panel-based lookahead requires a different approach.
fn bars_to_snapshots(bars: &[Ohlcv], context_window: usize, asset_venue_symbol: &str) -> Vec<MarketSnapshot> {
    // Map venue symbol to AssetSymbol (best-effort; default BTC)
    let asset = match asset_venue_symbol.to_uppercase().as_str() {
        s if s.contains("ETH") => AssetSymbol::Eth,
        s if s.contains("SOL") => AssetSymbol::Sol,
        _ => AssetSymbol::Btc,
    };

    bars.iter()
        .enumerate()
        .map(|(t, bar)| {
            let start = t.saturating_sub(context_window.saturating_sub(1));
            let recent: Vec<Ohlcv> = bars[start..=t].to_vec();
            MarketSnapshot {
                cycle_id: Uuid::new_v4(),
                asset,
                timestamp: bar.timestamp,
                price: bar.close,
                volume_24h: None,
                recent_bars: recent,
                indicators: IndicatorPanel::default(),
                onchain: OnchainPanel::default(),
                regime: Regime::Chop,
                horizon_hours: 24,
            }
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bars_to_snapshots_window_is_capped() {
        // 5 bars, context_window=3 → snapshot[4] has recent_bars of length 3
        let bars: Vec<Ohlcv> = (0..5)
            .map(|i| Ohlcv {
                timestamp: chrono::Utc::now() + chrono::Duration::hours(i),
                open: 50_000.0,
                high: 50_100.0,
                low: 49_900.0,
                close: 50_000.0 + i as f64,
                volume: 1_000.0,
            })
            .collect();

        let snaps = bars_to_snapshots(&bars, 3, "BTC/USD");
        assert_eq!(snaps.len(), 5);
        // First snapshot: only 1 bar available
        assert_eq!(snaps[0].recent_bars.len(), 1);
        // Fourth snapshot (index 3): 3 bars (indices 1,2,3 — window = 3)
        assert_eq!(snaps[3].recent_bars.len(), 3);
        // Fifth snapshot (index 4): 3 bars (indices 2,3,4 — window = 3)
        assert_eq!(snaps[4].recent_bars.len(), 3);
    }

    #[test]
    fn bars_to_snapshots_empty_returns_empty() {
        let snaps = bars_to_snapshots(&[], 200, "BTC/USD");
        assert!(snaps.is_empty());
    }
}
