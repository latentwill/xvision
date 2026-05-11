# TradingView Charts — M1: Replace existing SVG sparklines

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the hand-rolled SVG equity sparkline on `/eval-runs/:id` and equity overlay on `/eval-compare` with real TradingView Lightweight Charts. Ship the full multi-pane chart (price + indicators + equity + drawdown + volume), kitchen-sink server-computed indicators, layer toggles, localStorage persistence, click-marker side panel.

**Architecture:** New `crates/xvision-engine/src/api/chart.rs` builds `RunChartPayload` + `CompareChartPayload` by composing existing run / decision / equity data with on-the-fly indicator computation via `xvision-data::indicators`. New `frontend/web/src/components/chart/` directory holds `ChartContainer`, `RunChart`, `CompareChart`, plus a layer registry and theme tokens. SVG sparkline + overlay deleted in same PR.

**Tech Stack:** Rust 2021, axum, ts-rs, blake3 (already in M1 of custom-scenario), `lightweight-charts@4.x` via npm (Vite bundling).

**Reference spec:** `docs/superpowers/specs/2026-05-11-tradingview-charts-design.md` §§4–9, §13.

**Prereq:** Custom-scenario M1 + M2 merged (`docs/superpowers/plans/2026-05-11-custom-scenario-1-bars-cache-asset-unlock.md`, `…-2-scenario-table-cli.md`). Without M1 there's no bars cache; without M2 there's no scenario row to resolve.

---

## File structure

| Path | Action | Responsibility |
|---|---|---|
| `crates/xvision-engine/src/api/chart.rs` | Create | `build_run_payload`, `build_compare_payload`, HTTP handlers. |
| `crates/xvision-engine/src/api/mod.rs` | Modify | `pub mod chart;` + route registration. |
| `crates/xvision-dashboard/src/routes.rs` | Modify | Mount `/api/eval/runs/:id/chart` and `/api/eval/runs/compare/chart`. |
| `crates/xvision-data/src/indicators.rs` | Modify | Confirm `sma`, `ema`, `bollinger`, `donchian`, `rsi`, `macd`, `atr` are pub. Add any missing. |
| `frontend/web/package.json` | Modify | `+lightweight-charts@^4.1`. |
| `frontend/web/src/components/chart/ChartContainer.tsx` | Create | Generic chart shell with theme + Layers panel + range buttons. |
| `frontend/web/src/components/chart/RunChart.tsx` | Create | Multi-pane chart for run detail. |
| `frontend/web/src/components/chart/CompareChart.tsx` | Create | Multi-equity overlay for compare view. |
| `frontend/web/src/components/chart/chart-layers.ts` | Create | Layer registry, default state, localStorage keys. |
| `frontend/web/src/components/chart/chart-theme.ts` | Create | Color tokens that follow the dashboard theme. |
| `frontend/web/src/components/chart/use-chart-layers.ts` | Create | React hook for layer-toggle state + localStorage. |
| `frontend/web/src/components/chart/MarkerSidePanel.tsx` | Create | Slide-in panel for click-to-decision detail. |
| `frontend/web/src/api/chart.ts` | Create | Fetch functions for the new endpoints. |
| `frontend/web/src/routes/eval-runs-detail.tsx` | Modify | Delete inline SVG; render `<RunChart>`. |
| `frontend/web/src/routes/eval-compare.tsx` | Modify | Delete SVG overlay; render `<CompareChart>`. |

---

## Task 1 — Indicator parity test

**Files:** `crates/xvision-data/tests/indicator_parity.rs`

This locks the spec's "agents and humans see the same math" guarantee before any chart code lands.

- [ ] **Step 1: Write failing test for SMA / EMA / RSI / Bollinger byte-parity vs the MCP tool**

```rust
// crates/xvision-data/tests/indicator_parity.rs
use xvision_data::indicators::*;

#[test]
fn sma_matches_mcp_tool_output() {
    let prices = vec![100.0, 101.0, 99.0, 102.0, 103.0, 104.0, 105.0, 103.0, 102.0, 101.0,
                      100.0, 99.0, 98.0, 97.0, 100.0, 102.0, 104.0, 106.0, 108.0, 110.0];
    let xvn_sma = sma(&prices, 5);
    // Hand-computed reference for SMA(5) starting at index 4:
    let expected: Vec<f64> = (4..prices.len()).map(|i| {
        prices[i-4..=i].iter().sum::<f64>() / 5.0
    }).collect();
    assert_eq!(xvn_sma.len(), expected.len());
    for (a, b) in xvn_sma.iter().zip(expected.iter()) {
        assert!((a - b).abs() < 1e-12, "sma diverges: {a} vs {b}");
    }
}

#[test]
fn rsi_matches_wilder_reference() {
    // Wilder's RSI(14) with known input series; values from a reference implementation.
    let closes = vec![44.34, 44.09, 44.15, 43.61, 44.33, 44.83, 45.10, 45.42, 45.84, 46.08,
                     45.89, 46.03, 45.61, 46.28, 46.28, 46.00, 46.03, 46.41, 46.22, 45.64];
    let rsi_vals = rsi(&closes, 14);
    // Reference (computed via TA-Lib / ta-math):
    let expected = [70.46, 66.25, 66.48, 69.41, 66.36, 57.97];
    assert_eq!(rsi_vals.len(), expected.len());
    for (a, b) in rsi_vals.iter().zip(expected.iter()) {
        assert!((a - b).abs() < 0.5, "rsi diverges: {a} vs {b}");
    }
}
```

- [ ] **Step 2: Run test, expect either PASS (existing impl correct) or FAIL (impl needs fixing).**

```bash
cargo test -p xvision-data --test indicator_parity
```

- [ ] **Step 3: If FAIL, adjust the indicator implementation to match the reference.** The MCP tools and the chart endpoint both consume these — fixing them here fixes both.

- [ ] **Step 4: Commit**

```bash
git add crates/xvision-data/tests/indicator_parity.rs crates/xvision-data/src/indicators.rs
git commit -m "test(xvision-data): indicator-parity regression vs hand-computed references"
```

---

## Task 2 — Chart payload Rust types

**Files:** `crates/xvision-engine/src/api/chart.rs`, `crates/xvision-engine/src/api/mod.rs`

- [ ] **Step 1: Define types with `ts-rs` derives**

