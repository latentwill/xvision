# CT5 — Live Deployment Summary contract (Epic s78 Wave 3)

**Status:** Draft for operator approval. Gated by §8.9 of `docs/superpowers/specs/2026-06-07-control-tower-dashboard-evaluation.md`.
**Owner:** Control Tower (CT) track, Epic s78.
**Consumers (downstream beads):** `n0k` (live/paper rows in ActiveTasksStrip), `awm` (Cancel-gate + 24h runaway warning), `8s4` (capital-risk strip).
**Hard constraint:** HONESTY MANDATE (§8.1 / §8.9). Every field is sourced from broker/execution state, never fabricated from `agent_runs` or eval summaries. Unsourceable values are omitted or `null` — never faked.
**Schema posture:** Operator will WIPE + REDEPLOY the DB (no users). All schema changes are DIRECT edits to the base schema / a fresh additive migration. No data-preserving / backfill logic.

---

## 1. Goal & scope

### 1.1 Goal
Define the single shared read contract — `LiveDeploymentSummary` — that exposes a live or paper trading deployment to the Control Tower dashboard, sourced *exclusively* from broker and execution-layer truth. This is the foundation that unblocks `n0k`, `awm`, and `8s4`. It is the "distinct resource from `agent_runs`" that §8.9 requires before any live-money UI may render.

### 1.2 What a "deployment" IS (no new entity)
A deployment is an `eval_runs` row with `mode = RunMode::Live`. There is no separate deployment table; `/api/live/deployments` is a **filtered, honesty-constrained projection** over `eval_runs WHERE mode='live'`, joined with execution-layer truth. This reuses the proven `Run`/`RunStore` lifecycle (status, pause, flatten, live_config) rather than inventing a parallel store. The endpoint name (`/api/live/deployments`) and DTO name (`LiveDeploymentSummary`) deliberately differ from `agent_runs`/`RunSummary` so the dashboard never *infers* live status from a trace record — acceptance (c) of §8.9.

> **Terminology note.** "Paper" today = `RunMode::Live` against the Alpaca **paper** venue (real market data, simulated money). "Live" = `RunMode::Live` against a real-money venue. `VenueLabel::Live` (real money) is rejected at validation today, so in Wave 3 every deployment renders `mode: "paper"` in practice; the contract carries both values for forward-compatibility. The paper/live distinction is sourced from `live_config.venue_label`, NOT inferred.

### 1.3 In scope
- The `LiveDeploymentSummary` type (every field + provenance).
- `GET /api/live/deployments` (list, ~5s poll) + `GET /api/live/deployments/:id/stream` (per-deployment SSE).
- Per-broker data sourcing (Alpaca paper + Orderly), available-now vs gap.
- In-memory per-session peak-equity + day-start-baseline tracking (execution layer, NOT a persisted snapshot table).
- The minimal persisted-schema additions (`eval_runs.source` for `awm`; one persisted unrealized-PnL field for `n0k`) and what stays in-memory.
- ts-rs binding plan, safety-pause interaction, phased Wave-3→5 build plan, honesty checklist.

### 1.4 Out of scope
- Multi-venue aggregation / Alpaca-live (stubbed) — paper Alpaca + Orderly only.
- `xvn` CLI / MCP parity for deployments (future wave).
- The dashboard panels themselves (`n0k`/`awm`/`8s4` UI) beyond the consume contract.
- Win-rate / success-rate metrics (separate §7.4 decision, gated below n=10).

---

## 2. The `LiveDeploymentSummary` type

Defined in **`crates/xvision-engine/src/api/live_deployments.rs`** (engine, ts-rs-exported — see §7). Serde `rename_all = "snake_case"`. Every nullable field renders as `—` / "no data" in the UI, never a fabricated `0`.

