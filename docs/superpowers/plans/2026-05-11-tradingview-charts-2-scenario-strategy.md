# TradingView Charts — M2: Scenario + Strategy charts

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add the `/scenarios/:id` Preview chart (price-only candles + cache-status badge + regime tinting) and the `/strategies/:id` Strategy chart (multi-run equity overlay color-coded by scenario). Wire the Runs and Bar-cache tabs on the scenario detail route end-to-end.

**Architecture:** Two new endpoints — `GET /api/scenarios/:id/chart` returns the scenario's bars + cache status; `GET /api/strategies/:id/chart` returns per-run equity series grouped by scenario. Two new React components reuse `ChartContainer` and the theme tokens from M1.

**Tech Stack:** Continues from M1. No new npm deps.

**Reference spec:** `docs/superpowers/specs/2026-05-11-tradingview-charts-design.md` §6.3, §6.4, §7.2.

**Prereq:** Custom-scenario M1 + M2 + M3 merged. TradingView M1 merged.

---

## File structure

| Path | Action | Responsibility |
|---|---|---|
| `crates/xvision-engine/src/api/chart.rs` | Modify | Add `build_scenario_payload`, `build_strategy_payload`, HTTP handlers, cache-status helper. |
| `crates/xvision-dashboard/src/routes.rs` | Modify | Mount `/api/scenarios/:id/chart` and `/api/strategies/:id/chart`. |
| `frontend/web/src/api/chart.ts` | Modify | Add `getScenarioChart`, `getStrategyChart`. |
| `frontend/web/src/components/chart/ScenarioChart.tsx` | Create | Price-only candle chart + cache-status badge + regime tinting. |
| `frontend/web/src/components/chart/StrategyChart.tsx` | Create | Multi-run equity overlay grouped by scenario. |
| `frontend/web/src/components/scenario/CacheStatusBadge.tsx` | Modify (real impl) | Reads `cache_status` enum, renders green/yellow with bar count. |
| `frontend/web/src/routes/scenarios-detail.tsx` | Modify | Wire Definition's Preview chart, Runs tab (uses existing eval runs API filtered by scenario), Bar-cache tab. |
| `frontend/web/src/routes/strategies.tsx` (or `authoring.tsx`) | Modify | Embed `<StrategyChart>` on the strategy detail surface. |

---

## Task 1 — `build_scenario_payload`

**Files:** `crates/xvision-engine/src/api/chart.rs`, `crates/xvision-engine/tests/chart_payload.rs`

- [ ] **Step 1: Failing test**

```rust
#[tokio::test]
async fn build_scenario_payload_returns_bars_and_cache_status() {
    let ctx = ApiContext::test_with_seeded_canonicals().await;
    let payload = xvision_engine::api::chart::build_scenario_payload(&ctx, "crypto-bull-q1-2025").await.unwrap();
    assert!(!payload.bars.is_empty());
    assert!(matches!(payload.cache_status, xvision_engine::api::chart::CacheStatus::FullyCached | xvision_engine::api::chart::CacheStatus::NotCached));
    assert_eq!(payload.scenario.id, "crypto-bull-q1-2025");
}
```

- [ ] **Step 2: Add types + impl**

