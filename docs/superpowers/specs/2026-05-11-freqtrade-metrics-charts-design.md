# FreqTrade-Inspired Backtest Metrics & Charts — Design

> **Status:** Design / spec — drafted 2026-05-11. Ready for implementation planning.
> **Author:** xvision team.
> **Companion specs:** [Eval Engine Design](./2026-05-08-eval-engine-design.md) (core metrics surface this extends) · [TradingView Charts Design](./2026-05-11-tradingview-charts-design.md) (chart-library decision — re-used here) · [Custom-Scenario Eval](./2026-05-11-custom-scenario-eval-design.md).
> **Tracking:** F-FT-METRICS (this spec). Extends, does not replace, the existing `xvision-eval::metrics` module.

---

## 1. Purpose

xvision today reports six per-arm descriptive metrics (Sharpe annualized, MaxDD%, Profit Factor, Win Rate, Total Return%, Realized PnL) plus a differentiated inferential layer (bootstrapped Δ-Sharpe with 95% CI, per-regime stratification, decision-divergence rate, anti-overfit gate). Output is markdown + JSON; there are no native charts.

FreqTrade — the dominant open-source backtesting reference in retail crypto — reports ~40 headline metrics, five breakdown tables, and four chart families. Operators expect that depth. The gap is most visible to power users in the eval-runs detail surface and the `xvn report` markdown.

This spec defines which FreqTrade-style metrics and charts to lift, how to make them optional (toggle by user, "show all" mode), and how to expose them across CLI, JSON, and the dashboard. Every proposed metric is computable from data xvision already records (`ArmResult.returns`, `equity_curve`, `decisions`, `risk_outcomes`, `fills`); no new instrumentation is required in the eval engine.

---

## 2. Locked decisions

| # | Decision |
|---|---|
| 1 | **Extend `xvision-eval::metrics`, don't replace.** New ratios (Sortino, Calmar, SQN, CAGR, Expectancy) ship as additional functions alongside `sharpe_annualized()` / `max_drawdown_pct()` / etc. The existing `MetricsSummary` struct gains optional fields; old consumers keep working. |
| 2 | **Three metric tiers.** `core` = today's six metrics (default). `extended` = Tier-1 ratios + distributional stats. `all` = everything including breakdown tables. Selected via `--metrics {core,extended,all}` or explicit comma list `--metrics sharpe,sortino,calmar,...`. |
| 3 | **Wallet vs closed-trade dual-track for ratios.** Where FreqTrade reports both (Sharpe / Sortino / Calmar), xvision does too: `*_closed` uses `ArmResult.returns` (per-cycle realized); `*_wallet` uses daily-resampled `equity_curve` (NAV including open positions). Both surface in `extended` tier. |
| 4 | **Breakdown tables = enum group-bys, no new data.** Exit-reason breakdown groups by `(Action, RiskDecision)` already on every cycle. Periodic breakdown buckets `equity_curve` by day/week/month/year. No schema changes. |
| 5 | **Charts re-use the TradingView Lightweight Charts dependency** chosen in `2026-05-11-tradingview-charts-design.md`. No new chart library. New chart families are added panes / surfaces inside the existing chart system. |
| 6 | **Static-export charts via `plotters` crate** for CLI / `xvn report` users without the dashboard. Same data, different renderer; SVG + PNG output. The dashboard remains the primary chart surface. |
| 7 | **Per-arm benchmark row.** When the run includes `buy_and_hold` as an arm, the per-arm dashboard adds an explicit "Δ vs buy-and-hold" column for every other arm. This is xvision's analog to FreqTrade's "Market change" row. |
| 8 | **No per-pair tables.** xvision is single-pair-per-arm by design. Per-arm and per-regime are the right analogs and already exist. Don't add a `--per-pair` dimension. |
| 9 | **No `plot-dataframe`-style indicator-overlay charts** in this spec. That work is the TradingView Charts spec's territory and not a metrics concern. This spec only covers metrics-derived charts (equity, drawdown, distributions, heatmaps). |
| 10 | **CLI flag for charts is `--charts <list>`** with aliases `all` / `none`. Default `none` on `xvn report` and `xvn ab-compare` (no behavior change for existing users); `xvn show-charts` is a new dedicated subcommand that defaults to `--charts all`. |
| 11 | **JSON output is additive only.** New optional fields on `MetricsSummary`, `ArmResult`, and `ComparisonReport`. Existing field names and types do not change. Frontend consumers tolerate missing fields. |

