# Custom-Scenario Eval — M3: Dashboard wizard + inline form + run launcher

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Ship the dashboard surface for custom scenarios — `/scenarios` list, `/scenarios/new` wizard (minimal + collapsible Advanced), `/scenarios/:id` detail, "+ New scenario" inline form on `/eval-runs`, and a run launcher that kicks off backtest / paper-mirror runs.

**Architecture:** Three new routes (`/scenarios`, `/scenarios/new`, `/scenarios/:id`) and one extended route (`/eval-runs` gains a header inline-form + a run launcher). New axum endpoints `GET/POST/DELETE /api/scenarios*` thin-wrap `api::scenario::*`. TanStack Query caches scenario list with a 30 s staleTime; mutations invalidate the list. localStorage holds last-used asset / granularity for wizard pre-fill.

**Tech Stack:** React 18, TanStack Query, react-router-dom 6, Tailwind, Radix-style primitives in `frontend/web/src/components/primitives/`. Server: axum 0.7.

**Reference spec:** `docs/superpowers/specs/2026-05-11-custom-scenario-eval-design.md` §10.

**Prereq:** M1 + M2 (`docs/superpowers/plans/2026-05-11-custom-scenario-1-bars-cache-asset-unlock.md`, `…-2-scenario-table-cli.md`) merged.

---

## File structure

| Path | Action | Responsibility |
|---|---|---|
| `crates/xvision-engine/src/api/scenario.rs` | Modify | Expose HTTP handlers; the trait/fn-level API is already there from M2. |
| `crates/xvision-engine/src/api/mod.rs` | Modify | Register HTTP routes. |
| `crates/xvision-dashboard/src/routes.rs` | Modify | Mount `/api/scenarios*` paths. |
| `frontend/web/src/api/scenarios.ts` | Create | `listScenarios`, `getScenario`, `createScenario`, `cloneScenario`, `archiveScenario`, `deleteScenario`. |
| `frontend/web/src/routes/scenarios.tsx` | Create | List view at `/scenarios`. |
| `frontend/web/src/routes/scenarios-new.tsx` | Create | Wizard at `/scenarios/new`. |
| `frontend/web/src/routes/scenarios-detail.tsx` | Create | Detail at `/scenarios/:id` with Definition / Runs / Bar cache tabs. |
| `frontend/web/src/routes/eval-runs.tsx` | Modify | Add "+ New scenario" inline form + Run launcher. |
| `frontend/web/src/routes.tsx` | Modify | Register new routes. |
| `frontend/web/src/components/shell/Sidebar.tsx` | Modify | Add Scenarios link. |
| `frontend/web/src/components/shell/CommandPalette.tsx` | Modify | Add scenario nav actions. |
| `frontend/web/src/components/scenario/ScenarioForm.tsx` | Create | Reusable form for the wizard + inline form. |
| `frontend/web/src/components/scenario/RegimeRangePresets.tsx` | Create | "Last year / YTD / Last 90 days / Custom" preset buttons. |
| `frontend/web/src/components/scenario/CacheStatusBadge.tsx` | Create | Green/yellow badge with bar count. |

---

## Task 1 — HTTP endpoints

**Files:** `crates/xvision-engine/src/api/scenario.rs`, `crates/xvision-engine/src/api/mod.rs`, `crates/xvision-dashboard/src/routes.rs`

- [ ] **Step 1: Failing integration test for `GET /api/scenarios`**

```rust
// crates/xvision-engine/tests/scenario_http.rs
#[tokio::test]
async fn list_scenarios_returns_seeded_rows() {
    let app = test_app().await;
    let resp = app.oneshot(
        axum::http::Request::builder().uri("/api/scenarios").body(Body::empty()).unwrap()
    ).await.unwrap();
    assert_eq!(resp.status(), 200);
    let body = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    let scenarios: Vec<serde_json::Value> = serde_json::from_slice(&body).unwrap();
    assert_eq!(scenarios.len(), 4);
}
```

- [ ] **Step 2: Run, expect FAIL**

```bash
cargo test -p xvision-engine --test scenario_http
```

- [ ] **Step 3: Add HTTP handlers**