```rust
// crates/xvision-engine/src/api/chart.rs (appended)
use crate::api::scenario as api_scenario;
use crate::eval::scenario::Scenario;

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../frontend/web/src/api/types.gen/")]
pub struct ScenarioChartPayload {
    pub scenario: Scenario,
    pub bars: Vec<ChartBar>,
    pub cache_status: CacheStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(tag = "type")]
#[ts(export, export_to = "../../../frontend/web/src/api/types.gen/")]
pub enum CacheStatus {
    FullyCached { bar_count: u32, fetched_at: chrono::DateTime<Utc> },
    PartiallyCached { fetched_count: u32, expected_count: u32 },
    NotCached { expected_count: u32 },
}

pub async fn build_scenario_payload(ctx: &ApiContext, id: &str) -> ApiResult<ScenarioChartPayload> {
    let scenario = api_scenario::get(ctx, id).await?;
    let expected = expected_bar_count(&scenario);
    let cache_status = match ctx.store.bars_cache_row(&scenario.bar_cache_policy.cache_key).await? {
        Some(row) if (row.bar_count as u64) >= expected => CacheStatus::FullyCached { bar_count: row.bar_count as u32, fetched_at: row.fetched_at },
        Some(row) => CacheStatus::PartiallyCached { fetched_count: row.bar_count as u32, expected_count: expected as u32 },
        None => CacheStatus::NotCached { expected_count: expected as u32 },
    };
    let bars = crate::eval::bars::load_bars(ctx, &crate::eval::bars::BarCacheArgs {
        cache_key: scenario.bar_cache_policy.cache_key.clone(),
        asset_pair: scenario.asset[0].venue_symbol.clone(),
        granularity: scenario.granularity,
        start: scenario.time_window.start,
        end: scenario.time_window.end,
        data_source_tag: "alpaca-historical-v1".into(),
    }).await?;
    Ok(ScenarioChartPayload {
        scenario,
        bars: bars.iter().map(bar_to_chart_bar).collect(),
        cache_status,
    })
}

fn expected_bar_count(s: &Scenario) -> u64 {
    let hours = (s.time_window.end - s.time_window.start).num_hours().max(0) as u64;
    match s.granularity {
        crate::eval::scenario::BarGranularity::Hour1 => hours,
        crate::eval::scenario::BarGranularity::Day1  => hours / 24,
        _ => hours, // v1 only supports Hour1/Day1
    }
}
```

- [ ] **Step 3: Add `bars_cache_row` to store**

```rust
// crates/xvision-engine/src/store.rs
pub struct BarsCacheRow { pub cache_key: String, pub bar_count: i64, pub fetched_at: chrono::DateTime<chrono::Utc>, pub asset: String, pub granularity: String, pub window_start: chrono::DateTime<chrono::Utc>, pub window_end: chrono::DateTime<chrono::Utc> }

pub async fn bars_cache_row(&self, cache_key: &str) -> ApiResult<Option<BarsCacheRow>> {
    let row = sqlx::query!("SELECT cache_key, bar_count, fetched_at, asset, granularity, window_start, window_end FROM bars_cache WHERE cache_key = ?", cache_key).fetch_optional(&self.pool).await?;
    Ok(row.map(|r| BarsCacheRow {
        cache_key: r.cache_key,
        bar_count: r.bar_count,
        fetched_at: chrono::DateTime::parse_from_rfc3339(&r.fetched_at).unwrap().with_timezone(&chrono::Utc),
        asset: r.asset, granularity: r.granularity,
        window_start: chrono::DateTime::parse_from_rfc3339(&r.window_start).unwrap().with_timezone(&chrono::Utc),
        window_end:   chrono::DateTime::parse_from_rfc3339(&r.window_end).unwrap().with_timezone(&chrono::Utc),
    }))
}
```

- [ ] **Step 4: Run test, expect PASS**

```bash
cargo test -p xvision-engine --test chart_payload build_scenario_payload
```

- [ ] **Step 5: Commit**

```bash
git add crates/xvision-engine/src/api/chart.rs crates/xvision-engine/src/store.rs crates/xvision-engine/tests/chart_payload.rs
git commit -m "feat(api): build_scenario_payload with cache status"
```

---

## Task 2 — `build_strategy_payload`

**Files:** `crates/xvision-engine/src/api/chart.rs`, `crates/xvision-engine/tests/chart_payload.rs`

- [ ] **Step 1: Failing test**

