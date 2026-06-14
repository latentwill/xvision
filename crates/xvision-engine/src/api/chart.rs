//! Chart payload types and builder functions.
//!
//! `RunChartPayload` is the server-computed, chart-ready representation of a
//! single eval run. It contains:
//! - OHLCV bars (from the bars cache via `eval::bars::load_bars`)
//! - Server-computed indicators (SMA/EMA/Bollinger/Donchian/RSI/MACD/ATR)
//! - Equity curve + drawdown series
//! - Per-bar position series
//! - Trade / hold markers derived from `DecisionRow` records
//!
//! Task 2 — types only (no builder yet).
//! Task 3 — `build_run_payload` builder appended below the types.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::api::{ApiContext, ApiError, ApiResult};
use crate::eval::run::{Run, RunMode};
use crate::eval::scenario::TimeWindow;
use crate::eval::store::{DecisionRow, RunStore};
use xvision_core::trading::AssetSymbol;

// ── chart-domain types ──────────────────────────────────────────────────────

#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts-export",
    ts(export, export_to = "../../../frontend/web/src/api/types.gen/")
)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunChartPayload {
    pub run_id: String,
    pub scenario_id: String,
    pub asset: String,
    pub granularity: String,
    pub time_window: TimeWindow,
    pub bars: Vec<ChartBar>,
    pub indicators: Indicators,
    pub equity: Vec<ChartEquityPoint>,
    pub drawdown: Vec<DrawdownPoint>,
    pub position: Vec<PositionPoint>,
    pub markers: ChartMarkers,
    /// Buy-and-hold comparison curve — present only when the request's
    /// `include` set contains `baseline` and the run has cached bars.
    pub baseline_equity: Option<Vec<ChartEquityPoint>>,
}

#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts-export",
    ts(export, export_to = "../../../frontend/web/src/api/types.gen/")
)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChartBar {
    #[cfg_attr(feature = "ts-export", ts(type = "number"))]
    pub time: i64,
    pub open: f64,
    pub high: f64,
    pub low: f64,
    pub close: f64,
    pub volume: f64,
}

#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts-export",
    ts(export, export_to = "../../../frontend/web/src/api/types.gen/")
)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndicatorPoint {
    #[cfg_attr(feature = "ts-export", ts(type = "number"))]
    pub time: i64,
    pub value: f64,
}

#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts-export",
    ts(export, export_to = "../../../frontend/web/src/api/types.gen/")
)]
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Indicators {
    pub sma_20: Vec<IndicatorPoint>,
    pub sma_30: Vec<IndicatorPoint>,
    pub sma_50: Vec<IndicatorPoint>,
    pub sma_60: Vec<IndicatorPoint>,
    pub sma_90: Vec<IndicatorPoint>,
    pub sma_200: Vec<IndicatorPoint>,
    pub ema_20: Vec<IndicatorPoint>,
    pub ema_30: Vec<IndicatorPoint>,
    pub ema_50: Vec<IndicatorPoint>,
    pub ema_60: Vec<IndicatorPoint>,
    pub ema_90: Vec<IndicatorPoint>,
    pub ema_200: Vec<IndicatorPoint>,
    pub bollinger: BollingerSeries,
    pub donchian: DonchianSeries,
    pub rsi_14: Vec<IndicatorPoint>,
    pub macd: MacdSeries,
    pub atr_14: Vec<IndicatorPoint>,
}

#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts-export",
    ts(export, export_to = "../../../frontend/web/src/api/types.gen/")
)]
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct BollingerSeries {
    pub upper: Vec<IndicatorPoint>,
    pub middle: Vec<IndicatorPoint>,
    pub lower: Vec<IndicatorPoint>,
}

#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts-export",
    ts(export, export_to = "../../../frontend/web/src/api/types.gen/")
)]
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DonchianSeries {
    pub upper: Vec<IndicatorPoint>,
    pub lower: Vec<IndicatorPoint>,
}

#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts-export",
    ts(export, export_to = "../../../frontend/web/src/api/types.gen/")
)]
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct MacdSeries {
    pub line: Vec<IndicatorPoint>,
    pub signal: Vec<IndicatorPoint>,
    pub histogram: Vec<IndicatorPoint>,
}

/// Equity point for the chart payload (distinct from `api::eval::EquityPoint`
/// which uses `equity_usd` and `timestamp` for the run-detail API).
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts-export",
    ts(export, export_to = "../../../frontend/web/src/api/types.gen/")
)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChartEquityPoint {
    #[cfg_attr(feature = "ts-export", ts(type = "number"))]
    pub time: i64,
    pub equity_usd: f64,
}

#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts-export",
    ts(export, export_to = "../../../frontend/web/src/api/types.gen/")
)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DrawdownPoint {
    #[cfg_attr(feature = "ts-export", ts(type = "number"))]
    pub time: i64,
    pub drawdown_pct: f64,
}

#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts-export",
    ts(export, export_to = "../../../frontend/web/src/api/types.gen/")
)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PositionPoint {
    #[cfg_attr(feature = "ts-export", ts(type = "number"))]
    pub time: i64,
    pub size: f64,
    pub side: PositionSide,
}

#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts-export",
    ts(export, export_to = "../../../frontend/web/src/api/types.gen/")
)]
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub enum PositionSide {
    Long,
    Short,
    Flat,
}

#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts-export",
    ts(export, export_to = "../../../frontend/web/src/api/types.gen/")
)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChartMarkers {
    pub trades: Vec<TradeMarker>,
    pub vetoes: Vec<VetoMarker>,
    pub holds: Vec<HoldMarker>,
}

#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts-export",
    ts(export, export_to = "../../../frontend/web/src/api/types.gen/")
)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TradeMarker {
    #[cfg_attr(feature = "ts-export", ts(type = "number"))]
    pub time: i64,
    pub side: TradeSide,
    pub price: f64,
    pub size: f64,
    pub fee: f64,
    pub pnl_realized: Option<f64>,
    pub decision_index: u32,
    pub justification: Option<String>,
}

#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts-export",
    ts(export, export_to = "../../../frontend/web/src/api/types.gen/")
)]
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub enum TradeSide {
    Buy,
    Sell,
}

#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts-export",
    ts(export, export_to = "../../../frontend/web/src/api/types.gen/")
)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VetoMarker {
    #[cfg_attr(feature = "ts-export", ts(type = "number"))]
    pub time: i64,
    pub price: f64,
    pub reason: String,
    pub decision_index: u32,
}

#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts-export",
    ts(export, export_to = "../../../frontend/web/src/api/types.gen/")
)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HoldMarker {
    #[cfg_attr(feature = "ts-export", ts(type = "number"))]
    pub time: i64,
    pub price: f64,
    pub conviction: Option<f64>,
    pub decision_index: u32,
}

#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts-export",
    ts(export, export_to = "../../../frontend/web/src/api/types.gen/")
)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompareChartPayload {
    pub runs: Vec<CompareRunSeries>,
    pub shared_scenario: Option<String>,
    pub price_backdrop: Option<Vec<ChartBar>>,
}

#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts-export",
    ts(export, export_to = "../../../frontend/web/src/api/types.gen/")
)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompareRunSeries {
    pub run_id: String,
    pub label: String,
    pub scenario_id: String,
    pub equity: Vec<ChartEquityPoint>,
}

// ── include-set (slim payload selection) ───────────────────────────────────