```rust
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts-export",
    ts(export, export_to = "../../../frontend/web/src/api/types.gen/"))]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LiveDeploymentSummary {
    pub deployment_id: String,                  // = eval_runs.id (ULID)
    pub strategy_id: String,                    // = run.agent_id (strategy bundle hash)
    pub strategy_name: Option<String>,          // resolved display name; None if unresolved
    pub mode: DeploymentMode,                   // Paper | Live
    pub status: DeploymentStatus,               // Starting|Running|Paused|Stopped|Failed
    #[cfg_attr(feature = "ts-export", ts(type = "string"))]
    pub started_at: DateTime<Utc>,
    pub last_decision_at: Option<String>,       // RFC3339; None if no decision yet
    pub venue: String,                          // "alpaca-paper" | "orderly" | ...
    pub venue_connected: bool,                  // execution-layer reachability
    pub deployed_capital_usd: Option<f64>,
    pub realized_pnl_usd: Option<f64>,
    pub unrealized_pnl_usd: Option<f64>,
    pub drawdown_pct: Option<f64>,
    pub daily_loss_limit_remaining_usd: Option<f64>,
    pub risk_veto_count_since_last_visit: Option<u32>,
    pub paused: bool,                           // per-run pause (eval_runs.paused)
    pub flatten_requested: bool,                // eval_runs.flatten_requested
    pub global_safety_paused: bool,             // GET /api/safety/state.paused
    pub source: DeploymentSource,               // Human | Optimizer  (awm)
    pub unavailable_reason: Option<String>,     // connection-as-data, see §2.3
}
```

### 2.1 Per-field PROVENANCE (the honesty core)

| Field | Type | PROVENANCE (source of truth) |
|---|---|---|
| `deployment_id` | string | `eval_runs.id` (ULID). The run is the deployment. |
| `strategy_id` | string | `eval_runs.agent_id` (strategy bundle hash). Persisted at run start. |
| `strategy_name` | string \| null | Resolved from the Strategy artifact / `live_config.display_name`; `null` if unresolved. NOT fabricated. |
| `mode` | "paper" \| "live" | `live_config.venue_label` (execution config), NOT inferred from `agent_runs`. |
| `status` | enum | `eval_runs.status` (Queued→Starting, Running, Cancelled/Failed→Stopped/Failed) overlaid with `paused`. Execution lifecycle state. |
| `started_at` | string | `eval_runs.started_at`. Execution lifecycle. |
| `last_decision_at` | string \| null | `MAX(eval_decisions.timestamp) WHERE run_id=?` — a real recorded broker-fed decision, derived from execution truth. `null` if no decision recorded yet (NOT `started_at`, NOT faked). |
| `venue` | string | `live_config.broker_creds_ref` → resolved venue id ("alpaca-paper" / "orderly"). Execution config. |
| `venue_connected` | bool | Live reachability probe of the execution venue (Orderly `venue_snapshot()` success; Alpaca `get_account` success). `false` ⇒ capital fields go `null`. |
| `deployed_capital_usd` | number \| null | Sum of open-position notional from **broker/book state**: Alpaca Σ `AlpacaPosition.market_value`; Orderly Σ `\|position_qty\| * mark_price`; engine-book Σ over `PortfolioBook.open_legs()`. `null` when no live snapshot is available for this run. NOT `live_config.capital.initial` (that is the *launch envelope*, a config value, not deployed capital). |
| `realized_pnl_usd` | number \| null | Engine book: `PortfolioBook.realized()` for the in-flight run (sum of `eval_decisions.pnl_realized`). `null` if the run has no decision/fill history yet. NEVER the Alpaca `equity - last_equity` proxy (it conflates unrealized) and NEVER `0.0` as a stand-in (Orderly portfolio hardcodes `0.0` today — that is a stub and MUST surface as `null`, see §3/§4). |
| `unrealized_pnl_usd` | number \| null | Per-run mark-to-market: `PortfolioBook.equity(marks) - initial - realized`, persisted per run (see §6.3) and/or live from Orderly `OrderlyAccount.unrealized_pnl`. `null` when unavailable. Account-wide venue `unrealized_pnl` is NOT attributed to a single run when multiple runs share one account — must be per-run. |
| `drawdown_pct` | number \| null | `(peak_equity - current_equity) / peak_equity * 100`, where `peak_equity` is the in-memory per-session high-water mark (§6.1). `null` when the session has no peak yet (run not started / no equity sample). Sourced from execution-layer equity, NOT the eval `max_drawdown_pct` field (that is a finalized backtest metric — different provenance, must not be reused). |
| `daily_loss_limit_remaining_usd` | number \| null | `(kill_pct * starting_capital) + realized_today`, where `kill_pct = RiskConfig.daily_loss_kill_pct` and `realized_today = book.realized() - daily_realized_at_day_start` (the in-memory UTC-day baseline, §6.2). This is the exact buffer before the enforced daily-loss kill fires (`backtest.rs:1906-1957`, shared by the live executor). `null` when no kill policy or no day baseline yet. |
| `risk_veto_count_since_last_visit` | number \| null | Count of `obs "risk_veto"` events / `supervisor_note(role=risk)` for this run since the operator's last-visit timestamp. `null` until last-visit tracking lands (§9 open question; render `null` not `0` until then). |
| `paused` | bool | `eval_runs.paused` (per-run pause; execution control). |
| `flatten_requested` | bool | `eval_runs.flatten_requested` (execution control). |
| `global_safety_paused` | bool | `GET /api/safety/state.paused` (SafetyManager). Surfaced so a deployment never shows green "running" while writes are globally paused (§8). |
| `source` | "human" \| "optimizer" | `eval_runs.source` (new persisted column, §6.4). Set at queue time. Drives `awm`'s Cancel-gate. |
| `unavailable_reason` | string \| null | Populated when `venue_connected=false` or capital snapshot unavailable. Connection-as-data, mirrors `VenueAccountDto.reason`. |