```rust
#[tokio::test]
async fn build_strategy_payload_groups_runs_by_scenario() {
    let ctx = ApiContext::test_with_strategy_runs(/* 3 runs, 2 scenarios */).await;
    let payload = xvision_engine::api::chart::build_strategy_payload(&ctx, "test-strategy-id").await.unwrap();
    assert_eq!(payload.strategy_id, "test-strategy-id");
    assert!(payload.run_series.len() >= 3);
    assert_eq!(payload.scenarios.len(), 2);
}

#[tokio::test]
async fn build_strategy_payload_empty_for_unused_strategy() {
    let ctx = ApiContext::test().await;
    let payload = xvision_engine::api::chart::build_strategy_payload(&ctx, "unused-strategy").await.unwrap();
    assert!(payload.run_series.is_empty());
    assert!(payload.scenarios.is_empty());
}
```

- [ ] **Step 2: Add types + impl**

```rust
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../frontend/web/src/api/types.gen/")]
pub struct StrategyChartPayload {
    pub strategy_id: String,
    pub run_series: Vec<RunEquitySeries>,
    pub scenarios: Vec<(String, String)>,  // (scenario_id, display_name)
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../frontend/web/src/api/types.gen/")]
pub struct RunEquitySeries {
    pub run_id: String,
    pub label: String,
    pub scenario_id: String,
    pub final_pnl_usd: f64,
    pub max_drawdown_pct: f64,
    pub sharpe: Option<f64>,
    pub equity_normalised: Vec<EquityPoint>,   // t=0 at run start
}

pub async fn build_strategy_payload(ctx: &ApiContext, strategy_id: &str) -> ApiResult<StrategyChartPayload> {
    let runs = ctx.store.runs_for_strategy(strategy_id).await?;
    let mut series = Vec::with_capacity(runs.len());
    let mut scenario_ids: std::collections::BTreeMap<String, String> = std::collections::BTreeMap::new();

    for r in &runs {
        let eq = ctx.store.equity_curve(&r.id).await?;
        if eq.is_empty() { continue; }
        let start_ts = eq[0].timestamp.timestamp();
        let initial = eq[0].equity_usd;
        let normalised: Vec<EquityPoint> = eq.iter().map(|p| EquityPoint {
            time: p.timestamp.timestamp() - start_ts,
            equity_usd: p.equity_usd,
        }).collect();
        let final_pnl = normalised.last().unwrap().equity_usd - initial;
        let mut peak = f64::NEG_INFINITY;
        let max_dd = normalised.iter().map(|p| { peak = peak.max(p.equity_usd); if peak > 0.0 { (peak - p.equity_usd) / peak * 100.0 } else { 0.0 } }).fold(0.0_f64, f64::max);
        let sharpe = ctx.store.run_metrics(&r.id).await?.and_then(|m| m.sharpe);

        if !scenario_ids.contains_key(&r.scenario_id) {
            let s = api_scenario::get(ctx, &r.scenario_id).await?;
            scenario_ids.insert(r.scenario_id.clone(), s.display_name);
        }
        series.push(RunEquitySeries {
            run_id: r.id.clone(),
            label: r.label.clone().unwrap_or_else(|| r.id.clone()),
            scenario_id: r.scenario_id.clone(),
            final_pnl_usd: final_pnl,
            max_drawdown_pct: max_dd,
            sharpe,
            equity_normalised: normalised,
        });
    }
    Ok(StrategyChartPayload {
        strategy_id: strategy_id.into(),
        run_series: series,
        scenarios: scenario_ids.into_iter().collect(),
    })
}
```

- [ ] **Step 3: Add `runs_for_strategy`, `run_metrics` to store** if not present.

- [ ] **Step 4: Run test, expect PASS**

```bash
cargo test -p xvision-engine --test chart_payload build_strategy_payload
```

- [ ] **Step 5: Commit**

```bash
git add crates/xvision-engine/src/api/chart.rs crates/xvision-engine/src/store.rs crates/xvision-engine/tests/chart_payload.rs
git commit -m "feat(api): build_strategy_payload — per-run normalised equity, grouped by scenario"
```

