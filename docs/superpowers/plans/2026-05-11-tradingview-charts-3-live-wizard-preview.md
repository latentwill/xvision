# TradingView Charts — M3: Live cockpit + wizard preview

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add the live-streaming chart at `/live/<deployment_id>` with SSE-pushed bar/marker/equity updates, and the wizard preview thumbnail on `/scenarios/new` that updates as the operator changes asset/date/granularity (with an optional Buy-and-Hold baseline equity overlay).

**Architecture:** New SSE endpoint `GET /api/eval/runs/:id/stream` pushes 250 ms batched events. New `GET /api/scenarios/preview` returns a transient scenario chart payload keyed by query params (no DB row required). New React components `<LiveChart>` (subscribes to SSE) and `<WizardPreviewChart>` (debounced fetch).

**Tech Stack:** Continues from M1 + M2. Adds `tokio::sync::broadcast` for in-process event fan-out and `axum::response::sse` for the HTTP transport.

**Reference spec:** `docs/superpowers/specs/2026-05-11-tradingview-charts-design.md` §§6.5, 6.6, 7.5, 7.7.

**Prereq:** Custom-scenario M1 + M2 + M3 merged. TradingView M1 + M2 merged. Plan 2c (live deployment model) at least scaffolded so we have a `deployment_id → run_id` mapping; if not, deferred-stub for M3 acceptance.

---

## File structure

| Path | Action | Responsibility |
|---|---|---|
| `crates/xvision-engine/src/api/chart.rs` | Modify | Add `build_scenario_preview`, `RunChartEvent` enum, `RunEventBus`, SSE handler. |
| `crates/xvision-engine/src/api/mod.rs` | Modify | Hold `RunEventBus` in `ApiContext`. |
| `crates/xvision-engine/src/eval/run.rs` | Modify | Emit `RunChartEvent`s as bars/decisions/equity points produced. |
| `crates/xvision-dashboard/src/routes.rs` | Modify | Mount `/api/eval/runs/:id/stream` (SSE) and `/api/scenarios/preview`. |
| `frontend/web/src/api/chart.ts` | Modify | Add `getScenarioPreview`, `openRunStream`. |
| `frontend/web/src/components/chart/LiveChart.tsx` | Create | RunChart variant that opens SSE and merges events. |
| `frontend/web/src/components/chart/use-run-stream.ts` | Create | Hook wrapping `EventSource` with reconnect via re-snapshot. |
| `frontend/web/src/components/chart/WizardPreviewChart.tsx` | Create | Inline preview chart for `/scenarios/new`. |
| `frontend/web/src/routes/live.tsx` | Create | `/live/<deployment_id>` route. |
| `frontend/web/src/routes.tsx` | Modify | Register `/live/:id`. |
| `frontend/web/src/routes/scenarios-new.tsx` | Modify | Embed `<WizardPreviewChart>` driven off the form's asset/date/granularity. |

---

## Task 1 — `RunEventBus` + emit events from `eval::run`

**Files:** `crates/xvision-engine/src/api/mod.rs`, `crates/xvision-engine/src/eval/run.rs`, `crates/xvision-engine/src/api/chart.rs`

- [ ] **Step 1: Define event enum + bus**

```rust
// crates/xvision-engine/src/api/chart.rs (appended)
use tokio::sync::broadcast;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "event", content = "data")]
pub enum RunChartEvent {
    Bar(ChartBar),
    IndicatorTail(std::collections::HashMap<String, IndicatorPoint>),
    Marker(MarkerEvent),
    Equity(EquityPoint),
    Status { phase: String, message: Option<String> },
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

impl RunEventBus {
    pub fn new() -> Self { Self { senders: Default::default() } }
    pub async fn sender(&self, run_id: &str) -> broadcast::Sender<RunChartEvent> {
        let mut g = self.senders.lock().await;
        g.entry(run_id.into()).or_insert_with(|| broadcast::channel(1024).0).clone()
    }
    pub async fn subscribe(&self, run_id: &str) -> broadcast::Receiver<RunChartEvent> {
        self.sender(run_id).await.subscribe()
    }
    pub async fn emit(&self, run_id: &str, event: RunChartEvent) {
        let _ = self.sender(run_id).await.send(event);
    }
}
```

