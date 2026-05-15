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
use crate::eval::scenario::TimeWindow;
use crate::eval::store::{DecisionRow, RunStore};

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

    let asset_ref = scenario
        .asset
        .first()
        .ok_or_else(|| ApiError::Internal(format!("scenario '{}' has empty asset list", scenario.id)))?;

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

    // 5. Convert bars to chart shape.
    let chart_bars: Vec<ChartBar> = bars.iter().map(bar_to_chart_bar).collect();

    // 6. Compute indicators. All functions return full-length vectors with
    //    leading NaN warmup; `series()` drops NaN entries before returning.
    let indicators = compute_indicators(&bars);

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
/// decision timestamp; if not found we fall back to 0.0.
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
}

/// Build a `ScenarioChartPayload` for the given scenario id.
///
/// Computes the expected bar count from (end - start) and granularity,
/// checks the `bars_cache` table for the scenario's `cache_key`, and
/// loads bars (cache-hit returns immediately; cache-miss fetches from
/// Alpaca and back-fills — which will fail in tests without credentials,
/// so `NotCached` is returned directly when no cached row exists).
pub async fn build_scenario_payload(ctx: &ApiContext, id: &str) -> ApiResult<ScenarioChartPayload> {
    use crate::api::scenario as api_scenario;

    let scenario = api_scenario::get(ctx, id).await?;

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
        let asset_ref = scenario
            .asset
            .first()
            .ok_or_else(|| ApiError::Internal(format!("scenario '{}' has empty asset list", scenario.id)))?;
        crate::eval::bars::load_bars(
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

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "event", content = "data", rename_all = "snake_case")]
pub enum RunChartEvent {
    Bar(ChartBar),
    IndicatorTail(std::collections::HashMap<String, IndicatorPoint>),
    Decision(LiveDecisionRow),
    Marker(MarkerEvent),
    Equity(ChartEquityPoint),
    Status { phase: String, message: Option<String> },
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
            timestamp: row.timestamp.clone(),
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
        let scenario = crate::api::scenario::get(ctx, &sid).await.map_err(|e| match e {
            ApiError::NotFound(_) => {
                ApiError::NotFound(format!("scenario '{sid}' referenced by compared runs"))
            }
            other => other,
        })?;

        let asset_ref = scenario
            .asset
            .first()
            .ok_or_else(|| ApiError::Internal(format!("scenario '{sid}' has empty asset list")))?;

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