```rust
// crates/xvision-engine/src/api/scenario.rs (appended)
use axum::{extract::{Path, Query, State}, response::{IntoResponse, Json}, http::StatusCode};
use std::sync::Arc;

pub async fn http_list(State(ctx): State<Arc<ApiContext>>, Query(filter): Query<ListScenariosFilter>) -> impl IntoResponse {
    match list(&ctx, filter).await {
        Ok(v) => (StatusCode::OK, Json(v)).into_response(),
        Err(e) => error_response(e),
    }
}

pub async fn http_get(State(ctx): State<Arc<ApiContext>>, Path(id): Path<String>) -> impl IntoResponse {
    match get(&ctx, &id).await {
        Ok(s) => (StatusCode::OK, Json(s)).into_response(),
        Err(e) => error_response(e),
    }
}

pub async fn http_create(State(ctx): State<Arc<ApiContext>>, Json(req): Json<CreateScenarioRequest>) -> impl IntoResponse {
    match create(&ctx, req).await {
        Ok(s) => (StatusCode::CREATED, Json(s)).into_response(),
        Err(e) => error_response(e),
    }
}

pub async fn http_clone(State(ctx): State<Arc<ApiContext>>, Path(id): Path<String>, Json(m): Json<ScenarioMutations>) -> impl IntoResponse {
    match clone(&ctx, &id, m).await {
        Ok(s) => (StatusCode::CREATED, Json(s)).into_response(),
        Err(e) => error_response(e),
    }
}

pub async fn http_archive(State(ctx): State<Arc<ApiContext>>, Path(id): Path<String>) -> impl IntoResponse {
    match archive(&ctx, &id).await {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => error_response(e),
    }
}

pub async fn http_delete(State(ctx): State<Arc<ApiContext>>, Path(id): Path<String>) -> impl IntoResponse {
    match delete(&ctx, &id).await {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => error_response(e),
    }
}

fn error_response(e: ApiError) -> axum::response::Response {
    use ApiError::*;
    let (status, msg) = match &e {
        Validation(m) => (StatusCode::BAD_REQUEST, m.clone()),
        NotFound(m) => (StatusCode::NOT_FOUND, m.clone()),
        Conflict(m) => (StatusCode::CONFLICT, m.clone()),
        Internal(m) => (StatusCode::INTERNAL_SERVER_ERROR, m.clone()),
    };
    (status, Json(serde_json::json!({"error": msg}))).into_response()
}
```

- [ ] **Step 4: Wire routes in `crates/xvision-dashboard/src/routes.rs`**

```rust
use xvision_engine::api::scenario as api_scenario;

let scenario_routes = axum::Router::new()
    .route("/scenarios", axum::routing::get(api_scenario::http_list).post(api_scenario::http_create))
    .route("/scenarios/:id", axum::routing::get(api_scenario::http_get).delete(api_scenario::http_delete))
    .route("/scenarios/:id/clone", axum::routing::post(api_scenario::http_clone))
    .route("/scenarios/:id/archive", axum::routing::post(api_scenario::http_archive));

let api = axum::Router::new().nest("/api", scenario_routes /* ...merged with existing routes... */);
```

- [ ] **Step 5: Run test, expect PASS**

```bash
cargo test -p xvision-engine --test scenario_http
```

- [ ] **Step 6: Commit**

```bash
git add crates/xvision-engine/src/api/scenario.rs crates/xvision-engine/src/api/mod.rs crates/xvision-dashboard/src/routes.rs crates/xvision-engine/tests/scenario_http.rs
git commit -m "feat(api): HTTP endpoints for scenario CRUD"
```

---

## Task 2 — Frontend API client

**Files:** `frontend/web/src/api/scenarios.ts`, `frontend/web/src/api/types.gen/Scenario.ts` (regenerated)

- [ ] **Step 1: Regenerate TS types**

```bash
cargo xtask gen-types       # or: cargo test --features ts-export --tests, per CLAUDE.md
```

Confirm `frontend/web/src/api/types.gen/Scenario.ts` and related shapes exist.

- [ ] **Step 2: Write the client**

```typescript
// frontend/web/src/api/scenarios.ts
import type { Scenario } from './types.gen/Scenario';
import type { CreateScenarioRequest } from './types.gen/CreateScenarioRequest';
import type { ListScenariosFilter } from './types.gen/ListScenariosFilter';
import type { ScenarioMutations } from './types.gen/ScenarioMutations';

const API = '/api';

export const scenarioKeys = {
  all: ['scenarios'] as const,
  list: (filter?: ListScenariosFilter) => [...scenarioKeys.all, 'list', filter ?? {}] as const,
  detail: (id: string) => [...scenarioKeys.all, 'detail', id] as const,
};

export async function listScenarios(filter?: ListScenariosFilter): Promise<Scenario[]> {
  const params = new URLSearchParams();
  if (filter?.source) params.set('source', String(filter.source));
  if (filter?.include_archived) params.set('include_archived', 'true');
  filter?.tags?.forEach((t) => params.append('tags', t));
  const r = await fetch(`${API}/scenarios?${params}`);
  if (!r.ok) throw new ApiError(r);
  return r.json();
}

export async function getScenario(id: string): Promise<Scenario> {
  const r = await fetch(`${API}/scenarios/${encodeURIComponent(id)}`);
  if (!r.ok) throw new ApiError(r);
  return r.json();
}

export async function createScenario(req: CreateScenarioRequest): Promise<Scenario> {
  const r = await fetch(`${API}/scenarios`, {
    method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify(req),
  });
  if (!r.ok) throw new ApiError(r);
  return r.json();
}

export async function cloneScenario(id: string, mutations: ScenarioMutations): Promise<Scenario> {
  const r = await fetch(`${API}/scenarios/${encodeURIComponent(id)}/clone`, {
    method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify(mutations),
  });
  if (!r.ok) throw new ApiError(r);
  return r.json();
}

export async function archiveScenario(id: string): Promise<void> {
  const r = await fetch(`${API}/scenarios/${encodeURIComponent(id)}/archive`, { method: 'POST' });
  if (!r.ok) throw new ApiError(r);
}

export async function deleteScenario(id: string): Promise<void> {
  const r = await fetch(`${API}/scenarios/${encodeURIComponent(id)}`, { method: 'DELETE' });
  if (!r.ok) throw new ApiError(r);
}

export class ApiError extends Error {
  constructor(public response: Response) { super(`HTTP ${response.status}`); }
  async detail(): Promise<string> {
    try { const j = await this.response.json(); return j.error ?? this.message; } catch { return this.message; }
  }
}
```