/// Which payload sections `GET /api/eval/runs/:id/chart?include=…` assembles.
/// Parsed from an explicit allowlist; unknown tokens are ignored and an
/// empty/unrecognized set degrades to equity-only. Indicators are NOT a
/// public token — they ship only on the full (no-param) payload.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IncludeSet {
    pub equity: bool,
    pub bars: bool,
    pub markers: bool,
    pub baseline: bool,
    pub indicators: bool,
}

impl IncludeSet {
    /// Full payload — the behavior when no `include` param is supplied.
    pub fn full() -> Self {
        Self {
            equity: true,
            bars: true,
            markers: true,
            baseline: false,
            indicators: true,
        }
    }

    pub fn parse(raw: &str) -> Self {
        let mut set = Self {
            equity: false,
            bars: false,
            markers: false,
            baseline: false,
            indicators: false,
        };
        for token in raw.split(',').map(str::trim) {
            match token {
                "equity" => set.equity = true,
                "bars" => set.bars = true,
                "markers" => set.markers = true,
                "baseline" => set.baseline = true,
                _ => {}
            }
        }
        if !(set.equity || set.bars || set.markers || set.baseline) {
            set.equity = true;
        }
        set
    }

    /// Bars must be loaded when they ship, when markers need bar context,
    /// or when the buy-and-hold baseline is computed from them.
    pub fn needs_bars(&self) -> bool {
        self.bars || self.markers || self.baseline
    }

    /// Indicators (and the full payload's position spans) compute only on
    /// the full, no-`include`-param payload.
    pub fn needs_indicators(&self) -> bool {
        self.indicators
    }
}

// ── Task 3 — build_run_payload ──────────────────────────────────────────────

use xvision_data::alpaca::MarketBar;
use xvision_data::indicators;

const MAX_BARS: usize = 100_000;

/// Build a `RunChartPayload` for the given run id.
///
/// Composes:
/// - OHLCV bars from the bars cache (via the scenario's `bar_cache_policy`)
/// - Server-computed indicators (SMA 20/50/200, EMA 20/50/200, Bollinger 20/2,
///   Donchian 20, RSI 14, MACD 12/26/9, ATR 14)
/// - Equity curve and drawdown series from `RunStore`
/// - Per-bar position series derived by walking decisions in timestamp order
/// - Trade / hold markers derived from `DecisionRow` records
///
/// Returns `ApiError::NotFound` when the run or scenario does not exist.
/// Returns `ApiError::Validation` when the bar count exceeds `MAX_BARS`.
/// Resolve the asset a strategy trades from its `asset_universe`
/// (single-asset for now — `asset_universe[0]`).
async fn resolve_strategy_asset_for_chart(ctx: &ApiContext, agent_id: &str) -> ApiResult<AssetSymbol> {
    let strategy = crate::api::strategy::get(ctx, agent_id).await?;
    let raw = strategy.manifest.asset_universe.first().ok_or_else(|| {
        ApiError::Validation(format!(
            "strategy '{}' has empty asset_universe",
            strategy.manifest.id
        ))
    })?;
    raw.parse::<AssetSymbol>().map_err(|e| {
        ApiError::Validation(format!(
            "strategy '{}' asset_universe entry '{}' is not a recognised asset: {e}",
            strategy.manifest.id, raw
        ))
    })
}

fn parse_chart_asset(raw: &str, source: &str) -> Option<AssetSymbol> {
    match raw.parse::<AssetSymbol>() {
        Ok(asset) => Some(asset),
        Err(err) => {
            tracing::warn!(
                source,
                asset = %raw,
                error = %err,
                "ignoring unparseable chart asset candidate"
            );
            None
        }
    }
}

fn asset_from_decisions(decisions: &[DecisionRow]) -> Option<AssetSymbol> {
    decisions
        .iter()
        .find_map(|decision| parse_chart_asset(&decision.asset, "eval_decisions.asset"))
}

fn asset_from_live_config(run: &Run) -> Option<AssetSymbol> {
    run.live_config.as_ref().and_then(|config| {
        config.assets.first().and_then(|asset| {
            parse_chart_asset(
                &asset.venue_symbol,
                "eval_runs.live_config_json.assets[0].venue_symbol",
            )
            .or_else(|| parse_chart_asset(&asset.symbol, "eval_runs.live_config_json.assets[0].symbol"))
        })
    })
}

/// Resolve the asset used for a run chart.
///
/// Prefer run-local persisted data over reloading the mutable strategy
/// artifact: old runs can outlive deleted or malformed strategy files, while
/// `eval_decisions.asset` reflects what the executor actually traded.
async fn resolve_run_asset_for_chart(
    ctx: &ApiContext,
    run: &Run,
    decisions: &[DecisionRow],
) -> ApiResult<AssetSymbol> {
    if let Some(asset) = asset_from_decisions(decisions) {
        return Ok(asset);
    }

    if let Some(asset) = asset_from_live_config(run) {
        return Ok(asset);
    }

    match resolve_strategy_asset_for_chart(ctx, &run.agent_id).await {
        Ok(asset) => Ok(asset),
        Err(err) => {
            tracing::warn!(
                run_id = %run.id,
                strategy_id = %run.agent_id,
                error = %err,
                "falling back to BTC/USD for run chart asset resolution"
            );
            Ok(AssetSymbol::Btc)
        }
    }
}

pub async fn build_run_payload(ctx: &ApiContext, run_id: &str) -> ApiResult<RunChartPayload> {
    build_run_payload_with(ctx, run_id, IncludeSet::full()).await
}