---

## Task 3 — HTTP endpoints

**Files:** `crates/xvision-engine/src/api/chart.rs`, `crates/xvision-dashboard/src/routes.rs`

- [ ] **Step 1: Handlers**

```rust
pub async fn http_scenario_chart(State(ctx): State<Arc<ApiContext>>, Path(id): Path<String>) -> impl IntoResponse {
    match build_scenario_payload(&ctx, &id).await {
        Ok(p) => (StatusCode::OK, Json(p)).into_response(),
        Err(e) => crate::api::scenario::error_response(e),
    }
}
pub async fn http_strategy_chart(State(ctx): State<Arc<ApiContext>>, Path(id): Path<String>) -> impl IntoResponse {
    match build_strategy_payload(&ctx, &id).await {
        Ok(p) => (StatusCode::OK, Json(p)).into_response(),
        Err(e) => crate::api::scenario::error_response(e),
    }
}
```

- [ ] **Step 2: Mount routes**

```rust
// crates/xvision-dashboard/src/routes.rs
.route("/scenarios/:id/chart", axum::routing::get(chart::http_scenario_chart))
.route("/strategies/:id/chart", axum::routing::get(chart::http_strategy_chart))
```

- [ ] **Step 3: Smoke**

```bash
curl http://localhost:8080/api/scenarios/crypto-bull-q1-2025/chart | jq '.bars | length'
curl http://localhost:8080/api/strategies/bundle-canonical-defaults/chart | jq '.run_series | length'
```

- [ ] **Step 4: Commit**

```bash
git add crates/xvision-engine/src/api/chart.rs crates/xvision-dashboard/src/routes.rs
git commit -m "feat(api): HTTP endpoints for /scenarios/:id/chart and /strategies/:id/chart"
```

---

## Task 4 — Frontend API clients

**Files:** `frontend/web/src/api/chart.ts`

- [ ] **Step 1: Append clients + regenerate types**

```typescript
import type { ScenarioChartPayload } from './types.gen/ScenarioChartPayload';
import type { StrategyChartPayload } from './types.gen/StrategyChartPayload';

export const scenarioChartKeys = {
  detail: (id: string) => ['chart', 'scenario', id] as const,
};

export const strategyChartKeys = {
  detail: (id: string) => ['chart', 'strategy', id] as const,
};

export async function getScenarioChart(id: string): Promise<ScenarioChartPayload> {
  const r = await fetch(`/api/scenarios/${encodeURIComponent(id)}/chart`);
  if (!r.ok) throw new Error(`HTTP ${r.status}`);
  return r.json();
}

export async function getStrategyChart(id: string): Promise<StrategyChartPayload> {
  const r = await fetch(`/api/strategies/${encodeURIComponent(id)}/chart`);
  if (!r.ok) throw new Error(`HTTP ${r.status}`);
  return r.json();
}
```

- [ ] **Step 2: Regenerate**

```bash
cargo xtask gen-types
```

- [ ] **Step 3: Commit**

```bash
git add frontend/web/src/api/chart.ts frontend/web/src/api/types.gen/
git commit -m "feat(web): API clients for scenario + strategy chart"
```

---

## Task 5 — `<ScenarioChart>` component

**Files:** `frontend/web/src/components/chart/ScenarioChart.tsx`, `frontend/web/src/components/scenario/CacheStatusBadge.tsx`

- [ ] **Step 1: Implement cache-status badge**