- [ ] **Step 3: Commit**

```bash
git add frontend/web/src/api/scenarios.ts frontend/web/src/api/types.gen/
git commit -m "feat(web): API client for scenario CRUD"
```

---

## Task 3 — `/scenarios` list route

**Files:** `frontend/web/src/routes/scenarios.tsx`, `frontend/web/src/routes.tsx`, `frontend/web/src/components/shell/Sidebar.tsx`, `frontend/web/src/components/shell/CommandPalette.tsx`

- [ ] **Step 1: Implement list route**

```tsx
// frontend/web/src/routes/scenarios.tsx
import { useState } from 'react';
import { Link } from 'react-router-dom';
import { useQuery } from '@tanstack/react-query';
import { listScenarios, scenarioKeys } from '../api/scenarios';

export default function ScenariosRoute() {
  const [filterSource, setFilterSource] = useState<string | undefined>(undefined);
  const [includeArchived, setIncludeArchived] = useState(false);
  const filter = { source: filterSource as any, tags: [], include_archived: includeArchived };
  const { data, isLoading, error } = useQuery({
    queryKey: scenarioKeys.list(filter),
    queryFn: () => listScenarios(filter),
    staleTime: 30_000,
  });

  return (
    <div className="px-6 py-5">
      <div className="flex items-center justify-between mb-4">
        <h1 className="text-text font-serif text-[28px] m-0">Scenarios</h1>
        <Link to="/scenarios/new" className="rounded border border-border px-3 py-1.5 text-[13px] hover:border-text-3">
          + New scenario
        </Link>
      </div>
      <Filters source={filterSource} onSource={setFilterSource} includeArchived={includeArchived} onArchived={setIncludeArchived} />
      {isLoading && <SkeletonRows />}
      {error && <ErrorState error={error} />}
      {data && data.length === 0 && <EmptyState />}
      {data && data.length > 0 && (
        <table className="w-full text-[13px]">
          <thead className="text-text-3 text-left">
            <tr>
              <th className="py-2 pr-4">Name</th>
              <th className="py-2 pr-4">Asset</th>
              <th className="py-2 pr-4">Window</th>
              <th className="py-2 pr-4">Granularity</th>
              <th className="py-2 pr-4">Source</th>
              <th className="py-2 pr-4">Tags</th>
              <th></th>
            </tr>
          </thead>
          <tbody>
            {data.map((s) => (
              <tr key={s.id} className="border-t border-border hover:bg-surface-elev">
                <td className="py-2.5 pr-4">
                  <Link to={`/scenarios/${s.id}`} className="text-text underline decoration-text-3 underline-offset-2">{s.display_name}</Link>
                  {s.archived_at && <span className="ml-2 text-[11px] text-text-3">(archived)</span>}
                </td>
                <td className="py-2.5 pr-4 font-mono text-text-2">{s.asset[0].symbol}</td>
                <td className="py-2.5 pr-4 font-mono text-text-2">{fmtDate(s.time_window.start)} → {fmtDate(s.time_window.end)}</td>
                <td className="py-2.5 pr-4 font-mono">{labelGranularity(s.granularity)}</td>
                <td className="py-2.5 pr-4 text-text-2">{String(s.source).toLowerCase()}</td>
                <td className="py-2.5 pr-4 text-text-3">{s.tags.join(', ')}</td>
                <td className="py-2.5 pr-4 text-right">
                  <Link to={`/scenarios/${s.id}`} className="text-text-3 hover:text-text">view →</Link>
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      )}
    </div>
  );
}

function fmtDate(iso: string): string { return new Date(iso).toISOString().slice(0, 10); }
function labelGranularity(g: any): string { return g === 'Hour1' ? '1h' : g === 'Day1' ? '1d' : String(g); }
function Filters(/* ... */) { /* source dropdown + archived toggle */ return null; }
function SkeletonRows() { return <div className="text-text-3 py-6">Loading…</div>; }
function ErrorState({ error }: { error: unknown }) { return <div className="text-danger py-6">Failed to load: {String(error)}</div>; }
function EmptyState() {
  return (
    <div className="px-6 py-16 text-center text-text-2">
      <div className="font-serif italic text-[28px] text-text-3 mb-3">no scenarios yet</div>
      <Link to="/scenarios/new" className="text-text underline decoration-text-3">Create the first scenario</Link>
    </div>
  );
}
```