pub async fn build_run_payload_with(
    ctx: &ApiContext,
    run_id: &str,
    include: IncludeSet,
) -> ApiResult<RunChartPayload> {
    let store = RunStore::new(ctx.db.clone());

    // 1. Load the run (maps "run not found" to NotFound).
    let run = store.get(run_id).await.map_err(|e| {
        let msg = e.to_string();
        if msg.contains("run not found") {
            ApiError::NotFound(format!("run '{run_id}'"))
        } else {
            ApiError::Internal(msg)
        }
    })?;

    // Read equity + drawdown before either early-return path.
    let equity: Vec<ChartEquityPoint> = store
        .read_equity_curve(run_id)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?
        .into_iter()
        .map(|(ts, equity_usd)| ChartEquityPoint {
            time: ts.timestamp(),
            equity_usd,
        })
        .collect();
    let drawdown = compute_drawdown(&equity);

    // Live runs / empty scenario: return a metric-only payload without bars.
    if run.mode == RunMode::Live || run.scenario_id.is_empty() {
        let decisions = store
            .read_decisions(run_id)
            .await
            .map_err(|e| ApiError::Internal(e.to_string()))?;
        let markers = if include.markers {
            split_markers(&decisions, &[])
        } else {
            ChartMarkers {
                trades: vec![],
                vetoes: vec![],
                holds: vec![],
            }
        };
        return Ok(RunChartPayload {
            run_id: run_id.into(),
            scenario_id: run.scenario_id.clone(),
            asset: String::new(),
            granularity: String::new(),
            time_window: TimeWindow {
                start: Default::default(),
                end: Default::default(),
            },
            bars: vec![],
            indicators: Indicators::default(),
            equity,
            drawdown,
            position: vec![],
            markers,
            baseline_equity: None,
        });
    }

    // Slim early-return path: equity-only (no bars needed).
    // Empty `asset` and `granularity` are intentional here — slim consumers
    // such as the home Pulse band take run metadata from the runs list, not
    // this payload; loading bars just to echo back asset/granularity strings
    // would defeat the point of the slim path.
    if !include.needs_bars() {
        return Ok(RunChartPayload {
            run_id: run_id.into(),
            scenario_id: run.scenario_id.clone(),
            asset: String::new(),
            granularity: String::new(),
            time_window: TimeWindow {
                start: Default::default(),
                end: Default::default(),
            },
            bars: vec![],
            indicators: Indicators::default(),
            equity,
            drawdown,
            position: vec![],
            markers: ChartMarkers {
                trades: vec![],
                vetoes: vec![],
                holds: vec![],
            },
            baseline_equity: None,
        });
    }

    // 2. Resolve the scenario so we know asset + window + granularity.
    let scenario = crate::api::scenario::get(ctx, &run.scenario_id)
        .await
        .map_err(|e| match e {
            ApiError::NotFound(_) => ApiError::NotFound(format!(
                "scenario '{}' referenced by run '{run_id}'",
                run.scenario_id
            )),
            other => other,
        })?;

    let decisions = store
        .read_decisions(run_id)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    // Scenarios are asset-free; the run's traded asset comes from persisted
    // run-local data when available, then the strategy's `asset_universe`,
    // then a BTC/USD backdrop fallback for metadata-only/legacy runs. The
    // per-asset cache key is computed from that asset over the scenario window.
    let asset_sym = resolve_run_asset_for_chart(ctx, &run, &decisions).await?;
    let asset_pair = asset_sym.as_alpaca_pair();
    let cache_key = crate::eval::bars::compute_cache_key(
        &asset_pair,
        scenario.granularity,
        scenario.time_window.start,
        scenario.time_window.end,
        "alpaca-historical-v1",
    );

    // 3. Load bars from the cache (cache-miss triggers an Alpaca fetch).
    let bars = crate::eval::bars::load_bars(
        ctx,
        &crate::eval::bars::BarCacheArgs {
            cache_key,
            asset_pair: asset_pair.clone(),
            granularity: scenario.granularity,
            start: scenario.time_window.start,
            end: scenario.time_window.end,
            data_source_tag: "alpaca-historical-v1".into(),
        },
    )
    .await?;

    if bars.len() > MAX_BARS {
        return Err(ApiError::Validation(format!(
            "payload exceeds 100K bars ({}); downsample granularity or shorten time_window",
            bars.len()
        )));
    }

    // Include-gated assembly.
    let chart_bars: Vec<ChartBar> = if include.bars {
        bars.iter().map(bar_to_chart_bar).collect()
    } else {
        vec![]
    };
    // Indicators (and position spans) compute only on the full, no-include-param payload.
    let indicators = if include.needs_indicators() {
        compute_indicators(&bars)
    } else {
        Indicators::default()
    };
    let position = if include.needs_indicators() {
        // Position spans ship with the full payload only (run-detail page).
        compute_position(&decisions, &bars)
    } else {
        vec![]
    };
    let markers = if include.markers {
        split_markers(&decisions, &bars)
    } else {
        ChartMarkers {
            trades: vec![],
            vetoes: vec![],
            holds: vec![],
        }
    };
    let baseline_equity = if include.baseline {
        compute_baseline_equity(&bars, &equity)
    } else {
        None
    };

    // 10. Granularity string (human-readable).
    let granularity_str = scenario.granularity.as_alpaca_str().to_string();

    Ok(RunChartPayload {
        run_id: run_id.into(),
        scenario_id: scenario.id.clone(),
        asset: asset_sym.as_short().to_string(),
        granularity: granularity_str,
        time_window: scenario.time_window.clone(),
        bars: chart_bars,
        indicators,
        equity,
        drawdown,
        position,
        markers,
        baseline_equity,
    })
}

// ── helpers ─────────────────────────────────────────────────────────────────

fn bar_to_chart_bar(b: &MarketBar) -> ChartBar {
    ChartBar {
        time: b.timestamp.timestamp(),
        open: b.open,
        high: b.high,
        low: b.low,
        close: b.close,
        volume: b.volume,
    }
}

fn compute_indicators(bars: &[MarketBar]) -> Indicators {
    let closes: Vec<f64> = bars.iter().map(|b| b.close).collect();
    let highs: Vec<f64> = bars.iter().map(|b| b.high).collect();
    let lows: Vec<f64> = bars.iter().map(|b| b.low).collect();
    let times: Vec<i64> = bars.iter().map(|b| b.timestamp.timestamp()).collect();
    let bb = indicators::bollinger(&closes, 20, 2.0);
    let dc = indicators::donchian(&highs, &lows, 20);
    let mc = indicators::macd(&closes, 12, 26, 9);

    Indicators {
        sma_20: series(&times, indicators::sma(&closes, 20)),
        sma_30: series(&times, indicators::sma(&closes, 30)),
        sma_50: series(&times, indicators::sma(&closes, 50)),
        sma_60: series(&times, indicators::sma(&closes, 60)),
        sma_90: series(&times, indicators::sma(&closes, 90)),
        sma_200: series(&times, indicators::sma(&closes, 200)),
        ema_20: series(&times, indicators::ema(&closes, 20)),
        ema_30: series(&times, indicators::ema(&closes, 30)),
        ema_50: series(&times, indicators::ema(&closes, 50)),
        ema_60: series(&times, indicators::ema(&closes, 60)),
        ema_90: series(&times, indicators::ema(&closes, 90)),
        ema_200: series(&times, indicators::ema(&closes, 200)),
        bollinger: BollingerSeries {
            upper: series(&times, bb.upper),
            middle: series(&times, bb.middle),
            lower: series(&times, bb.lower),
        },
        donchian: DonchianSeries {
            upper: series(&times, dc.upper),
            lower: series(&times, dc.lower),
        },
        rsi_14: series(&times, indicators::rsi(&closes, 14)),
        macd: MacdSeries {
            line: series(&times, mc.macd),
            signal: series(&times, mc.signal),
            histogram: series(&times, mc.histogram),
        },
        atr_14: series(&times, indicators::atr(&highs, &lows, &closes, 14)),
    }
}

/// Convert a parallel (times, values) pair to `Vec<IndicatorPoint>`, dropping
/// NaN entries. `times` and `values` must be the same length (both come from
/// iterating the same bar slice, so this invariant is guaranteed by the caller).
fn series(times: &[i64], values: Vec<f64>) -> Vec<IndicatorPoint> {
    assert_eq!(times.len(), values.len(), "series: times/values length mismatch");
    values
        .into_iter()
        .enumerate()
        .filter(|(_, v)| !v.is_nan())
        .map(|(i, v)| IndicatorPoint {
            time: times[i],
            value: v,
        })
        .collect()
}

/// Compute per-point drawdown from an equity curve. Returns a parallel vector.
fn compute_drawdown(equity: &[ChartEquityPoint]) -> Vec<DrawdownPoint> {
    let mut peak = f64::NEG_INFINITY;
    equity
        .iter()
        .map(|p| {
            peak = peak.max(p.equity_usd);
            let dd = if peak <= 0.0 {
                0.0
            } else {
                (peak - p.equity_usd) / peak * 100.0
            };
            DrawdownPoint {
                time: p.time,
                drawdown_pct: dd,
            }
        })
        .collect()
}