---

## 3. In scope / out of scope

### 3.1 In scope (v1)

**Tier-1 ratios (additive on `MetricsSummary` + per-arm dashboard):**
- Sortino (closed + wallet)
- Calmar (closed + wallet)
- SQN — System Quality Number
- CAGR — annualized compound growth rate
- Expectancy ($) + Expectancy Ratio

**Tier-2 distributional stats (additive on `ArmResult`):**
- Best / worst single cycle %
- Best / worst day (from `equity_curve`)
- Days-win / days-draw / days-lose counts
- Max consecutive wins, max consecutive losses
- Trade duration min/max/avg for **winners** and **losers** separately (requires duration field — see §5)
- Drawdown duration, drawdown start/end timestamps, NAV at drawdown start vs end

**Tier-2 breakdown tables (new sections in `xvn report` markdown):**
- **Exit-reason breakdown** — cycles grouped by closing `(Action, RiskDecision)` tuple: count, Σ PnL, Σ PnL%, win rate, avg duration
- **Periodic breakdown** — day / week / month / year buckets from `equity_curve`: cycles, Σ PnL, profit factor, win rate per bucket
- **Day-of-week breakdown** — Mon-Sun aggregation

**Charts (Tier-3, lift in order of value-per-effort):**
1. **Equity curve per arm** — one line per arm on shared axes
2. **Underwater plot** — drawdown % over time, one line per arm
3. **Trade-duration histogram** — winners vs losers stacked, one panel per arm
4. **Per-arm profit-over-time** — area chart showing each arm's cumulative PnL
5. **Monthly returns heatmap** — calendar grid (rows = year, cols = month, color = return %)
6. **Per-regime Δ-Sharpe with CI error bars** — xvision-native, makes the anti-overfit gate visually legible. Not from FreqTrade.

**CLI:**
- New flag `--metrics {core,extended,all}|<csv>` on `xvn report`, `xvn show-metrics`, `xvn ab-compare`.
- New flag `--charts <csv>|all|none` on `xvn report`.
- New subcommand `xvn show-charts --report <path> --output-dir <dir> [--format svg|png|html]` that renders all six chart families to disk.

**JSON:**
- New optional fields on `MetricsSummary` (Tier-1 ratios).
- New optional fields on `ArmResult` (Tier-2 distributional stats).
- New optional sections on `BacktestResult` (`exit_reason_breakdown`, `periodic_breakdown`, `day_of_week_breakdown`).
- New optional `charts: ChartPayloads` on `ComparisonReport` carrying plotly-JSON for each chart family (dashboard consumes this directly).

**Dashboard surfaces:**
- Tier-1 ratios + benchmark row land in the existing per-arm dashboard table on `/eval-runs/:id`.
- Breakdown tables land as collapsible sections below the per-arm dashboard.
- Charts land as new panes in the existing chart container (`frontend/web/src/components/chart/`).

### 3.2 Out of scope (deferred)

- **plot-dataframe-style indicator overlays.** Covered by `2026-05-11-tradingview-charts-design.md`. This spec doesn't touch the price-pane.
- **Per-pair tables.** xvision is single-pair-per-arm. See locked-decision #8.
- **Hyperopt-style parameter search** driven by these metrics. Different spec (autooptimizer mutator).
- **Custom user-defined metrics** (Lua / Pyodine / etc.). Tier-1 metrics are hardcoded Rust.
- **CSV export.** JSON + markdown only in v1. CSV is a follow-up if a user asks.
- **Live-cockpit streaming of breakdown tables.** They render once on run completion. Equity/underwater charts in live cockpit are covered by the existing SSE stream in the TradingView Charts spec.
- **Backwards-compat shim for the existing `--metrics` flag** — there is no such flag today, so no migration needed.