```tsx
// frontend/web/src/components/scenario/CacheStatusBadge.tsx
import type { CacheStatus } from '../../api/types.gen/CacheStatus';

export function CacheStatusBadge({ status, onFetch }: { status: CacheStatus; onFetch?: () => void }) {
  if (status.type === 'FullyCached') {
    return <span className="px-2 py-0.5 rounded text-[11px] bg-green-500/15 text-green-400 border border-green-500/30">cached: {status.bar_count} bars</span>;
  }
  if (status.type === 'PartiallyCached') {
    return <span className="px-2 py-0.5 rounded text-[11px] bg-amber-500/15 text-amber-300 border border-amber-500/30">partial: {status.fetched_count}/{status.expected_count}</span>;
  }
  return (
    <span className="inline-flex items-center gap-2 px-2 py-0.5 rounded text-[11px] bg-amber-500/15 text-amber-300 border border-amber-500/30">
      not cached ({status.expected_count} bars on first run)
      {onFetch && <button onClick={onFetch} className="underline">Fetch bars</button>}
    </span>
  );
}
```

- [ ] **Step 2: Implement `<ScenarioChart>`**

```tsx
// frontend/web/src/components/chart/ScenarioChart.tsx
import { useEffect, useRef, useState } from 'react';
import { createChart, ColorType, CrosshairMode } from 'lightweight-charts';
import type { ScenarioChartPayload } from '../../api/types.gen/ScenarioChartPayload';
import { chartTheme } from './chart-theme';
import { ChartContainer, RangePreset } from './ChartContainer';
import { CacheStatusBadge } from '../scenario/CacheStatusBadge';

const REGIME_BG: Record<string, string> = {
  'regime:bull':  'rgba(34,197,94,0.05)',
  'regime:bear':  'rgba(239,68,68,0.05)',
  'regime:chop':  'rgba(148,163,184,0.05)',
  'regime:event': 'rgba(245,158,11,0.05)',
};

export function ScenarioChart({ payload, themeMode = 'dark', onFetch }: { payload: ScenarioChartPayload; themeMode?: 'dark'|'light'; onFetch?: () => void }) {
  const ref = useRef<HTMLDivElement>(null);
  const [range, setRange] = useState<RangePreset>('All');
  const [showVolume, setShowVolume] = useState(false);

  const regime = payload.scenario.tags.find((t) => t.startsWith('regime:'));
  const bg = regime ? REGIME_BG[regime] : undefined;

  useEffect(() => {
    if (!ref.current) return;
    const theme = chartTheme(themeMode);
    const c = createChart(ref.current, {
      layout: { background: { type: ColorType.Solid, color: theme.background }, textColor: theme.text },
      grid: { vertLines: { color: theme.grid }, horzLines: { color: theme.grid } },
      crosshair: { mode: CrosshairMode.Normal },
    });
    const candle = c.addCandlestickSeries({
      upColor: theme.series.candleUp, downColor: theme.series.candleDown,
      wickUpColor: theme.series.candleUp, wickDownColor: theme.series.candleDown, borderVisible: false,
    });
    candle.setData(payload.bars.map((b) => ({ time: b.time as any, open: b.open, high: b.high, low: b.low, close: b.close })));
    if (showVolume) {
      const vol = c.addHistogramSeries({ priceScaleId: 'volume' });
      vol.setData(payload.bars.map((b) => ({ time: b.time as any, value: b.volume, color: b.close >= b.open ? theme.series.candleUp : theme.series.candleDown })));
      c.priceScale('volume').applyOptions({ scaleMargins: { top: 0.8, bottom: 0 } });
    }
    return () => c.remove();
  }, [payload, themeMode, showVolume]);

  return (
    <div style={{ background: bg }}>
      <div className="flex items-center justify-between mb-2">
        <span className="text-text-3 text-[12px]">{payload.scenario.asset[0].symbol} · {payload.scenario.granularity}</span>
        <CacheStatusBadge status={payload.cache_status} onFetch={onFetch} />
      </div>
      <ChartContainer
        range={range}
        onRange={setRange}
        layersPanel={
          <label className="flex items-center gap-2">
            <input type="checkbox" checked={showVolume} onChange={(e) => setShowVolume(e.target.checked)} /> Volume histogram
          </label>
        }
      >
        <div ref={ref} style={{ height: 360 }} />
      </ChartContainer>
    </div>
  );
}
```