### 2.2 Enums
```rust
#[serde(rename_all = "snake_case")] pub enum DeploymentMode   { Paper, Live }
#[serde(rename_all = "snake_case")] pub enum DeploymentStatus { Starting, Running, Paused, Stopped, Failed }
#[serde(rename_all = "snake_case")] pub enum DeploymentSource { Human, Optimizer }
```
`DeploymentStatus` is derived: `Queued → Starting`; `Running` (or `Paused` if `eval_runs.paused`); `Completed`/`Cancelled` → `Stopped`; `Failed` → `Failed`. Live runs are long-lived; `Completed` for a live run means an operator stopped it cleanly.

### 2.3 Connection-as-data (never 500)
The list and detail endpoints follow the `venue_account()` pattern: a deployment whose venue is unreachable or has no live snapshot still returns a row with `venue_connected=false`, capital fields `null`, and an `unavailable_reason`. The endpoint NEVER 500s on a venue outage — it is a status surface, not a trade path. The UI renders a quiet "no data" state.

---

## 3. `GET /api/live/deployments` (poll, ~5s)

Mirrors the agent-runs list-poll. Read-only GET, registered in `readonly_router` next to `/api/live/venue-account` (respect static-before-`:id` ordering; extend the R-audit comment block).

**Handler:** `crates/xvision-dashboard/src/routes/live_deployments.rs::list_deployments(State, Query<ListParams>)`.

**Request (query params):**
```
status?: "running" | "paused" | "stopped" | ...   (filter; default = active only)
mode?:   "paper" | "live"
limit?:  usize (default 20, max 100)
```

**Response (200, always):**
```json
{
  "items": [ LiveDeploymentSummary, ... ],
  "total": 7
}
```
- `items` = `eval_runs WHERE mode='live'` projected to `LiveDeploymentSummary`, joined with execution truth (last-decision-at via `MAX(eval_decisions.timestamp)`; capital fields via per-run book snapshot / venue probe; `global_safety_paused` via one `GET /api/safety/state` read).
- `total` = pre-limit filtered count.
- Default filter = active deployments (`status IN (running, paused)`), matching the ActiveTasksStrip 5s poll use. Stopped/failed available via `status` filter.
- **Frontend:** `frontend/web/src/api/live-deployments.ts::listDeployments()` via `apiFetch`, consumed with `useQuery({ refetchInterval: 5_000 })` (mirror `VenueAccountPanel.tsx` poll). This is list *membership* only; per-row live values stream via SSE.

---

## 4. `GET /api/live/deployments/:id/stream` (SSE)

Mirrors the agent-runs `/:id/stream` (snapshot-first) plus the eval_runs terminal pre-check. Read-only GET in `readonly_router`.