---

## 4. Architecture

### 4.1 Module layout

```
crates/xvision-eval/src/
├── metrics.rs              (existing — gains Tier-1 ratio functions)
├── distributional.rs       (NEW — best/worst, streaks, durations)
├── breakdowns.rs           (NEW — exit-reason, periodic, day-of-week)
├── benchmark.rs            (NEW — Δ vs buy-and-hold helper)
└── charts/                 (NEW)
    ├── mod.rs              (public API: build_chart_payloads())
    ├── equity.rs
    ├── underwater.rs
    ├── duration_hist.rs
    ├── profit_over_time.rs
    ├── monthly_heatmap.rs
    └── regime_bars.rs

crates/xvision-cli/src/commands/
├── report.rs               (existing — add --metrics, --charts flags)
├── show_metrics.rs         (existing — add --metrics flag)
├── ab_compare.rs           (existing — add --metrics, --charts flags)
└── show_charts.rs          (NEW)

crates/xvision-engine/src/api/
└── eval.rs                 (existing — include new fields in chart endpoint payload)

frontend/web/src/
├── components/chart/
│   ├── UnderwaterChart.tsx       (NEW)
│   ├── DurationHistogram.tsx     (NEW)
│   ├── ProfitOverTimeChart.tsx   (NEW)
│   ├── MonthlyHeatmap.tsx        (NEW)
│   └── RegimeDeltaSharpe.tsx     (NEW)
└── routes/
    └── eval-runs-detail.tsx      (existing — wire new charts + Tier-1 cells)
```

### 4.2 Data flow

```
ArmResult (existing — returns, equity_curve, decisions, risk_outcomes, fills)
   │
   ├──> distributional::compute(...)  → DistributionalStats   (new field on ArmResult)
   ├──> metrics::extended(...)        → ExtendedRatios         (new field on MetricsSummary)
   ├──> breakdowns::compute(...)      → BreakdownTables        (new section on BacktestResult)
   ├──> benchmark::vs_buy_and_hold(...)→ BenchmarkDelta        (new field per ArmResult when bnh present)
   └──> charts::build_chart_payloads(...) → ChartPayloads     (new field on ComparisonReport)
                                            │
                                            ├──> plotters → SVG/PNG (CLI `xvn show-charts`)
                                            └──> plotly JSON → dashboard (lightweight-charts renders)
```

All new code is pure post-processing of `ArmResult`. Nothing in the trader / risk / executor / cycle pipeline changes.

### 4.3 Metric tiers — full list