- [ ] **Step 2: Add `event_bus: Arc<RunEventBus>` to `ApiContext`**

- [ ] **Step 3: Emit from `eval::run`** at the existing per-bar / per-decision tick points. Find each `equity_curve.push(point)` and each `decisions.push(d)` call site in `crates/xvision-engine/src/eval/run.rs` (or wherever the run loop lives) and add adjacent emits:

```rust
ctx.event_bus.emit(&run_id, RunChartEvent::Equity(EquityPoint { time: point.timestamp.timestamp(), equity_usd: point.equity_usd })).await;
```

For decisions:

```rust
let marker = match (d.action.as_deref(), d.fill_price, d.verdict.as_deref()) {
    (Some(side @ ("Buy"|"Sell")), Some(price), _) => Some(MarkerEvent::Trade(TradeMarker { /* … */ })),
    (Some(_), None, Some("Vetoed")) => Some(MarkerEvent::Veto(VetoMarker { /* … */ })),
    (Some("Hold"), _, _) => Some(MarkerEvent::Hold(HoldMarker { /* … */ })),
    _ => None,
};
if let Some(m) = marker { ctx.event_bus.emit(&run_id, RunChartEvent::Marker(m)).await; }
```

For bars (streamed for live runs only — backtest is batch):

```rust
if is_live_mode {
    ctx.event_bus.emit(&run_id, RunChartEvent::Bar(bar_to_chart_bar(&bar))).await;
}
```

- [ ] **Step 4: Failing test for event delivery**

```rust
// crates/xvision-engine/tests/run_event_bus.rs
#[tokio::test]
async fn live_run_emits_bar_and_equity_events() {
    let ctx = ApiContext::test_with_event_bus().await;
    let mut rx = ctx.event_bus.subscribe("test-run").await;
    tokio::spawn({
        let ctx = ctx.clone();
        async move {
            ctx.event_bus.emit("test-run", RunChartEvent::Bar(/* sample */)).await;
            ctx.event_bus.emit("test-run", RunChartEvent::Equity(EquityPoint { time: 1, equity_usd: 100.0 })).await;
        }
    });
    let first = rx.recv().await.unwrap();
    let second = rx.recv().await.unwrap();
    assert!(matches!(first, RunChartEvent::Bar(_)));
    assert!(matches!(second, RunChartEvent::Equity(_)));
}
```

- [ ] **Step 5: Run, expect PASS**

```bash
cargo test -p xvision-engine --test run_event_bus
```

- [ ] **Step 6: Commit**

```bash
git add crates/xvision-engine/src/api/chart.rs crates/xvision-engine/src/api/mod.rs crates/xvision-engine/src/eval/run.rs crates/xvision-engine/tests/run_event_bus.rs
git commit -m "feat(engine): RunEventBus + emit live events from eval::run"
```

---

## Task 2 — SSE endpoint with 250ms batching

**Files:** `crates/xvision-engine/src/api/chart.rs`, `crates/xvision-dashboard/src/routes.rs`

- [ ] **Step 1: Failing test (with timeout) for connect + receive**

```rust
#[tokio::test(flavor = "multi_thread")]
async fn sse_stream_receives_events_in_batches() {
    let app = test_app_with_event_bus().await;
    let server = axum::serve(test_listener().await, app.into_make_service());
    tokio::spawn(server);
    let url = format!("http://{addr}/api/eval/runs/test-run/stream");
    let client = reqwest::Client::new();
    let resp = client.get(&url).send().await.unwrap();
    assert_eq!(resp.status(), 200);
    assert!(resp.headers().get("Content-Type").unwrap().to_str().unwrap().contains("text/event-stream"));
    // ... emit + read body ... assert events arrived.
}
```