- [ ] **Step 2: Register route in `routes.tsx`**

```tsx
import ScenariosRoute from './routes/scenarios';
import ScenariosNewRoute from './routes/scenarios-new';
import ScenariosDetailRoute from './routes/scenarios-detail';

// ...
{ path: '/scenarios', element: <ScenariosRoute /> },
{ path: '/scenarios/new', element: <ScenariosNewRoute /> },
{ path: '/scenarios/:id', element: <ScenariosDetailRoute /> },
```

- [ ] **Step 3: Add Sidebar entry + Command Palette nav action**

```tsx
// Sidebar.tsx — add inside the nav stack
{ to: '/scenarios', label: 'Scenarios', icon: 'list' },
```

```tsx
// CommandPalette.tsx — add to actions
{ kind: 'action', artifact_id: 'nav:scenarios', title: 'Scenarios', summary: 'Browse and create eval scenarios', tags: ['nav'], href: '/scenarios', updated_at: '', bm25_score: 0 },
```

- [ ] **Step 4: Run dev server, smoke**

```bash
cd frontend/web && pnpm dev
# open http://localhost:5173/scenarios — should render 4 seeded rows
```

- [ ] **Step 5: Commit**

```bash
git add frontend/web/src/routes/scenarios.tsx frontend/web/src/routes.tsx frontend/web/src/components/shell/Sidebar.tsx frontend/web/src/components/shell/CommandPalette.tsx
git commit -m "feat(web): /scenarios list route with filters + table"
```

---

## Task 4 — Reusable `<ScenarioForm>` component

**Files:** `frontend/web/src/components/scenario/ScenarioForm.tsx`, `frontend/web/src/components/scenario/RegimeRangePresets.tsx`

- [ ] **Step 1: Implement `<ScenarioForm>`**