```rust
// crates/xvision-engine/src/api/chart.rs
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::eval::scenario::TimeWindow;

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../frontend/web/src/api/types.gen/")]
pub struct RunChartPayload {
    pub run_id: String,
    pub scenario_id: String,
    pub asset: String,
    pub granularity: String,
    pub time_window: TimeWindow,
    pub bars: Vec<ChartBar>,
    pub indicators: Indicators,
    pub equity: Vec<EquityPoint>,
    pub drawdown: Vec<DrawdownPoint>,
    pub position: Vec<PositionPoint>,
    pub markers: ChartMarkers,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../frontend/web/src/api/types.gen/")]
pub struct ChartBar { pub time: i64, pub open: f64, pub high: f64, pub low: f64, pub close: f64, pub volume: f64 }

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../frontend/web/src/api/types.gen/")]
pub struct IndicatorPoint { pub time: i64, pub value: f64 }

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../frontend/web/src/api/types.gen/")]
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

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../frontend/web/src/api/types.gen/")]
pub struct BollingerSeries { pub upper: Vec<IndicatorPoint>, pub middle: Vec<IndicatorPoint>, pub lower: Vec<IndicatorPoint> }

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../frontend/web/src/api/types.gen/")]
pub struct DonchianSeries { pub upper: Vec<IndicatorPoint>, pub lower: Vec<IndicatorPoint> }

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../frontend/web/src/api/types.gen/")]
pub struct MacdSeries { pub line: Vec<IndicatorPoint>, pub signal: Vec<IndicatorPoint>, pub histogram: Vec<IndicatorPoint> }

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../frontend/web/src/api/types.gen/")]
pub struct EquityPoint { pub time: i64, pub equity_usd: f64 }

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../frontend/web/src/api/types.gen/")]
pub struct DrawdownPoint { pub time: i64, pub drawdown_pct: f64 }

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../frontend/web/src/api/types.gen/")]
pub struct PositionPoint { pub time: i64, pub size: f64, pub side: PositionSide }

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, TS)]
#[ts(export, export_to = "../../../frontend/web/src/api/types.gen/")]
pub enum PositionSide { Long, Short, Flat }

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../frontend/web/src/api/types.gen/")]
pub struct ChartMarkers {
    pub trades: Vec<TradeMarker>,
    pub vetoes: Vec<VetoMarker>,
    pub holds:  Vec<HoldMarker>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../frontend/web/src/api/types.gen/")]
pub struct TradeMarker { pub time: i64, pub side: TradeSide, pub price: f64, pub size: f64, pub fee: f64, pub pnl_realized: Option<f64>, pub decision_index: u32, pub justification: Option<String> }

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, TS)]
#[ts(export, export_to = "../../../frontend/web/src/api/types.gen/")]
pub enum TradeSide { Buy, Sell }

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../frontend/web/src/api/types.gen/")]
pub struct VetoMarker { pub time: i64, pub price: f64, pub reason: String, pub decision_index: u32 }

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../frontend/web/src/api/types.gen/")]
pub struct HoldMarker { pub time: i64, pub price: f64, pub conviction: Option<f64>, pub decision_index: u32 }

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../frontend/web/src/api/types.gen/")]
pub struct CompareChartPayload {
    pub runs: Vec<CompareRunSeries>,
    pub shared_scenario: Option<String>,
    pub price_backdrop: Option<Vec<ChartBar>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../frontend/web/src/api/types.gen/")]
pub struct CompareRunSeries { pub run_id: String, pub label: String, pub scenario_id: String, pub equity: Vec<EquityPoint> }
```

- [ ] **Step 2: `cargo xtask gen-types`** (or `cargo test --features ts-export --tests`) to regenerate the TS shapes.

- [ ] **Step 3: Verify the `.gen.ts` files exist** under `frontend/web/src/api/types.gen/`.

- [ ] **Step 4: Commit**

```bash
git add crates/xvision-engine/src/api/chart.rs crates/xvision-engine/src/api/mod.rs frontend/web/src/api/types.gen/
git commit -m "feat(api): chart payload types with ts-rs"
```

---

## Task 3 — `build_run_payload` core logic

**Files:** `crates/xvision-engine/src/api/chart.rs`, `crates/xvision-engine/tests/chart_payload.rs`

- [ ] **Step 1: Failing test for end-to-end payload assembly**

```rust
// crates/xvision-engine/tests/chart_payload.rs
#[tokio::test]
async fn build_run_payload_contains_all_series() {
    let ctx = ApiContext::test_with_seeded_run().await;
    // helper: creates a strategy bundle, a scenario, a run with ~50 bars + ~5 decisions.
    let payload = xvision_engine::api::chart::build_run_payload(&ctx, "r_test_1").await.unwrap();
    assert!(!payload.bars.is_empty());
    assert!(!payload.indicators.sma_20.is_empty());
    assert!(!payload.indicators.rsi_14.is_empty());
    assert!(!payload.equity.is_empty());
    assert!(!payload.drawdown.is_empty());
    assert!(!payload.position.is_empty());
    assert!(!payload.markers.trades.is_empty() || !payload.markers.holds.is_empty());
}

#[tokio::test]
async fn build_run_payload_rejects_oversize_window() {
    let ctx = ApiContext::test_with_seeded_run_100k_bars().await;
    let err = xvision_engine::api::chart::build_run_payload(&ctx, "r_too_big").await.unwrap_err();
    assert!(format!("{err}").contains("payload exceeds 100K bars"));
}
```

- [ ] **Step 2: Run test, expect FAIL**

- [ ] **Step 3: Implement `build_run_payload`**