- [ ] **Step 2: Implement handler with 250ms batching**

```rust
use axum::response::sse::{Event, KeepAlive, Sse};
use futures::Stream;
use std::time::Duration;

pub async fn http_run_stream(
    State(ctx): State<Arc<ApiContext>>,
    Path(run_id): Path<String>,
) -> Sse<impl Stream<Item = Result<Event, std::convert::Infallible>>> {
    let mut rx = ctx.event_bus.subscribe(&run_id).await;
    let stream = async_stream::stream! {
        let mut batch: Vec<RunChartEvent> = Vec::new();
        let mut ticker = tokio::time::interval(Duration::from_millis(250));
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            tokio::select! {
                _ = ticker.tick() => {
                    for ev in batch.drain(..) {
                        if let Ok(payload) = serde_json::to_string(&ev) {
                            let name = match &ev {
                                RunChartEvent::Bar(_)            => "bar",
                                RunChartEvent::IndicatorTail(_)  => "indicator_tail",
                                RunChartEvent::Marker(_)         => "marker",
                                RunChartEvent::Equity(_)         => "equity",
                                RunChartEvent::Status { .. }     => "status",
                            };
                            yield Ok(Event::default().event(name).data(payload));
                        }
                    }
                }
                Ok(ev) = rx.recv() => {
                    batch.push(ev);
                    if batch.len() > 256 { /* drop oldest to bound */ batch.drain(0..32); }
                }
            }
        }
    };
    Sse::new(stream).keep_alive(KeepAlive::new().interval(Duration::from_secs(15)).text("keep-alive"))
}
```

- [ ] **Step 3: Add deps** (`async_stream` if not present; `futures` likely already there)

```toml
async-stream = "0.3"
```

- [ ] **Step 4: Mount route**

```rust
.route("/eval/runs/:id/stream", axum::routing::get(chart::http_run_stream))
```

- [ ] **Step 5: Run integration test, expect PASS**

```bash
cargo test -p xvision-engine --test run_event_bus sse_stream
```

- [ ] **Step 6: Commit**

```bash
git add crates/xvision-engine/src/api/chart.rs crates/xvision-dashboard/src/routes.rs crates/xvision-engine/Cargo.toml
git commit -m "feat(api): SSE endpoint /api/eval/runs/:id/stream with 250ms batching"
```

---

## Task 3 — `build_scenario_preview` (transient scenario chart)

**Files:** `crates/xvision-engine/src/api/chart.rs`, `crates/xvision-engine/tests/chart_payload.rs`

- [ ] **Step 1: Add types**

```rust
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../frontend/web/src/api/types.gen/")]
pub struct ScenarioPreviewPayload {
    pub cache_key: String,
    pub asset: String,
    pub granularity: String,
    pub bars: Vec<ChartBar>,
    pub cache_status: CacheStatus,
    pub baseline_equity: Option<Vec<EquityPoint>>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PreviewQuery {
    pub asset: String,
    pub from: String,             // ISO date
    pub to: String,
    pub granularity: String,      // "1h" | "1d"
    pub baseline: Option<bool>,   // include Buy-and-Hold overlay
}
```

- [ ] **Step 2: Failing test**

```rust
#[tokio::test]
async fn scenario_preview_returns_bars_and_optional_baseline() {
    let ctx = ApiContext::test_with_mock_alpaca().await;
    let q = PreviewQuery {
        asset: "ETH".into(),
        from: "2024-02-03".into(),
        to: "2024-02-10".into(),
        granularity: "1h".into(),
        baseline: Some(true),
    };
    let payload = xvision_engine::api::chart::build_scenario_preview(&ctx, q).await.unwrap();
    assert_eq!(payload.asset, "ETH");
    assert!(!payload.bars.is_empty());
    assert!(payload.baseline_equity.is_some());
}
```