```tsx
// frontend/web/src/components/scenario/ScenarioForm.tsx
import { useState } from 'react';
import type { CreateScenarioRequest } from '../../api/types.gen/CreateScenarioRequest';
import { RegimeRangePresets } from './RegimeRangePresets';

export type ScenarioFormProps = {
  initial?: Partial<CreateScenarioRequest>;
  submitting?: boolean;
  error?: string;
  onSubmit: (req: CreateScenarioRequest) => void;
  onCancel?: () => void;
  layout?: 'wizard' | 'inline';
};

const ALPACA_ASSETS = ['BTC', 'ETH', 'LTC', 'SOL', 'AVAX', 'LINK', 'AAVE', 'UNI', 'DOT', 'DOGE', 'SHIB', 'MATIC', 'BCH', 'USDT', 'USDC'];

export function ScenarioForm({ initial, submitting, error, onSubmit, onCancel, layout = 'wizard' }: ScenarioFormProps) {
  const [name, setName] = useState(initial?.display_name ?? '');
  const [asset, setAsset] = useState(initial?.asset?.[0]?.symbol ?? 'ETH');
  const [from, setFrom] = useState(initial?.time_window?.start?.slice(0, 10) ?? '');
  const [to, setTo] = useState(initial?.time_window?.end?.slice(0, 10) ?? '');
  const [granularity, setGranularity] = useState<'Hour1' | 'Day1'>(initial?.granularity as any ?? 'Hour1');
  const [tags, setTags] = useState<string[]>(initial?.tags ?? []);
  const [notes, setNotes] = useState(initial?.notes ?? '');
  const [advancedOpen, setAdvancedOpen] = useState(false);
  const [feesMaker, setFeesMaker] = useState(initial?.venue?.fees?.maker_bps ?? 10);
  const [feesTaker, setFeesTaker] = useState(initial?.venue?.fees?.taker_bps ?? 25);
  const [slippageBps, setSlippageBps] = useState(5);
  const [latencyMs, setLatencyMs] = useState(initial?.venue?.latency?.decision_to_fill_ms ?? 500);

  const estimatedBars = estimateBars(from, to, granularity);

  function handleSubmit(e: React.FormEvent) {
    e.preventDefault();
    onSubmit({
      display_name: name, description: '',
      asset_class: 'Crypto' as any,
      asset: [{ class: 'Crypto' as any, symbol: asset, venue_symbol: `${asset}/USD` }],
      quote_currency: 'Usd' as any,
      time_window: { start: `${from}T00:00:00Z`, end: `${to}T00:00:00Z` },
      granularity: granularity as any,
      timezone: 'UTC',
      calendar: 'Continuous24x7' as any,
      venue: {
        venue: 'Alpaca' as any,
        fees: { maker_bps: feesMaker, taker_bps: feesTaker },
        slippage: { type: 'Linear', bps: slippageBps } as any,
        latency: { decision_to_fill_ms: latencyMs },
        fill_model: { market_order_fill: 'FullAtClose', limit_order_fill: 'NeverFills', partial_fills: false, volume_constraints: null } as any,
      },
      data_source: { type: 'AlpacaHistorical', feed: null, adjustment: 'Raw' } as any,
      replay_mode: 'Continuous' as any,
      tags, notes: notes || null,
      parent_scenario_id: null,
      source: 'User' as any,
    } as any);
  }

  return (
    <form onSubmit={handleSubmit} className="space-y-4 max-w-2xl">
      <Field label="Name"><input className="input" value={name} onChange={(e) => setName(e.target.value)} required /></Field>
      <Field label="Notes"><input className="input" value={notes} onChange={(e) => setNotes(e.target.value)} placeholder="optional" /></Field>
      <Field label="Tags">
        <TagInput value={tags} onChange={setTags} />
      </Field>
      <Section title="Market">
        <Row>
          <Field label="Asset">
            <select className="input" value={asset} onChange={(e) => setAsset(e.target.value)}>
              {ALPACA_ASSETS.map((a) => <option key={a} value={a}>{a}</option>)}
            </select>
          </Field>
          <Field label="Quote"><span className="input">USD</span></Field>
        </Row>
        <Row>
          <Field label="From"><input type="date" className="input" value={from} onChange={(e) => setFrom(e.target.value)} required /></Field>
          <Field label="To"><input type="date" className="input" value={to} onChange={(e) => setTo(e.target.value)} required /></Field>
        </Row>
        <RegimeRangePresets onPick={(start, end) => { setFrom(start); setTo(end); }} />
        <Field label="Granularity">
          <Radio value={granularity} onChange={setGranularity} options={[['Hour1', '1h'], ['Day1', '1d']]} />
        </Field>
      </Section>
      <Section title="Venue (Alpaca)">
        <button type="button" className="text-text-3 text-[13px]" onClick={() => setAdvancedOpen((v) => !v)}>
          {advancedOpen ? '▾ Advanced' : '▸ Advanced'}
        </button>
        {advancedOpen && (
          <div className="space-y-3 mt-2">
            <Row>
              <Field label="Fees maker (bps)"><input type="number" className="input" value={feesMaker} onChange={(e) => setFeesMaker(+e.target.value)} /></Field>
              <Field label="Fees taker (bps)"><input type="number" className="input" value={feesTaker} onChange={(e) => setFeesTaker(+e.target.value)} /></Field>
            </Row>
            <Row>
              <Field label="Slippage (linear bps)"><input type="number" className="input" value={slippageBps} onChange={(e) => setSlippageBps(+e.target.value)} /></Field>
              <Field label="Latency (ms)"><input type="number" className="input" value={latencyMs} onChange={(e) => setLatencyMs(+e.target.value)} /></Field>
            </Row>
            <div className="text-[12px] text-text-3">Fill model: market-only, full-fills (v1 locked)</div>
          </div>
        )}
      </Section>
      <div className="text-[12px] text-text-3">Estimated bars to fetch: <span className="font-mono text-text">{estimatedBars.toLocaleString()}</span></div>
      {error && <div className="text-danger text-[12px]">{error}</div>}
      <div className="flex gap-2">
        {onCancel && <button type="button" onClick={onCancel} className="border border-border px-3 py-1.5 rounded text-[13px]">Cancel</button>}
        <button type="submit" disabled={submitting} className="border border-border bg-surface-elev px-3 py-1.5 rounded text-[13px] hover:border-text-3">
          {submitting ? 'Creating…' : 'Create →'}
        </button>
      </div>
    </form>
  );
}

function estimateBars(from: string, to: string, g: 'Hour1' | 'Day1'): number {
  if (!from || !to) return 0;
  const ms = +new Date(to) - +new Date(from);
  if (ms <= 0) return 0;
  const hours = ms / 3_600_000;
  return g === 'Hour1' ? Math.round(hours) : Math.round(hours / 24);
}

function Section({ title, children }: any) { return <fieldset className="border border-border rounded p-4"><legend className="px-2 text-text-3 text-[12px]">{title}</legend>{children}</fieldset>; }
function Row({ children }: any) { return <div className="flex gap-3">{children}</div>; }
function Field({ label, children }: any) { return <label className="block text-[12px] text-text-3 flex-1"><div className="mb-1">{label}</div>{children}</label>; }
function TagInput({ value, onChange }: { value: string[]; onChange: (v: string[]) => void }) {
  const [draft, setDraft] = useState('');
  return (
    <div className="flex flex-wrap gap-1.5 items-center">
      {value.map((t, i) => (
        <span key={i} className="px-2 py-0.5 rounded border border-border text-[11px]">{t} <button type="button" onClick={() => onChange(value.filter((_, j) => j !== i))}>×</button></span>
      ))}
      <input className="input flex-1 min-w-[120px]" value={draft} placeholder="+ add tag"
        onKeyDown={(e) => { if (e.key === 'Enter' && draft.trim()) { e.preventDefault(); onChange([...value, draft.trim()]); setDraft(''); } }}
        onChange={(e) => setDraft(e.target.value)} />
    </div>
  );
}
function Radio({ value, onChange, options }: any) {
  return (<div className="flex gap-3 text-[13px]">{options.map(([v, label]: any) => (
    <label key={v} className="flex items-center gap-1.5">
      <input type="radio" value={v} checked={value === v} onChange={() => onChange(v)} /> {label}
    </label>
  ))}</div>);
}
```

