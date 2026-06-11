# Pulse Chart Fast-Load + View Switcher Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make the dashboard hero chart load near-instantly (slim `include=` payloads), fix the duplicate-looking drawdown line (band only), and add a 5-view chip switcher (Return % / Price + trades / vs Buy & Hold / Drawdown / All runs).

**Architecture:** Backend adds an `IncludeSet`-gated assembly path to `build_run_payload` (new `build_run_payload_with`; old signature delegates with `IncludeSet::full()`), plus a server-computed buy-and-hold `baseline_equity` sampled at equity timestamps. Frontend adds pure selectors in `features/home/pulse.ts`, a `PulseViewSwitcher` chip row as its own sub-row in `PulseBand`, and four small view components reusing chart-v2 primitives. Spec: `docs/superpowers/specs/2026-06-11-pulse-chart-views-design.md`.

**Tech Stack:** Rust (axum, sqlx/SQLite, ts-rs), React + TypeScript, TanStack Query, uPlot, klinecharts (`KlineCandlePane`), Vitest + Testing Library, tokio tests.

**Working directory:** `/Users/edkennedy/Code/xvision/.worktrees/pulse-chart-views` (branch `feat/pulse-chart-views`, issue `xvision-0er`).

**Cargo invocations:** ALWAYS `export CARGO_TARGET_DIR="$HOME/.cargo-target/xvision-pulse-chart-views"` first (per-branch dir — this branch touches the shared `xvision-engine` crate; the shared dir collides across concurrent checkouts), and build through `scripts/cargo` (disk guard wrapper).