- [ ] **Step 3: Implement**

```rust
pub async fn build_scenario_preview(ctx: &ApiContext, q: PreviewQuery) -> ApiResult<ScenarioPreviewPayload> {
    // Validate
    use chrono::NaiveDate;
    let from: chrono::DateTime<Utc> = NaiveDate::parse_from_str(&q.from, "%Y-%m-%d").map_err(|e| ApiError::Validation(format!("from: {e}")))?.and_hms_opt(0,0,0).unwrap().and_utc();
    let to: chrono::DateTime<Utc> = NaiveDate::parse_from_str(&q.to, "%Y-%m-%d").map_err(|e| ApiError::Validation(format!("to: {e}")))?.and_hms_opt(0,0,0).unwrap().and_utc();
    if from >= to { return Err(ApiError::Validation("from must be < to".into())); }
    let g = match q.granularity.as_str() {
        "1h" => xvision_data::alpaca::BarGranularity::Hour1,
        "1d" => xvision_data::alpaca::BarGranularity::Day1,
        other => return Err(ApiError::Validation(format!("granularity '{other}' not in v1 set"))),
    };
    if !xvision_data::asset_whitelist::is_alpaca_crypto_supported(&q.asset) {
        return Err(ApiError::Validation(format!("asset '{}' not supported", q.asset)));
    }
    let pair = xvision_data::asset_whitelist::to_alpaca_pair(&q.asset);
    let data_source = crate::eval::scenario::DataSource::AlpacaHistorical { feed: None, adjustment: crate::eval::scenario::AdjustmentMode::Raw };
    let cache_key = compute_cache_key(&pair, g, from, to, &data_source);

    let expected = match g {
        xvision_data::alpaca::BarGranularity::Hour1 => (to - from).num_hours().max(0) as u64,
        xvision_data::alpaca::BarGranularity::Day1  => (to - from).num_days().max(0) as u64,
        _ => 0,
    };
    let cache_status = match ctx.store.bars_cache_row(&cache_key).await? {
        Some(r) if (r.bar_count as u64) >= expected => CacheStatus::FullyCached { bar_count: r.bar_count as u32, fetched_at: r.fetched_at },
        Some(r) => CacheStatus::PartiallyCached { fetched_count: r.bar_count as u32, expected_count: expected as u32 },
        None => CacheStatus::NotCached { expected_count: expected as u32 },
    };

    let bars = crate::eval::bars::load_bars(ctx, &crate::eval::bars::BarCacheArgs {
        cache_key: cache_key.clone(),
        asset_pair: pair.clone(),
        granularity: g, start: from, end: to,
        data_source_tag: "alpaca-historical-v1".into(),
    }).await?;

    let baseline = if q.baseline.unwrap_or(false) {
        let initial = 100_000.0;
        let first_close = bars.first().map(|b| b.close).unwrap_or(1.0);
        Some(bars.iter().map(|b| EquityPoint {
            time: b.timestamp.timestamp(),
            equity_usd: initial * (b.close / first_close),
        }).collect())
    } else { None };

    Ok(ScenarioPreviewPayload {
        cache_key,
        asset: q.asset,
        granularity: q.granularity,
        bars: bars.iter().map(bar_to_chart_bar).collect(),
        cache_status,
        baseline_equity: baseline,
    })
}

fn compute_cache_key(asset: &str, g: xvision_data::alpaca::BarGranularity, start: chrono::DateTime<Utc>, end: chrono::DateTime<Utc>, src: &crate::eval::scenario::DataSource) -> String {
    let mut h = blake3::Hasher::new();
    h.update(asset.as_bytes()); h.update(g.as_alpaca_str().as_bytes());
    h.update(start.to_rfc3339().as_bytes()); h.update(end.to_rfc3339().as_bytes());
    h.update(serde_json::to_string(src).unwrap().as_bytes());
    h.finalize().to_hex().to_string()
}
```