> **✅ Wave 3a + metrics streaming (s78.1) DELIVERED (2026-06-13).** The SSE
> delivers the **snapshot** frame, the per-tick **capital block** (`event:
> metrics` carrying `equity_usd` / `drawdown_pct` / `deployed_capital_usd` /
> `unrealized_pnl_usd` / `realized_pnl_usd` / `daily_loss_limit_remaining_usd` /
> `n_trades`), lifecycle/terminal **status**, and `lagged`. The capital block is
> delivered via a new `RunChartEvent::DeploymentMetrics(DeploymentMetricsTick)`
> variant emitted in the live loop alongside `RunChartEvent::Equity` — over the
> **same `RunEventBus`** the dashboard SSE already subscribes to (approach (b):
> widen `RunChartEvent`). This was chosen over bridging `ProgressBus` →
> `RunEventBus` (approach (a)) because the live executor's `ProgressTx` is **not
> wired in the dashboard launch path** (`Executor::live(...)` sets `with_event_bus`
> but no progress sender), so bridging would have required new `ApiContext` +
> `AppState` plumbing plus run-id filtering of the global, non-keyed `ProgressBus`
> — strictly larger blast radius than one additive enum variant whose two
> `event_name()` match sites are compiler-enforced.
>
> **HONESTY MANDATE:** the streamed capital values are the SAME honest
> book/execution-sourced numbers as the poll; a field with no real data is
> OMITTED from the frame (`skip_serializing_if`), NEVER a fabricated `0`. No
> value is sourced from `agent_runs`/eval. The bare `RunChartEvent::Equity` tick
> still maps to `event: metrics` (equity-only, tagged envelope) so a client that
> connects before the first capital tick gets a live equity heartbeat and
> **degrades to the 5s poll** (`GET /api/live/deployments`, §3) for the capital
> fields with no blank/`0` flash.
>
> **STILL DEFERRED:** the `risk_veto` event (requires obs-event wiring +
> last-visit tracking, §9 / Wave 5) and 250ms batching (current builder forwards
> per-event). The §4 event table below; rows marked *(deferred)* are not yet
> emitted. `DeploymentMetricsTick` is ts-rs-exported to
> `frontend/web/src/api/types.gen/DeploymentMetricsTick.ts`.

**Handler:** `live_deployments.rs::stream(State, Path<id>)`. **Builder:** new `crates/xvision-dashboard/src/sse/live_deployment_sse.rs`.

**Flow:**
1. **Terminal pre-check** (copy `eval_runs::stream:460-485`): `RunStore::get(&id)`; if `status.is_terminal()` (run already stopped), emit ONE synthetic `status` event with the final `LiveDeploymentSummary` snapshot and `return;` so late subscribers don't hang.
2. **Subscribe before snapshot** (copy `agent_runs::stream:322`): `let rx = state.event_bus.subscribe(&id).await;` BEFORE building the snapshot, so events committed during snapshot assembly are still delivered. Deployments are live eval_runs, so **reuse the existing eval `event_bus`** (`state.event_bus`, keyed by run id) — NO new broadcast surface on `AppState`.
3. **Snapshot-first frame:** `event: snapshot`, `data:` = full `LiveDeploymentSummary` at subscribe time.
4. **Per-event loop:** map each `RunChartEvent` / `ProgressEvent` variant to a snake_case `event:` name; emit `lagged` synthetic event on `RecvError::Lagged(n)`; break on terminal lifecycle; 15s keep-alive.

**Event names → payloads (live values; deployment id implicit in the stream path):**

| `event:` | Source variant | Payload (honest fields) |
|---|---|---|
| `snapshot` | (assembled) | full `LiveDeploymentSummary` |
| `metrics` *(capital block — DELIVERED s78.1)* | `RunChartEvent::DeploymentMetrics(DeploymentMetricsTick)` (live loop, emitted alongside `Equity`); the bare `RunChartEvent::Equity` tick also maps here (equity-only heartbeat) | `{ time, equity_usd, drawdown_pct?, deployed_capital_usd?, unrealized_pnl_usd?, realized_pnl_usd?, daily_loss_limit_remaining_usd?, n_trades }` — the FLAT tick (not the `{event,data}` envelope; `event:` already names it). All derived in-loop from the book + in-memory peak/day-start (§6). Null capital fields are **omitted** (honesty), never `0`. The equity heartbeat keeps its tagged envelope so a pre-capital-tick client degrades to the poll |
| `decision` | per-decision record | `{ last_decision_at, action, asset, fill_price, fill_size, pnl_realized }` from the just-written `eval_decisions` row |
| `risk_veto` *(deferred — not yet emitted in 3a)* | obs `risk_veto` event | `{ reason: "daily_loss_kill" \| "max_concurrent_positions", severity, at }` — increments `risk_veto_count_since_last_visit` |
| `status` | lifecycle / pause / flatten / safety | `{ status, paused, flatten_requested, global_safety_paused }` |
| `lagged` | `RecvError::Lagged(n)` | `{ dropped: n }` |