| Metric | Tier | Closed | Wallet | Source field |
|---|---|---|---|---|
| Sharpe (annualized) | core | ✅ today | ✅ NEW | `returns` / daily `equity_curve` |
| Max Drawdown % | core | ✅ today | (same — already wallet) | `equity_curve` |
| Profit Factor | core | ✅ today | n/a | `returns` |
| Win Rate | core | ✅ today | n/a | `returns` |
| Total Return % | core | ✅ today | n/a | `equity_curve` |
| Realized PnL (USD) | core | ✅ today | n/a | `realized_pnl_total_usd` |
| **Sortino** | extended | ✅ NEW | ✅ NEW | `returns` / daily `equity_curve` |
| **Calmar** | extended | ✅ NEW | ✅ NEW | derived |
| **SQN** | extended | ✅ NEW | n/a | `returns` |
| **CAGR** | extended | ✅ NEW | n/a | `equity_curve` |
| **Expectancy ($)** | extended | ✅ NEW | n/a | `returns`, `realized_pnl_total_usd` |
| **Expectancy Ratio** | extended | ✅ NEW | n/a | `returns` |
| **Best / worst trade %** | extended | ✅ NEW | n/a | `returns` |
| **Best / worst day** | extended | ✅ NEW | n/a | daily-resampled `equity_curve` |
| **Days win/draw/lose** | extended | ✅ NEW | n/a | daily-resampled `equity_curve` |
| **Max consecutive W/L** | extended | ✅ NEW | n/a | `returns` |
| **Duration min/max/avg (winners)** | extended | ✅ NEW | n/a | `fills` timestamps |
| **Duration min/max/avg (losers)** | extended | ✅ NEW | n/a | `fills` timestamps |
| **Drawdown duration** | extended | ✅ NEW | (same) | `equity_curve` |
| **Drawdown start/end timestamps** | extended | ✅ NEW | (same) | `equity_curve` |
| **NAV at DD start / end** | extended | ✅ NEW | (same) | `equity_curve` |
| **Benchmark Δ (vs buy-and-hold)** | extended | ✅ NEW | n/a | other arm's `total_return_pct` |
| **Exit-reason breakdown table** | all | ✅ NEW | n/a | `decisions` × `risk_outcomes` |
| **Periodic breakdown (D/W/M/Y)** | all | ✅ NEW | n/a | `equity_curve` |
| **Day-of-week breakdown** | all | ✅ NEW | n/a | `equity_curve` |
| **Δ-Sharpe bootstrap (paired)** | core inferential | ✅ today | — | xvision-native |
| **Decision divergence rate** | core inferential | ✅ today | — | xvision-native |
| **Per-regime Δ-Sharpe** | core inferential | ✅ today | — | xvision-native |
| **Anti-overfit gate verdict** | core inferential | ✅ today | — | xvision-native |

xvision-native inferential metrics (bottom of table) are kept on every tier; they're not optional. They're what makes xvision's report distinct from FreqTrade's.

### 4.4 CLI surface

```bash
# Core metrics only (today's behavior — default)
xvn report --input run.json --output report.md

# Extended metrics (Tier-1 ratios + distributional stats)
xvn report --input run.json --metrics extended --output report.md

# Everything including breakdown tables
xvn report --input run.json --metrics all --output report.md

# Explicit metric list
xvn report --input run.json --metrics sharpe,sortino,calmar,expectancy --output report.md

# Render all charts to a directory (default: all six chart families)
xvn show-charts --report run.json --output-dir ./charts --format svg

# Inline-render charts in markdown report (links to generated SVGs)
xvn report --input run.json --metrics all --charts all --output-dir report-bundle/
```

### 4.5 JSON additive shape

```rust
// crates/xvision-eval/src/metrics.rs
#[derive(Serialize, Deserialize)]
pub struct MetricsSummary {
    // existing fields
    pub total_return_pct: f64,
    pub sharpe: f64,
    pub max_drawdown_pct: f64,
    pub win_rate: f64,
    pub n_trades: usize,
    pub n_decisions: usize,
    // NEW — Tier-1 (all Option<f64> for additive compat)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sortino_closed: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sortino_wallet: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub calmar_closed: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub calmar_wallet: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sqn: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cagr: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expectancy_usd: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expectancy_ratio: Option<f64>,
}
```

Existing JSON consumers (frontend, `xvn eval list --json`, downstream tools) keep working with no changes. Frontend renders `null`/missing as `—`.

---

## 5. Open questions / risks

1. **Trade duration field.** Tier-2 winner/loser-duration stats require a duration per cycle. `ArmResult.fills: Vec<ExecutionReceipt>` carries timestamps, but the pairing logic (entry-fill ↔ exit-fill) needs verification. If `ExecutionReceipt` doesn't currently carry both timestamps, a minor extension is needed — flag this in the implementation plan.

2. **Daily resampling of `equity_curve`.** Day-aligned buckets need a timezone choice. **Decision:** use UTC throughout, matching the existing snapshot timestamps. Document the boundary clearly in the markdown report ("days are UTC days").

3. **Sortino's "negative returns only" denominator.** When an arm has zero negative returns (clean run), Sortino is `+∞`. Mirror the `profit_factor` convention (return `f64::INFINITY`, frontend renders as `∞`).