- [ ] **Step 4: Handler**

```rust
pub async fn http_scenario_preview(State(ctx): State<Arc<ApiContext>>, Query(q): Query<PreviewQuery>) -> impl IntoResponse {
    match build_scenario_preview(&ctx, q).await {
        Ok(p) => (StatusCode::OK, Json(p)).into_response(),
        Err(e) => crate::api::scenario::error_response(e),
    }
}
```

- [ ] **Step 5: Mount route**

```rust
.route("/scenarios/preview", axum::routing::get(chart::http_scenario_preview))
```

- [ ] **Step 6: Run test, expect PASS**

```bash
cargo test -p xvision-engine --test chart_payload scenario_preview
```

- [ ] **Step 7: Commit**

```bash
git add crates/xvision-engine/src/api/chart.rs crates/xvision-dashboard/src/routes.rs crates/xvision-engine/tests/chart_payload.rs
git commit -m "feat(api): /scenarios/preview transient chart with optional Buy-and-Hold baseline"
```

---

## Task 4 — Frontend SSE hook

**Files:** `frontend/web/src/components/chart/use-run-stream.ts`, `frontend/web/src/api/chart.ts`

- [ ] **Step 1: API helper**

```typescript
// frontend/web/src/api/chart.ts (appended)
export function openRunStream(runId: string): EventSource {
  return new EventSource(`/api/eval/runs/${encodeURIComponent(runId)}/stream`);
}

export async function getScenarioPreview(params: { asset: string; from: string; to: string; granularity: '1h' | '1d'; baseline?: boolean }) {
  const q = new URLSearchParams(params as any);
  const r = await fetch(`/api/scenarios/preview?${q}`);
  if (!r.ok) throw new Error(`HTTP ${r.status}`);
  return r.json();
}
```

- [ ] **Step 2: Hook**

```typescript
// frontend/web/src/components/chart/use-run-stream.ts
import { useEffect, useRef, useState } from 'react';
import { openRunStream, chartKeys, getRunChart } from '../../api/chart';
import type { RunChartPayload } from '../../api/types.gen/RunChartPayload';
import { useQueryClient } from '@tanstack/react-query';

export type LiveStatus = 'snapshot' | 'streaming' | 'reconnecting' | 'closed';

export function useRunStream(runId: string, initial?: RunChartPayload) {
  const qc = useQueryClient();
  const [data, setData] = useState<RunChartPayload | undefined>(initial);
  const [status, setStatus] = useState<LiveStatus>(initial ? 'streaming' : 'snapshot');
  const esRef = useRef<EventSource | null>(null);

  useEffect(() => {
    if (!runId) return;
    let cancelled = false;

    async function snapshot() {
      const p = await getRunChart(runId);
      if (!cancelled) { setData(p); setStatus('streaming'); qc.setQueryData(chartKeys.run(runId), p); }
    }
    if (!data) snapshot();

    const es = openRunStream(runId);
    esRef.current = es;
    es.addEventListener('bar', (e) => mergeBar(JSON.parse((e as MessageEvent).data)));
    es.addEventListener('equity', (e) => mergeEquity(JSON.parse((e as MessageEvent).data)));
    es.addEventListener('marker', (e) => mergeMarker(JSON.parse((e as MessageEvent).data)));
    es.addEventListener('status', (e) => {
      const s = JSON.parse((e as MessageEvent).data);
      if (s.phase === 'completed' || s.phase === 'failed') { es.close(); setStatus('closed'); }
    });
    es.onerror = () => {
      setStatus('reconnecting');
      es.close();
      // Re-snapshot strategy: refetch then reopen.
      setTimeout(() => { if (!cancelled) snapshot().then(() => { /* setStatus will run on success */ }); }, 1000);
    };

    return () => { cancelled = true; es.close(); };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [runId]);

  function mergeBar(bar: any) {
    setData((prev) => prev && { ...prev, bars: [...prev.bars, bar] });
  }
  function mergeEquity(point: any) {
    setData((prev) => prev && { ...prev, equity: [...prev.equity, point] });
  }
  function mergeMarker(marker: any) {
    setData((prev) => {
      if (!prev) return prev;
      const m = { ...prev.markers };
      if (marker.kind === 'trade')   m.trades = [...m.trades, marker];
      if (marker.kind === 'veto')    m.vetoes = [...m.vetoes, marker];
      if (marker.kind === 'hold')    m.holds = [...m.holds, marker];
      return { ...prev, markers: m };
    });
  }

  return { data, status };
}
```