/// Buy-and-hold equity: $100k initial, proportional to bar close (same
/// convention as the scenario-preview baseline at `build_scenario_preview`),
/// sampled at the equity curve's timestamps so both series share one time
/// axis. Returns `None` when either input is empty.
fn compute_baseline_equity(bars: &[MarketBar], equity: &[ChartEquityPoint]) -> Option<Vec<ChartEquityPoint>> {
    if bars.is_empty() || equity.is_empty() {
        return None;
    }
    // Precondition: bars are time-ordered (enforced by load_bars → validate_bar_series).
    let initial = 100_000.0;
    let first_close = bars[0].close.max(f64::EPSILON);
    let times: Vec<i64> = bars.iter().map(|b| b.timestamp.timestamp()).collect();
    Some(
        equity
            .iter()
            .map(|p| {
                // Latest bar at-or-before the sample; clamp to the first bar.
                let idx = match times.binary_search(&p.time) {
                    Ok(i) => i,
                    Err(0) => 0,
                    Err(i) => i - 1,
                };
                ChartEquityPoint {
                    time: p.time,
                    equity_usd: initial * (bars[idx].close / first_close),
                }
            })
            .collect(),
    )
}

/// Walk decisions in decision_index order (already sorted by `read_decisions`)
/// alongside bars in timestamp order, emitting a `PositionPoint` per bar.
///
/// Action mapping:
/// - `"long_open"` with `fill_size` → size += fill_size
/// - `"short_open"` with `fill_size` → size -= fill_size
/// - `"flat"` with `fill_size`       → size = 0.0 (close-out)
/// - `"hold"`                         → no change
fn compute_position(decisions: &[crate::eval::store::DecisionRow], bars: &[MarketBar]) -> Vec<PositionPoint> {
    let mut out = Vec::with_capacity(bars.len());
    let mut size: f64 = 0.0;
    let mut decision_iter = decisions.iter().peekable();

    for bar in bars {
        // Consume all decisions whose timestamp falls at or before this bar's close.
        while let Some(d) = decision_iter.peek() {
            if d.timestamp > bar.timestamp {
                break;
            }
            if let Some(fill_size) = d.fill_size {
                match d.action.as_str() {
                    "long_open" => size += fill_size,
                    "short_open" => size -= fill_size,
                    "flat" => size = 0.0,
                    _ => {}
                }
            }
            decision_iter.next();
        }

        let side = if size > 0.0 {
            PositionSide::Long
        } else if size < 0.0 {
            PositionSide::Short
        } else {
            PositionSide::Flat
        };
        out.push(PositionPoint {
            time: bar.timestamp.timestamp(),
            size,
            side,
        });
    }
    out
}

/// Derive trade / hold markers from `DecisionRow` records.
///
/// Action → marker mapping:
/// - `"long_open"`  + fill_price + fill_size → `TradeMarker(Buy)`
/// - `"short_open"` + fill_price + fill_size → `TradeMarker(Sell)`
/// - `"flat"`       + fill_price + fill_size → `TradeMarker` with side opposite
///   to implicit position (simplified: always Sell, i.e. closing a long).
///   For v1 this is best-effort; callers needing exact side can reconstruct from
///   the position series.
/// - `"hold"`                                → `HoldMarker`
///
/// Vetoes are not recorded as a distinct action in v1 — add a `verdict`
/// column to `DecisionRow` if needed.
///
/// For `HoldMarker.price` we look up the bar whose timestamp matches the
/// decision timestamp; if not found we skip the marker. Rendering a marker at
/// zero distorts autoscaling and falsely implies a market crash.
fn split_markers(decisions: &[crate::eval::store::DecisionRow], bars: &[MarketBar]) -> ChartMarkers {
    // Build a timestamp → close price index for hold-marker price lookup.
    let bar_close: std::collections::HashMap<i64, f64> =
        bars.iter().map(|b| (b.timestamp.timestamp(), b.close)).collect();

    let mut trades: Vec<TradeMarker> = Vec::new();
    // Vetoes aren't recorded as a distinct action in v1 — add `verdict`
    // column to DecisionRow if needed.
    let vetoes: Vec<VetoMarker> = Vec::new();
    let mut holds: Vec<HoldMarker> = Vec::new();

    for d in decisions {
        let t = d.timestamp.timestamp();
        match d.action.as_str() {
            "long_open" => {
                if let (Some(price), Some(fill_size)) = (d.fill_price, d.fill_size) {
                    trades.push(TradeMarker {
                        time: t,
                        side: TradeSide::Buy,
                        price,
                        size: fill_size,
                        fee: d.fee.unwrap_or(0.0),
                        pnl_realized: d.pnl_realized,
                        decision_index: d.decision_index,
                        justification: d.justification.clone(),
                    });
                }
                // If fill_price/fill_size absent: no-fill (dropped in v1 with TODO below).
                // TODO: emit VetoMarker when a bar_close source is available and
                // a `verdict` column is added to DecisionRow.
            }
            "short_open" => {
                if let (Some(price), Some(fill_size)) = (d.fill_price, d.fill_size) {
                    trades.push(TradeMarker {
                        time: t,
                        side: TradeSide::Sell,
                        price,
                        size: fill_size,
                        fee: d.fee.unwrap_or(0.0),
                        pnl_realized: d.pnl_realized,
                        decision_index: d.decision_index,
                        justification: d.justification.clone(),
                    });
                }
            }
            "flat" => {
                // Close-out trade. Use Sell as the simplified side (closing a long).
                // For v1, exact close-side is best-effort; the position series
                // provides the authoritative state transition.
                if let (Some(price), Some(fill_size)) = (d.fill_price, d.fill_size) {
                    trades.push(TradeMarker {
                        time: t,
                        side: TradeSide::Sell,
                        price,
                        size: fill_size,
                        fee: d.fee.unwrap_or(0.0),
                        pnl_realized: d.pnl_realized,
                        decision_index: d.decision_index,
                        justification: d.justification.clone(),
                    });
                }
            }
            "hold" => {
                if let Some(price) = bar_close.get(&t).copied() {
                    holds.push(HoldMarker {
                        time: t,
                        price,
                        conviction: d.conviction,
                        decision_index: d.decision_index,
                    });
                }
            }
            _ => {}
        }
    }

    ChartMarkers {
        trades,
        vetoes,
        holds,
    }
}

// ── Task 5 — ScenarioChartPayload + CacheStatus ────────────────────────────

/// Cache status for a scenario's bar window.
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts-export",
    ts(export, export_to = "../../../frontend/web/src/api/types.gen/")
)]
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum CacheStatus {
    FullyCached {
        bar_count: u32,
        #[cfg_attr(feature = "ts-export", ts(type = "string"))]
        fetched_at: chrono::DateTime<chrono::Utc>,
    },
    PartiallyCached {
        fetched_count: u32,
        expected_count: u32,
    },
    NotCached {
        expected_count: u32,
    },
}