- [ ] **Step 3: Commit**

```bash
git add frontend/web/src/components/chart/ScenarioChart.tsx frontend/web/src/components/scenario/CacheStatusBadge.tsx
git commit -m "feat(web): ScenarioChart with cache-status badge + regime tinting"
```

---

## Task 6 — Wire `/scenarios/:id` Preview + Runs + Bar-cache tabs

**Files:** `frontend/web/src/routes/scenarios-detail.tsx`

- [ ] **Step 1: Add Preview chart to the Definition tab**

```tsx
import { useQuery } from '@tanstack/react-query';
import { getScenarioChart, scenarioChartKeys } from '../api/chart';
import { ScenarioChart } from '../components/chart/ScenarioChart';

// inside DefinitionTab:
const chart = useQuery({ queryKey: scenarioChartKeys.detail(s.id), queryFn: () => getScenarioChart(s.id) });
{chart.data && <ScenarioChart payload={chart.data} />}
{chart.isLoading && <div className="text-text-3 text-[13px]">Loading chart…</div>}
{chart.error && <div className="text-danger text-[13px]">Chart unavailable.</div>}
```

- [ ] **Step 2: Replace stub `RunsTab` with a real list**

```tsx
import { listRuns } from '../api/eval';
function RunsTab({ scenarioId }: { scenarioId: string }) {
  const { data, isLoading, error } = useQuery({
    queryKey: ['runs', { scenarioId }],
    queryFn: () => listRuns({ scenario: scenarioId }),
  });
  if (isLoading) return <div className="text-text-3 text-[13px]">Loading runs…</div>;
  if (error) return <div className="text-danger text-[13px]">{String(error)}</div>;
  if (!data || data.length === 0) return <div className="text-text-3 text-[13px]">No runs yet against this scenario.</div>;
  return (
    <table className="w-full text-[13px]">
      <thead><tr className="text-text-3 text-left"><th>Run</th><th>Strategy</th><th>Mode</th><th>Status</th><th>Completed</th></tr></thead>
      <tbody>
        {data.map((r) => (
          <tr key={r.id} className="border-t border-border">
            <td className="py-2 pr-4"><Link to={`/eval-runs/${r.id}`} className="underline">{r.id}</Link></td>
            <td className="py-2 pr-4 font-mono">{r.agent_id}</td>
            <td className="py-2 pr-4">{r.mode}</td>
            <td className="py-2 pr-4">{r.status}</td>
            <td className="py-2 pr-4 text-text-3">{r.completed_at ?? '—'}</td>
          </tr>
        ))}
      </tbody>
    </table>
  );
}
```

- [ ] **Step 3: Replace stub `BarCacheTab` with a real view**

```tsx
function BarCacheTab({ cacheKey }: { cacheKey: string }) {
  const { data } = useQuery({ queryKey: ['bars-cache', cacheKey], queryFn: () => fetch(`/api/bars/${cacheKey}`).then((r) => r.json()) });
  if (!data) return <div className="text-text-3 text-[13px]">No cache row yet.</div>;
  return (
    <dl className="grid grid-cols-[180px_1fr] gap-y-2 text-[13px]">
      <dt className="text-text-3">Cache key</dt><dd className="font-mono text-[11px] break-all">{cacheKey}</dd>
      <dt className="text-text-3">Asset</dt><dd className="font-mono">{data.asset}</dd>
      <dt className="text-text-3">Granularity</dt><dd className="font-mono">{data.granularity}</dd>
      <dt className="text-text-3">Bars</dt><dd className="font-mono">{data.bar_count}</dd>
      <dt className="text-text-3">Fetched</dt><dd className="font-mono">{data.fetched_at}</dd>
      <dt className="text-text-3"></dt><dd><button className="border border-border px-2 py-1 rounded text-[12px]">Refetch</button></dd>
    </dl>
  );
}
```