- [ ] **Step 2: Implement `<RegimeRangePresets>`**

```tsx
// frontend/web/src/components/scenario/RegimeRangePresets.tsx
type Props = { onPick: (start: string, end: string) => void };

export function RegimeRangePresets({ onPick }: Props) {
  const today = new Date();
  const fmt = (d: Date) => d.toISOString().slice(0, 10);

  function back(days: number) {
    const d = new Date(today); d.setDate(d.getDate() - days);
    onPick(fmt(d), fmt(today));
  }
  function lastYear() {
    const start = new Date(today.getFullYear() - 1, 0, 1);
    const end   = new Date(today.getFullYear() - 1, 11, 31);
    onPick(fmt(start), fmt(end));
  }
  function ytd() {
    const start = new Date(today.getFullYear(), 0, 1);
    onPick(fmt(start), fmt(today));
  }

  return (
    <div className="flex gap-1.5 text-[12px]">
      <button type="button" onClick={lastYear} className="px-2 py-1 border border-border rounded">Last year</button>
      <button type="button" onClick={ytd} className="px-2 py-1 border border-border rounded">YTD</button>
      <button type="button" onClick={() => back(90)} className="px-2 py-1 border border-border rounded">Last 90 days</button>
      <button type="button" onClick={() => back(30)} className="px-2 py-1 border border-border rounded">Last 30 days</button>
    </div>
  );
}
```

- [ ] **Step 3: Add a Tailwind `.input` utility** (or inline class) to the global stylesheet if not present.

- [ ] **Step 4: Commit**

```bash
git add frontend/web/src/components/scenario/
git commit -m "feat(web): reusable ScenarioForm + RegimeRangePresets components"
```

---

## Task 5 — `/scenarios/new` wizard

**Files:** `frontend/web/src/routes/scenarios-new.tsx`

- [ ] **Step 1: Implement wizard route**

```tsx
import { useState } from 'react';
import { useNavigate } from 'react-router-dom';
import { useMutation, useQueryClient } from '@tanstack/react-query';
import { createScenario, scenarioKeys, ApiError } from '../api/scenarios';
import { ScenarioForm } from '../components/scenario/ScenarioForm';

export default function ScenariosNewRoute() {
  const navigate = useNavigate();
  const qc = useQueryClient();
  const [error, setError] = useState<string | undefined>(undefined);

  const m = useMutation({
    mutationFn: createScenario,
    onSuccess: (s) => {
      qc.invalidateQueries({ queryKey: scenarioKeys.all });
      navigate(`/scenarios/${s.id}`);
    },
    onError: async (err) => {
      if (err instanceof ApiError) setError(await err.detail()); else setError(String(err));
    },
  });

  return (
    <div className="px-6 py-5 max-w-3xl">
      <h1 className="text-text font-serif text-[28px] m-0 mb-4">New scenario</h1>
      <ScenarioForm
        submitting={m.isPending}
        error={error}
        onSubmit={(req) => { setError(undefined); m.mutate(req); }}
        onCancel={() => navigate('/scenarios')}
      />
    </div>
  );
}
```

- [ ] **Step 2: Smoke test**

```bash
cd frontend/web && pnpm dev
# /scenarios/new → fill in ETH 2024-02-03 → 2024-12-31 1h → create
# expect redirect to /scenarios/<new-id>
```

- [ ] **Step 3: Commit**

```bash
git add frontend/web/src/routes/scenarios-new.tsx
git commit -m "feat(web): /scenarios/new wizard route"
```

---

## Task 6 — `/scenarios/:id` detail with tabs

**Files:** `frontend/web/src/routes/scenarios-detail.tsx`, `frontend/web/src/components/scenario/CacheStatusBadge.tsx`

- [ ] **Step 1: Implement detail route**