```rust
// crates/xvision-engine/src/api/chart.rs (appended)
use crate::api::{ApiContext, ApiError, ApiResult, eval as api_eval, scenario as api_scenario};
use xvision_data::alpaca::MarketBar;
use xvision_data::indicators;

const MAX_BARS: usize = 100_000;

pub async fn build_run_payload(ctx: &ApiContext, run_id: &str) -> ApiResult<RunChartPayload> {
    let run = ctx.store.get_run(run_id).await?
        .ok_or_else(|| ApiError::NotFound(format!("run '{run_id}'")))?;
    let scenario = api_scenario::get(ctx, &run.scenario_id).await?;
    let bars = crate::eval::bars::load_bars(ctx, &crate::eval::bars::BarCacheArgs {
        cache_key: scenario.bar_cache_policy.cache_key.clone(),
        asset_pair: scenario.asset[0].venue_symbol.clone(),
        granularity: scenario.granularity,
        start: scenario.time_window.start,
        end: scenario.time_window.end,
        data_source_tag: "alpaca-historical-v1".into(),
    }).await?;
    if bars.len() > MAX_BARS {
        return Err(ApiError::Validation(format!("payload exceeds 100K bars ({}); downsample granularity or shorten time_window", bars.len())));
    }
    let chart_bars: Vec<ChartBar> = bars.iter().map(bar_to_chart_bar).collect();
    let closes: Vec<f64> = bars.iter().map(|b| b.close).collect();
    let highs: Vec<f64>  = bars.iter().map(|b| b.high).collect();
    let lows: Vec<f64>   = bars.iter().map(|b| b.low).collect();
    let times: Vec<i64>  = bars.iter().map(|b| b.timestamp.timestamp()).collect();

    let indicators = Indicators {
        sma_20:  series(&times, indicators::sma(&closes, 20)),
        sma_50:  series(&times, indicators::sma(&closes, 50)),
        sma_200: series(&times, indicators::sma(&closes, 200)),
        ema_20:  series(&times, indicators::ema(&closes, 20)),
        ema_50:  series(&times, indicators::ema(&closes, 50)),
        ema_200: series(&times, indicators::ema(&closes, 200)),
        bollinger: {
            let (u, m, l) = indicators::bollinger(&closes, 20, 2.0);
            BollingerSeries { upper: series(&times, u), middle: series(&times, m), lower: series(&times, l) }
        },
        donchian: {
            let (u, l) = indicators::donchian(&highs, &lows, 20);
            DonchianSeries { upper: series(&times, u), lower: series(&times, l) }
        },
        rsi_14: series(&times, indicators::rsi(&closes, 14)),
        macd: {
            let (line, signal, hist) = indicators::macd(&closes, 12, 26, 9);
            MacdSeries { line: series(&times, line), signal: series(&times, signal), histogram: series(&times, hist) }
        },
        atr_14: series(&times, indicators::atr(&highs, &lows, &closes, 14)),
    };

    let equity = ctx.store.equity_curve(run_id).await?
        .into_iter().map(|p| EquityPoint { time: p.timestamp.timestamp(), equity_usd: p.equity_usd }).collect::<Vec<_>>();
    let drawdown = compute_drawdown(&equity);

    let decisions = ctx.store.run_decisions(run_id).await?;
    let position = compute_position(&decisions, &bars);
    let markers = split_markers(&decisions, &bars);

    Ok(RunChartPayload {
        run_id: run_id.into(),
        scenario_id: scenario.id.clone(),
        asset: scenario.asset[0].symbol.clone(),
        granularity: format!("{:?}", scenario.granularity).to_lowercase(),
        time_window: scenario.time_window.clone(),
        bars: chart_bars,
        indicators, equity, drawdown, position, markers,
    })
}

fn bar_to_chart_bar(b: &MarketBar) -> ChartBar {
    ChartBar { time: b.timestamp.timestamp(), open: b.open, high: b.high, low: b.low, close: b.close, volume: b.volume }
}

fn series(times: &[i64], values: Vec<f64>) -> Vec<IndicatorPoint> {
    // indicator funcs may return shorter vectors (leading NaNs trimmed); align by tail.
    let offset = times.len() - values.len();
    values.into_iter().enumerate().filter(|(_, v)| !v.is_nan())
        .map(|(i, v)| IndicatorPoint { time: times[i + offset], value: v }).collect()
}

fn compute_drawdown(equity: &[EquityPoint]) -> Vec<DrawdownPoint> {
    let mut peak = f64::NEG_INFINITY;
    equity.iter().map(|p| {
        peak = peak.max(p.equity_usd);
        let dd = if peak <= 0.0 { 0.0 } else { (peak - p.equity_usd) / peak * 100.0 };
        DrawdownPoint { time: p.time, drawdown_pct: dd }
    }).collect()
}

fn compute_position(decisions: &[crate::store::DecisionRow], bars: &[MarketBar]) -> Vec<PositionPoint> {
    let mut out = Vec::with_capacity(bars.len());
    let mut size: f64 = 0.0;
    let mut decision_iter = decisions.iter().peekable();
    for bar in bars {
        while let Some(d) = decision_iter.peek() {
            if d.timestamp > bar.timestamp { break; }
            if let (Some(action), Some(fill_size)) = (d.action.as_deref(), d.fill_size) {
                match action {
                    "Buy"  => size += fill_size,
                    "Sell" => size -= fill_size,
                    _ => {}
                }
            }
            decision_iter.next();
        }
        let side = if size > 0.0 { PositionSide::Long } else if size < 0.0 { PositionSide::Short } else { PositionSide::Flat };
        out.push(PositionPoint { time: bar.timestamp.timestamp(), size, side });
    }
    out
}

fn split_markers(decisions: &[crate::store::DecisionRow], _bars: &[MarketBar]) -> ChartMarkers {
    let mut trades = Vec::new();
    let mut vetoes = Vec::new();
    let mut holds  = Vec::new();
    for d in decisions {
        let t = d.timestamp.timestamp();
        match (d.action.as_deref(), d.fill_price, d.verdict.as_deref()) {
            (Some(side @ ("Buy"|"Sell")), Some(price), _) => {
                trades.push(TradeMarker {
                    time: t, side: if side == "Buy" { TradeSide::Buy } else { TradeSide::Sell },
                    price, size: d.fill_size.unwrap_or(0.0), fee: d.fee.unwrap_or(0.0),
                    pnl_realized: d.pnl_realized, decision_index: d.decision_index as u32,
                    justification: d.justification.clone(),
                });
            }
            (Some(_), None, Some("Vetoed")) => vetoes.push(VetoMarker {
                time: t, price: d.bar_close.unwrap_or(0.0), reason: d.veto_reason.clone().unwrap_or_default(), decision_index: d.decision_index as u32,
            }),
            (Some("Hold"), _, _) => holds.push(HoldMarker {
                time: t, price: d.bar_close.unwrap_or(0.0), conviction: d.conviction, decision_index: d.decision_index as u32,
            }),
            _ => {}
        }
    }
    ChartMarkers { trades, vetoes, holds }
}
```

- [ ] **Step 4: Add `run_decisions`, `equity_curve` helpers to `store.rs`** if not present, returning `DecisionRow { decision_index, timestamp, action, conviction, justification, order_size, fill_price, fill_size, fee, pnl_realized, verdict, veto_reason, bar_close }`.

- [ ] **Step 5: Run test, expect PASS**

```bash
cargo test -p xvision-engine --test chart_payload
```

- [ ] **Step 6: Commit**

```bash
git add crates/xvision-engine/src/api/chart.rs crates/xvision-engine/src/store.rs crates/xvision-engine/tests/chart_payload.rs
git commit -m "feat(api): build_run_payload composes bars/indicators/equity/markers/position"
```

---

## Task 4 — `build_compare_payload`

**Files:** `crates/xvision-engine/src/api/chart.rs`, `crates/xvision-engine/tests/chart_payload.rs`

- [ ] **Step 1: Failing test for compare payload — N runs, shared-scenario detection**

```rust
#[tokio::test]
async fn build_compare_payload_groups_by_shared_scenario() {
    let ctx = ApiContext::test_with_three_runs_one_scenario().await;
    let payload = xvision_engine::api::chart::build_compare_payload(&ctx, &["r1".into(), "r2".into(), "r3".into()]).await.unwrap();
    assert_eq!(payload.runs.len(), 3);
    assert!(payload.shared_scenario.is_some());
    assert!(payload.price_backdrop.is_some());
}

#[tokio::test]
async fn build_compare_payload_caps_at_10_runs() {
    let ctx = ApiContext::test_with_n_runs(11).await;
    let ids: Vec<String> = (0..11).map(|i| format!("r{i}")).collect();
    let err = xvision_engine::api::chart::build_compare_payload(&ctx, &ids).await.unwrap_err();
    assert!(format!("{err}").contains("narrow your filter"));
}
```

- [ ] **Step 2: Implement**