- [ ] **Step 4: Add `GET /api/bars/:cache_key` endpoint** if not present:

```rust
// crates/xvision-engine/src/api/chart.rs
pub async fn http_bars_cache_row(State(ctx): State<Arc<ApiContext>>, Path(key): Path<String>) -> impl IntoResponse {
    match ctx.store.bars_cache_row(&key).await {
        Ok(Some(r)) => (StatusCode::OK, Json(r)).into_response(),
        Ok(None)    => (StatusCode::NOT_FOUND, Json(serde_json::json!({"error":"not cached"}))).into_response(),
        Err(e)      => crate::api::scenario::error_response(ApiError::Internal(e.to_string())),
    }
}
```

And mount: `.route("/bars/:cache_key", axum::routing::get(chart::http_bars_cache_row))`.

- [ ] **Step 5: Smoke test**

Visit `/scenarios/crypto-bull-q1-2025` and `/scenarios/<a-user-scenario>` — Preview chart renders, Runs tab shows associated runs, Bar-cache tab shows cache row.

- [ ] **Step 6: Commit**

```bash
git add frontend/web/src/routes/scenarios-detail.tsx crates/xvision-engine/src/api/chart.rs crates/xvision-dashboard/src/routes.rs
git commit -m "feat(web): scenario detail Preview chart + Runs tab + Bar-cache tab"
```

---

## Task 7 — `<StrategyChart>` component

**Files:** `frontend/web/src/components/chart/StrategyChart.tsx`

- [ ] **Step 1: Implement**

```tsx
import { useEffect, useRef, useMemo, useState } from 'react';
import { createChart, ColorType, CrosshairMode } from 'lightweight-charts';
import type { StrategyChartPayload } from '../../api/types.gen/StrategyChartPayload';
import { chartTheme } from './chart-theme';
import { ChartContainer, RangePreset } from './ChartContainer';

const SCENARIO_PALETTE = ['#22d3ee', '#a78bfa', '#34d399', '#fbbf24', '#f87171', '#60a5fa', '#fb923c', '#10b981'];

export function StrategyChart({ payload, themeMode = 'dark' }: { payload: StrategyChartPayload; themeMode?: 'dark'|'light' }) {
  const ref = useRef<HTMLDivElement>(null);
  const [range, setRange] = useState<RangePreset>('All');

  // Stable color per scenario_id
  const scenarioColors = useMemo(() => {
    const m = new Map<string, string>();
    payload.scenarios.forEach(([id], i) => m.set(id, SCENARIO_PALETTE[i % SCENARIO_PALETTE.length]));
    return m;
  }, [payload.scenarios]);

  useEffect(() => {
    if (!ref.current) return;
    const theme = chartTheme(themeMode);
    const c = createChart(ref.current, {
      layout: { background: { type: ColorType.Solid, color: theme.background }, textColor: theme.text },
      grid: { vertLines: { color: theme.grid }, horzLines: { color: theme.grid } },
      crosshair: { mode: CrosshairMode.Normal },
      timeScale: { timeVisible: false, secondsVisible: false },
    });
    for (const r of payload.run_series) {
      const color = scenarioColors.get(r.scenario_id) ?? '#94a3b8';
      const line = c.addLineSeries({ color, lineWidth: 1, title: r.label });
      line.setData(r.equity_normalised.map((p) => ({ time: p.time as any, value: p.equity_usd })));
    }
    return () => c.remove();
  }, [payload, themeMode, scenarioColors]);

  if (payload.run_series.length === 0) {
    return <div className="px-4 py-8 text-text-3 text-[13px] text-center">This strategy has no completed runs yet. Launch one from <code>/eval-runs</code>.</div>;
  }

  return (
    <div>
      <Legend payload={payload} scenarioColors={scenarioColors} />
      <ChartContainer
        range={range}
        onRange={setRange}
        layersPanel={<div className="text-text-3">No layers in v1.</div>}
      >
        <div ref={ref} style={{ height: 420 }} />
      </ChartContainer>
    </div>
  );
}

function Legend({ payload, scenarioColors }: { payload: StrategyChartPayload; scenarioColors: Map<string, string> }) {
  // group counts by scenario
  const counts = new Map<string, number>();
  for (const r of payload.run_series) counts.set(r.scenario_id, (counts.get(r.scenario_id) ?? 0) + 1);
  return (
    <div className="flex flex-wrap gap-3 text-[12px] mb-2">
      {payload.scenarios.map(([sid, name]) => (
        <span key={sid} className="inline-flex items-center gap-1.5">
          <span className="inline-block w-3 h-1.5" style={{ background: scenarioColors.get(sid) }} />
          {name} ({counts.get(sid) ?? 0} runs)
        </span>
      ))}
    </div>
  );
}
```