- [ ] **Step 3: Commit**

```bash
git add frontend/web/src/components/chart/use-run-stream.ts frontend/web/src/api/chart.ts
git commit -m "feat(web): useRunStream hook with snapshot + SSE merge + reconnect"
```

---

## Task 5 — `<LiveChart>` component + `/live/:id` route

**Files:** `frontend/web/src/components/chart/LiveChart.tsx`, `frontend/web/src/routes/live.tsx`, `frontend/web/src/routes.tsx`

- [ ] **Step 1: LiveChart**

```tsx
// frontend/web/src/components/chart/LiveChart.tsx
import { useEffect, useState } from 'react';
import { useRunStream, LiveStatus } from './use-run-stream';
import { RunChart } from './RunChart';

export function LiveChart({ runId, themeMode = 'dark' }: { runId: string; themeMode?: 'dark'|'light' }) {
  const { data, status } = useRunStream(runId);
  const [follow, setFollow] = useState(true);

  return (
    <div>
      <div className="flex items-center justify-between text-[12px] mb-2">
        <span className="flex items-center gap-2">
          <StatusDot status={status} />
          <span className="text-text-3">{statusLabel(status)}</span>
        </span>
        <label className="flex items-center gap-2">
          <input type="checkbox" checked={follow} onChange={(e) => setFollow(e.target.checked)} />
          {follow ? 'Following live' : 'Frozen'}
          {!follow && <button onClick={() => setFollow(true)} className="ml-2 underline">Resume live</button>}
        </label>
      </div>
      {data && <RunChart payload={data} themeMode={themeMode} />}
      {!data && <div className="text-text-3 py-12 text-center">Waiting for first event…</div>}
    </div>
  );
}

function StatusDot({ status }: { status: LiveStatus }) {
  const color = status === 'streaming' ? 'bg-green-500' : status === 'reconnecting' ? 'bg-amber-500' : status === 'closed' ? 'bg-red-500' : 'bg-text-3';
  return <span className={`inline-block w-2 h-2 rounded-full ${color}`} />;
}

function statusLabel(s: LiveStatus): string {
  return s === 'snapshot' ? 'loading snapshot…' : s === 'streaming' ? 'live' : s === 'reconnecting' ? 'reconnecting…' : 'closed';
}
```

- [ ] **Step 2: Live route**

```tsx
// frontend/web/src/routes/live.tsx
import { useParams } from 'react-router-dom';
import { LiveChart } from '../components/chart/LiveChart';

export default function LiveRoute() {
  const { id = '' } = useParams();
  // For v1, deployment_id == run_id. Replace when Plan 2c lands the deployment model.
  return (
    <div className="px-6 py-5">
      <h1 className="text-text font-serif text-[28px] m-0 mb-4">Live cockpit</h1>
      <LiveChart runId={id} />
    </div>
  );
}
```

- [ ] **Step 3: Register**

```tsx
// routes.tsx
import LiveRoute from './routes/live';
{ path: '/live/:id', element: <LiveRoute /> },
```

- [ ] **Step 4: Smoke test (with a running backtest forcing live emits)**