#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts-export",
    ts(export, export_to = "../../../frontend/web/src/api/types.gen/")
)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScenarioChartPayload {
    pub scenario: crate::eval::scenario::Scenario,
    pub bars: Vec<ChartBar>,
    pub indicators: Indicators,
    pub cache_status: CacheStatus,
    /// The asset the standalone preview resolved to (short ticker, e.g.
    /// `"BTC"`). Scenarios are asset-free; this echoes the operator-chosen
    /// `asset` query param, or the `BTC` default when none was supplied.
    pub preview_asset: String,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct ScenarioChartQuery {
    pub granularity: Option<String>,
    /// Optional preview asset (e.g. `BTC`, `ETH/USD`). Scenarios have no
    /// strategy context, so the operator picks which market backs the
    /// standalone chart preview. Absent ⇒ `BTC/USD`.
    pub asset: Option<String>,
}

/// Build a `ScenarioChartPayload` for the given scenario id.
///
/// Computes the expected bar count from (end - start) and granularity,
/// checks the `bars_cache` table for the scenario's `cache_key`, and
/// loads bars (cache-hit returns immediately; cache-miss fetches from
/// Alpaca and back-fills — which will fail in tests without credentials,
/// so `NotCached` is returned directly when no cached row exists).
pub async fn build_scenario_payload(ctx: &ApiContext, id: &str) -> ApiResult<ScenarioChartPayload> {
    build_scenario_payload_with_granularity(ctx, id, None, None).await
}

pub async fn build_scenario_payload_with_granularity(
    ctx: &ApiContext,
    id: &str,
    granularity: Option<&str>,
    asset: Option<&str>,
) -> ApiResult<ScenarioChartPayload> {
    use crate::api::scenario as api_scenario;

    let mut scenario = api_scenario::get(ctx, id).await?;
    let requested_granularity = match granularity {
        Some(raw) if !raw.trim().is_empty() => raw
            .parse::<xvision_data::alpaca::BarGranularity>()
            .map_err(ApiError::Validation)?,
        _ => scenario.granularity,
    };
    // Scenarios are asset-free; a standalone scenario preview has no strategy
    // to source the asset from. The operator picks a preview asset via the
    // `asset` query param; absent that, the backdrop defaults to the canonical
    // BTC/USD market for v1 crypto scenarios. An unrecognised asset is a
    // validation error rather than a silent BTC fallback. The bar load needs
    // an asset-specific cache key (the scenario's stored cache_key is
    // asset-free), so we always derive it here.
    let preview_asset = match asset {
        Some(raw) if !raw.trim().is_empty() => raw
            .parse::<xvision_core::trading::AssetSymbol>()
            .map_err(ApiError::Validation)?,
        _ => xvision_core::trading::AssetSymbol::Btc,
    };
    if !xvision_data::asset_whitelist::is_alpaca_crypto_supported(preview_asset.as_str()) {
        return Err(ApiError::Validation(format!(
            "asset '{}' not in alpaca crypto whitelist",
            preview_asset.as_str()
        )));
    }
    let asset_pair = xvision_data::asset_whitelist::to_alpaca_pair(preview_asset.as_str());
    let data_source_tag = "alpaca-historical-v1";
    let cache_key = crate::eval::bars::compute_cache_key(
        &asset_pair,
        requested_granularity,
        scenario.time_window.start,
        scenario.time_window.end,
        data_source_tag,
    );

    scenario.granularity = requested_granularity;
    scenario.bar_cache_policy.cache_key = cache_key.clone();

    // Compute expected bar count from the window and granularity.
    let window_secs = (scenario.time_window.end - scenario.time_window.start)
        .num_seconds()
        .max(0) as u64;
    let bar_secs = scenario.granularity.seconds();
    let expected_count = (window_secs / bar_secs) as u32;

    // Query bars_cache for cache status metadata — don't go through
    // load_bars (which would trigger a live Alpaca fetch on miss).
    let cache_row = query_bars_cache_meta(ctx, &scenario.bar_cache_policy.cache_key).await?;

    let cache_status = match cache_row {
        None => CacheStatus::NotCached { expected_count },
        Some((bar_count, fetched_at)) => {
            if bar_count >= expected_count {
                CacheStatus::FullyCached {
                    bar_count,
                    fetched_at,
                }
            } else {
                CacheStatus::PartiallyCached {
                    fetched_count: bar_count,
                    expected_count,
                }
            }
        }
    };

    // Load bars only if cached; otherwise return empty series.
    let market_bars = if matches!(
        cache_status,
        CacheStatus::FullyCached { .. } | CacheStatus::PartiallyCached { .. }
    ) {
        crate::eval::bars::load_bars(
            ctx,
            &crate::eval::bars::BarCacheArgs {
                cache_key: scenario.bar_cache_policy.cache_key.clone(),
                asset_pair,
                granularity: scenario.granularity,
                start: scenario.time_window.start,
                end: scenario.time_window.end,
                data_source_tag: data_source_tag.into(),
            },
        )
        .await?
    } else {
        vec![]
    };
    let bars: Vec<ChartBar> = market_bars.iter().map(bar_to_chart_bar).collect();
    let indicators = compute_indicators(&market_bars);

    Ok(ScenarioChartPayload {
        scenario,
        bars,
        indicators,
        cache_status,
        preview_asset: preview_asset.as_short().to_string(),
    })
}

/// Query just the metadata columns from `bars_cache` (bar_count + fetched_at)
/// without loading the blob. Returns `None` when no cached row exists.
async fn query_bars_cache_meta(
    ctx: &ApiContext,
    cache_key: &str,
) -> ApiResult<Option<(u32, chrono::DateTime<chrono::Utc>)>> {
    let row: Option<(i64, String)> =
        sqlx::query_as("SELECT bar_count, fetched_at FROM bars_cache WHERE cache_key = ?")
            .bind(cache_key)
            .fetch_optional(&ctx.db)
            .await
            .map_err(|e| ApiError::Internal(format!("query_bars_cache_meta: {e}")))?;

    match row {
        None => Ok(None),
        Some((count, ts_str)) => {
            let fetched_at = chrono::DateTime::parse_from_rfc3339(&ts_str)
                .map_err(|e| ApiError::Internal(format!("parse fetched_at: {e}")))?
                .with_timezone(&chrono::Utc);
            Ok(Some((count as u32, fetched_at)))
        }
    }
}

// ── Task 6 — StrategyChartPayload ───────────────────────────────────────────

#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts-export",
    ts(export, export_to = "../../../frontend/web/src/api/types.gen/")
)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunEquitySeries {
    pub run_id: String,
    pub label: String,
    pub scenario_id: String,
    pub final_pnl_usd: f64,
    pub max_drawdown_pct: f64,
    pub sharpe: Option<f64>,
    pub equity_normalised: Vec<ChartEquityPoint>,
}

#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts-export",
    ts(export, export_to = "../../../frontend/web/src/api/types.gen/")
)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StrategyChartPayload {
    pub strategy_id: String,
    pub run_series: Vec<RunEquitySeries>,
    /// `(scenario_id, display_name)` pairs for every scenario referenced by
    /// the runs. Deduplicated and sorted by scenario_id.
    pub scenarios: Vec<(String, String)>,
}