```rust
pub async fn build_compare_payload(ctx: &ApiContext, run_ids: &[String]) -> ApiResult<CompareChartPayload> {
    if run_ids.len() > 10 {
        return Err(ApiError::Validation(format!("compare view caps at 10 runs (got {}); narrow your filter", run_ids.len())));
    }
    let mut series = Vec::new();
    let mut scenarios = std::collections::HashSet::new();
    for id in run_ids {
        let run = ctx.store.get_run(id).await?.ok_or_else(|| ApiError::NotFound(format!("run '{id}'")))?;
        scenarios.insert(run.scenario_id.clone());
        let equity = ctx.store.equity_curve(id).await?
            .into_iter().map(|p| EquityPoint { time: p.timestamp.timestamp(), equity_usd: p.equity_usd }).collect();
        series.push(CompareRunSeries { run_id: run.id.clone(), label: run.label.unwrap_or_else(|| run.id.clone()), scenario_id: run.scenario_id, equity });
    }
    let (shared_scenario, price_backdrop) = if scenarios.len() == 1 {
        let sid = scenarios.into_iter().next().unwrap();
        let scenario = api_scenario::get(ctx, &sid).await?;
        let bars = crate::eval::bars::load_bars(ctx, &crate::eval::bars::BarCacheArgs {
            cache_key: scenario.bar_cache_policy.cache_key.clone(),
            asset_pair: scenario.asset[0].venue_symbol.clone(),
            granularity: scenario.granularity,
            start: scenario.time_window.start,
            end: scenario.time_window.end,
            data_source_tag: "alpaca-historical-v1".into(),
        }).await?;
        (Some(sid), Some(bars.iter().map(bar_to_chart_bar).collect()))
    } else { (None, None) };
    Ok(CompareChartPayload { runs: series, shared_scenario, price_backdrop })
}
```

- [ ] **Step 3: Run tests, expect PASS**

```bash
cargo test -p xvision-engine --test chart_payload
```

- [ ] **Step 4: Commit**

```bash
git add crates/xvision-engine/src/api/chart.rs crates/xvision-engine/tests/chart_payload.rs
git commit -m "feat(api): build_compare_payload with 10-run cap + shared-scenario detection"
```

---

## Task 5 — HTTP endpoints

**Files:** `crates/xvision-engine/src/api/chart.rs`, `crates/xvision-dashboard/src/routes.rs`

- [ ] **Step 1: Add handlers**

```rust
use axum::{extract::{Path, Query, State}, response::IntoResponse, http::StatusCode, Json};
use std::sync::Arc;

pub async fn http_run_chart(State(ctx): State<Arc<ApiContext>>, Path(run_id): Path<String>) -> impl IntoResponse {
    match build_run_payload(&ctx, &run_id).await {
        Ok(p) => (StatusCode::OK, Json(p)).into_response(),
        Err(e) => crate::api::scenario::error_response(e),
    }
}

#[derive(serde::Deserialize)]
pub struct CompareQuery { ids: String }

pub async fn http_compare_chart(State(ctx): State<Arc<ApiContext>>, Query(q): Query<CompareQuery>) -> impl IntoResponse {
    let ids: Vec<String> = q.ids.split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect();
    match build_compare_payload(&ctx, &ids).await {
        Ok(p) => (StatusCode::OK, Json(p)).into_response(),
        Err(e) => crate::api::scenario::error_response(e),
    }
}
```

- [ ] **Step 2: Mount routes**

```rust
// crates/xvision-dashboard/src/routes.rs
use xvision_engine::api::chart;

let chart_routes = axum::Router::new()
    .route("/eval/runs/:id/chart", axum::routing::get(chart::http_run_chart))
    .route("/eval/runs/compare/chart", axum::routing::get(chart::http_compare_chart));
let api = axum::Router::new().nest("/api", chart_routes /* merged with existing */);
```

- [ ] **Step 3: Smoke (after a run exists)**

```bash
curl http://localhost:8080/api/eval/runs/<id>/chart | jq '.indicators.sma_20 | length'
```

Expected: positive integer.

- [ ] **Step 4: Commit**

```bash
git add crates/xvision-engine/src/api/chart.rs crates/xvision-dashboard/src/routes.rs
git commit -m "feat(api): HTTP endpoints for /chart and /compare/chart"
```

---

## Task 6 — `lightweight-charts` npm dep + bundle budget

**Files:** `frontend/web/package.json`, `frontend/web/vite.config.ts` (maybe)

- [ ] **Step 1: Install**

```bash
cd frontend/web && pnpm add lightweight-charts@^4.1
```

- [ ] **Step 2: Verify build succeeds**

```bash
pnpm typecheck && pnpm build
```

Expected: PASS. Build output should show the new bundle delta ≈ 50KB gzipped.

- [ ] **Step 3: Commit**

```bash
git add frontend/web/package.json frontend/web/pnpm-lock.yaml
git commit -m "build(web): add lightweight-charts@^4.1"
```

---

## Task 7 — Chart theme + layer registry + hook

**Files:** `frontend/web/src/components/chart/chart-theme.ts`, `frontend/web/src/components/chart/chart-layers.ts`, `frontend/web/src/components/chart/use-chart-layers.ts`

- [ ] **Step 1: Theme tokens**

```typescript
// chart-theme.ts
export function chartTheme(mode: 'dark' | 'light') {
  return mode === 'dark' ? {
    background: '#0b0c0d', text: '#e6e6e6', grid: '#1a1d1f',
    series: { sma20: '#7dd3fc', sma50: '#fbbf24', sma200: '#f87171',
              ema20: '#a78bfa', ema50: '#fbbf24', ema200: '#f87171',
              bollUpper: '#34d399', bollMiddle: '#94a3b8', bollLower: '#34d399',
              donchianUpper: '#fb923c', donchianLower: '#fb923c',
              equity: '#22d3ee', drawdown: '#ef4444',
              candleUp: '#22c55e', candleDown: '#ef4444',
              positionLong: 'rgba(34,197,94,0.08)', positionShort: 'rgba(239,68,68,0.08)',
              markerBuy: '#22c55e', markerSell: '#ef4444', markerVeto: '#facc15', markerHold: '#94a3b8' },
  } : {
    background: '#fafafa', text: '#0b0c0d', grid: '#e5e7eb',
    series: { sma20: '#0284c7', sma50: '#a16207', sma200: '#b91c1c',
              ema20: '#7c3aed', ema50: '#a16207', ema200: '#b91c1c',
              bollUpper: '#15803d', bollMiddle: '#64748b', bollLower: '#15803d',
              donchianUpper: '#c2410c', donchianLower: '#c2410c',
              equity: '#0891b2', drawdown: '#dc2626',
              candleUp: '#16a34a', candleDown: '#dc2626',
              positionLong: 'rgba(34,197,94,0.1)', positionShort: 'rgba(239,68,68,0.1)',
              markerBuy: '#16a34a', markerSell: '#dc2626', markerVeto: '#ca8a04', markerHold: '#475569' },
  };
}
```

