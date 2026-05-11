//! Chart payload types and builder functions.
//!
//! `RunChartPayload` is the server-computed, chart-ready representation of a
//! single eval run. It bundles:
//! - OHLCV bars (from the bars cache via `eval::bars::load_bars`)
//! - Server-computed indicators (SMA/EMA/Bollinger/Donchian/RSI/MACD/ATR)
//! - Equity curve + drawdown series
//! - Per-bar position series
//! - Trade / hold markers derived from `DecisionRow` records
//!
//! Task 2 — types only (no builder yet).
//! Task 3 — `build_run_payload` builder appended below the types.

use serde::{Deserialize, Serialize};

use crate::api::{ApiContext, ApiError, ApiResult};
use crate::eval::scenario::TimeWindow;
use crate::eval::store::RunStore;

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
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Indicators {
    pub sma_20: Vec<IndicatorPoint>,
    pub sma_50: Vec<IndicatorPoint>,
    pub sma_200: Vec<IndicatorPoint>,
    pub ema_20: Vec<IndicatorPoint>,
    pub ema_50: Vec<IndicatorPoint>,
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
#[derive(Debug, Clone, Serialize, Deserialize)]
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
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DonchianSeries {
    pub upper: Vec<IndicatorPoint>,
    pub lower: Vec<IndicatorPoint>,
}

#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts-export",
    ts(export, export_to = "../../../frontend/web/src/api/types.gen/")
)]
#[derive(Debug, Clone, Serialize, Deserialize)]
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
pub async fn build_run_payload(ctx: &ApiContext, run_id: &str) -> ApiResult<RunChartPayload> {
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

    let asset_ref = scenario.asset.first().ok_or_else(|| {
        ApiError::Internal(format!(
            "scenario '{}' has empty asset list",
            scenario.id
        ))
    })?;

    // 3. Load bars from the cache (cache-miss triggers an Alpaca fetch).
    let bars = crate::eval::bars::load_bars(
        ctx,
        &crate::eval::bars::BarCacheArgs {
            cache_key: scenario.bar_cache_policy.cache_key.clone(),
            asset_pair: asset_ref.venue_symbol.clone(),
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

    // 4. Extract price series for indicator computation.
    let closes: Vec<f64> = bars.iter().map(|b| b.close).collect();
    let highs: Vec<f64> = bars.iter().map(|b| b.high).collect();
    let lows: Vec<f64> = bars.iter().map(|b| b.low).collect();
    let times: Vec<i64> = bars.iter().map(|b| b.timestamp.timestamp()).collect();

    // 5. Convert bars to chart shape.
    let chart_bars: Vec<ChartBar> = bars.iter().map(bar_to_chart_bar).collect();

    // 6. Compute indicators. All functions return full-length vectors with
    //    leading NaN warmup; `series()` drops NaN entries before returning.
    let bb = indicators::bollinger(&closes, 20, 2.0);
    let dc = indicators::donchian(&highs, &lows, 20);
    let mc = indicators::macd(&closes, 12, 26, 9);

    let indicators = Indicators {
        sma_20: series(&times, indicators::sma(&closes, 20)),
        sma_50: series(&times, indicators::sma(&closes, 50)),
        sma_200: series(&times, indicators::sma(&closes, 200)),
        ema_20: series(&times, indicators::ema(&closes, 20)),
        ema_50: series(&times, indicators::ema(&closes, 50)),
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
    };

    // 7. Equity curve.
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

    // 8. Drawdown derived from equity.
    let drawdown = compute_drawdown(&equity);

    // 9. Decisions → position series + markers.
    let decisions = store
        .read_decisions(run_id)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    let position = compute_position(&decisions, &bars);
    let markers = split_markers(&decisions, &bars);

    // 10. Granularity string (human-readable).
    let granularity_str = scenario.granularity.as_alpaca_str().to_string();

    Ok(RunChartPayload {
        run_id: run_id.into(),
        scenario_id: scenario.id.clone(),
        asset: asset_ref.symbol.clone(),
        granularity: granularity_str,
        time_window: scenario.time_window.clone(),
        bars: chart_bars,
        indicators,
        equity,
        drawdown,
        position,
        markers,
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

/// Convert a parallel (times, values) pair to `Vec<IndicatorPoint>`, dropping
/// NaN entries. `times` and `values` must be the same length (both come from
/// iterating the same bar slice, so this invariant is guaranteed by the caller).
fn series(times: &[i64], values: Vec<f64>) -> Vec<IndicatorPoint> {
    assert_eq!(
        times.len(),
        values.len(),
        "series: times/values length mismatch"
    );
    values
        .into_iter()
        .enumerate()
        .filter(|(_, v)| !v.is_nan())
        .map(|(i, v)| IndicatorPoint { time: times[i], value: v })
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

/// Walk decisions in decision_index order (already sorted by `read_decisions`)
/// alongside bars in timestamp order, emitting a `PositionPoint` per bar.
///
/// Action mapping:
/// - `"long_open"` with `fill_size` → size += fill_size
/// - `"short_open"` with `fill_size` → size -= fill_size
/// - `"flat"` with `fill_size`       → size = 0.0 (close-out)
/// - `"hold"`                         → no change
fn compute_position(
    decisions: &[crate::eval::store::DecisionRow],
    bars: &[MarketBar],
) -> Vec<PositionPoint> {
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
/// decision timestamp; if not found we fall back to 0.0.
fn split_markers(
    decisions: &[crate::eval::store::DecisionRow],
    bars: &[MarketBar],
) -> ChartMarkers {
    // Build a timestamp → close price index for hold-marker price lookup.
    let bar_close: std::collections::HashMap<i64, f64> = bars
        .iter()
        .map(|b| (b.timestamp.timestamp(), b.close))
        .collect();

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
                let price = bar_close.get(&t).copied().unwrap_or(0.0);
                holds.push(HoldMarker {
                    time: t,
                    price,
                    conviction: d.conviction,
                    decision_index: d.decision_index,
                });
            }
            _ => {}
        }
    }

    ChartMarkers { trades, vetoes, holds }
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
pub async fn build_compare_payload(
    ctx: &ApiContext,
    run_ids: &[String],
) -> ApiResult<CompareChartPayload> {
    if run_ids.len() > 10 {
        return Err(ApiError::Validation(format!(
            "compare view caps at 10 runs (got {}); narrow your filter",
            run_ids.len()
        )));
    }

    let store = RunStore::new(ctx.db.clone());
    let mut series: Vec<CompareRunSeries> = Vec::new();
    let mut scenario_ids: std::collections::HashSet<String> = std::collections::HashSet::new();

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
        let scenario = crate::api::scenario::get(ctx, &sid)
            .await
            .map_err(|e| match e {
                ApiError::NotFound(_) => {
                    ApiError::NotFound(format!("scenario '{sid}' referenced by compared runs"))
                }
                other => other,
            })?;

        let asset_ref = scenario.asset.first().ok_or_else(|| {
            ApiError::Internal(format!("scenario '{sid}' has empty asset list"))
        })?;

        let bars = crate::eval::bars::load_bars(
            ctx,
            &crate::eval::bars::BarCacheArgs {
                cache_key: scenario.bar_cache_policy.cache_key.clone(),
                asset_pair: asset_ref.venue_symbol.clone(),
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