**Producer additions (engine) — DELIVERED s78.1 (metrics); `risk_veto` still deferred.** The live loop emits, on each post-tick equity sample, BOTH `ProgressEvent::MetricsUpdated { … }` (engine `ProgressBus`, for CLI/optimizer subscribers) AND `RunChartEvent::DeploymentMetrics(DeploymentMetricsTick { time, equity_usd, drawdown_pct, deployed_capital_usd, unrealized_pnl_usd, realized_pnl_usd, daily_loss_limit_remaining_usd, n_trades })` via `emit_chart` onto the `RunEventBus` the dashboard SSE subscribes to (`backtest.rs`, alongside the existing `RunChartEvent::Equity`). **Chosen approach (b)** — widen `RunChartEvent` — over (a) bridging `ProgressBus` → `RunEventBus`, because the dashboard live launch path (`api/eval.rs::build_live_executor`, `Executor::live(...).with_event_bus(...)`) does NOT wire a `ProgressTx` into the executor (so `ProgressEvent::MetricsUpdated` is dropped on the floor in production today), and the global `ProgressBus` is not run-id-keyed; bridging it would mean new `ApiContext`/`AppState` plumbing + run-id filtering. The new variant is additive on the wire (`RunChartEvent` is `#[serde(tag="event", content="data")]`); its two exhaustive `event_name()` match sites (deployment SSE + eval-runs SSE) are compiler-enforced, so no chart consumer breaks silently. The `risk_veto` emission remains deferred (needs obs-event wiring + last-visit tracking, §9 / Wave 5); until then `risk_veto_count_since_last_visit` stays `null` per the honesty rule.

**Frontend:** `live-deployments.ts::openDeploymentStream(id, onEvent)` — `EventSource("/api/live/deployments/:id/stream")`, a `LIVE_SSE_EVENTS` const array matching the Rust `event_name()` exactly, exponential-backoff reconnect (copy `SSE_BACKOFF_MS` from `agent-runs.ts`), `close()` handle. Hand-written `DeploymentStreamEvent` union until ts-rs covers the event payloads.

---

## 5. Data sourcing per broker (available-now vs gap)

| Target | Alpaca (paper) — available now | Alpaca gap | Orderly — available now | Orderly gap |
|---|---|---|---|---|
| account equity | `AlpacaAccount.equity` (`alpaca.rs:135`) → `balance()` | — | `OrderlyAccount.equity()` = usdc+unrealized (`orderly.rs:211`) → `VenueSnapshot.equity_usd` | — |
| deployed_capital_usd | raw inputs exist (Σ `AlpacaPosition.market_value`); **no aggregate emitted** | add aggregate (server-side sum) | raw inputs exist (Σ `\|qty\|*mark`); **no aggregate emitted** | add aggregate |
| unrealized_pnl_usd | **MISSING** — `apca_position_to_plain` drops `unrealized_pl` (`alpaca.rs:334`) | add per-position unrealized to mapping; OR use per-run book mark-to-market | account-level `unrealized_pnl` + per-position `unsettled_pnl` (`orderly.rs:207,224`) — **strongest field** | account-wide, not per-run → must attribute via engine book |
| realized_pnl_usd | proxy only: `equity - last_equity` (`alpaca.rs:568`) — conflates unrealized, **not true realized** | use engine `book.realized()` per run instead | **MISSING** — portfolio hardcodes `0.0` (`orderly.rs:1029`); no realized field | use engine `book.realized()` per run; surface `null` if no book |
| drawdown_pct | equity available; **no persisted peak** | in-memory peak (§6.1) | equity available; **no persisted peak** | in-memory peak (§6.1) |
| daily_loss buffer | no day boundary on client | in-memory day-start (§6.2) + enforced `daily_loss_kill_pct` | no day boundary, no realized | in-memory day-start (§6.2) using `book.realized()` |

**Sourcing ruling for CT5:** the **engine `PortfolioBook`** (the in-loop authoritative cash/positions/realized/unrealized holder, `book.rs`) is the per-run source of truth for `realized_pnl_usd`, `unrealized_pnl_usd`, `deployed_capital_usd`, `drawdown_pct`, and the daily-loss buffer — because it is already per-run, broker-fed (fills applied from `RealBrokerFills`), and venue-agnostic. The Orderly `venue_snapshot()` is account-wide and CANNOT be attributed to one run when multiple runs share an account; it is used only for `venue_connected` and as a cross-check, not as the per-run capital source. The Alpaca `equity - last_equity` realized proxy is explicitly REJECTED — it is not true realized PnL. Where the book has no data yet (pre-first-fill), fields are `null`.

---

## 6. In-memory peak / day-start tracking (execution layer, NOT a persisted snapshot table)