/// Build a `StrategyChartPayload` for the given strategy agent id.
///
/// Lists all runs for the strategy, loads each run's equity curve, normalises
/// the time axis so `time = 0` at run start, and computes headline metrics
/// (final PnL, max drawdown, Sharpe from `metrics_json`).
/// Runs with empty equity curves are included as zero-series rather than
/// omitted, so the frontend always gets a row per run.
pub async fn build_strategy_payload(ctx: &ApiContext, strategy_id: &str) -> ApiResult<StrategyChartPayload> {
    use crate::eval::store::{ListFilter, RunStore};

    let store = RunStore::new(ctx.db.clone());

    let runs = store
        .list(ListFilter {
            agent_id: Some(strategy_id.to_string()),
            ..Default::default()
        })
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    let mut run_series: Vec<RunEquitySeries> = Vec::with_capacity(runs.len());
    let mut scenario_ids: std::collections::HashMap<String, Option<String>> =
        std::collections::HashMap::new();

    for r in &runs {
        // Track scenario ids for name resolution.
        scenario_ids.entry(r.scenario_id.clone()).or_insert(None);

        let raw_equity: Vec<ChartEquityPoint> = store
            .read_equity_curve(&r.id)
            .await
            .map_err(|e| ApiError::Internal(e.to_string()))?
            .into_iter()
            .map(|(ts, equity_usd)| ChartEquityPoint {
                time: ts.timestamp(),
                equity_usd,
            })
            .collect();

        // Normalise time axis to relative offset from first point.
        let time_origin = raw_equity.first().map(|p| p.time).unwrap_or(0);
        let equity_normalised: Vec<ChartEquityPoint> = raw_equity
            .iter()
            .map(|p| ChartEquityPoint {
                time: p.time - time_origin,
                equity_usd: p.equity_usd,
            })
            .collect();

        // Final PnL = last equity − first equity (or 0 if curve is empty).
        let final_pnl_usd = match (raw_equity.first(), raw_equity.last()) {
            (Some(first), Some(last)) => last.equity_usd - first.equity_usd,
            _ => 0.0,
        };

        // Peak-to-trough max drawdown from the normalised equity curve.
        let max_drawdown_pct = compute_max_drawdown_pct(&equity_normalised);

        // Sharpe from run metrics if present.
        let sharpe = r.metrics.as_ref().map(|m| m.sharpe);

        run_series.push(RunEquitySeries {
            run_id: r.id.clone(),
            label: r.id.clone(),
            scenario_id: r.scenario_id.clone(),
            final_pnl_usd,
            max_drawdown_pct,
            sharpe,
            equity_normalised,
        });
    }

    // Resolve scenario display names (best-effort — if the scenario is gone,
    // use the id as fallback).
    for (sid, name_slot) in scenario_ids.iter_mut() {
        if let Ok(s) = crate::api::scenario::get(ctx, sid).await {
            *name_slot = Some(s.display_name);
        }
    }

    let mut scenarios: Vec<(String, String)> = scenario_ids
        .into_iter()
        .map(|(sid, name)| (sid.clone(), name.unwrap_or_else(|| sid.clone())))
        .collect();
    scenarios.sort_by(|a, b| a.0.cmp(&b.0));

    Ok(StrategyChartPayload {
        strategy_id: strategy_id.to_string(),
        run_series,
        scenarios,
    })
}

/// Compute peak-to-trough max drawdown as a percentage from an equity curve.
fn compute_max_drawdown_pct(equity: &[ChartEquityPoint]) -> f64 {
    let mut peak = f64::NEG_INFINITY;
    let mut max_dd: f64 = 0.0;
    for p in equity {
        peak = peak.max(p.equity_usd);
        if peak > 0.0 {
            let dd = (peak - p.equity_usd) / peak * 100.0;
            max_dd = max_dd.max(dd);
        }
    }
    max_dd
}

// ── Task 1 (M3) — RunEventBus and live-stream event types ──────────────────

use tokio::sync::broadcast;

/// Snapshot of a live run's capital-risk state, emitted per bar over SSE.
/// Fields are nullable because the live loop only populates them once the
/// first decision fires; subscribers connected before any decision has run
/// receive the zero-state row with all `Option` fields as `None`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LiveRunStatePayload {
    pub equity_usd: Option<f64>,
    pub unrealized_pnl_usd: Option<f64>,
    pub realized_today_usd: Option<f64>,
    pub daily_loss_remaining_usd: Option<f64>,
    pub drawdown_pct: Option<f64>,
    pub risk_veto_count: i64,
    pub last_decision_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "event", content = "data", rename_all = "snake_case")]
pub enum RunChartEvent {
    Bar(ChartBar),
    IndicatorTail(std::collections::HashMap<String, IndicatorPoint>),
    Decision(LiveDecisionRow),
    Marker(MarkerEvent),
    Equity(ChartEquityPoint),
    /// CT5 (Epic s78 Wave 3, §4): the per-tick capital block for a live
    /// deployment, emitted on the SAME `RunEventBus` the dashboard SSE already
    /// subscribes to (next to `Equity`). The deployment SSE maps this to
    /// `event: metrics` so consumers (`n0k` / `awm` / `8s4`) stream the honest
    /// capital/P&L/drawdown fields per-tick instead of waiting on the 5s poll.
    /// Only the LIVE loop emits this — backtests never do.
    DeploymentMetrics(DeploymentMetricsTick),
    Status {
        phase: String,
        message: Option<String>,
    },
    LiveRunState(LiveRunStatePayload),
}