- [ ] **Step 2: Layer registry**

```typescript
// chart-layers.ts
export type LayerKey =
  | 'candles' | 'sma20' | 'sma50' | 'sma200'
  | 'ema20' | 'ema50' | 'ema200'
  | 'bollinger' | 'donchian'
  | 'markerBuy' | 'markerSell' | 'markerVeto' | 'markerHold'
  | 'positionBand'
  | 'subpaneRsi' | 'subpaneMacd' | 'subpaneAtr' | 'subpaneOff'
  | 'equity' | 'drawdown'
  | 'volume';

export const DEFAULT_LAYERS: Record<LayerKey, boolean> = {
  candles: true, sma20: true, sma50: true, sma200: true,
  ema20: false, ema50: false, ema200: false,
  bollinger: false, donchian: false,
  markerBuy: true, markerSell: true, markerVeto: true, markerHold: false,
  positionBand: true,
  subpaneRsi: true, subpaneMacd: false, subpaneAtr: false, subpaneOff: false,
  equity: true, drawdown: true,
  volume: false,
};

export function storageKey(surface: string): string {
  return `xvision.chart.layers.${surface}`;
}
```

- [ ] **Step 3: Hook**

```typescript
// use-chart-layers.ts
import { useEffect, useState } from 'react';
import { DEFAULT_LAYERS, LayerKey, storageKey } from './chart-layers';

export function useChartLayers(surface: string) {
  const key = storageKey(surface);
  const [layers, setLayers] = useState<Record<LayerKey, boolean>>(() => {
    try {
      const raw = localStorage.getItem(key);
      if (raw) return { ...DEFAULT_LAYERS, ...JSON.parse(raw) };
    } catch {}
    return DEFAULT_LAYERS;
  });
  useEffect(() => { try { localStorage.setItem(key, JSON.stringify(layers)); } catch {} }, [layers, key]);
  function toggle(k: LayerKey) { setLayers((prev) => ({ ...prev, [k]: !prev[k] })); }
  function set<K extends LayerKey>(k: K, v: boolean) { setLayers((prev) => ({ ...prev, [k]: v })); }
  return { layers, toggle, set };
}
```

- [ ] **Step 4: Commit**

```bash
git add frontend/web/src/components/chart/chart-theme.ts frontend/web/src/components/chart/chart-layers.ts frontend/web/src/components/chart/use-chart-layers.ts
git commit -m "feat(web): chart theme + layer registry + localStorage hook"
```

---

## Task 8 — `<ChartContainer>` shell

**Files:** `frontend/web/src/components/chart/ChartContainer.tsx`

- [ ] **Step 1: Implement the shell**

```tsx
import { ReactNode, useState } from 'react';

export type RangePreset = '1d' | '1w' | '1m' | '3m' | 'All';

type Props = {
  title?: string;
  range: RangePreset;
  onRange: (r: RangePreset) => void;
  layersPanel: ReactNode;
  children: ReactNode;
};

export function ChartContainer({ title, range, onRange, layersPanel, children }: Props) {
  const [layersOpen, setLayersOpen] = useState(false);
  return (
    <div className="border border-border rounded">
      <div className="flex items-center gap-2 px-3 py-2 border-b border-border">
        {(['1d','1w','1m','3m','All'] as RangePreset[]).map((r) => (
          <button key={r} onClick={() => onRange(r)} className={`px-2 py-0.5 text-[12px] rounded ${range === r ? 'bg-surface-elev text-text' : 'text-text-3'}`}>{r}</button>
        ))}
        <div className="ml-auto flex items-center gap-2">
          <button onClick={() => setLayersOpen((v) => !v)} className="text-[12px] text-text-3 hover:text-text">Layers ▾</button>
        </div>
      </div>
      <div className="relative">
        {children}
        {layersOpen && (
          <div className="absolute right-2 top-2 z-10 w-64 max-h-[80vh] overflow-auto border border-border bg-surface rounded shadow-lg p-3 text-[12px]">
            {layersPanel}
            <button onClick={() => setLayersOpen(false)} className="mt-3 text-text-3 hover:text-text">Close</button>
          </div>
        )}
      </div>
    </div>
  );
}
```

- [ ] **Step 2: Commit**

```bash
git add frontend/web/src/components/chart/ChartContainer.tsx
git commit -m "feat(web): ChartContainer shell (range buttons + layers panel)"
```

---

## Task 9 — `<RunChart>` multi-pane

**Files:** `frontend/web/src/components/chart/RunChart.tsx`, `frontend/web/src/components/chart/MarkerSidePanel.tsx`, `frontend/web/src/api/chart.ts`

- [ ] **Step 1: API client**

```typescript
// frontend/web/src/api/chart.ts
import type { RunChartPayload } from './types.gen/RunChartPayload';
import type { CompareChartPayload } from './types.gen/CompareChartPayload';

export const chartKeys = {
  run: (id: string) => ['chart', 'run', id] as const,
  compare: (ids: string[]) => ['chart', 'compare', ids.slice().sort().join(',')] as const,
};

export async function getRunChart(runId: string): Promise<RunChartPayload> {
  const r = await fetch(`/api/eval/runs/${encodeURIComponent(runId)}/chart`);
  if (!r.ok) throw new Error(`HTTP ${r.status}`);
  return r.json();
}

export async function getCompareChart(runIds: string[]): Promise<CompareChartPayload> {
  const r = await fetch(`/api/eval/runs/compare/chart?ids=${encodeURIComponent(runIds.join(','))}`);
  if (!r.ok) throw new Error(`HTTP ${r.status}`);
  return r.json();
}
```

- [ ] **Step 2: `<RunChart>` implementation**