The synthesis ruling allows **in-memory per-session execution-layer tracking**, not a persisted snapshot table. The live loop already holds the exact variables; CT5 promotes them from "drives a one-off SSE number" to "the authoritative source for the contract's drawdown + daily-loss fields," and exposes them via the existing in-loop emissions — without persisting a peak/day-start snapshot table.

### 6.1 Peak equity (high-water mark) — IN-MEMORY
- Already present: `let mut peak_equity = initial.max(0.0)` (`backtest.rs:2972`), updated `if equity > peak_equity { peak_equity = equity }` (`backtest.rs:3330`).
- CT5: this stays a loop-local session variable. `drawdown_pct = (peak_equity - equity)/peak_equity*100` is emitted on every `metrics` SSE (already computed at `backtest.rs:3333`). For the **poll snapshot** (`/api/live/deployments`), when the run is in-flight the snapshot reads the *last emitted* drawdown from the SSE producer's last-known state; if no live producer is attached (e.g. between polls with no recent tick), `drawdown_pct` is recomputed on read as the running max over the persisted `eval_equity_samples` curve (peak = `max(equity_usd)`), which is the honest derivable fallback. Either way the value is execution-layer-sourced.
- **NOT persisted as a peak column.** Reset to `initial` each loop start is acceptable: a session's drawdown is per-session, and the equity-curve fallback reconstructs it for the poll path.

### 6.2 Day-start baseline (daily-loss buffer) — IN-MEMORY
- Already present: `daily_loss_day: Option<NaiveDate>` + `daily_realized_at_day_start: f64` (`backtest.rs:3018-3019`). On each UTC-day boundary, `daily_realized_at_day_start = book.realized()`; `realized_today = book.realized() - daily_realized_at_day_start`.
- CT5: `daily_loss_limit_remaining_usd = (RiskConfig.daily_loss_kill_pct * starting_capital) + realized_today`, rolled per UTC day, emitted on each `metrics` SSE. This is the exact headroom before the enforced kill (`backtest.rs:1906-1957`). Stays in-memory.
- **Poll-path fallback:** when no live producer is attached, `realized_today` is derivable as Σ `eval_decisions.pnl_realized WHERE timestamp >= UTC-midnight` from persisted decision rows — honest, no new column.