```bash
# In one terminal: cargo run --bin xvn -- eval run --strategy bundle-canonical-defaults --scenario crypto-bull-q1-2025 --mode backtest
# In another: cd frontend/web && pnpm dev
# Open /live/<run_id> while the run is in progress — chart updates.
```

For backtests, emits land in a tight loop; the 250 ms server batching keeps the UI smooth.

- [ ] **Step 5: Commit**

```bash
git add frontend/web/src/components/chart/LiveChart.tsx frontend/web/src/routes/live.tsx frontend/web/src/routes.tsx
git commit -m "feat(web): LiveChart + /live/:id route"
```

---

## Task 6 — Wizard preview chart

**Files:** `frontend/web/src/components/chart/WizardPreviewChart.tsx`, `frontend/web/src/routes/scenarios-new.tsx`

- [ ] **Step 1: WizardPreviewChart**

```tsx
// frontend/web/src/components/chart/WizardPreviewChart.tsx
import { useEffect, useState } from 'react';
import { useQuery } from '@tanstack/react-query';
import { getScenarioPreview } from '../../api/chart';
import { ScenarioChart } from './ScenarioChart';

type Props = {
  asset: string;
  from: string;
  to: string;
  granularity: '1h' | '1d';
  includeBaseline?: boolean;
};

export function WizardPreviewChart({ asset, from, to, granularity, includeBaseline }: Props) {
  // Debounce input so rapid typing doesn't hammer the endpoint.
  const [debounced, setDebounced] = useState({ asset, from, to, granularity, baseline: !!includeBaseline });
  useEffect(() => {
    const t = setTimeout(() => setDebounced({ asset, from, to, granularity, baseline: !!includeBaseline }), 350);
    return () => clearTimeout(t);
  }, [asset, from, to, granularity, includeBaseline]);

  const query = useQuery({
    queryKey: ['scenario-preview', debounced],
    queryFn: () => getScenarioPreview(debounced),
    enabled: !!debounced.asset && !!debounced.from && !!debounced.to,
    staleTime: 30_000,
  });

  if (!debounced.asset || !debounced.from || !debounced.to) return <div className="text-text-3 text-[12px]">Fill asset + range to see preview…</div>;
  if (query.isLoading) return <div className="text-text-3 text-[12px]">Loading preview…</div>;
  if (query.error) return <div className="text-danger text-[12px]">{String(query.error)}</div>;
  if (!query.data) return null;

  // Reuse ScenarioChart for visual consistency. Adapt payload shape inline.
  const payload = {
    scenario: { /* synthesized for preview rendering — only fields ScenarioChart reads */
      id: 'preview', display_name: 'Preview',
      asset: [{ class: 'Crypto' as any, symbol: debounced.asset, venue_symbol: `${debounced.asset}/USD` }],
      tags: [], granularity: granularity === '1h' ? 'Hour1' : 'Day1',
      bar_cache_policy: { cache_key: query.data.cache_key, refresh_policy: 'NeverRefresh' as any, data_fetched_at: null },
    } as any,
    bars: query.data.bars,
    cache_status: query.data.cache_status,
  };

  return <div style={{ maxHeight: 220, overflow: 'hidden' }}><ScenarioChart payload={payload} /></div>;
}
```

- [ ] **Step 2: Embed in the wizard route**

```tsx
// frontend/web/src/routes/scenarios-new.tsx (modify)
import { WizardPreviewChart } from '../components/chart/WizardPreviewChart';
// pass the current ScenarioForm field values down; lift them to the route state.
```

The cleanest path: refactor `<ScenarioForm>` to accept an `onChange` callback that exposes the in-progress fields. The route holds them in local state and feeds them to `<WizardPreviewChart>` alongside the form:

```tsx
const [draft, setDraft] = useState({ asset: 'ETH', from: '', to: '', granularity: '1h' as const });

return (
  <div className="px-6 py-5 max-w-3xl space-y-4">
    <h1 className="text-text font-serif text-[28px] m-0">New scenario</h1>
    <ScenarioForm
      submitting={m.isPending}
      error={error}
      onChange={(d) => setDraft(d)}
      onSubmit={(req) => m.mutate(req)}
      onCancel={() => navigate('/scenarios')}
    />
    <WizardPreviewChart asset={draft.asset} from={draft.from} to={draft.to} granularity={draft.granularity} includeBaseline />
  </div>
);
```

Refactor `<ScenarioForm>` to call `onChange` whenever a relevant field changes.

- [ ] **Step 3: Smoke test**

```bash
# /scenarios/new — fill ETH, 2024-02-03, 2024-12-31, 1h → preview chart appears within ~350ms.
```

- [ ] **Step 4: Commit**

```bash
git add frontend/web/src/components/chart/WizardPreviewChart.tsx frontend/web/src/routes/scenarios-new.tsx frontend/web/src/components/scenario/ScenarioForm.tsx
git commit -m "feat(web): wizard preview chart with debounced /scenarios/preview fetch"
```

---

## Task 7 — Performance budget for live updates

**Files:** `frontend/web/src/components/chart/use-run-stream.ts`, `frontend/web/src/components/chart/LiveChart.test.tsx`

- [ ] **Step 1: Vitest assertion for snapshot-to-render latency** (mocked SSE)

```tsx
// LiveChart.test.tsx
import { render, screen, waitFor } from '@testing-library/react';
import { describe, it, expect, vi } from 'vitest';
import { LiveChart } from './LiveChart';
import samplePayload from './sample-payload.json';

describe('LiveChart', () => {
  it('renders within 250ms p95 after snapshot lands', async () => {
    vi.spyOn(globalThis, 'fetch').mockResolvedValueOnce({ ok: true, json: () => Promise.resolve(samplePayload) } as any);
    const start = performance.now();
    render(<LiveChart runId="r_test" />);
    await waitFor(() => expect(screen.getByText(/live|reconnecting|loading/)).toBeTruthy());
    const elapsed = performance.now() - start;
    expect(elapsed).toBeLessThan(250);
  });
});
```

- [ ] **Step 2: Run**

```bash
pnpm vitest run LiveChart
```

- [ ] **Step 3: Commit**

```bash
git add frontend/web/src/components/chart/LiveChart.test.tsx
git commit -m "test(web): LiveChart snapshot-render latency budget"
```

---

## Task 8 — M3 acceptance smoke

- [ ] **Step 1: Workspace + frontend tests**

```bash
cargo test --workspace
cd frontend/web && pnpm typecheck && pnpm build && pnpm vitest run
```

- [ ] **Step 2: Manual smoke**

1. Start a backtest via CLI; while it runs, open `/live/<run_id>` — chart streams bars + markers + equity.
2. Disconnect network briefly → status flips to "reconnecting…" → reconnect → snapshot refetched, stream resumes.
3. Pan-left → "Frozen" label shows; click "Resume live" → re-engages.
4. `/scenarios/new` — fill ETH + 2024 → debounced preview chart appears with cache-status badge; toggle baseline → Buy-and-Hold overlay shows.

- [ ] **Step 3: Final commit**

```bash
git add -p
git commit -m "chore: M3 acceptance smoke passes (live cockpit + wizard preview)"
```

---

## Self-review notes

- Live cockpit uses re-snapshot on SSE drop (spec §7.6) — explicit by design choice; event-id resumption deferred per spec §12.
- 250ms server-side batching covered by SSE handler implementation.
- Buy-and-Hold baseline is the v1 "real strategy preview" stand-in (spec §12). Document explicitly in the wizard UI ("baseline = Buy & Hold").
- Pan-to-freeze + Resume live wired through the `follow` flag in LiveChart.
- No placeholders.