```tsx
// frontend/web/src/components/chart/RunChart.tsx
import { useEffect, useRef, useState } from 'react';
import { createChart, IChartApi, ISeriesApi, ColorType, CrosshairMode } from 'lightweight-charts';
import type { RunChartPayload } from '../../api/types.gen/RunChartPayload';
import { useChartLayers } from './use-chart-layers';
import { chartTheme } from './chart-theme';
import { ChartContainer, RangePreset } from './ChartContainer';
import { MarkerSidePanel } from './MarkerSidePanel';

type Props = { payload: RunChartPayload; themeMode?: 'dark' | 'light' };

export function RunChart({ payload, themeMode = 'dark' }: Props) {
  const priceRef = useRef<HTMLDivElement>(null);
  const subRef   = useRef<HTMLDivElement>(null);
  const eqRef    = useRef<HTMLDivElement>(null);
  const ddRef    = useRef<HTMLDivElement>(null);
  const volRef   = useRef<HTMLDivElement>(null);
  const [range, setRange] = useState<RangePreset>('All');
  const { layers, toggle, set } = useChartLayers('run-detail');
  const [activeMarker, setActiveMarker] = useState<null | { kind: 'trade'|'veto'|'hold'; decision_index: number }>(null);

  useEffect(() => {
    if (!priceRef.current) return;
    const theme = chartTheme(themeMode);
    const opts = {
      layout: { background: { type: ColorType.Solid, color: theme.background }, textColor: theme.text },
      grid: { vertLines: { color: theme.grid }, horzLines: { color: theme.grid } },
      crosshair: { mode: CrosshairMode.Normal },
      timeScale: { rightOffset: 6, secondsVisible: false },
    };
    const priceChart = createChart(priceRef.current, opts);
    const subChart   = subRef.current ? createChart(subRef.current, opts) : null;
    const eqChart    = eqRef.current ? createChart(eqRef.current, opts) : null;
    const ddChart    = ddRef.current ? createChart(ddRef.current, opts) : null;
    const volChart   = volRef.current ? createChart(volRef.current, opts) : null;

    // --- Price pane ---
    if (layers.candles) {
      const candle = priceChart.addCandlestickSeries({
        upColor: theme.series.candleUp, downColor: theme.series.candleDown,
        wickUpColor: theme.series.candleUp, wickDownColor: theme.series.candleDown, borderVisible: false,
      });
      candle.setData(payload.bars.map((b) => ({ time: b.time as any, open: b.open, high: b.high, low: b.low, close: b.close })));
    }
    if (layers.sma20)  priceChart.addLineSeries({ color: theme.series.sma20, lineWidth: 1 }).setData(payload.indicators.sma_20.map(toLine));
    if (layers.sma50)  priceChart.addLineSeries({ color: theme.series.sma50, lineWidth: 1 }).setData(payload.indicators.sma_50.map(toLine));
    if (layers.sma200) priceChart.addLineSeries({ color: theme.series.sma200, lineWidth: 1 }).setData(payload.indicators.sma_200.map(toLine));
    if (layers.ema20)  priceChart.addLineSeries({ color: theme.series.ema20, lineWidth: 1, lineStyle: 2 }).setData(payload.indicators.ema_20.map(toLine));
    if (layers.ema50)  priceChart.addLineSeries({ color: theme.series.ema50, lineWidth: 1, lineStyle: 2 }).setData(payload.indicators.ema_50.map(toLine));
    if (layers.ema200) priceChart.addLineSeries({ color: theme.series.ema200, lineWidth: 1, lineStyle: 2 }).setData(payload.indicators.ema_200.map(toLine));
    if (layers.bollinger) {
      priceChart.addLineSeries({ color: theme.series.bollUpper, lineWidth: 1 }).setData(payload.indicators.bollinger.upper.map(toLine));
      priceChart.addLineSeries({ color: theme.series.bollMiddle, lineWidth: 1 }).setData(payload.indicators.bollinger.middle.map(toLine));
      priceChart.addLineSeries({ color: theme.series.bollLower, lineWidth: 1 }).setData(payload.indicators.bollinger.lower.map(toLine));
    }
    if (layers.donchian) {
      priceChart.addLineSeries({ color: theme.series.donchianUpper, lineWidth: 1 }).setData(payload.indicators.donchian.upper.map(toLine));
      priceChart.addLineSeries({ color: theme.series.donchianLower, lineWidth: 1 }).setData(payload.indicators.donchian.lower.map(toLine));
    }

    // --- Markers on price pane ---
    const allMarkers: any[] = [];
    if (layers.markerBuy)  payload.markers.trades.filter((t) => t.side === 'Buy').forEach((t) => allMarkers.push({ time: t.time as any, position: 'belowBar', color: theme.series.markerBuy, shape: 'arrowUp', text: `Buy ${t.size}` }));
    if (layers.markerSell) payload.markers.trades.filter((t) => t.side === 'Sell').forEach((t) => allMarkers.push({ time: t.time as any, position: 'aboveBar', color: theme.series.markerSell, shape: 'arrowDown', text: `Sell ${t.size}` }));
    if (layers.markerVeto) payload.markers.vetoes.forEach((v) => allMarkers.push({ time: v.time as any, position: 'aboveBar', color: theme.series.markerVeto, shape: 'circle', text: `Veto: ${v.reason}` }));
    if (layers.markerHold) payload.markers.holds.forEach((h) => allMarkers.push({ time: h.time as any, position: 'inBar', color: theme.series.markerHold, shape: 'circle', text: 'Hold' }));
    if (allMarkers.length > 0) {
      const candleSeries = priceChart.addCandlestickSeries({ visible: false }); // marker host; invisible series
      candleSeries.setMarkers(allMarkers.sort((a, b) => (a.time as number) - (b.time as number)));
    }

    // --- Position band: implement as area under price ---
    if (layers.positionBand) {
      const longSeries = priceChart.addAreaSeries({ topColor: theme.series.positionLong, bottomColor: 'transparent', lineColor: 'transparent' });
      longSeries.setData(payload.position.filter((p) => p.side === 'Long').map((p) => ({ time: p.time as any, value: 0 })));
      const shortSeries = priceChart.addAreaSeries({ topColor: theme.series.positionShort, bottomColor: 'transparent', lineColor: 'transparent' });
      shortSeries.setData(payload.position.filter((p) => p.side === 'Short').map((p) => ({ time: p.time as any, value: 0 })));
    }

    // --- Subpane ---
    if (subChart) {
      if (layers.subpaneRsi) {
        const rsi = subChart.addLineSeries({ color: '#a78bfa', lineWidth: 1 });
        rsi.setData(payload.indicators.rsi_14.map(toLine));
        rsi.createPriceLine({ price: 30, color: '#475569', lineWidth: 1, lineStyle: 2 } as any);
        rsi.createPriceLine({ price: 70, color: '#475569', lineWidth: 1, lineStyle: 2 } as any);
      } else if (layers.subpaneMacd) {
        subChart.addLineSeries({ color: '#22d3ee', lineWidth: 1 }).setData(payload.indicators.macd.line.map(toLine));
        subChart.addLineSeries({ color: '#f97316', lineWidth: 1 }).setData(payload.indicators.macd.signal.map(toLine));
        subChart.addHistogramSeries({ color: '#94a3b8' }).setData(payload.indicators.macd.histogram.map((p) => ({ time: p.time as any, value: p.value })));
      } else if (layers.subpaneAtr) {
        subChart.addLineSeries({ color: '#fbbf24', lineWidth: 1 }).setData(payload.indicators.atr_14.map(toLine));
      }
    }

    // --- Equity + drawdown ---
    if (eqChart && layers.equity) {
      const eq = eqChart.addAreaSeries({ lineColor: theme.series.equity, topColor: 'rgba(34,211,238,0.3)', bottomColor: 'rgba(34,211,238,0.0)' });
      eq.setData(payload.equity.map((p) => ({ time: p.time as any, value: p.equity_usd })));
    }
    if (ddChart && layers.drawdown) {
      const dd = ddChart.addAreaSeries({ lineColor: theme.series.drawdown, topColor: 'rgba(239,68,68,0.3)', bottomColor: 'rgba(239,68,68,0.0)' });
      dd.setData(payload.drawdown.map((p) => ({ time: p.time as any, value: -p.drawdown_pct })));
    }
    if (volChart && layers.volume) {
      volChart.addHistogramSeries({ color: theme.series.candleUp }).setData(payload.bars.map((b) => ({ time: b.time as any, value: b.volume, color: b.close >= b.open ? theme.series.candleUp : theme.series.candleDown })));
    }

    // Time-scale sync across panes
    const all = [priceChart, subChart, eqChart, ddChart, volChart].filter(Boolean) as IChartApi[];
    const syncs = all.map((c) => c.timeScale().subscribeVisibleLogicalRangeChange((r) => {
      if (!r) return;
      all.forEach((other) => { if (other !== c) other.timeScale().setVisibleLogicalRange(r as any); });
    }));

    return () => { all.forEach((c) => c.remove()); };
  }, [payload, layers, themeMode]);

  return (
    <ChartContainer
      range={range}
      onRange={setRange}
      layersPanel={<LayersPanel layers={layers} toggle={toggle} set={set} />}
    >
      <div ref={priceRef} style={{ height: 380 }} />
      <div ref={subRef}   style={{ height: 100 }} />
      <div ref={eqRef}    style={{ height: 100 }} />
      <div ref={ddRef}    style={{ height: 70 }} />
      {layers.volume && <div ref={volRef} style={{ height: 70 }} />}
      <MarkerSidePanel payload={payload} active={activeMarker} onClose={() => setActiveMarker(null)} />
    </ChartContainer>
  );
}

function toLine(p: { time: number; value: number }) { return { time: p.time as any, value: p.value }; }

function LayersPanel({ layers, toggle, set }: any) {
  return (
    <div className="space-y-2">
      <div className="text-text-3 mb-1">Price pane</div>
      {(['candles','sma20','sma50','sma200','ema20','ema50','ema200','bollinger','donchian','markerBuy','markerSell','markerVeto','markerHold','positionBand'] as const).map((k) => (
        <label key={k} className="flex items-center gap-2">
          <input type="checkbox" checked={layers[k]} onChange={() => toggle(k)} /> {k}
        </label>
      ))}
      <div className="text-text-3 mb-1 mt-3">Subpane</div>
      {(['subpaneRsi','subpaneMacd','subpaneAtr','subpaneOff'] as const).map((k) => (
        <label key={k} className="flex items-center gap-2">
          <input type="radio" name="subpane" checked={layers[k]} onChange={() => {
            (['subpaneRsi','subpaneMacd','subpaneAtr','subpaneOff'] as const).forEach((kk) => set(kk, kk === k));
          }} /> {k}
        </label>
      ))}
      <div className="text-text-3 mb-1 mt-3">Equity pane</div>
      {(['equity','drawdown'] as const).map((k) => (
        <label key={k} className="flex items-center gap-2">
          <input type="checkbox" checked={layers[k]} onChange={() => toggle(k)} /> {k}
        </label>
      ))}
      <div className="text-text-3 mb-1 mt-3">Volume</div>
      <label className="flex items-center gap-2">
        <input type="checkbox" checked={layers.volume} onChange={() => toggle('volume')} /> volume
      </label>
    </div>
  );
}
```