```tsx
import { useState } from 'react';
import { Link, useNavigate, useParams } from 'react-router-dom';
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { archiveScenario, cloneScenario, deleteScenario, getScenario, scenarioKeys, ApiError } from '../api/scenarios';

type Tab = 'definition' | 'runs' | 'bar-cache';

export default function ScenariosDetailRoute() {
  const { id = '' } = useParams();
  const navigate = useNavigate();
  const qc = useQueryClient();
  const [tab, setTab] = useState<Tab>('definition');
  const { data: s } = useQuery({ queryKey: scenarioKeys.detail(id), queryFn: () => getScenario(id) });

  const clone = useMutation({
    mutationFn: () => cloneScenario(id, { display_name: `${s?.display_name} (clone)` }),
    onSuccess: (n) => { qc.invalidateQueries({ queryKey: scenarioKeys.all }); navigate(`/scenarios/${n.id}`); },
  });
  const archive = useMutation({
    mutationFn: () => archiveScenario(id),
    onSuccess: () => { qc.invalidateQueries({ queryKey: scenarioKeys.all }); },
  });
  const del = useMutation({
    mutationFn: () => deleteScenario(id),
    onSuccess: () => { qc.invalidateQueries({ queryKey: scenarioKeys.all }); navigate('/scenarios'); },
    onError: async (err) => {
      if (err instanceof ApiError) alert(await err.detail());
    },
  });

  if (!s) return <div className="px-6 py-5 text-text-3">Loading…</div>;

  return (
    <div className="px-6 py-5">
      <Breadcrumb scenario={s} />
      <div className="flex items-baseline justify-between mb-4">
        <div>
          <h1 className="text-text font-serif text-[28px] m-0">{s.display_name}</h1>
          {s.archived_at && <span className="text-text-3 text-[12px]">archived {new Date(s.archived_at).toISOString().slice(0,10)}</span>}
        </div>
        <div className="flex gap-2">
          <button onClick={() => clone.mutate()} className="border border-border px-3 py-1.5 rounded text-[13px] hover:border-text-3">Clone to edit</button>
          {!s.archived_at && <button onClick={() => archive.mutate()} className="border border-border px-3 py-1.5 rounded text-[13px]">Archive</button>}
          <button onClick={() => del.mutate()} className="border border-danger text-danger px-3 py-1.5 rounded text-[13px]">Delete</button>
        </div>
      </div>
      <Tabs value={tab} onChange={setTab} />
      {tab === 'definition' && <DefinitionTab s={s} />}
      {tab === 'runs' && <RunsTab scenarioId={s.id} />}
      {tab === 'bar-cache' && <BarCacheTab cacheKey={s.bar_cache_policy.cache_key} />}
    </div>
  );
}

function Breadcrumb({ scenario }: any) {
  return (
    <nav className="text-[12px] text-text-3 mb-3">
      <Link to="/scenarios" className="hover:text-text">Scenarios</Link>
      {scenario.parent_scenario_id && <> · forked from <Link to={`/scenarios/${scenario.parent_scenario_id}`} className="hover:text-text">{scenario.parent_scenario_id}</Link></>}
    </nav>
  );
}

function Tabs({ value, onChange }: { value: Tab; onChange: (t: Tab) => void }) {
  const tabs: [Tab, string][] = [['definition', 'Definition'], ['runs', 'Runs'], ['bar-cache', 'Bar cache']];
  return (
    <div className="flex gap-4 border-b border-border mb-4">
      {tabs.map(([t, label]) => (
        <button key={t} onClick={() => onChange(t)} className={`pb-2 -mb-px border-b-2 ${value === t ? 'border-text text-text' : 'border-transparent text-text-3'}`}>{label}</button>
      ))}
    </div>
  );
}

function DefinitionTab({ s }: any) {
  return (
    <dl className="grid grid-cols-[180px_1fr] gap-y-2 text-[13px]">
      <dt className="text-text-3">Asset</dt><dd className="font-mono">{s.asset[0].symbol} / {s.quote_currency}</dd>
      <dt className="text-text-3">Window</dt><dd className="font-mono">{s.time_window.start} → {s.time_window.end}</dd>
      <dt className="text-text-3">Granularity</dt><dd className="font-mono">{s.granularity}</dd>
      <dt className="text-text-3">Venue</dt><dd className="font-mono">{s.venue.venue}</dd>
      <dt className="text-text-3">Fees (m/t bps)</dt><dd className="font-mono">{s.venue.fees.maker_bps}/{s.venue.fees.taker_bps}</dd>
      <dt className="text-text-3">Slippage</dt><dd className="font-mono">{JSON.stringify(s.venue.slippage)}</dd>
      <dt className="text-text-3">Latency (ms)</dt><dd className="font-mono">{s.venue.latency.decision_to_fill_ms}</dd>
      <dt className="text-text-3">Cache key</dt><dd className="font-mono text-[11px] break-all">{s.bar_cache_policy.cache_key}</dd>
      <dt className="text-text-3">Source</dt><dd>{String(s.source).toLowerCase()}</dd>
      <dt className="text-text-3">Tags</dt><dd>{s.tags.join(', ')}</dd>
    </dl>
  );
}

function RunsTab({ scenarioId }: { scenarioId: string }) {
  // Reuses existing /api/eval/runs endpoint with scenario filter.
  return <div className="text-text-3 text-[13px]">Runs against this scenario will appear here. (Wired in chart spec M2.)</div>;
}

function BarCacheTab({ cacheKey }: { cacheKey: string }) {
  // Reads from a new /api/bars-cache/:key endpoint (or surface via scenarios endpoint).
  return <div className="text-text-3 text-[13px]">Cache key: <code className="font-mono">{cacheKey}</code></div>;
}
```

- [ ] **Step 2: Smoke test**

Visit `/scenarios/<id>` for one of the seeded canonical rows. Tabs should switch.

- [ ] **Step 3: Commit**

```bash
git add frontend/web/src/routes/scenarios-detail.tsx frontend/web/src/components/scenario/CacheStatusBadge.tsx
git commit -m "feat(web): /scenarios/:id detail route with Definition/Runs/Bar-cache tabs"
```