- [ ] **Step 2: Commit**

```bash
git add frontend/web/src/components/chart/StrategyChart.tsx
git commit -m "feat(web): StrategyChart — per-run equity overlay grouped by scenario"
```

---

## Task 8 — Embed `<StrategyChart>` on strategy detail

**Files:** `frontend/web/src/routes/strategies.tsx` (or `authoring.tsx` if strategy-detail lives there)

- [ ] **Step 1: Locate the strategy-detail surface**

```bash
grep -rn "strategy.*detail\|StrategyDetail\|/strategies/:id\|/authoring/" frontend/web/src/routes/
```

The route at `/authoring/<id>` or `/strategies/<id>` (depending on the existing pattern).

- [ ] **Step 2: Add chart query + render**

```tsx
import { getStrategyChart, strategyChartKeys } from '../api/chart';
import { StrategyChart } from '../components/chart/StrategyChart';

const chart = useQuery({ queryKey: strategyChartKeys.detail(strategyId), queryFn: () => getStrategyChart(strategyId) });
{chart.data && <StrategyChart payload={chart.data} />}
{chart.isLoading && <div className="text-text-3 text-[13px]">Loading history…</div>}
```

Position the chart in a "Performance history" section on the detail page (above or below the existing strategy-info block).

- [ ] **Step 3: Smoke test**

```bash
cd frontend/web && pnpm dev
# Open a strategy that has at least 2 runs → chart renders with one line per run, grouped color by scenario.
```

- [ ] **Step 4: Commit**

```bash
git add frontend/web/src/routes/strategies.tsx
git commit -m "feat(web): embed StrategyChart on strategy detail"
```

---

## Task 9 — M2 acceptance smoke

- [ ] **Step 1: Workspace + frontend tests**

```bash
cargo test --workspace
cd frontend/web && pnpm typecheck && pnpm build && pnpm vitest run
```

- [ ] **Step 2: Manual smoke**

1. `/scenarios/crypto-bull-q1-2025` → Definition tab shows price candles; cache-status badge present; regime tint background green-ish.
2. `/scenarios/<new ETH scenario>` → cache-status starts "not cached"; clicking Fetch populates; reload → cache-status reads "cached: N bars".
3. `/scenarios/<id>` → Runs tab lists runs; Bar-cache tab shows row.
4. `/strategies/bundle-canonical-defaults` → StrategyChart renders if any runs exist; legend lists scenarios with run counts.

- [ ] **Step 3: Commit cleanup**

```bash
git add -p
git commit -m "chore: M2 acceptance smoke passes (scenario + strategy charts live)"
```

---

## Self-review notes

- All three tabs on scenario detail now render real data; no stubs remain.
- Cache-status badge handles all three `CacheStatus` enum variants.
- Strategy chart color-codes by scenario_id (stable across re-renders via `useMemo`).
- 10-run cap from M1 still applies; strategy chart doesn't impose its own cap since runs are scoped to one strategy.
- No placeholders.