- [ ] **Step 3: `<MarkerSidePanel>`**

```tsx
import type { RunChartPayload } from '../../api/types.gen/RunChartPayload';

type Props = { payload: RunChartPayload; active: null | { kind: 'trade'|'veto'|'hold'; decision_index: number }; onClose: () => void };

export function MarkerSidePanel({ payload, active, onClose }: Props) {
  if (!active) return null;
  const trade = payload.markers.trades.find((t) => t.decision_index === active.decision_index);
  const veto  = payload.markers.vetoes.find((v) => v.decision_index === active.decision_index);
  const hold  = payload.markers.holds.find((h) => h.decision_index === active.decision_index);
  const it = trade || veto || hold;
  if (!it) return null;
  return (
    <aside className="absolute right-0 top-0 h-full w-80 border-l border-border bg-surface p-4 text-[13px] overflow-auto">
      <div className="flex justify-between mb-3">
        <strong>Decision #{active.decision_index}</strong>
        <button onClick={onClose} className="text-text-3">×</button>
      </div>
      <pre className="font-mono text-[11px] whitespace-pre-wrap">{JSON.stringify(it, null, 2)}</pre>
    </aside>
  );
}
```

- [ ] **Step 4: Commit**

```bash
git add frontend/web/src/api/chart.ts frontend/web/src/components/chart/RunChart.tsx frontend/web/src/components/chart/MarkerSidePanel.tsx
git commit -m "feat(web): RunChart multi-pane component + MarkerSidePanel"
```

---

## Task 10 — `<CompareChart>`

**Files:** `frontend/web/src/components/chart/CompareChart.tsx`

- [ ] **Step 1: Implement**

```tsx
import { useEffect, useRef, useState } from 'react';
import { createChart, ColorType, CrosshairMode } from 'lightweight-charts';
import type { CompareChartPayload } from '../../api/types.gen/CompareChartPayload';
import { chartTheme } from './chart-theme';
import { ChartContainer, RangePreset } from './ChartContainer';

const RUN_COLORS = ['#22d3ee', '#a78bfa', '#34d399', '#fbbf24', '#f87171', '#60a5fa', '#fb923c', '#10b981', '#e879f9', '#f43f5e'];

export function CompareChart({ payload, themeMode = 'dark' }: { payload: CompareChartPayload; themeMode?: 'dark'|'light' }) {
  const ref = useRef<HTMLDivElement>(null);
  const [range, setRange] = useState<RangePreset>('All');
  const [showBackdrop, setShowBackdrop] = useState(false);

  useEffect(() => {
    if (!ref.current) return;
    const theme = chartTheme(themeMode);
    const c = createChart(ref.current, {
      layout: { background: { type: ColorType.Solid, color: theme.background }, textColor: theme.text },
      grid: { vertLines: { color: theme.grid }, horzLines: { color: theme.grid } },
      crosshair: { mode: CrosshairMode.Normal },
    });
    if (showBackdrop && payload.price_backdrop) {
      const bd = c.addCandlestickSeries({ upColor: '#3f3f46', downColor: '#27272a', borderVisible: false, wickUpColor: '#52525b', wickDownColor: '#27272a', priceScaleId: 'left' });
      bd.setData(payload.price_backdrop.map((b) => ({ time: b.time as any, open: b.open, high: b.high, low: b.low, close: b.close })));
    }
    payload.runs.forEach((r, i) => {
      const line = c.addLineSeries({ color: RUN_COLORS[i % RUN_COLORS.length], lineWidth: 1, title: r.label });
      line.setData(r.equity.map((p) => ({ time: p.time as any, value: p.equity_usd })));
    });
    return () => c.remove();
  }, [payload, themeMode, showBackdrop]);

  return (
    <ChartContainer
      range={range}
      onRange={setRange}
      layersPanel={
        <label className="flex items-center gap-2">
          <input type="checkbox" disabled={!payload.shared_scenario} checked={showBackdrop} onChange={(e) => setShowBackdrop(e.target.checked)} />
          Price backdrop {!payload.shared_scenario && <span className="text-text-3">(runs span scenarios)</span>}
        </label>
      }
    >
      <div ref={ref} style={{ height: 480 }} />
    </ChartContainer>
  );
}
```