/// CT5 per-tick capital block (Epic s78 Wave 3, §4). Carried on
/// [`RunChartEvent::DeploymentMetrics`] over the `RunEventBus` and surfaced as
/// the deployment SSE `event: metrics` frame.
///
/// HONESTY MANDATE (§8.1 / §8.9): every capital/P&L field is `Option` — an
/// unsourceable value serializes as `null` (rendered "—" / "no data" in the UI),
/// **NEVER** a fabricated `0`. These are the SAME honest book/execution-sourced
/// numbers the 5s poll surfaces — no value is ever sourced from `agent_runs` or
/// eval summaries. `null`-skipping is intentional: a field with no real data is
/// OMITTED from the frame, not coerced to zero.
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts-export",
    ts(export, export_to = "../../../frontend/web/src/api/types.gen/")
)]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DeploymentMetricsTick {
    /// Bar/decision timestamp (unix seconds) this tick is keyed at. Shared time
    /// axis with the `Equity` ticks.
    #[cfg_attr(feature = "ts-export", ts(type = "number"))]
    pub time: i64,
    /// Pooled NAV at this tick (`book.equity(marks)`). Always present on a live
    /// tick — the equity sample is what triggers the emission.
    pub equity_usd: f64,
    /// `(peak_equity - equity) / peak_equity * 100` from the in-memory per-session
    /// peak. `None` when no positive peak exists yet (NOT a faked `0`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub drawdown_pct: Option<f64>,
    /// Σ open-position notional (`PortfolioBook::open_legs()`). `None` pre-first-fill.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub deployed_capital_usd: Option<f64>,
    /// `book.equity(marks) - initial - book.realized()`. `None` when unavailable.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unrealized_pnl_usd: Option<f64>,
    /// `book.realized()`. `None` when there is no fill history yet (NOT `0`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub realized_pnl_usd: Option<f64>,
    /// Headroom before the enforced daily-loss kill (§6.2). `None` when no kill
    /// policy / no day baseline yet.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub daily_loss_limit_remaining_usd: Option<f64>,
    /// Cumulative filled-trade count for the run.
    pub n_trades: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LiveDecisionRow {
    pub decision_index: u32,
    pub timestamp: DateTime<Utc>,
    pub asset: String,
    pub action: String,
    pub conviction: Option<f64>,
    pub justification: Option<String>,
    pub reasoning: Option<String>,
    pub order_size: Option<f64>,
    pub fill_price: Option<f64>,
    pub fill_size: Option<f64>,
    pub fee: Option<f64>,
    pub pnl_realized: Option<f64>,
}

impl From<&DecisionRow> for LiveDecisionRow {
    fn from(row: &DecisionRow) -> Self {
        Self {
            decision_index: row.decision_index,
            timestamp: row.timestamp,
            asset: row.asset.clone(),
            action: row.action.clone(),
            conviction: row.conviction,
            justification: row.justification.clone(),
            reasoning: row.reasoning.clone(),
            order_size: row.order_size,
            fill_price: row.fill_price,
            fill_size: row.fill_size,
            fee: row.fee,
            pnl_realized: row.pnl_realized,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum MarkerEvent {
    Trade(TradeMarker),
    Veto(VetoMarker),
    Hold(HoldMarker),
}

pub struct RunEventBus {
    senders: tokio::sync::Mutex<std::collections::HashMap<String, broadcast::Sender<RunChartEvent>>>,
}

impl Default for RunEventBus {
    fn default() -> Self {
        Self::new()
    }
}

impl RunEventBus {
    pub fn new() -> Self {
        Self {
            senders: Default::default(),
        }
    }

    pub async fn sender(&self, run_id: &str) -> broadcast::Sender<RunChartEvent> {
        let mut g = self.senders.lock().await;
        g.entry(run_id.into())
            .or_insert_with(|| broadcast::channel(1024).0)
            .clone()
    }

    pub async fn subscribe(&self, run_id: &str) -> broadcast::Receiver<RunChartEvent> {
        self.sender(run_id).await.subscribe()
    }

    pub async fn emit(&self, run_id: &str, event: RunChartEvent) {
        let _ = self.sender(run_id).await.send(event);
    }

    /// Remove the broadcast sender for `run_id` from the map. Once removed,
    /// existing subscribers will see the channel as closed (next `recv` returns
    /// `RecvError::Closed`), giving SSE consumers a clean "stream ending" signal.
    /// Call this after emitting the terminal `Status` event so subscribers drain
    /// the last event before the channel drops.
    pub async fn drop_channel(&self, run_id: &str) {
        self.senders.lock().await.remove(run_id);
    }
}

// ── Task 4 — build_compare_payload ─────────────────────────────────────────

/// Build a `CompareChartPayload` for the given set of run ids.
///
/// Validates that at most 10 runs are requested. Fetches each run's equity
/// curve and assembles a `CompareRunSeries` per run. When all runs share the
/// same scenario, populates `shared_scenario` with the scenario id and
/// `price_backdrop` with the OHLCV bars for that scenario; otherwise both
/// fields are `None`.
///
/// Returns `ApiError::Validation` if more than 10 ids are provided.
/// Returns `ApiError::NotFound` for the first id not found in the store.
pub async fn build_compare_payload(ctx: &ApiContext, run_ids: &[String]) -> ApiResult<CompareChartPayload> {
    if run_ids.len() > 10 {
        return Err(ApiError::Validation(format!(
            "compare view caps at 10 runs (got {}); narrow your filter",
            run_ids.len()
        )));
    }

    let store = RunStore::new(ctx.db.clone());
    let mut series: Vec<CompareRunSeries> = Vec::new();
    let mut scenario_ids: std::collections::HashSet<String> = std::collections::HashSet::new();
    // Asset for the shared-scenario backdrop comes from the first run's
    // persisted decision asset when available, then its launch metadata.
    let mut backdrop_run: Option<Run> = None;

    for id in run_ids {
        let run = store.get(id).await.map_err(|e| {
            let msg = e.to_string();
            if msg.contains("run not found") {
                ApiError::NotFound(format!("run '{id}'"))
            } else {
                ApiError::Internal(msg)
            }
        })?;

        scenario_ids.insert(run.scenario_id.clone());
        if backdrop_run.is_none() {
            backdrop_run = Some(run.clone());
        }

        let equity: Vec<ChartEquityPoint> = store
            .read_equity_curve(id)
            .await
            .map_err(|e| ApiError::Internal(e.to_string()))?
            .into_iter()
            .map(|(ts, equity_usd)| ChartEquityPoint {
                time: ts.timestamp(),
                equity_usd,
            })
            .collect();

        series.push(CompareRunSeries {
            run_id: run.id.clone(),
            label: run.id.clone(),
            scenario_id: run.scenario_id.clone(),
            equity,
        });
    }

    // Shared-scenario detection: populate price_backdrop only when every run
    // is from the same scenario.
    let (shared_scenario, price_backdrop) = if scenario_ids.len() == 1 {
        let sid = scenario_ids.into_iter().next().unwrap();
        let scenario = crate::api::scenario::get(ctx, &sid).await.map_err(|e| match e {
            ApiError::NotFound(_) => {
                ApiError::NotFound(format!("scenario '{sid}' referenced by compared runs"))
            }
            other => other,
        })?;

        let asset_sym = match &backdrop_run {
            Some(run) => {
                let decisions = store
                    .read_decisions(&run.id)
                    .await
                    .map_err(|e| ApiError::Internal(e.to_string()))?;
                resolve_run_asset_for_chart(ctx, run, &decisions).await?
            }
            None => AssetSymbol::Btc,
        };
        let asset_pair = asset_sym.as_alpaca_pair();
        let cache_key = crate::eval::bars::compute_cache_key(
            &asset_pair,
            scenario.granularity,
            scenario.time_window.start,
            scenario.time_window.end,
            "alpaca-historical-v1",
        );

        let bars = crate::eval::bars::load_bars(
            ctx,
            &crate::eval::bars::BarCacheArgs {
                cache_key,
                asset_pair,
                granularity: scenario.granularity,
                start: scenario.time_window.start,
                end: scenario.time_window.end,
                data_source_tag: "alpaca-historical-v1".into(),
            },
        )
        .await?;

        let chart_bars: Vec<ChartBar> = bars.iter().map(bar_to_chart_bar).collect();
        (Some(sid), Some(chart_bars))
    } else {
        (None, None)
    };

    Ok(CompareChartPayload {
        runs: series,
        shared_scenario,
        price_backdrop,
    })
}

// ── Task 3 (M3) — scenario preview transient payload ───────────────────────

#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts-export",
    ts(export, export_to = "../../../frontend/web/src/api/types.gen/")
)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScenarioPreviewPayload {
    pub cache_key: String,
    pub asset: String,
    pub granularity: String,
    pub bars: Vec<ChartBar>,
    pub cache_status: CacheStatus,
    pub baseline_equity: Option<Vec<ChartEquityPoint>>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PreviewQuery {
    pub asset: String,
    pub from: String, // YYYY-MM-DD
    pub to: String,
    pub granularity: String,
    pub baseline: Option<bool>,
}

pub async fn build_scenario_preview(ctx: &ApiContext, q: PreviewQuery) -> ApiResult<ScenarioPreviewPayload> {
    use chrono::NaiveDate;

    // Validate dates.
    let from = NaiveDate::parse_from_str(&q.from, "%Y-%m-%d")
        .map_err(|e| ApiError::Validation(format!("from: {e}")))?
        .and_hms_opt(0, 0, 0)
        .unwrap()
        .and_utc();
    let to = NaiveDate::parse_from_str(&q.to, "%Y-%m-%d")
        .map_err(|e| ApiError::Validation(format!("to: {e}")))?
        .and_hms_opt(0, 0, 0)
        .unwrap()
        .and_utc();
    if from >= to {
        return Err(ApiError::Validation("from must be < to".into()));
    }

    let g = q
        .granularity
        .parse::<xvision_data::alpaca::BarGranularity>()
        .map_err(ApiError::Validation)?;

    // Asset whitelist.
    if !xvision_data::asset_whitelist::is_alpaca_crypto_supported(&q.asset) {
        return Err(ApiError::Validation(format!(
            "asset '{}' not in alpaca crypto whitelist",
            q.asset
        )));
    }
    let pair = xvision_data::asset_whitelist::to_alpaca_pair(&q.asset);
    let data_source_tag = "alpaca-historical-v1";
    let cache_key = crate::eval::bars::compute_cache_key(&pair, g, from, to, data_source_tag);

    // Cache status — query first, before load_bars triggers a fetch.
    let expected_count = preview_expected_bar_count(from, to, g);
    let cache_row = query_bars_cache_meta(ctx, &cache_key).await?;
    let cache_status = match cache_row {
        None => CacheStatus::NotCached { expected_count },
        Some((bar_count, fetched_at)) => {
            if bar_count >= expected_count {
                CacheStatus::FullyCached {
                    bar_count,
                    fetched_at,
                }
            } else {
                CacheStatus::PartiallyCached {
                    fetched_count: bar_count,
                    expected_count,
                }
            }
        }
    };

    // Bars: only load when the cache says we have data. Without credentials
    // a cache-miss load_bars will fail — same defensive pattern used in
    // build_scenario_payload.
    let bars: Vec<ChartBar> = if matches!(
        cache_status,
        CacheStatus::FullyCached { .. } | CacheStatus::PartiallyCached { .. }
    ) {
        crate::eval::bars::load_bars(
            ctx,
            &crate::eval::bars::BarCacheArgs {
                cache_key: cache_key.clone(),
                asset_pair: pair,
                granularity: g,
                start: from,
                end: to,
                data_source_tag: data_source_tag.into(),
            },
        )
        .await?
        .iter()
        .map(bar_to_chart_bar)
        .collect()
    } else {
        Vec::new()
    };

    // Optional Buy-and-Hold baseline equity (initial = $100k, proportional
    // to bar close).
    let baseline_equity = if q.baseline.unwrap_or(false) && !bars.is_empty() {
        let initial = 100_000.0;
        let first_close = bars.first().map(|b| b.close).unwrap_or(1.0);
        Some(
            bars.iter()
                .map(|b| ChartEquityPoint {
                    time: b.time,
                    equity_usd: initial * (b.close / first_close.max(f64::EPSILON)),
                })
                .collect(),
        )
    } else {
        None
    };

    Ok(ScenarioPreviewPayload {
        cache_key,
        asset: q.asset,
        granularity: g.canonical(),
        bars,
        cache_status,
        baseline_equity,
    })
}

fn preview_expected_bar_count(
    from: chrono::DateTime<chrono::Utc>,
    to: chrono::DateTime<chrono::Utc>,
    g: xvision_data::alpaca::BarGranularity,
) -> u32 {
    let secs = (to - from).num_seconds().max(0) as u64;
    let bar_secs = g.seconds();
    (secs / bar_secs) as u32
}

#[cfg(test)]
mod include_set_tests {
    use super::IncludeSet;

    #[test]
    fn parse_single_token() {
        let s = IncludeSet::parse("equity");
        assert!(s.equity && !s.bars && !s.markers && !s.baseline && !s.indicators);
    }

    #[test]
    fn parse_multiple_tokens_with_whitespace() {
        let s = IncludeSet::parse(" bars , markers ");
        assert!(s.bars && s.markers && !s.equity && !s.baseline);
    }

    #[test]
    fn parse_ignores_unknown_tokens() {
        let s = IncludeSet::parse("equity,bogus,indicators");
        // "indicators" is deliberately NOT a public token — full payload only.
        assert!(s.equity && !s.indicators && !s.bars);
    }

    #[test]
    fn parse_empty_or_garbage_degrades_to_equity_only() {
        for raw in ["", "  ", "bogus", ",,,"] {
            let s = IncludeSet::parse(raw);
            assert!(s.equity, "raw={raw:?} should degrade to equity-only");
            assert!(!s.bars && !s.markers && !s.baseline && !s.indicators);
        }
    }

    #[test]
    fn full_enables_everything_except_baseline() {
        let s = IncludeSet::full();
        assert!(s.equity && s.bars && s.markers && s.indicators);
        assert!(!s.baseline, "full payload does not compute baseline");
    }

    #[test]
    fn needs_bars_when_bars_markers_or_baseline() {
        assert!(IncludeSet::parse("bars").needs_bars());
        assert!(IncludeSet::parse("markers").needs_bars());
        assert!(IncludeSet::parse("equity,baseline").needs_bars());
        assert!(!IncludeSet::parse("equity").needs_bars());
    }

    #[test]
    fn needs_indicators_only_on_full() {
        assert!(IncludeSet::full().needs_indicators());
        assert!(!IncludeSet::parse("bars,markers").needs_indicators());
        assert!(!IncludeSet::parse("equity").needs_indicators());
    }
}

#[cfg(test)]
mod baseline_tests {
    use super::{compute_baseline_equity, ChartEquityPoint};
    use chrono::TimeZone;
    use xvision_data::alpaca::MarketBar;

    fn bar(offset_h: i64, close: f64) -> MarketBar {
        let ts =
            chrono::Utc.with_ymd_and_hms(2025, 1, 1, 0, 0, 0).unwrap() + chrono::Duration::hours(offset_h);
        MarketBar {
            timestamp: ts,
            open: close,
            high: close + 1.0,
            low: close - 1.0,
            close,
            volume: 1_000.0,
        }
    }

    fn eq_point(offset_h: i64, equity_usd: f64) -> ChartEquityPoint {
        let ts =
            chrono::Utc.with_ymd_and_hms(2025, 1, 1, 0, 0, 0).unwrap() + chrono::Duration::hours(offset_h);
        ChartEquityPoint {
            time: ts.timestamp(),
            equity_usd,
        }
    }

    #[test]
    fn baseline_is_100k_buy_and_hold_sampled_at_equity_times() {
        let bars = vec![bar(0, 100.0), bar(1, 110.0), bar(2, 120.0)];
        let equity = vec![
            eq_point(0, 100_000.0),
            eq_point(1, 99_000.0),
            eq_point(2, 101_000.0),
        ];
        let baseline = compute_baseline_equity(&bars, &equity).unwrap();
        assert_eq!(baseline.len(), 3);
        assert_eq!(baseline[0].time, equity[0].time);
        assert!((baseline[0].equity_usd - 100_000.0).abs() < 1e-6);
        assert!((baseline[1].equity_usd - 110_000.0).abs() < 1e-6);
        assert!((baseline[2].equity_usd - 120_000.0).abs() < 1e-6);
    }

    #[test]
    fn baseline_uses_latest_bar_at_or_before_sample() {
        let bars = vec![bar(0, 100.0), bar(1, 110.0), bar(2, 120.0)];
        let mid = ChartEquityPoint {
            time: bars[1].timestamp.timestamp() + 1_800,
            equity_usd: 100_500.0,
        };
        let baseline = compute_baseline_equity(&bars, &[mid]).unwrap();
        assert!((baseline[0].equity_usd - 110_000.0).abs() < 1e-6);
    }

    #[test]
    fn baseline_clamps_samples_before_first_bar() {
        let bars = vec![bar(1, 100.0), bar(2, 110.0)];
        let early = ChartEquityPoint {
            time: bars[0].timestamp.timestamp() - 3_600,
            equity_usd: 100_000.0,
        };
        let baseline = compute_baseline_equity(&bars, &[early]).unwrap();
        assert!((baseline[0].equity_usd - 100_000.0).abs() < 1e-6);
    }

    #[test]
    fn baseline_none_on_empty_inputs() {
        let bars = vec![bar(0, 100.0)];
        let equity = vec![eq_point(0, 1.0)];
        assert!(compute_baseline_equity(&[], &equity).is_none());
        assert!(compute_baseline_equity(&bars, &[]).is_none());
    }

    #[test]
    fn baseline_holds_last_close_after_final_bar() {
        let bars = vec![bar(0, 100.0), bar(1, 110.0)];
        let late = ChartEquityPoint {
            time: bars[1].timestamp.timestamp() + 7_200,
            equity_usd: 100_000.0,
        };
        let baseline = compute_baseline_equity(&bars, &[late]).unwrap();
        assert!((baseline[0].equity_usd - 110_000.0).abs() < 1e-6);
    }
}