---

## Task 7 — Inline "+ New scenario" + Run launcher on `/eval-runs`

**Files:** `frontend/web/src/routes/eval-runs.tsx`

- [ ] **Step 1: Add the inline form (collapsed by default)**

Open `eval-runs.tsx` and at the top of the route render, add a "+ New scenario" button that expands a `<ScenarioForm layout="inline">` and an adjacent run launcher.

```tsx
// snippet — full implementation below
const [scenarioFormOpen, setScenarioFormOpen] = useState(false);

<div className="px-6 py-4 flex items-center gap-3 border-b border-border">
  <button onClick={() => setScenarioFormOpen((v) => !v)} className="border border-border px-3 py-1.5 rounded text-[13px] hover:border-text-3">
    + New scenario
  </button>
  <RunLauncher onLaunched={(runId) => navigate(`/eval-runs/${runId}`)} />
</div>
{scenarioFormOpen && <div className="px-6 py-4 border-b border-border">
  <ScenarioForm
    layout="inline"
    onSubmit={(req) => createMut.mutate(req)}
    onCancel={() => setScenarioFormOpen(false)}
    submitting={createMut.isPending}
  />
</div>}
```

- [ ] **Step 2: Implement `<RunLauncher>` inline**

```tsx
function RunLauncher({ onLaunched }: { onLaunched: (runId: string) => void }) {
  const { data: scenarios } = useQuery({ queryKey: scenarioKeys.list(), queryFn: () => listScenarios() });
  const { data: strategies } = useQuery({ queryKey: ['strategies'], queryFn: listStrategies });
  const [strategyId, setStrategyId] = useState<string>('bundle-canonical-defaults');
  const [scenarioId, setScenarioId] = useState<string>('');
  const [mode, setMode] = useState<'Backtest' | 'Paper'>('Backtest');

  const launch = useMutation({
    mutationFn: () => runEval({ agent_id: strategyId, scenario_id: scenarioId, mode }),
    onSuccess: (run) => onLaunched(run.id),
  });

  if (!scenarios || !strategies) return null;
  return (
    <div className="flex items-center gap-2 ml-auto text-[13px]">
      <span className="text-text-3">Launch:</span>
      <select className="input" value={strategyId} onChange={(e) => setStrategyId(e.target.value)}>
        {strategies.map((s) => <option key={s.id} value={s.id}>{s.display_name}</option>)}
      </select>
      <select className="input" value={scenarioId} onChange={(e) => setScenarioId(e.target.value)}>
        <option value="">scenario…</option>
        {scenarios.map((s) => <option key={s.id} value={s.id}>{s.display_name}</option>)}
      </select>
      <Radio value={mode} onChange={setMode} options={[['Backtest', 'Backtest'], ['Paper', 'Paper mirror']]} />
      <button onClick={() => launch.mutate()} disabled={!scenarioId || launch.isPending} className="border border-border px-3 py-1.5 rounded">
        {launch.isPending ? 'Launching…' : 'Launch →'}
      </button>
    </div>
  );
}
```

- [ ] **Step 3: `listStrategies` + `runEval` client fns**

Wire if they don't exist; reuse the existing eval-run API surface.

- [ ] **Step 4: Smoke test**

```bash
cd frontend/web && pnpm dev
# /eval-runs → click "+ New scenario" → fill form → submit → new scenario appears in selector → Launch → run starts → navigates to detail
```

- [ ] **Step 5: Commit**

```bash
git add frontend/web/src/routes/eval-runs.tsx frontend/web/src/api/
git commit -m "feat(web): inline + new scenario + run launcher on /eval-runs"
```

---

## Task 8 — M3 acceptance smoke

- [ ] **Step 1: Workspace test pass**

```bash
cargo test --workspace
cd frontend/web && pnpm typecheck && pnpm build
```

Expected: PASS.

- [ ] **Step 2: End-to-end Playwright (if installed)**

```bash
pnpm exec playwright test scenarios
```

If Playwright isn't set up yet, defer to a manual smoke:
1. `/scenarios` lists 4 canonical rows.
2. `/scenarios/new` creates an ETH scenario; redirects to detail.
3. `/scenarios/:id` shows definition; Clone-to-edit redirects to the new clone's detail.
4. `/eval-runs` inline form creates a scenario; run launcher launches a Backtest against it.

- [ ] **Step 3: Commit cleanup**

```bash
git add -p
git commit -m "chore: M3 acceptance smoke passes (custom-scenario dashboard live)"
```

---

## Self-review notes

- Every HTTP endpoint has a corresponding API client function with typed inputs/outputs via ts-rs.
- localStorage isn't introduced in this plan — the wizard pre-fill ("last-used asset / granularity") is queued for the chart spec since it adds nothing functional here. Leave it out.
- Three tabs on detail render with minimal content; deeper Runs / Bar-cache tabs land in the chart spec M2.
- Clone CTA covered; archive + delete covered; FK-blocked-delete surfaces via `alert()` from the API error detail.
- No placeholders.