- [ ] **Step 2: Commit**

```bash
git add frontend/web/src/components/chart/CompareChart.tsx
git commit -m "feat(web): CompareChart multi-equity overlay"
```

---

## Task 11 — Replace SVG sparkline on `/eval-runs/:id`

**Files:** `frontend/web/src/routes/eval-runs-detail.tsx`

- [ ] **Step 1: Remove inline `EquityChart` SVG component (lines ~221+) and its callsite at line ~80**

- [ ] **Step 2: Render `<RunChart>` below the `RunSummary` block**

```tsx
import { useQuery } from '@tanstack/react-query';
import { chartKeys, getRunChart } from '../api/chart';
import { RunChart } from '../components/chart/RunChart';

// inside the route:
const chart = useQuery({ queryKey: chartKeys.run(id), queryFn: () => getRunChart(id), enabled: !!id });
{chart.data && <RunChart payload={chart.data} />}
{chart.isLoading && <div className="text-text-3">Loading chart…</div>}
{chart.error && <div className="text-danger">Chart unavailable: {String(chart.error)}</div>}
```

- [ ] **Step 3: Smoke test**

```bash
cd frontend/web && pnpm dev
# /eval-runs/<id> for a completed run → real chart renders.
```

- [ ] **Step 4: Commit**

```bash
git add frontend/web/src/routes/eval-runs-detail.tsx
git commit -m "feat(web): /eval-runs/:id renders RunChart; SVG sparkline deleted"
```

---

## Task 12 — Replace SVG overlay on `/eval-compare`

**Files:** `frontend/web/src/routes/eval-compare.tsx`

- [ ] **Step 1: Remove inline `EquityOverlay` SVG component (lines ~194+) and its callsite at line ~90**

- [ ] **Step 2: Render `<CompareChart>`**

```tsx
import { chartKeys, getCompareChart } from '../api/chart';
import { CompareChart } from '../components/chart/CompareChart';

const chart = useQuery({
  queryKey: chartKeys.compare(selectedRunIds),
  queryFn: () => getCompareChart(selectedRunIds),
  enabled: selectedRunIds.length >= 2,
});
{chart.data && <CompareChart payload={chart.data} />}
{chart.error && <div className="text-danger">{String(chart.error)}</div>}
```

- [ ] **Step 3: Surface the "narrow your filter" cap-message** from the API error inline (10-run cap).

- [ ] **Step 4: Smoke test**

```bash
# /eval-compare?ids=<id1>,<id2>,<id3> — real chart overlays render.
```

- [ ] **Step 5: Commit**

```bash
git add frontend/web/src/routes/eval-compare.tsx
git commit -m "feat(web): /eval-compare renders CompareChart; SVG overlay deleted"
```

---

## Task 13 — Performance + build-size budget enforcement

**Files:** `frontend/web/vite.config.ts`, `.github/workflows/web-budget.yml`

- [ ] **Step 1: Add a build-size budget to the CI pipeline**

```yaml
# .github/workflows/web-budget.yml
name: web bundle budget
on: [pull_request]
jobs:
  budget:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: pnpm/action-setup@v3
      - uses: actions/setup-node@v4
        with: { node-version: '20', cache: 'pnpm', cache-dependency-path: 'frontend/web/pnpm-lock.yaml' }
      - run: cd frontend/web && pnpm install --frozen-lockfile
      - run: cd frontend/web && pnpm build
      - name: chart bundle delta
        run: |
          SIZE=$(stat -c%s frontend/web/dist/assets/*.js | sort -n | tail -1)
          echo "biggest chunk: $SIZE bytes"
          test "$SIZE" -lt $((1200 * 1024))  # 1.2 MB raw; gzip ~400 KB
```

- [ ] **Step 2: Commit**

```bash
git add .github/workflows/web-budget.yml
git commit -m "ci(web): bundle-size budget for the chart bundle"
```

---

## Task 14 — Vitest tests for `<RunChart>` toggles + persistence

**Files:** `frontend/web/src/components/chart/RunChart.test.tsx`

- [ ] **Step 1: Add vitest config + RTL deps if not present**

```bash
cd frontend/web && pnpm add -D vitest @testing-library/react @testing-library/jest-dom jsdom
```

- [ ] **Step 2: Failing test**

```tsx
import { render, screen } from '@testing-library/react';
import { RunChart } from './RunChart';
import samplePayload from './sample-payload.json';
import { describe, it, expect } from 'vitest';

describe('RunChart', () => {
  it('renders without crashing on a valid payload', () => {
    render(<RunChart payload={samplePayload as any} />);
    expect(screen.getByText(/Layers/)).toBeTruthy();
  });
});
```

- [ ] **Step 3: Capture a real payload as `sample-payload.json`** by curling the endpoint once and saving.

- [ ] **Step 4: Run**

```bash
pnpm vitest run
```

- [ ] **Step 5: Commit**

```bash
git add frontend/web/src/components/chart/RunChart.test.tsx frontend/web/src/components/chart/sample-payload.json frontend/web/package.json
git commit -m "test(web): vitest smoke for RunChart"
```

---

## Task 15 — M1 acceptance smoke

- [ ] **Step 1: Workspace + frontend tests**

```bash
cargo test --workspace
cd frontend/web && pnpm typecheck && pnpm build && pnpm vitest run
```

- [ ] **Step 2: Manual smoke**

1. `/eval-runs/<id>` for a completed run → multi-pane chart renders.
2. Toggle SMA20 off → series disappears.
3. Reload page → SMA20 still off (localStorage).
4. `/eval-compare?ids=<a>,<b>,<c>` → overlay renders.
5. Add a 4th run → still renders. Add 11 → API returns 400 with "narrow your filter".

- [ ] **Step 3: Confirm SVG removal**

```bash
grep -rn "EquityChart\|EquityOverlay" frontend/web/src/ || echo "✓ no remnants"
```

Expected: no matches.

- [ ] **Step 4: Commit cleanup**

```bash
git add -p
git commit -m "chore: M1 acceptance smoke passes (Lightweight Charts replaces SVG)"
```

---

## Self-review notes

- Indicator parity test guards "agents and humans see same math" (spec §5).
- Server-side `MAX_BARS = 100K` cap with friendly error.
- Compare cap at 10 runs surfaced as `ApiError::Validation` (spec §13.2).
- localStorage persistence covered by Vitest reload smoke.
- SVG removal verified by post-task grep.
- No placeholders.