4. **Chart-rendering crate choice.** `plotters` is the obvious Rust pick; we should confirm it supports the heatmap shape (rectangles with continuous color scale) before locking. If not, fall back to `plotly` Rust crate emitting JSON-only and rely on the dashboard to render heatmaps.

5. **Bundle size impact on dashboard.** Five new chart components shouldn't move the needle (TradingView Lightweight Charts is already loaded), but verify CI build-size budget from the TradingView Charts spec is not blown.

6. **Tier-2 breakdown tables in markdown can get long.** A run with 5 arms × 4 periodic buckets × day-of-week has 5 × (4 + 7) = 55 rows minimum. The markdown report should section these into a collapsible "Breakdowns" appendix, not inline them at the top.

7. **Per-arm benchmark only applies when `buy_and_hold` is among the arms.** If a user runs a custom arm set without it, the benchmark column is omitted — not synthesized from market data. (Synthesizing would require bar-data access from `xvision-eval`, which it doesn't have today.)

---

## 6. Phasing

Three PRs, each independently shippable:

**PR 1 — Tier-1 ratios + benchmark row.** Pure additive metrics on `MetricsSummary`. ~70% of FreqTrade's table value, zero new data plumbing. Lands `Sortino`, `Calmar`, `SQN`, `CAGR`, `Expectancy`, benchmark Δ. New `--metrics extended` flag. Updates dashboard per-arm table.

**PR 2 — Tier-2 distributional + breakdown tables.** Adds `DistributionalStats` to `ArmResult` and breakdown tables to `BacktestResult`. New `--metrics all` flag. Requires verifying trade-duration plumbing (open question #1). Updates markdown report with collapsible appendix sections.

**PR 3 — Charts.** Adds `xvision-eval::charts` module, `xvn show-charts` subcommand, `--charts` flag on `xvn report`, and the five new dashboard chart components. Re-uses TradingView Lightweight Charts in the dashboard; uses `plotters` (or fallback) for static export.

Each PR is ~3-5 days of work and ships independently. PR 1 unblocks the most user-visible improvement and should land first.

---

## 7. Acceptance criteria

- `cargo test --workspace` passes with new unit tests for every Tier-1 ratio (golden-value tests against hand-calculated examples).
- `xvn report --metrics core` produces byte-identical output to today's `xvn report` (no regressions).
- `xvn report --metrics extended` adds all Tier-1 ratios to the per-arm dashboard.
- `xvn report --metrics all` adds Tier-2 sections + breakdown tables.
- `xvn show-charts --report <path> --output-dir <dir>` produces six SVG files (one per chart family) with no panics on representative test fixtures (`tests/fixtures/`).
- Dashboard `/eval-runs/:id` shows Tier-1 ratios + benchmark Δ column + five new chart panes.
- Existing JSON consumers (frontend, `eval list --json`, `eval compare --json`) continue to parse old fixtures without errors.
- New fields are `Option<T>` and `#[serde(skip_serializing_if = "Option::is_none")]` — old fixture files round-trip cleanly.
- A property test confirms `closed` ratios computed from `returns` and `wallet` ratios computed from daily-resampled `equity_curve` agree to within tolerance for a synthetic no-overlap scenario (positions never open across day boundaries).

---

## 8. References

- FreqTrade backtesting docs: https://www.freqtrade.io/en/stable/backtesting/
- FreqTrade plotting docs: https://www.freqtrade.io/en/stable/plotting/
- FreqTrade hyperopt loss functions: https://www.freqtrade.io/en/stable/hyperopt/ (Sharpe / Sortino / Calmar / SQN / MaxDD / ProfitDrawdown / MultiMetric — strong signal for which metrics matter to optimize against)
- xvision current metrics: `crates/xvision-eval/src/metrics.rs`, `crates/xvision-engine/src/eval/metrics.rs`
- xvision report renderer: `crates/xvision-cli/src/commands/report.rs`
- xvision per-arm CLI tables: `crates/xvision-eval/src/result.rs`
- Companion: TradingView Charts design (chart-library decision, dashboard chart container): `docs/superpowers/specs/2026-05-11-tradingview-charts-design.md`