### 6.3 Per-run unrealized — the one place persistence helps the poll path
The peak and day-start are session-transient; the SSE path covers in-flight live values fully. The **poll snapshot** for `unrealized_pnl_usd` is the one field with no clean persisted-derivation (open legs aren't in any table). Two options:
- **(A) Persisted single field:** add `eval_runs.unrealized_pnl_usd REAL NULL`, updated in the same buffered flush as equity samples (alongside `record_equity_upsert_batch`). Cheap, gives the poll path an honest per-run number, survives between polls. **Recommended.**
- **(B) Snapshot `PortfolioBook.open_legs()` into a per-run positions table** — richer (per-position breakdown) but heavier; defer to a later wave if the strip needs per-position detail.

This is a single nullable column, NOT a peak/day-start snapshot table — consistent with the in-memory ruling for peak/day-start while giving the poll a non-fabricated unrealized figure. When `null`, the UI shows "—".

### 6.4 Summary: persisted-schema vs in-memory
| State | Where | Persisted? |
|---|---|---|
| `peak_equity` | live loop local | **NO** (in-memory; poll fallback = max over equity_samples) |
| `daily_realized_at_day_start` + `daily_loss_day` | live loop local | **NO** (in-memory; poll fallback = Σ today's pnl_realized) |
| `unrealized_pnl_usd` (per run, poll path) | `eval_runs.unrealized_pnl_usd` | **YES** (single nullable column, option A) |
| `source` (human/optimizer) | `eval_runs.source` | **YES** (single column, default 'human') |
| `realized_pnl_usd`, `deployed_capital_usd` | derived from book / decisions | NO new column (book in-loop; poll = Σ pnl_realized / open-position notional) |
| `last_decision_at` | derived `MAX(eval_decisions.timestamp)` | NO new column (projection) |

---

## 7. ts-rs binding plan

- Put `LiveDeploymentSummary` + its enums in **`crates/xvision-engine/src/api/live_deployments.rs`** (engine), gated `#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]` + `ts(export, export_to = "../../../frontend/web/src/api/types.gen/")` — mirror `RunSummary` (`eval.rs:229-233`). This gives **generated** `LiveDeploymentSummary.ts`, unlike the hand-written `VenueAccountDto` mirror (the venue DTO predates the engine-ts-rs convention).
- The dashboard route's list envelope (`{items, total}`) and the SSE event-payload structs stay dashboard-route-local (Serialize only); their TS is **hand-written** in `live-deployments.ts` (`DeploymentStreamEvent` union + `ListDeploymentsResponse`), matching the agent-runs convention. Header the file: "replace with generated bindings when backend lands ts-rs derives."
- Regenerate: `cargo test -p xvision-engine --features ts-export` (writes `LiveDeploymentSummary.ts`), or `cargo xtask gen-types`. Add a CI assertion that the generated file matches (mirrors `RunSummary`).

---

## 8. Safety-pause interaction

- `global_safety_paused` is read from `GET /api/safety/state.paused` (`SafetyManager`) per list build and surfaced on every `LiveDeploymentSummary`.
- **A deployment must NEVER render green "running" while the global safety gate is paused.** The UI shows the global pause as the TOP red `SafetyPauseBanner` above all live panels (per §4.4 / §7.1), and per-row status reflects the gate: when `global_safety_paused=true`, the effective status is paused even if `eval_runs.status='running'`.
- Precedence (matches `SafetyGate.check_broker_submit:125`): (1) global pause → all submits blocked; (2) venue-label mismatch; (3) per-run `paused`; (4) per-run safety limits. The contract surfaces (1) via `global_safety_paused`, (3) via `paused`.
- **Fail-safe bootstrap** (`state.rs:92-121`): on a fresh install with a live venue configured, `paused=true` by default. CT5's snapshot must honestly reflect this — a just-deployed live run on a fresh box shows `global_safety_paused=true` until the operator explicitly resumes. Do not paper over it.
- Safety pause is NOT duplicated in the gentle-nag strip; it is the top banner only.

---

## 9. Downstream consumption

### 9.1 `n0k` — live/paper rows in ActiveTasksStrip (P1)
- `ActiveTasksStrip` adds a deployments query: `listDeployments({status:'running,paused'})` at 5s poll, rendering a row per `LiveDeploymentSummary` alongside the existing eval queue.
- Row shows: `mode` (Paper/Live badge from `live_config`, **free today**), `strategy_name`, `status`, `last_decision_at`, and **running unrealized P&L** (`unrealized_pnl_usd`, the CT5-supplied honesty field — §6.3; renders "—" when `null`).
- Per-row live values (unrealized P&L ticking, last-decision) come from `openDeploymentStream(id)` SSE; list membership from the poll. This is exactly the agent-runs list-poll + per-run-SSE split.

### 9.2 `awm` — Cancel-gate + runaway warning (P2)
- **Cancel-gate:** render the Cancel button only when `source === "human"`. This unblocks the standing TODO at `ActiveTasksStrip.tsx:93`. Requires the new persisted `eval_runs.source` column.
  - **`eval_runs.source` addition (persisted-schema):** new column `source TEXT NOT NULL DEFAULT 'human'` in the base schema / fresh migration. Enum `RunSource { Human, Optimizer }` (snake_case serde, ts-rs-gated) on `Run`, defaulting to `Human` in `Run::new_queued`. Set `run.source = RunSource::Optimizer` at the two optimizer call sites (`eval_adapter.rs:121`, `:274`); the human path (`eval.rs:3874`) keeps the default. Add to INSERT (`store.rs:152`) + both SELECTs (`store.rs:575`, `:796`) + `row_to_run` (tolerant default). Map into `RunSummary` AND `LiveDeploymentSummary` via `summarise()`. Because the DB is wiped, no backfill — but keep the tolerant `try_get().unwrap_or('human')` read for forward-binary safety. Agent_id is NOT a reliable discriminator (optimizer uses `strategy.manifest.id`); the explicit column is required.
- **24h runaway warning:** frontend-side, gated to `mode==='live'` (real money) with a 24h threshold on `started_at` (mirror the existing `isStuck()` heuristic). No backend change strictly required for the warning. If the warning must *clear on acknowledgement*, that needs a new operator-check-in field — deferred to an open question. The watchdog's 30min default must NOT kill live runs; verify live runs set/override `max_run_duration_secs` (live runs are exactly the long-running case).

### 9.3 `8s4` — capital-risk strip (P2)
- Renders **deployed capital · drawdown · daily-loss-limit buffer** (§7.2, "non-negotiable for live money"), color-coded as the buffer approaches 0.
- All three fields come straight off `LiveDeploymentSummary` (`deployed_capital_usd`, `drawdown_pct`, `daily_loss_limit_remaining_usd`), each broker/execution-sourced per §2.1/§5/§6.
- `risk_veto_count_since_last_visit` shown as a since-last-visit chip (renders `null`→"—" until last-visit tracking lands).
- Strip renders "—" / "no data" for any `null` field — never a fabricated `0`. Below the data floor (no fills yet) the strip shows an "insufficient data" state, not a calm green zero.

---

## 10. Phased Wave-3→5 build plan

**Wave 3 (this contract — backend foundation, no live-money UI yet):**
1. `eval_runs.source` column + `RunSource` enum + set at optimizer call sites + persist/read/summarise. (unblocks `awm` Cancel-gate)
2. `eval_runs.unrealized_pnl_usd` column + write in the live-loop equity flush. (unblocks `n0k` running P&L poll path)
3. `LiveDeploymentSummary` engine type (ts-rs) + `live_deployments.rs` route: `list_deployments` (poll) + `stream` (SSE), registered in `readonly_router`, R-audit updated.
4. Widen the live-loop `metrics` SSE emission to carry deployed_capital / unrealized / realized / daily-loss-buffer; add `status` + `risk_veto` emissions.
5. Honesty TDD test (§11): assert no live-money field ever renders from `agent_runs`/eval inputs; assert `null` (not `0`) when unsourced.
6. Terminology-lock rows for all new operator labels BEFORE merge (§11).

**Wave 4 (consumers wired to the contract):**
7. `n0k`: live/paper rows in ActiveTasksStrip (poll + per-row SSE).
8. `awm`: Cancel-gate (`source==='human'`) + 24h live runaway warning chip.
9. `8s4`: capital-risk strip (deployed capital · drawdown · daily-loss buffer) + safety-pause top banner precedence.

**Wave 5 (hardening / depth, optional):**
10. Last-visit tracking → real `risk_veto_count_since_last_visit` (replaces `null`).
11. Per-run positions table (§6.3 option B) for per-position breakdown in the strip.
12. Alpaca per-position unrealized in `apca_position_to_plain` (cross-broker parity); `xvn`/MCP deployments parity.
13. Resumable SSE (Last-Event-ID replay) if reconnect gaps matter for live.

---

## 11. Honesty checklist (gate to merge)

- [ ] Every `LiveDeploymentSummary` field has a documented broker/execution provenance (§2.1). None sourced from `agent_runs` trace records or eval `RunSummary` outcome fields.
- [ ] `drawdown_pct` is execution-equity-derived (in-memory peak / equity-curve max), NOT the eval `max_drawdown_pct` field.
- [ ] `realized_pnl_usd` is `book.realized()` / Σ `pnl_realized`, NEVER the Alpaca `equity - last_equity` proxy and NEVER Orderly's hardcoded `0.0` (that surfaces as `null`).
- [ ] `unrealized_pnl_usd` is per-run (book mark-to-market / persisted column), NOT the account-wide venue snapshot when runs share an account.
- [ ] `last_decision_at` is `MAX(eval_decisions.timestamp)` (a real recorded decision) or `null` — never `started_at` as a stand-in.
- [ ] `deployed_capital_usd` is open-position notional from book/broker, NOT `live_config.capital.initial` (launch config).
- [ ] Every nullable field renders "—"/"no data"; no fabricated `0`. Below the data floor → "insufficient data," no calm green.
- [ ] `mode` (paper/live) comes from `live_config.venue_label`, NOT inferred from `agent_runs` (§8.9 acceptance c).
- [ ] `global_safety_paused` is surfaced; a paused system never renders green "running" (§8). Fail-safe bootstrap honestly reflected.
- [ ] Endpoints never 500 on venue outage — connection-as-data with `unavailable_reason`.
- [ ] A TDD test asserts the live-money labels do not render from `agent_runs`/eval inputs (analogous to §8.7 step 1).
- [ ] Terminology-lock rows added for every new operator label ("capital-risk strip", "running P&L", "deployed capital", "daily-loss-limit buffer", "deployment", "paper/live") in `docs/superpowers/specs/2026-05-27-autooptimizer-terminology-lock.md` (or the CT terminology lock) BEFORE merge. Codename stays `autooptimizer`, never bare `optimizer`.
- [ ] §8.9 acceptance boxes (a)-(e) all pass before any CT5 UI ships.