**Frontend test command:** `cd frontend/web && npx vitest run <file>` (or `npm test -- <file>` if the repo's package.json defines it that way — check `package.json` scripts once and reuse).

---

### Task 1: Rust — `IncludeSet` (pure parser)

**Files:**
- Modify: `crates/xvision-engine/src/api/chart.rs` (add after the type definitions, around line 250)
- Test: same file, `#[cfg(test)] mod include_set_tests` at the bottom

- [ ] **Step 1: Write the failing tests**

Append to `crates/xvision-engine/src/api/chart.rs`:

```rust
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
```

- [ ] **Step 2: Run tests to verify they fail**

```bash
export CARGO_TARGET_DIR="$HOME/.cargo-target/xvision-pulse-chart-views"
scripts/cargo test -p xvision-engine include_set_tests 2>&1 | tail -20
```
Expected: COMPILE ERROR — `IncludeSet` not found.

- [ ] **Step 3: Implement `IncludeSet`**

Add to `chart.rs` after the marker types (~line 250), before the builder section:

```rust
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
```

- [ ] **Step 4: Run tests to verify they pass**

```bash
scripts/cargo test -p xvision-engine include_set_tests 2>&1 | tail -5
```
Expected: `6 passed`.

- [ ] **Step 5: Commit**

```bash
git add crates/xvision-engine/src/api/chart.rs
git commit -m "feat(chart): IncludeSet allowlist parser for slim run-chart payloads (xvision-0er)"
```

---

### Task 2: Rust — `compute_baseline_equity` (pure)

**Files:**
- Modify: `crates/xvision-engine/src/api/chart.rs` (helpers section, near `compute_drawdown`)
- Test: same file, `#[cfg(test)] mod baseline_tests`

- [ ] **Step 1: Write the failing tests**

```rust
#[cfg(test)]
mod baseline_tests {
    use super::{compute_baseline_equity, ChartEquityPoint};
    use chrono::TimeZone;
    // MarketBar lives in xvision_data::alpaca (chart.rs already imports it
    // around line 281 for compute_indicators) — NOT xvision_core::trading.
    use xvision_data::alpaca::MarketBar;

    fn bar(offset_h: i64, close: f64) -> MarketBar {
        let ts = chrono::Utc
            .with_ymd_and_hms(2025, 1, 1, 0, 0, 0)
            .unwrap()
            + chrono::Duration::hours(offset_h);
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
        let ts = chrono::Utc
            .with_ymd_and_hms(2025, 1, 1, 0, 0, 0)
            .unwrap()
            + chrono::Duration::hours(offset_h);
        ChartEquityPoint { time: ts.timestamp(), equity_usd }
    }

    #[test]
    fn baseline_is_100k_buy_and_hold_sampled_at_equity_times() {
        // Closes 100 → 110 → 120 over 3 hourly bars.
        let bars = vec![bar(0, 100.0), bar(1, 110.0), bar(2, 120.0)];
        let equity = vec![eq_point(0, 100_000.0), eq_point(1, 99_000.0), eq_point(2, 101_000.0)];
        let baseline = compute_baseline_equity(&bars, &equity).unwrap();
        assert_eq!(baseline.len(), 3);
        assert_eq!(baseline[0].time, equity[0].time);
        assert!((baseline[0].equity_usd - 100_000.0).abs() < 1e-6);
        assert!((baseline[1].equity_usd - 110_000.0).abs() < 1e-6);
        assert!((baseline[2].equity_usd - 120_000.0).abs() < 1e-6);
    }

    #[test]
    fn baseline_uses_latest_bar_at_or_before_sample() {
        // Equity sampled at +90min falls between the +1h and +2h bars →
        // uses the +1h close (110).
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
}
```

NOTE: `MarketBar` is `xvision_data::alpaca::MarketBar` — the same type `compute_indicators(bars: &[MarketBar])` already uses in this file (imported ~line 281). If field names differ from the fixture above, mirror the construction used by `bar_to_chart_bar` (chart.rs:527) which reads `b.timestamp, b.open, b.high, b.low, b.close, b.volume`.

- [ ] **Step 2: Run tests to verify they fail**

```bash
scripts/cargo test -p xvision-engine baseline_tests 2>&1 | tail -20
```
Expected: COMPILE ERROR — `compute_baseline_equity` not found.

- [ ] **Step 3: Implement**

Add to the helpers section of `chart.rs` (near `compute_drawdown`):

```rust
/// Buy-and-hold equity: $100k initial, proportional to bar close (same
/// convention as the scenario-preview baseline at `build_scenario_preview`),
/// sampled at the equity curve's timestamps so both series share one time
/// axis. Returns `None` when either input is empty.
fn compute_baseline_equity(
    bars: &[MarketBar],
    equity: &[ChartEquityPoint],
) -> Option<Vec<ChartEquityPoint>> {
    if bars.is_empty() || equity.is_empty() {
        return None;
    }
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
```

- [ ] **Step 4: Run tests to verify they pass**

```bash
scripts/cargo test -p xvision-engine baseline_tests 2>&1 | tail -5
```
Expected: `4 passed`.

- [ ] **Step 5: Commit**

```bash
git add crates/xvision-engine/src/api/chart.rs
git commit -m "feat(chart): buy-and-hold baseline sampled at equity timestamps (xvision-0er)"
```

---

### Task 3: Rust — include-aware `build_run_payload_with` + `baseline_equity` field

**Files:**
- Modify: `crates/xvision-engine/src/api/chart.rs:31-43` (struct), `:381-523` (builder)
- Test: `crates/xvision-engine/tests/chart_payload.rs` (existing harness: `test_ctx()`, `seed_cached_bars()`)

- [ ] **Step 1: Write the failing integration tests**

Append to `crates/xvision-engine/tests/chart_payload.rs`:

```rust
// ── include-set payload variants (pulse chart views) ────────────────────────

use chrono::Utc;
use xvision_engine::api::chart::IncludeSet;

/// Seed a completed backtest run with decisions + an equity curve against
/// the canonical scenario, with bars cached for ETH/USD.
async fn seed_backtest_run_with_equity(ctx: &ApiContext) -> String {
    let scenario = xvision_engine::api::scenario::get(ctx, "crypto-bull-q1-2025")
        .await
        .unwrap();
    let cache_key = xvision_engine::eval::bars::compute_cache_key(
        "ETH/USD",
        scenario.granularity,
        scenario.time_window.start,
        scenario.time_window.end,
        "alpaca-historical-v1",
    );
    seed_cached_bars(ctx, &cache_key, "ETH/USD", 8).await;

    let store = RunStore::new(ctx.db.clone());
    let run = Run::new_queued("pulse-test-strategy".into(), scenario.id.clone(), RunMode::Backtest);
    store.create(&run).await.unwrap();
    store
        .record_decision(&hold_decision_for_asset(&run.id, "ETH/USD"))
        .await
        .unwrap();

    // Equity samples aligned to the first three seeded bar hours
    // (seed_cached_bars starts at 2025-01-01T00:00Z, hourly).
    let t0 = chrono::TimeZone::with_ymd_and_hms(&Utc, 2025, 1, 1, 0, 0, 0).unwrap();
    let samples: Vec<(chrono::DateTime<Utc>, f64)> = (0..3)
        .map(|i| (t0 + chrono::Duration::hours(i), 100_000.0 + i as f64 * 500.0))
        .collect();
    store.record_equity_batch(&run.id, &samples).await.unwrap();
    run.id
}

#[tokio::test]
async fn include_equity_only_skips_bars_indicators_markers() {
    let ctx = test_ctx().await;
    let run_id = seed_backtest_run_with_equity(&ctx).await;

    let payload = xvision_engine::api::chart::build_run_payload_with(
        &ctx,
        &run_id,
        IncludeSet::parse("equity"),
    )
    .await
    .unwrap();

    assert_eq!(payload.equity.len(), 3, "equity always ships");
    assert_eq!(payload.drawdown.len(), 3, "drawdown derives from equity");
    assert!(payload.bars.is_empty(), "equity-only must not ship bars");
    assert!(payload.indicators.sma_20.is_empty(), "indicators skipped");
    assert!(payload.indicators.macd.line.is_empty(), "indicators skipped");
    assert!(payload.markers.holds.is_empty(), "markers skipped");
    assert!(payload.position.is_empty(), "position skipped");
    assert!(payload.baseline_equity.is_none(), "baseline not requested");
}

#[tokio::test]
async fn include_bars_markers_skips_indicators_but_ships_candles() {
    let ctx = test_ctx().await;
    let run_id = seed_backtest_run_with_equity(&ctx).await;

    let payload = xvision_engine::api::chart::build_run_payload_with(
        &ctx,
        &run_id,
        IncludeSet::parse("bars,markers"),
    )
    .await
    .unwrap();

    assert_eq!(payload.bars.len(), 8, "bars ship");
    assert_eq!(payload.markers.holds.len(), 1, "markers ship");
    assert!(payload.indicators.sma_20.is_empty(), "indicators skipped");
    assert!(payload.indicators.rsi_14.is_empty(), "indicators skipped");
    assert!(!payload.equity.is_empty(), "equity always ships");
}

#[tokio::test]
async fn include_baseline_ships_aligned_buy_and_hold() {
    let ctx = test_ctx().await;
    let run_id = seed_backtest_run_with_equity(&ctx).await;

    let payload = xvision_engine::api::chart::build_run_payload_with(
        &ctx,
        &run_id,
        IncludeSet::parse("equity,baseline"),
    )
    .await
    .unwrap();

    let baseline = payload.baseline_equity.expect("baseline requested");
    assert_eq!(baseline.len(), payload.equity.len(), "aligned to equity");
    for (b, e) in baseline.iter().zip(payload.equity.iter()) {
        assert_eq!(b.time, e.time, "baseline sampled at equity timestamps");
    }
    // seed_cached_bars closes are base+1 with base=100+i → first close 101,
    // second 102: baseline[1] = 100k * 102/101.
    assert!((baseline[0].equity_usd - 100_000.0).abs() < 1e-6);
    assert!((baseline[1].equity_usd - 100_000.0 * (102.0 / 101.0)).abs() < 1e-6);
    assert!(payload.bars.is_empty(), "baseline mode does not SHIP bars");
}

#[tokio::test]
async fn live_run_baseline_is_null_not_error() {
    let ctx = test_ctx().await;
    let store = RunStore::new(ctx.db.clone());
    let run = Run::new_queued("live-strategy".into(), String::new(), RunMode::Live);
    store.create(&run).await.unwrap();

    let payload = xvision_engine::api::chart::build_run_payload_with(
        &ctx,
        &run.id,
        IncludeSet::parse("equity,baseline"),
    )
    .await
    .unwrap();
    assert!(payload.baseline_equity.is_none());
    assert!(payload.bars.is_empty());
}

#[tokio::test]
async fn full_payload_unchanged_and_baseline_absent() {
    let ctx = test_ctx().await;
    let run_id = seed_backtest_run_with_equity(&ctx).await;

    let payload = xvision_engine::api::chart::build_run_payload(&ctx, &run_id)
        .await
        .unwrap();
    assert_eq!(payload.bars.len(), 8);
    assert!(!payload.indicators.ema_20.is_empty() || payload.bars.len() < 20,
        "full payload computes indicators (empty only from warmup)");
    assert_eq!(payload.markers.holds.len(), 1);
    assert!(payload.baseline_equity.is_none(), "full mode never computes baseline");
}
```

NOTE: `Run::new_queued` — verify the exact constructor signature used by the existing test at `chart_payload.rs:164` and mirror it. If `RunMode::Live` runs need different construction, check `crates/xvision-engine/src/eval/run.rs` for the field to set; the live early-return triggers on `run.mode == RunMode::Live || run.scenario_id.is_empty()` so an empty `scenario_id` with `RunMode::Backtest` also exercises it if `new_queued` rejects Live.

- [ ] **Step 2: Run tests to verify they fail**

```bash
scripts/cargo test -p xvision-engine --test chart_payload 2>&1 | tail -20
```
Expected: COMPILE ERROR — `build_run_payload_with` / `baseline_equity` not found.

- [ ] **Step 3: Implement struct field + builder split**

3a. Add to `RunChartPayload` (chart.rs:31-43), after `markers`:

```rust
    /// Buy-and-hold comparison curve — present only when the request's
    /// `include` set contains `baseline` and the run has cached bars.
    pub baseline_equity: Option<Vec<ChartEquityPoint>>,
```

(The `#[cfg_attr(feature = "ts-export", …)]` derives already sit on the struct; no per-field attribute is needed for an `Option<Vec<…>>` of an already-exported type.)

3b. Derive empty indicators: add `Default` to the derive list of ALL FOUR structs — `Indicators` (chart.rs:79), `BollingerSeries` (:105), `DonchianSeries` (:117), AND `MacdSeries` (:128) — each becomes `#[derive(Debug, Clone, Serialize, Deserialize, Default)]`. `Indicators::default()` does not compile unless every nested struct also derives `Default`.

3c. Rewrite the builder (chart.rs:381-523). The old entry point delegates:

```rust
pub async fn build_run_payload(ctx: &ApiContext, run_id: &str) -> ApiResult<RunChartPayload> {
    build_run_payload_with(ctx, run_id, IncludeSet::full()).await
}

pub async fn build_run_payload_with(
    ctx: &ApiContext,
    run_id: &str,
    include: IncludeSet,
) -> ApiResult<RunChartPayload> {
    let store = RunStore::new(ctx.db.clone());

    // 1. Load the run (maps "run not found" to NotFound).  [unchanged]
    let run = store.get(run_id).await.map_err(|e| {
        let msg = e.to_string();
        if msg.contains("run not found") {
            ApiError::NotFound(format!("run '{run_id}'"))
        } else {
            ApiError::Internal(msg)
        }
    })?;

    // Live runs have no scenario; metric-only payload without bars. [unchanged
    // except: baseline_equity: None, markers gated on include]
    if run.mode == RunMode::Live || run.scenario_id.is_empty() {
        let decisions = store
            .read_decisions(run_id)
            .await
            .map_err(|e| ApiError::Internal(e.to_string()))?;
        let equity: Vec<ChartEquityPoint> = store
            .read_equity_curve(run_id)
            .await
            .map_err(|e| ApiError::Internal(e.to_string()))?
            .into_iter()
            .map(|(ts, equity_usd)| ChartEquityPoint { time: ts.timestamp(), equity_usd })
            .collect();
        let drawdown = compute_drawdown(&equity);
        let markers = if include.markers {
            split_markers(&decisions, &[])
        } else {
            ChartMarkers { trades: vec![], vetoes: vec![], holds: vec![] }
        };
        return Ok(RunChartPayload {
            run_id: run_id.into(),
            scenario_id: run.scenario_id.clone(),
            asset: String::new(),
            granularity: String::new(),
            time_window: TimeWindow { start: Default::default(), end: Default::default() },
            bars: vec![],
            indicators: Indicators::default(),
            equity,
            drawdown,
            position: vec![],
            markers,
            baseline_equity: None,
        });
    }

    // Equity curve is needed by every include mode — read it first.
    let equity: Vec<ChartEquityPoint> = store
        .read_equity_curve(run_id)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?
        .into_iter()
        .map(|(ts, equity_usd)| ChartEquityPoint { time: ts.timestamp(), equity_usd })
        .collect();
    let drawdown = compute_drawdown(&equity);

    // Slim equity-only path: no scenario resolution, no bar I/O, no
    // indicator computation. asset/granularity/window are empty — the home
    // Pulse views don't read them (run metadata comes from the runs list).
    if !include.needs_bars() {
        return Ok(RunChartPayload {
            run_id: run_id.into(),
            scenario_id: run.scenario_id.clone(),
            asset: String::new(),
            granularity: String::new(),
            time_window: TimeWindow { start: Default::default(), end: Default::default() },
            bars: vec![],
            indicators: Indicators::default(),
            equity,
            drawdown,
            position: vec![],
            markers: ChartMarkers { trades: vec![], vetoes: vec![], holds: vec![] },
            baseline_equity: None,
        });
    }

    // 2.–4. Scenario, decisions, asset resolution, bar load. [unchanged —
    // keep the existing code verbatim: scenario fetch, read_decisions,
    // resolve_run_asset_for_chart, compute_cache_key, load_bars, MAX_BARS
    // guard]
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
    let asset_sym = resolve_run_asset_for_chart(ctx, &run, &decisions).await?;
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

    // 5.–9. Include-gated assembly.
    let chart_bars: Vec<ChartBar> = if include.bars {
        bars.iter().map(bar_to_chart_bar).collect()
    } else {
        vec![]
    };
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
        ChartMarkers { trades: vec![], vetoes: vec![], holds: vec![] }
    };
    let baseline_equity = if include.baseline {
        compute_baseline_equity(&bars, &equity)
    } else {
        None
    };

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
```

IMPORTANT details while editing:
- The original function read equity at step 7; the new shape reads it before the slim early-return. Delete the now-duplicate equity/drawdown reads from the lower section.
- `ChartMarkers` may not be constructible literally if fields differ — check its definition (chart.rs:191) and construct accordingly (it is `{trades, vetoes, holds}`).
- Any OTHER Rust call sites constructing `RunChartPayload` literally (search: `rg "RunChartPayload \{" crates/`) must gain `baseline_equity: None`.
- `position` gating piggybacks on `include.indicators` (full-mode only) — add the brief comment shown; do not invent a `position` token.

- [ ] **Step 4: Run tests to verify they pass**

```bash
scripts/cargo test -p xvision-engine --test chart_payload 2>&1 | tail -10
scripts/cargo test -p xvision-engine include_set_tests baseline_tests 2>&1 | tail -5
```
Expected: all pass, including the pre-existing chart_payload tests (full-payload behavior unchanged).

- [ ] **Step 5: Workspace compile check (other RunChartPayload constructors)**

```bash
scripts/cargo check --workspace 2>&1 | tail -10
```
Expected: clean. If `xvision-dashboard` or SSE code constructs `RunChartPayload`, add `baseline_equity: None` there.

- [ ] **Step 6: Commit**

```bash
git add crates/
git commit -m "feat(chart): include-gated build_run_payload_with + baseline_equity field (xvision-0er)"
```

---

### Task 4: Rust — route wiring (`?include=`)

**Files:**
- Modify: `crates/xvision-dashboard/src/routes/eval_runs.rs:315-325`

- [ ] **Step 1: Update the handler**

```rust
#[derive(serde::Deserialize)]
pub struct ChartParams {
    pub include: Option<String>,
}

/// `GET /api/eval/runs/:id/chart?include=equity,baseline` — build the chart
/// payload for a single run. Absent `include` returns the full payload
/// (back-compat for the run-detail page); see `IncludeSet::parse` for the
/// token allowlist.
pub async fn chart(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Query(params): Query<ChartParams>,
) -> Result<Json<RunChartPayload>, DashboardError> {
    let include = params
        .include
        .as_deref()
        .map(chart_api::IncludeSet::parse)
        .unwrap_or_else(chart_api::IncludeSet::full);
    let payload = chart_api::build_run_payload_with(&state.api_context(), &id, include).await?;
    Ok(Json(payload))
}
```

(`chart_api` is the existing alias `use xvision_engine::api::chart as chart_api` at the top of the file — `IncludeSet` and `build_run_payload_with` ride through it. `Query` is already imported at line 15.)

- [ ] **Step 2: Compile + run dashboard tests**

```bash
scripts/cargo test -p xvision-dashboard 2>&1 | tail -5
```
Expected: PASS (no behavior change for include-less requests).

- [ ] **Step 3: Commit**

```bash
git add crates/xvision-dashboard/src/routes/eval_runs.rs
git commit -m "feat(dashboard): include query param on run chart route (xvision-0er)"
```

---

### Task 5: Regenerate TS types (ts-rs)

**Files:**
- Generated: `frontend/web/src/api/types.gen/RunChartPayload.ts`

- [ ] **Step 1: Regenerate**

```bash
scripts/cargo test -p xvision-engine --features ts-export 2>&1 | tail -5
```
Expected: PASS; exporter rewrites `types.gen/`.

- [ ] **Step 2: Verify the field landed**

```bash
grep baseline_equity frontend/web/src/api/types.gen/RunChartPayload.ts
```
Expected: `baseline_equity: Array<ChartEquityPoint> | null` appears.

- [ ] **Step 3: Frontend typecheck still green**

```bash
cd frontend/web && npx tsc --noEmit 2>&1 | tail -5; cd ../..
```
Expected: clean (field is additive).

- [ ] **Step 4: Commit**

```bash
git add frontend/web/src/api/types.gen/
git commit -m "chore(types): regenerate RunChartPayload with baseline_equity (xvision-0er)"
```

---

### Task 6: Frontend — `getRunChart` include + cache keys

**Files:**
- Modify: `frontend/web/src/api/chart.ts:13-32`
- Test: Create `frontend/web/src/api/chart.test.ts`

- [ ] **Step 1: Write the failing tests**

```ts
// frontend/web/src/api/chart.test.ts
import { describe, expect, it } from "vitest";
import { chartKeys, runChartIncludeKey, runChartPath } from "./chart";

describe("runChartIncludeKey", () => {
  it("is empty for the full payload", () => {
    expect(runChartIncludeKey(undefined)).toBe("");
    expect(runChartIncludeKey([])).toBe("");
  });

  it("sorts tokens canonically so key order never splits the cache", () => {
    expect(runChartIncludeKey(["baseline", "equity"])).toBe("baseline,equity");
    expect(runChartIncludeKey(["equity", "baseline"])).toBe("baseline,equity");
  });
});

describe("chartKeys.run", () => {
  it("locks the key shape [chart, run, id, includeKey]", () => {
    expect(chartKeys.run("r1")).toEqual(["chart", "run", "r1", ""]);
    expect(chartKeys.run("r1", ["equity"])).toEqual(["chart", "run", "r1", "equity"]);
    expect(chartKeys.run("r1", ["markers", "bars"])).toEqual([
      "chart", "run", "r1", "bars,markers",
    ]);
  });
});

describe("runChartPath", () => {
  it("omits the query for the full payload", () => {
    expect(runChartPath("r1")).toBe("/api/eval/runs/r1/chart");
  });

  it("appends a canonical include param", () => {
    expect(runChartPath("r 1", ["equity", "baseline"])).toBe(
      "/api/eval/runs/r%201/chart?include=baseline%2Cequity",
    );
  });
});
```

- [ ] **Step 2: Run tests to verify they fail**

```bash
cd frontend/web && npx vitest run src/api/chart.test.ts 2>&1 | tail -8
```
Expected: FAIL — `runChartIncludeKey` / `runChartPath` not exported.

- [ ] **Step 3: Implement in `api/chart.ts`**

Replace the `chartKeys` const and `getRunChart` (keep everything else):

```ts
/** Allowlisted slim-payload sections of GET /api/eval/runs/:id/chart. */
export type RunChartInclude = "equity" | "bars" | "markers" | "baseline";

/** Canonical (sorted, comma-joined) include key; "" = full payload. */
export function runChartIncludeKey(include?: RunChartInclude[]): string {
  return include && include.length > 0 ? [...include].sort().join(",") : "";
}

export function runChartPath(
  runId: string,
  include?: RunChartInclude[],
): string {
  const key = runChartIncludeKey(include);
  const suffix = key ? `?include=${encodeURIComponent(key)}` : "";
  return `/api/eval/runs/${encodeURIComponent(runId)}/chart${suffix}`;
}

export const chartKeys = {
  run: (id: string, include?: RunChartInclude[]) =>
    ["chart", "run", id, runChartIncludeKey(include)] as const,
  compare: (ids: string[]) =>
    ["chart", "compare", ids.slice().sort().join(",")] as const,
};

export function getRunChart(
  runId: string,
  include?: RunChartInclude[],
): Promise<RunChartPayload> {
  return apiFetch<RunChartPayload>(runChartPath(runId, include));
}
```

Existing call sites (`grep -rn "chartKeys.run(" src/`) keep compiling — the new argument is optional and the key gains a stable `""` segment.

- [ ] **Step 4: Run tests + typecheck**

```bash
npx vitest run src/api/chart.test.ts 2>&1 | tail -5 && npx tsc --noEmit 2>&1 | tail -3; cd ../..
```
Expected: PASS, clean.

- [ ] **Step 5: Commit**

```bash
git add frontend/web/src/api/chart.ts frontend/web/src/api/chart.test.ts
git commit -m "feat(frontend): run chart include param + locked cache-key shape (xvision-0er)"
```

---

### Task 7: Frontend — pulse selectors (view type, field normalization, hold-compare)

**Files:**
- Modify: `frontend/web/src/features/home/pulse.ts`
- Test: `frontend/web/src/features/home/pulse.test.ts` (append)

- [ ] **Step 1: Write the failing tests**

Append to `pulse.test.ts` (mirror its existing imports style):

```ts
import {
  alignFieldSeries,
  fieldRunSeries,
  holdCompareSeries,
  normalizePulseView,
  PULSE_VIEWS,
} from "./pulse";

describe("normalizePulseView", () => {
  it("accepts every known view", () => {
    for (const v of PULSE_VIEWS) expect(normalizePulseView(v)).toBe(v);
  });
  it("falls back to return for unknown/null", () => {
    expect(normalizePulseView(null)).toBe("return");
    expect(normalizePulseView("bogus")).toBe("return");
  });
});

describe("fieldRunSeries", () => {
  const eq = (t: number, e: number) => ({ time: t, equity_usd: e });

  it("normalizes to elapsed fraction and return pct", () => {
    const s = fieldRunSeries("r1", "Alpha", [eq(100, 100_000), eq(150, 110_000), eq(200, 99_000)]);
    expect(s).not.toBeNull();
    expect(s!.fraction).toEqual([0, 0.5, 1]);
    expect(s!.returnPct[0]).toBeCloseTo(0);
    expect(s!.returnPct[1]).toBeCloseTo(10);
    expect(s!.returnPct[2]).toBeCloseTo(-1);
  });

  it("rejects degenerate series", () => {
    expect(fieldRunSeries("r1", "x", [])).toBeNull();
    expect(fieldRunSeries("r1", "x", [eq(100, 100_000)])).toBeNull();
    expect(fieldRunSeries("r1", "x", [eq(100, 0), eq(200, 5)])).toBeNull(); // zero base
    expect(fieldRunSeries("r1", "x", [eq(100, 1), eq(100, 2)])).toBeNull(); // zero span
  });
});

describe("alignFieldSeries", () => {
  it("unions fractions and gaps non-shared samples with null", () => {
    const a = { runId: "a", label: "A", fraction: [0, 1], returnPct: [0, 4] };
    const b = { runId: "b", label: "B", fraction: [0, 0.5, 1], returnPct: [0, 1, 2] };
    const { x, ys } = alignFieldSeries([a, b]);
    expect(x).toEqual([0, 0.5, 1]);
    expect(ys[0]).toEqual([0, null, 4]);
    expect(ys[1]).toEqual([0, 1, 2]);
  });
});

describe("holdCompareSeries", () => {
  it("normalizes both curves to return pct on the shared axis", () => {
    const equity = [
      { time: 1, value: 0 },
      { time: 2, value: 5 },
    ];
    const baseline = [
      { time: 1, equity_usd: 100_000 },
      { time: 2, equity_usd: 120_000 },
    ];
    const s = holdCompareSeries(equity, baseline);
    expect(s.time).toEqual([1, 2]);
    expect(s.strategy).toEqual([0, 5]);
    expect(s.hold[0]).toBeCloseTo(0);
    expect(s.hold[1]).toBeCloseTo(20);
  });

  it("gaps baseline timestamps missing from the equity axis", () => {
    const equity = [
      { time: 1, value: 0 },
      { time: 2, value: 5 },
    ];
    const baseline = [{ time: 1, equity_usd: 100_000 }];
    const s = holdCompareSeries(equity, baseline);
    expect(s.hold).toEqual([0, null]);
  });
});
```

- [ ] **Step 2: Run tests to verify they fail**

```bash
cd frontend/web && npx vitest run src/features/home/pulse.test.ts 2>&1 | tail -8
```
Expected: FAIL — exports missing.

- [ ] **Step 3: Implement in `pulse.ts`**

Append:

```ts
// ─── pulse view switcher ─────────────────────────────────────────────────────

export const PULSE_VIEWS = [
  "return",
  "trades",
  "hold",
  "drawdown",
  "field",
] as const;
export type PulseView = (typeof PULSE_VIEWS)[number];
export const PULSE_VIEW_STORAGE_KEY = "xvn:pulse-view";

export function normalizePulseView(raw: string | null): PulseView {
  return (PULSE_VIEWS as readonly string[]).includes(raw ?? "")
    ? (raw as PulseView)
    : "return";
}

// ─── "All runs" field view ───────────────────────────────────────────────────

export interface FieldRunSeries {
  runId: string;
  label: string;
  /** Elapsed fraction of the run's own window, 0..1. */
  fraction: number[];
  returnPct: (number | null)[];
}

/** Normalize one run's raw equity curve for the field overlay. Returns null
 * for series that can't be charted (under 2 finite samples, zero base
 * equity, or zero time span). */
export function fieldRunSeries(
  runId: string,
  label: string,
  equity: { time: number; equity_usd: number }[],
): FieldRunSeries | null {
  const pts = equity.filter(
    (p) => Number.isFinite(p.time) && Number.isFinite(p.equity_usd),
  );
  if (pts.length < 2) return null;
  const base = pts[0].equity_usd;
  const t0 = pts[0].time;
  const span = pts[pts.length - 1].time - t0;
  if (base === 0 || span <= 0) return null;
  return {
    runId,
    label,
    fraction: pts.map((p) => (p.time - t0) / span),
    returnPct: pts.map((p) => (p.equity_usd / base - 1) * 100),
  };
}

/** Align per-run fraction grids onto one shared x column (union of all
 * fractions); missing samples become null gaps (chart uses spanGaps). */
export function alignFieldSeries(series: FieldRunSeries[]): {
  x: number[];
  ys: (number | null)[][];
} {
  const x = [...new Set(series.flatMap((s) => s.fraction))].sort(
    (a, b) => a - b,
  );
  const ys = series.map((s) => {
    const byFraction = new Map(
      s.fraction.map((f, i) => [f, s.returnPct[i]] as const),
    );
    return x.map((f) => byFraction.get(f) ?? null);
  });
  return { x, ys };
}

// ─── "vs Buy & Hold" view ────────────────────────────────────────────────────

/** Merge the strategy return-% curve with the server baseline (raw USD,
 * sampled at equity timestamps) onto one axis; baseline normalizes to its
 * own first sample. */
export function holdCompareSeries(
  equity: SeriesPoint[],
  baseline: { time: number; equity_usd: number }[],
): { time: number[]; strategy: (number | null)[]; hold: (number | null)[] } {
  const time = equity.map((p) => p.time);
  const strategy = equity.map((p) =>
    Number.isFinite(p.value) ? p.value : null,
  );
  const base = baseline.find((b) => Number.isFinite(b.equity_usd))?.equity_usd;
  const holdByTime = new Map(
    base
      ? baseline
          .filter((b) => Number.isFinite(b.equity_usd))
          .map((b) => [b.time, (b.equity_usd / base - 1) * 100] as const)
      : [],
  );
  const hold = time.map((t) => holdByTime.get(t) ?? null);
  return { time, strategy, hold };
}
```

- [ ] **Step 4: Run tests to verify they pass**

```bash
npx vitest run src/features/home/pulse.test.ts 2>&1 | tail -5; cd ../..
```
Expected: PASS (existing + new).

- [ ] **Step 5: Commit**

```bash
git add frontend/web/src/features/home/pulse.ts frontend/web/src/features/home/pulse.test.ts
git commit -m "feat(frontend): pulse view selectors — field normalization, hold compare, view type (xvision-0er)"
```

---

### Task 8: Frontend — drawdown becomes band-only

**Files:**
- Modify: `frontend/web/src/components/home/PulseEquityChart.tsx:71-78`

- [ ] **Step 1: Remove the drawdown stroke**

Replace the drawdown series entry:

```ts
      {
        label: "Drawdown",
        // Band only — the xvnAreaFill plugin paints the underwater tint;
        // a visible stroke here reads as a duplicate earnings line.
        stroke: "transparent",
        width: 0,
        points: { show: false },
      },
```

Also update the component header comment (lines 3-7): the drawdown is "rendered as a subdued red-tinted band below zero (no stroke)".

- [ ] **Step 2: Run the home component tests**

```bash
cd frontend/web && npx vitest run src/components/home 2>&1 | tail -5; cd ../..
```
Expected: PASS (PulseBand tests don't assert stroke color; if one does, update it to expect the band-only contract).

- [ ] **Step 3: Commit**

```bash
git add frontend/web/src/components/home/PulseEquityChart.tsx
git commit -m "fix(frontend): pulse drawdown is a band, not a second line (xvision-0er)"
```

---

### Task 9: Frontend — `PulseViewSwitcher`

**Files:**
- Create: `frontend/web/src/components/home/PulseViewSwitcher.tsx`
- Test: `frontend/web/src/components/home/PulseViewSwitcher.test.tsx`

- [ ] **Step 1: Write the failing test**

```tsx
// frontend/web/src/components/home/PulseViewSwitcher.test.tsx
import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { PulseViewSwitcher } from "./PulseViewSwitcher";

describe("PulseViewSwitcher", () => {
  it("renders all five views and marks the active one", () => {
    render(<PulseViewSwitcher view="return" onViewChange={() => {}} />);
    for (const label of [
      "Return %",
      "Price + trades",
      "vs Buy & Hold",
      "Drawdown",
      "All runs",
    ]) {
      expect(screen.getByRole("button", { name: label })).toBeInTheDocument();
    }
    expect(
      screen.getByRole("button", { name: "Return %" }),
    ).toHaveAttribute("aria-pressed", "true");
    expect(
      screen.getByRole("button", { name: "Drawdown" }),
    ).toHaveAttribute("aria-pressed", "false");
  });

  it("fires onViewChange with the view id", () => {
    const onViewChange = vi.fn();
    render(<PulseViewSwitcher view="return" onViewChange={onViewChange} />);
    fireEvent.click(screen.getByRole("button", { name: "All runs" }));
    expect(onViewChange).toHaveBeenCalledWith("field");
  });
});
```

- [ ] **Step 2: Run test to verify it fails**

```bash
cd frontend/web && npx vitest run src/components/home/PulseViewSwitcher.test.tsx 2>&1 | tail -5
```
Expected: FAIL — module not found.

- [ ] **Step 3: Implement**

```tsx
// frontend/web/src/components/home/PulseViewSwitcher.tsx
//
// Chip row for the Pulse band chart views. Renders as its own full-width
// sub-row below the band header (the header row is already crowded at small
// breakpoints). No popups; selection is plain buttons with aria-pressed.

import type { ReactElement } from "react";
import type { PulseView } from "@/features/home/pulse";

const VIEW_LABELS: Record<PulseView, string> = {
  return: "Return %",
  trades: "Price + trades",
  hold: "vs Buy & Hold",
  drawdown: "Drawdown",
  field: "All runs",
};

export interface PulseViewSwitcherProps {
  view: PulseView;
  onViewChange: (view: PulseView) => void;
}

export function PulseViewSwitcher({
  view,
  onViewChange,
}: PulseViewSwitcherProps): ReactElement {
  return (
    <div
      data-testid="pulse-view-switcher"
      className="flex flex-wrap items-center gap-1.5 px-5 pb-2"
      role="group"
      aria-label="Chart view"
    >
      {(Object.keys(VIEW_LABELS) as PulseView[]).map((v) => (
        <button
          key={v}
          type="button"
          aria-pressed={view === v}
          onClick={() => onViewChange(v)}
          className={`rounded-sm border px-2.5 py-1 text-[11px] font-medium uppercase tracking-wide transition-colors ${
            view === v
              ? "border-gold/40 text-gold"
              : "border-border-soft text-text-3 hover:text-text"
          }`}
        >
          {VIEW_LABELS[v]}
        </button>
      ))}
    </div>
  );
}
```

- [ ] **Step 4: Run test to verify it passes**

```bash
npx vitest run src/components/home/PulseViewSwitcher.test.tsx 2>&1 | tail -5; cd ../..
```
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add frontend/web/src/components/home/PulseViewSwitcher.tsx frontend/web/src/components/home/PulseViewSwitcher.test.tsx
git commit -m "feat(frontend): pulse view switcher chip row (xvision-0er)"
```

---

### Task 10: Frontend — the four view chart components

**Files:**
- Create: `frontend/web/src/components/home/views/PulseTradesChart.tsx`
- Create: `frontend/web/src/components/home/views/PulseHoldChart.tsx`
- Create: `frontend/web/src/components/home/views/PulseDrawdownChart.tsx`
- Create: `frontend/web/src/components/home/views/PulseFieldChart.tsx`
- Test: `frontend/web/src/components/home/views/pulse-views.test.tsx`

All four are presentational: data in, canvas out. They reuse `usePlot`, `useChart2Theme`, `themeToUplotOptions`, the xvn plugins, and (for trades) `KlineCandlePane` + `runChartPayloadToV2`. Mirror `PulseEquityChart.tsx`'s structure exactly (hostRef + opts + usePlot + 210px default).

- [ ] **Step 1: Write the failing tests**

```tsx
// frontend/web/src/components/home/views/pulse-views.test.tsx
import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import type { RunChartPayload } from "@/api/types.gen";
import { PulseDrawdownChart } from "./PulseDrawdownChart";
import { PulseFieldChart } from "./PulseFieldChart";
import { PulseHoldChart } from "./PulseHoldChart";
import { PulseTradesChart } from "./PulseTradesChart";

function slimPayload(over: Partial<RunChartPayload> = {}): RunChartPayload {
  return {
    run_id: "r1",
    scenario_id: "s1",
    asset: "ETH",
    granularity: "1h",
    time_window: { start: "2025-01-01T00:00:00Z", end: "2025-01-02T00:00:00Z" },
    bars: [],
    indicators: {
      sma_20: [], sma_30: [], sma_50: [], sma_60: [], sma_90: [], sma_200: [],
      ema_20: [], ema_30: [], ema_50: [], ema_60: [], ema_90: [], ema_200: [],
      bollinger: { upper: [], middle: [], lower: [] },
      donchian: { upper: [], lower: [] },
      rsi_14: [],
      macd: { line: [], signal: [], histogram: [] },
      atr_14: [],
    },
    equity: [
      { time: 100, equity_usd: 100_000 },
      { time: 200, equity_usd: 105_000 },
    ],
    drawdown: [],
    position: [],
    markers: { trades: [], vetoes: [], holds: [] },
    baseline_equity: null,
    ...over,
  } as RunChartPayload;
}

describe("pulse view charts", () => {
  it("PulseTradesChart renders a candle host for a bars payload", () => {
    const payload = slimPayload({
      bars: [
        { time: 100, open: 1, high: 2, low: 0.5, close: 1.5, volume: 10 },
        { time: 200, open: 1.5, high: 2.5, low: 1, close: 2, volume: 12 },
      ],
      markers: {
        trades: [
          {
            time: 100, side: "Buy", price: 1.2, size: 1, fee: 0,
            pnl_realized: null, decision_index: 0, justification: null,
          },
        ],
        vetoes: [],
        holds: [],
      },
    });
    render(<PulseTradesChart payload={payload} />);
    expect(screen.getByTestId("pulse-trades-chart")).toBeInTheDocument();
  });

  it("PulseHoldChart renders for an equity+baseline payload", () => {
    const payload = slimPayload({
      baseline_equity: [
        { time: 100, equity_usd: 100_000 },
        { time: 200, equity_usd: 101_000 },
      ],
    });
    render(<PulseHoldChart payload={payload} />);
    expect(screen.getByTestId("pulse-hold-chart")).toBeInTheDocument();
  });

  it("PulseDrawdownChart renders from a slim equity payload", () => {
    render(<PulseDrawdownChart payload={slimPayload()} />);
    expect(screen.getByTestId("pulse-drawdown-chart")).toBeInTheDocument();
  });

  it("PulseFieldChart renders an overlay and inline caption row", () => {
    render(
      <PulseFieldChart
        runs={[
          {
            runId: "r1",
            label: "Alpha",
            equity: [
              { time: 100, equity_usd: 100_000 },
              { time: 200, equity_usd: 104_000 },
            ],
          },
          {
            runId: "r2",
            label: "Beta",
            equity: [
              { time: 50, equity_usd: 100_000 },
              { time: 60, equity_usd: 98_000 },
            ],
          },
        ]}
        heroRunId="r1"
      />,
    );
    expect(screen.getByTestId("pulse-field-chart")).toBeInTheDocument();
    expect(screen.getByTestId("pulse-field-caption")).toHaveTextContent("Alpha");
  });
});
```

(Follow whatever jsdom/canvas mocking setup the existing `KlineCandlePane.test.tsx` and `b1-primitives.test.tsx` use — check their imports/setup first and copy the same harness. If they mock `klinecharts` or `ResizeObserver`, this file needs the same mocks.)

- [ ] **Step 2: Run tests to verify they fail**

```bash
cd frontend/web && npx vitest run src/components/home/views 2>&1 | tail -5
```
Expected: FAIL — modules not found.

- [ ] **Step 3: Implement the four components**

```tsx
// frontend/web/src/components/home/views/PulseTradesChart.tsx
//
// "Price + trades" Pulse view: the run's market candles with buy/sell
// markers, reusing the chart-v2 KlineCandlePane BARE — no ChartFrame
// wrapper, so no chart-v2 range/zoom window events fire on the home page.

import type { ReactElement } from "react";
import type { RunChartPayload } from "@/api/types.gen";
import { runChartPayloadToV2 } from "@/components/chart/v2/adapters/run-chart-payload";
import { KlineCandlePane } from "@/components/chart/v2/primitives/KlineCandlePane";

export function PulseTradesChart({
  payload,
  height = 210,
}: {
  payload: RunChartPayload;
  height?: number;
}): ReactElement {
  const v2 = runChartPayloadToV2(payload);
  return (
    <div data-testid="pulse-trades-chart">
      <KlineCandlePane
        candles={v2.candles}
        markers={v2.markers.filter((m) => m.kind === "buy" || m.kind === "sell")}
        height={height}
      />
    </div>
  );
}
```

```tsx
// frontend/web/src/components/home/views/PulseHoldChart.tsx
//
// "vs Buy & Hold" Pulse view: strategy return % (gold) vs buy-and-hold
// return % (muted), shared axis, zero line. Inline series labels — no
// floating legend.

import "uplot/dist/uPlot.min.css";

import { useRef, type ReactElement } from "react";
import uPlot from "uplot";

import type { RunChartPayload } from "@/api/types.gen";
import { normalizeEquityToReturnPct } from "@/components/chart/v2/adapters/columnar-to-uplot";
import { themeToUplotOptions } from "@/components/chart/v2/adapters/theme-to-uplot";
import { xvnLastDot, xvnZeroLine } from "@/components/chart/v2/adapters/uplot-plugins";
import { useChart2Theme } from "@/components/chart/v2/hooks/useChart2Theme";
import { usePlot } from "@/components/chart/v2/primitives/usePlot";
import { holdCompareSeries } from "@/features/home/pulse";

export function PulseHoldChart({
  payload,
  height = 210,
}: {
  payload: RunChartPayload;
  height?: number;
}): ReactElement {
  const hostRef = useRef<HTMLDivElement>(null);
  const theme = useChart2Theme();
  const series = holdCompareSeries(
    normalizeEquityToReturnPct(payload.equity),
    payload.baseline_equity ?? [],
  );

  const baseOpts = themeToUplotOptions(theme) as Partial<uPlot.Options>;
  const baseAxes = (baseOpts.axes as uPlot.Axis[] | undefined) ?? [];
  const opts: uPlot.Options = {
    ...(baseOpts as Omit<uPlot.Options, "width" | "height" | "series">),
    width: 0,
    height,
    legend: { show: false },
    axes: [
      baseAxes[0] ?? {},
      {
        ...baseAxes[1],
        size: 52,
        values: (_u: uPlot, vals: (number | null)[]) =>
          vals.map((v) => (v != null ? `${v.toFixed(2)}%` : "")),
      },
    ],
    series: [
      {},
      {
        label: "Strategy",
        stroke: theme.panes.equity,
        width: 1.5,
        points: { show: false },
        spanGaps: true,
      },
      {
        label: "Buy & Hold",
        // Muted/neutral tone — NOT theme.panes.drawdown: red would read as
        // a loss indicator. Use theme.surface.gridStrong if present on the
        // theme shape, else the muted text/axis token from useChart2Theme.
        stroke: theme.surface.gridStrong,
        width: 1,
        dash: [4, 4],
        points: { show: false },
        spanGaps: true,
      },
    ],
    plugins: [
      xvnZeroLine(),
      xvnLastDot(1, theme.panes.equity, { backgroundFill: theme.surface.bg }),
    ],
  };

  usePlot(
    opts,
    [series.time, series.strategy, series.hold] as uPlot.AlignedData,
    hostRef,
    height,
  );

  return (
    <div data-testid="pulse-hold-chart" style={{ width: "100%" }}>
      <div ref={hostRef} style={{ width: "100%" }} />
      <div className="flex items-center gap-4 px-2 pt-1 text-[11px] text-text-4">
        <span className="text-gold">— Strategy</span>
        <span>┄ Buy &amp; Hold</span>
      </div>
    </div>
  );
}
```

(`dash` on a uPlot series may need to live under the series' `dash` prop directly — it does: `uPlot.Series.dash?: number[]`. If `theme.panes` lacks a suitable muted tone for Buy & Hold, use `theme.panes.drawdown` as shown or a `text-3`-equivalent theme token — check `useChart2Theme`'s shape and pick the muted axis/text color rather than hardcoding hex.)

```tsx
// frontend/web/src/components/home/views/PulseDrawdownChart.tsx
//
// Dedicated underwater view: drawdown depth (≤ 0) as a red-tinted area.
// Same client-side computation the band uses (pulseChartSeries).

import "uplot/dist/uPlot.min.css";

import { useRef, type ReactElement } from "react";
import uPlot from "uplot";

import type { RunChartPayload } from "@/api/types.gen";
import { normalizeEquityToReturnPct } from "@/components/chart/v2/adapters/columnar-to-uplot";
import { themeToUplotOptions } from "@/components/chart/v2/adapters/theme-to-uplot";
import { xvnAreaFill, xvnZeroLine } from "@/components/chart/v2/adapters/uplot-plugins";
import { useChart2Theme } from "@/components/chart/v2/hooks/useChart2Theme";
import { usePlot } from "@/components/chart/v2/primitives/usePlot";
import { pulseChartSeries } from "@/features/home/pulse";

export function PulseDrawdownChart({
  payload,
  height = 210,
}: {
  payload: RunChartPayload;
  height?: number;
}): ReactElement {
  const hostRef = useRef<HTMLDivElement>(null);
  const theme = useChart2Theme();
  const series = pulseChartSeries(normalizeEquityToReturnPct(payload.equity));

  const baseOpts = themeToUplotOptions(theme) as Partial<uPlot.Options>;
  const baseAxes = (baseOpts.axes as uPlot.Axis[] | undefined) ?? [];
  const opts: uPlot.Options = {
    ...(baseOpts as Omit<uPlot.Options, "width" | "height" | "series">),
    width: 0,
    height,
    legend: { show: false },
    axes: [
      baseAxes[0] ?? {},
      {
        ...baseAxes[1],
        size: 52,
        values: (_u: uPlot, vals: (number | null)[]) =>
          vals.map((v) => (v != null ? `${v.toFixed(2)}%` : "")),
      },
    ],
    series: [
      {},
      {
        label: "Drawdown",
        stroke: theme.panes.drawdown,
        width: 1.5,
        points: { show: false },
      },
    ],
    plugins: [xvnAreaFill(1, "rgba(255,77,77,0.16)"), xvnZeroLine()],
  };

  usePlot(
    opts,
    [series.time, series.drawdown] as uPlot.AlignedData,
    hostRef,
    height,
  );

  return (
    <div ref={hostRef} data-testid="pulse-drawdown-chart" style={{ width: "100%" }} />
  );
}
```

```tsx
// frontend/web/src/components/home/views/PulseFieldChart.tsx
//
// "All runs" field view: every recent completed run as a faint return-%
// line over its own elapsed fraction (0..1), hero run highlighted. Run
// identification is inline (caption row), never a popup; the x-axis is
// unlabeled because elapsed fraction is not wall-clock time.

import "uplot/dist/uPlot.min.css";

import { useRef, useState, type ReactElement } from "react";
import uPlot from "uplot";

import { themeToUplotOptions } from "@/components/chart/v2/adapters/theme-to-uplot";
import { xvnZeroLine } from "@/components/chart/v2/adapters/uplot-plugins";
import { useChart2Theme } from "@/components/chart/v2/hooks/useChart2Theme";
import { usePlot } from "@/components/chart/v2/primitives/usePlot";
import {
  alignFieldSeries,
  fieldRunSeries,
  type FieldRunSeries,
} from "@/features/home/pulse";

export interface PulseFieldRun {
  runId: string;
  label: string;
  equity: { time: number; equity_usd: number }[];
}

export function PulseFieldChart({
  runs,
  heroRunId,
  height = 210,
}: {
  runs: PulseFieldRun[];
  heroRunId: string | null;
  height?: number;
}): ReactElement {
  const hostRef = useRef<HTMLDivElement>(null);
  const theme = useChart2Theme();
  const [focusLabel, setFocusLabel] = useState<string | null>(null);

  const normalized: FieldRunSeries[] = runs
    .map((r) => fieldRunSeries(r.runId, r.label, r.equity))
    .filter((s): s is FieldRunSeries => s !== null);
  const heroLabel =
    normalized.find((s) => s.runId === heroRunId)?.label ??
    normalized[0]?.label ??
    "";
  const { x, ys } = alignFieldSeries(normalized);

  const baseOpts = themeToUplotOptions(theme) as Partial<uPlot.Options>;
  const baseAxes = (baseOpts.axes as uPlot.Axis[] | undefined) ?? [];
  const opts: uPlot.Options = {
    ...(baseOpts as Omit<uPlot.Options, "width" | "height" | "series">),
    width: 0,
    height,
    legend: { show: false },
    cursor: { focus: { prox: 16 } },
    scales: { x: { time: false } },
    axes: [
      { ...baseAxes[0], show: false },
      {
        ...baseAxes[1],
        size: 52,
        values: (_u: uPlot, vals: (number | null)[]) =>
          vals.map((v) => (v != null ? `${v.toFixed(1)}%` : "")),
      },
    ],
    series: [
      {},
      ...normalized.map((s): uPlot.Series => {
        const isHero = s.runId === heroRunId;
        return {
          label: s.label,
          stroke: isHero ? theme.panes.equity : theme.panes.drawdown,
          alpha: isHero ? 1 : 0.35,
          width: isHero ? 1.8 : 1,
          points: { show: false },
          spanGaps: true,
        };
      }),
    ],
    plugins: [xvnZeroLine()],
    hooks: {
      setSeries: [
        (u: uPlot, idx: number | null) => {
          setFocusLabel(
            idx != null && idx > 0 ? (u.series[idx]?.label as string) : null,
          );
        },
      ],
    },
  };

  usePlot(opts, [x, ...ys] as uPlot.AlignedData, hostRef, height);

  return (
    <div data-testid="pulse-field-chart" style={{ width: "100%" }}>
      <div ref={hostRef} style={{ width: "100%" }} />
      <div
        data-testid="pulse-field-caption"
        className="flex items-center gap-3 px-2 pt-1 text-[11px] text-text-4"
      >
        <span className="text-gold">● {heroLabel} (latest)</span>
        <span>{normalized.length} runs · x = elapsed fraction of each run</span>
        {focusLabel && focusLabel !== heroLabel ? (
          <span className="text-text-3">hover: {focusLabel}</span>
        ) : null}
      </div>
    </div>
  );
}
```

(If `uPlot.Series` has no `alpha` per-series in the installed typings, drop `alpha` and instead encode the faintness into the stroke via an rgba conversion of the theme color or `stroke: "rgba(148,163,184,0.35)"`-style theme-derived value; check how other multi-series panes like `MultiStrategyEquityPane.tsx` or `UplotCompareOverlayPane` stroke their non-primary series and copy that approach.)

- [ ] **Step 4: Run tests to verify they pass**

```bash
npx vitest run src/components/home/views 2>&1 | tail -6 && npx tsc --noEmit 2>&1 | tail -3; cd ../..
```
Expected: PASS, clean typecheck.

- [ ] **Step 5: Commit**

```bash
git add frontend/web/src/components/home/views/
git commit -m "feat(frontend): pulse view chart components — trades, hold, drawdown, field (xvision-0er)"
```

---

### Task 11: Frontend — wire `PulseBand` (view state, lazy queries, error retry)

**Files:**
- Modify: `frontend/web/src/components/home/PulseBand.tsx`
- Test: `frontend/web/src/components/home/PulseBand.test.tsx` (extend)

- [ ] **Step 1: Write failing tests** (extend `PulseBand.test.tsx`, reusing its existing render helper/query mocks — read the file first and follow its setup conventions):

```tsx
it("renders the view switcher and persists the selection", async () => {
  // render PulseBand with a completed chartable hero run via the file's
  // existing fixture helpers
  // ...existing setup...
  expect(screen.getByTestId("pulse-view-switcher")).toBeInTheDocument();
  fireEvent.click(screen.getByRole("button", { name: "Drawdown" }));
  expect(window.localStorage.getItem("xvn:pulse-view")).toBe("drawdown");
  expect(await screen.findByTestId("pulse-drawdown-chart")).toBeInTheDocument();
});

it("initial view comes from localStorage", () => {
  window.localStorage.setItem("xvn:pulse-view", "drawdown");
  // ...render...
  expect(
    screen.getByRole("button", { name: "Drawdown" }),
  ).toHaveAttribute("aria-pressed", "true");
});

it("failed lazy view fetch shows an inline retry, not a crash", async () => {
  // mock getRunChart for include=bars,markers to reject once
  // ...render, click "Price + trades"...
  expect(await screen.findByTestId("pulse-view-error")).toBeInTheDocument();
  expect(screen.getByRole("button", { name: /retry/i })).toBeInTheDocument();
});
```

(Read `PulseBand.test.tsx` FIRST and reuse its existing render/fixture helper — e.g. a `renderBand()`-style wrapper with QueryClientProvider and run fixtures — rather than writing a duplicate harness. It must already mock `@/api/chart`'s `getRunChart` or `apiFetch` for the hero chart; extend that mock to key off the `include` argument.)

- [ ] **Step 2: Run to verify they fail**

```bash
cd frontend/web && npx vitest run src/components/home/PulseBand.test.tsx 2>&1 | tail -6
```
Expected: FAIL — no switcher rendered.

- [ ] **Step 3: Implement the PulseBand changes**

Key edits (keep everything else — header, KPI rail, empty states — as is):

```tsx
import { useState } from "react";
import { chartKeys, getCompareChart, getRunChart } from "@/api/chart";
import {
  normalizePulseView,
  isChartableRun,
  PULSE_VIEW_STORAGE_KEY,
  type PulseView,
  // ...existing pulse imports
} from "@/features/home/pulse";
import { PulseViewSwitcher } from "./PulseViewSwitcher";
import { PulseDrawdownChart } from "./views/PulseDrawdownChart";
import { PulseFieldChart } from "./views/PulseFieldChart";
import { PulseHoldChart } from "./views/PulseHoldChart";
import { PulseTradesChart } from "./views/PulseTradesChart";
```

Inside the component:

```tsx
  // View selection — read localStorage in the initializer so the lazy view's
  // query is enabled on first render (no flash-fire of the default view).
  const [view, setView] = useState<PulseView>(() =>
    normalizePulseView(window.localStorage.getItem(PULSE_VIEW_STORAGE_KEY)),
  );
  const changeView = (v: PulseView) => {
    setView(v);
    window.localStorage.setItem(PULSE_VIEW_STORAGE_KEY, v);
  };

  // Slim hero chart — equity only (Return % + Drawdown views).
  const chart = useQuery({
    queryKey: chartKeys.run(heroRunId, ["equity"]),
    queryFn: () => getRunChart(heroRunId, ["equity"]),
    enabled: heroRunId !== "",
    staleTime: 30_000,
  });

  // Lazy per-view payloads.
  const tradesChart = useQuery({
    queryKey: chartKeys.run(heroRunId, ["bars", "markers"]),
    queryFn: () => getRunChart(heroRunId, ["bars", "markers"]),
    enabled: heroRunId !== "" && view === "trades",
    staleTime: 30_000,
  });
  const holdChart = useQuery({
    queryKey: chartKeys.run(heroRunId, ["equity", "baseline"]),
    queryFn: () => getRunChart(heroRunId, ["equity", "baseline"]),
    enabled: heroRunId !== "" && view === "hold",
    staleTime: 30_000,
  });
  const fieldRunIds = runs
    .filter((r) => r.status === "completed" && isChartableRun(r))
    .sort((a, b) => (b.completed_at ?? "").localeCompare(a.completed_at ?? ""))
    .slice(0, 10)
    .map((r) => r.id);
  const fieldChart = useQuery({
    queryKey: chartKeys.compare(fieldRunIds),
    queryFn: () => getCompareChart(fieldRunIds),
    enabled: view === "field" && fieldRunIds.length >= 2,
    staleTime: 30_000,
  });
```

Render slot — replace the current `hasSeries ? <PulseEquityChart/>` body with a per-view switch. Shared sub-components inside the file:

```tsx
function ViewSkeleton() {
  return <div className="h-[210px] animate-pulse rounded bg-surface-elev" />;
}

function ViewError({ onRetry }: { onRetry: () => void }) {
  return (
    <div
      data-testid="pulse-view-error"
      className="flex h-[210px] flex-col items-center justify-center gap-2 rounded border border-border-soft"
    >
      <p className="text-[13px] text-text-3">Couldn&apos;t load this view.</p>
      <button
        type="button"
        onClick={onRetry}
        className="rounded-sm border border-border-soft px-2.5 py-1 text-[11px] font-medium uppercase tracking-wide text-text-3 hover:text-text"
      >
        Retry
      </button>
    </div>
  );
}

function ViewEmpty({ message }: { message: string }) {
  return (
    <div className="rounded border border-border-soft px-4 py-10 text-center">
      <p className="text-[13px] text-text-3">{message}</p>
    </div>
  );
}
```

Body (after the header row, before the KPI rail):

```tsx
        {heroRun !== null && !runsPending ? (
          <PulseViewSwitcher view={view} onViewChange={changeView} />
        ) : null}

        {runsPending ? (
          <div className="px-5 pb-4"><ViewSkeleton /></div>
        ) : heroRun === null ? (
          <HeroEmptyState />
        ) : (
          <div className="relative px-3 pb-2">
            {view === "return" &&
              (chart.isPending ? <ViewSkeleton /> :
               chart.isError ? <ViewError onRetry={() => chart.refetch()} /> :
               hasSeries ? <PulseEquityChart series={series!} /> :
               <ViewEmpty message="No equity samples recorded for this run." />)}
            {view === "drawdown" &&
              (chart.isPending ? <ViewSkeleton /> :
               chart.isError ? <ViewError onRetry={() => chart.refetch()} /> :
               hasSeries ? <PulseDrawdownChart payload={chart.data!} /> :
               <ViewEmpty message="No equity samples recorded for this run." />)}
            {view === "trades" &&
              (tradesChart.isPending ? <ViewSkeleton /> :
               tradesChart.isError ? <ViewError onRetry={() => tradesChart.refetch()} /> :
               (tradesChart.data?.bars.length ?? 0) >= 2 ?
                 <PulseTradesChart payload={tradesChart.data!} /> :
               <ViewEmpty message="No market bars cached for this run." />)}
            {view === "hold" &&
              (holdChart.isPending ? <ViewSkeleton /> :
               holdChart.isError ? <ViewError onRetry={() => holdChart.refetch()} /> :
               (holdChart.data?.baseline_equity?.length ?? 0) >= 2 ?
                 <PulseHoldChart payload={holdChart.data!} /> :
               <ViewEmpty message="Buy & Hold baseline unavailable for this run." />)}
            {view === "field" &&
              (fieldRunIds.length < 2 ?
                 <ViewEmpty message="Need at least two completed runs for the field view." /> :
               fieldChart.isPending ? <ViewSkeleton /> :
               fieldChart.isError ? <ViewError onRetry={() => fieldChart.refetch()} /> :
                 <PulseFieldChart
                   heroRunId={heroRunId}
                   runs={fieldChart.data!.runs.map((r) => ({
                     runId: r.run_id,
                     label: displayStrategyName(
                       runs.find((x) => x.id === r.run_id)?.agent_id ?? r.label,
                       strategies,
                     ),
                     equity: r.equity,
                   }))}
                 />)}
          </div>
        )}
```

(The old `pulse-chart-unavailable` empty card markup is superseded by `ViewEmpty`; keep its `data-testid` on the return-view ViewEmpty if `PulseBand.test.tsx` asserts it, or update that assertion.)

Check `CompareChartPayload`'s `runs[*]` field names in `types.gen` (`run_id`, `label`, `equity`) before wiring — adjust the mapping if they differ.

- [ ] **Step 4: Run the tests**

```bash
npx vitest run src/components/home/PulseBand.test.tsx 2>&1 | tail -6
```
Expected: PASS (existing + new).

- [ ] **Step 5: Commit**

```bash
git add frontend/web/src/components/home/PulseBand.tsx frontend/web/src/components/home/PulseBand.test.tsx
git commit -m "feat(frontend): pulse band view switcher with lazy per-view payloads (xvision-0er)"
```

---

### Task 12: Frontend — prefetch the slim hero chart from the home route

**Files:**
- Modify: `frontend/web/src/routes/home.tsx`

- [ ] **Step 1: Implement**

In `HomeRoute`, after the runs query is declared:

```tsx
import { useEffect } from "react";
import { useQueryClient } from "@tanstack/react-query";
import { chartKeys, getRunChart } from "@/api/chart";
import { pickHeroRun } from "@/features/home/pulse";

  // Kill the runs→chart waterfall: start the slim hero-chart fetch the
  // moment the runs page lands, before PulseBand mounts its own query.
  const queryClient = useQueryClient();
  const runsData = runsQuery.data; // ← use the actual variable name in the file
  useEffect(() => {
    const hero = pickHeroRun(runsData ?? []);
    if (!hero) return;
    void queryClient.prefetchQuery({
      queryKey: chartKeys.run(hero.id, ["equity"]),
      queryFn: () => getRunChart(hero.id, ["equity"]),
      staleTime: 30_000,
    });
  }, [runsData, queryClient]);
```

(Match the actual runs-query variable name in `home.tsx:50-52`; `pickHeroRun` may already be imported.)

- [ ] **Step 2: Typecheck + full home test sweep**

```bash
cd frontend/web && npx tsc --noEmit 2>&1 | tail -3 && npx vitest run src/components/home src/features/home src/api/chart.test.ts 2>&1 | tail -6; cd ../..
```
Expected: clean, all PASS.

- [ ] **Step 3: Commit**

```bash
git add frontend/web/src/routes/home.tsx
git commit -m "perf(frontend): prefetch slim hero chart when runs list lands (xvision-0er)"
```

---

### Task 13: Full verification sweep

- [ ] **Step 1: Rust workspace tests**

```bash
export CARGO_TARGET_DIR="$HOME/.cargo-target/xvision-pulse-chart-views"
scripts/cargo test --workspace 2>&1 | tail -15
```
Expected: PASS.

- [ ] **Step 2: Frontend full suite + lint**

```bash
cd frontend/web && npx vitest run 2>&1 | tail -8 && npx eslint src --max-warnings 0 2>&1 | tail -5; cd ../..
```
Expected: PASS / clean (match whatever lint command `package.json` defines if different).

- [ ] **Step 3: Coverage gate**

Run the configured enforcement command from `.coverage-thresholds.json` and report the result honestly (if the repo-wide tarpaulin run is impractical/failing for pre-existing reasons, report numbers for the touched crates and flag it — do NOT silently skip):

```bash
export CARGO_TARGET_DIR="$HOME/.cargo-target/xvision-pulse-chart-views"
scripts/cargo tarpaulin --fail-under 100 -p xvision-engine 2>&1 | tail -10
```

- [ ] **Step 4: Live smoke test (verify skill)**

Boot the dashboard locally, load the home page, and click through all five views; confirm the slim payload (network tab: `?include=equity` response is KB-scale), the single-line hero with band, and each view rendering. Screenshot each view (agent-browser; set a tall viewport rather than `screenshot --full`).

- [ ] **Step 5: Rebase onto latest main (Coordination requirement)**

Concurrent tracks are landing changes to `crates/xvision-engine/src/eval/run.rs`, `eval/store.rs`, and `crates/xvision-dashboard/src/routes/*` — this branch touches `eval_runs.rs` and must absorb whatever they merged:

```bash
git fetch origin && git rebase origin/main
```

If conflicts hit (most likely `eval_runs.rs`, possibly `chart.rs` neighbors), resolve preserving BOTH sides' intents, then re-run the full test sweep (Steps 1–2) before continuing.

- [ ] **Step 6: Update beads + push**

```bash
git push -u origin feat/pulse-chart-views
bd update xvision-0er --notes="implementation complete on feat/pulse-chart-views; awaiting PR review"
```

---

## Self-Review (completed at plan-writing time)

- **Spec coverage:** include param (T1, T3, T4), baseline (T2, T3), codegen (T5), band-only fix (T8), switcher + 5 views + persistence + lazy loading + error/empty states (T7, T9, T10, T11), prefetch (T12), locked query-key shape (T6), no-ChartFrame candle note (T10), field-view inline caption + unlabeled x-axis (T10). Out-of-scope items untouched. ✓
- **Type consistency:** `IncludeSet` / `build_run_payload_with(ctx, run_id, IncludeSet)` (by value, Copy) used consistently in T1/T3/T4; `RunChartInclude[]`/`runChartIncludeKey` in T6/T11/T12; `PulseView` ids `return|trades|hold|drawdown|field` in T7/T9/T11. ✓
- **Known judgment points for the implementer (not placeholders):** exact `MarketBar` import path (T2 note), `Run::new_queued` signature (T3 note), jsdom canvas mocks (T10 note), PulseBand test fixture conventions (T11 note), runs-query variable name (T12 note). Each names the file to check.
