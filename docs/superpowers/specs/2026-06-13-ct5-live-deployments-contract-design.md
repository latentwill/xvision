# CT5 — Live-deployments contract (design)

- **Date:** 2026-06-13
- **Status:** Design — pending design-review gate + user approval
- **Epic:** `xvision-s78` (Control Tower) → unblocks `n0k`, `awm`, `8s4` (and `8wn`'s live-cost inputs)
- **Supersedes the placeholder in:** `docs/superpowers/specs/2026-06-07-control-tower-dashboard-evaluation.md` §8.9 (CT5)
- **Register:** product UI (design SERVES the product) — impeccable principles applied to the strip section.

## 1. Problem

The Control Tower home cannot honestly show "what's trading right now." Three beads are gated on this:

- `n0k` — Active-tasks live/paper rows (mode, last decision, running P&L)
- `8s4` — Capital-risk strip (deployed capital, drawdown, daily-loss-limit buffer)
- `awm` — ETA + Stop/cancel-gating + runaway >24h warning for live runs

The 2026-06-07 evaluation deferred all three behind a "live-trading backend contract" that was assumed not to exist.

## 2. Current state (verified against `main`, 2026-06-13)

Live execution **already exists and runs** — the gating assumption was wrong:

- `xvn eval run --mode live` (and `POST /api/eval/runs`) drives a real `run_inner_live` loop in `crates/xvision-engine/src/eval/executor/backtest.rs` against **Alpaca paper** + **Orderly testnet/mainnet**.
- A live run **is an `eval_run`** (`mode='live'`, `venue_label ∈ {paper,testnet,live}`, non-NULL `live_config_json`). There is **no** separate deployments table.
- Real money is the only blocked part: `VenueLabel::Live` is rejected by `LiveConfig::validate()`, and `AlpacaLiveSurface` is a stub. Today every live run is **paper or testnet (simulated)**.
- Broker truth is partly exposed: `BrokerSurface { submit_order, position, balance, buying_power }`; Orderly adds `venue_snapshot { equity_usd, unrealized_pnl, positions[] }`. `GET /api/live/venue-account` exposes the Orderly snapshot — but **account-scoped, not run-scoped**.
- The daily-loss accumulator (`daily_loss_day`, `daily_realized_at_day_start`) lives **only in-memory** in the executor task — invisible to the dashboard, lost on restart.

**The single missing seam:** nothing joins `run_id` → its live capital-risk state. There is no typed live-deployments resource; the dashboard would have to `GET /api/eval/runs?status=running` and client-filter `mode='live'`, with no per-run portfolio/risk data.

## 3. Scope decision

**Paper/testnet read layer now, forward-compatible shape.** Build the contract over live runs that already execute. Defer real-money (`AlpacaLiveSurface` impl, `VenueLabel::Live` unblock, real-money safety review) to a **CT5-real** follow-up. The `mode`/`venue_label` fields already distinguish paper/live, so the API shape will not need a revision when real-money lands.

## 4. Architecture — "a deployment *is* a live eval-run + a volatile state row"

Two thin seams; no parallel runtime, no second lifecycle:

1. **Identity reuse.** `deployment_id == run_id`. Listing = query `eval_runs WHERE mode='live'`, enriched. The executor already owns run status (`starting`/`running`/`paused`/`stopped`/`failed`).
2. **One new volatile table, `live_run_state`** — exactly one upserted row per live run, written by the executor each bar. This is the only genuinely-new persistence, and it finally drags the in-memory daily-loss accumulator into something the dashboard can read.

The API joins `eval_runs` (identity/status/config) ⨝ `live_run_state` (capital-risk) → `LiveDeploymentSummary`.

**Rejected alternative:** a separate `deployments` table referencing runs — adds a second status lifecycle to keep in sync for zero benefit while real-money doesn't exist. Premature.

### 4.1 `live_run_state` schema

New migration under `crates/xvision-engine/migrations/` (co-located with `eval_runs`):

```sql
CREATE TABLE live_run_state (
  run_id                   TEXT PRIMARY KEY REFERENCES eval_runs(id) ON DELETE CASCADE,
  deployed_capital_usd     REAL NOT NULL,   -- static, from live_config.capital.initial
  equity_usd               REAL,            -- broker truth (BrokerSurface.balance / venue_snapshot)
  unrealized_pnl_usd       REAL,            -- Orderly venue_snapshot; Alpaca: mark − cost basis
  realized_pnl_usd         REAL,            -- book.realized() since run start
  realized_today_usd       REAL,            -- book.realized() − daily_realized_at_day_start
  daily_loss_remaining_usd REAL,            -- max_daily_loss_pct·equity + realized_today (when < 0)
  drawdown_pct             REAL,            -- (peak_equity − equity)/peak_equity
  peak_equity_usd          REAL,            -- running max
  last_decision_at         TEXT,            -- RFC-3339
  updated_at               TEXT NOT NULL    -- freshness stamp
);
```

- **Writer:** `LiveStateStore::upsert`, called from `run_inner_live` once per bar right after the decision/fill (the loop already writes `eval_equity_samples` per bar — marginal single-row upsert). The executor already holds `book`, the daily-loss accumulators, and broker access, so it computes the snapshot locally — **no cross-process channel**.
- **Honesty:** `equity_usd`/`unrealized` come from broker truth at write time; `deployed_capital` from config; `daily_loss_remaining` from the now-persisted accumulator. Nothing is derived from eval summaries.
- **Lifecycle:** terminal upsert on stop/fail; row retained until the run is deleted (CASCADE). The list endpoint filters `status='running'`; detail works for any status.

### 4.2 `LiveDeploymentSummary` type

Rust type (`xvision-engine` api or `xvision-dashboard`), ts-rs → `types.gen`. Matches §8.9, forward-compatible:

```ts
type LiveDeploymentSummary = {
  deployment_id: string;            // = run_id
  strategy_id: string;
  strategy_name: string;
  mode: "paper" | "live";           // 'live' impossible today (validation-rejected)
  venue_label: "paper" | "testnet" | "live";
  status: "starting" | "running" | "paused" | "stopped" | "failed";
  started_at: string;
  last_decision_at: string | null;
  deployed_capital_usd: number | null;
  equity_usd: number | null;
  realized_pnl_usd: number | null;
  unrealized_pnl_usd: number | null;
  realized_today_usd: number | null;
  drawdown_pct: number | null;
  daily_loss_limit_remaining_usd: number | null;
  risk_veto_count: number;          // total vetoes for the run (eval_decisions); "since last here"
                                    //   delta computed client-side via jlm's last-visit boundary
};
```

Every capital-risk field is **nullable** — null when `live_run_state` is absent (just-started run) → UI shows "—", never faked.

### 4.3 Endpoint surface

| Route | Purpose |
|---|---|
| `GET /api/live/deployments` | list paper/testnet live runs as `LiveDeploymentSummary[]` (default `status=running`; `?status=` filter) |
| `GET /api/live/deployments/:id` | one deployment (`:id` = run_id), any status |
| `GET /api/live/deployments/:id/stream` | SSE — extend the existing `/api/eval/runs/:id/stream`, enriched with `live_run_state` pushes on each upsert |

Backtest runs **must never** appear here (`mode='live'` filter is the contract boundary).

## 5. UI consumption (impeccable — design SERVES the product)

The strips are **follow-on implementation plans**; this section sets their design bar and acceptance. Grounded in the project's existing strip system (`DeployReadinessStrip`, `HomeOutcomeStrip`): full-width inline strips, `Card` primitive, ink ramp (`text-text`…`text-text-4`), semantic tokens, **glyph + color never color-alone** (colorblind/monochrome legible), null-on-empty ("say nothing when you have nothing to say").

### 5.1 Layout & placement (CLAUDE.md hard rules)

- **Full-width stacked strips only** (QA30). No `grid-cols-12` 8/4 split, no fourth column / right-side card (the chat rail owns the right edge).
- **No popups / modals / sheets / overlays.** Stop-confirm, deployment detail → route or inline-expand, never a dialog.
- Capital-risk strip mounts within the existing home stack (candidate: inside `AttentionBand` near Active tasks, or a sibling strip directly under it — decided in the strip plan, not here).

### 5.2 Color-coding (verified-contrast tokens, paired with glyph + label)

Risk semantics use the real token palette — `--gold` #00e676 (healthy), `--warn` #ffb020 (caution), `--danger` #ff4d4d (breach); light-theme variants meet ≥4.5:1. **Color is never the sole signal** — each carries a glyph and a literal value:

| Signal | gold (healthy) | warn (caution) | danger (breach) |
|---|---|---|---|
| Daily-loss buffer (`daily_loss_remaining_usd / limit`) | > 50% left | ≤ 25% left | ≤ 0 (breached / paused) |
| Drawdown (`drawdown_pct`) | small | moderate | large |
| Running P&L (`unrealized + realized_today`) | ≥ 0 (`▲`) | — | < 0 (`▼`) |

Thresholds are defined in a pure selector (`features/live/deployment-risk.ts`) with unit tests, mirroring `deploy-readiness.ts` — components stay presentational.

### 5.3 Honesty (non-negotiable)

- Every deployment row/strip is **explicitly labeled `paper` or `testnet` (simulated)** — never "live," "real money," or bare currency without the simulated qualifier. A simulated run with a calm "live money" frame is a correctness bug.
- Null capital-risk values render as "—", never `$0.00` or an inferred number.
- `deployed_capital` is config-stated; `equity`/`unrealized` are broker-truth — these are presented as distinct, not blended into one vanity number.

### 5.4 Motion

- Value updates (P&L tick, buffer change) use a brief crossfade/number-transition on the changed cell only — not a whole-strip re-entrance. Reduced-motion is globally collapsed (`globals.css`), so no per-component guard needed, but transitions target value cells, not layout.
- No reveal that gates content visibility on a class-triggered transition (the home page's existing rule).

### 5.5 AI-slop avoidance

- No hero-metric template (big number + gradient). No identical card grid of deployments — a compact row list. No side-stripe borders for risk state (use the glyph + tinted `--*-bg` fill or a full border). No per-section uppercase eyebrow.

### 5.6 Per-bead acceptance

- **`n0k`** — Active-tasks renders live/paper rows from `GET /api/live/deployments?status=running`: mode pill, `last_decision_at` (relative), running unrealized P&L (glyph + sign + simulated label).
- **`8s4`** — Capital-risk strip: `deployed_capital`, `drawdown_pct`, `daily_loss_remaining` (each color+glyph+value per §5.2), simulated-labeled.
- **`awm`** — ETA from `live_config.stop_policy` + elapsed (rendered **only when a real limit exists**, never a fake estimate); Stop/flatten control (sets `flatten_requested`, the column already exists); runaway >24h warning from `started_at`.

## 6. Terminology lock additions

Per §7.2 + the 2026-05-27 lock, new operator-facing labels get lock-doc rows **before merge**: "deployment" (operator surface for a live/paper run), "running P&L", "deployed capital", "daily-loss buffer", "simulated" (paper/testnet qualifier). Developer-surface names stay precise (`live_run_state`, `LiveDeploymentSummary`, `deployment_id`).

## 7. Testing (TDD)

**Rust:**
- Migration test (table + CASCADE).
- `LiveStateStore` upsert/read (insert, update-in-place, null fields).
- **Extend the existing 21 live-loop integration tests:** assert a `live_run_state` row appears after N bars with correct `equity_usd`, `realized_today_usd`, and `daily_loss_remaining_usd`; assert daily-loss resets across a day boundary.
- API handler tests: list filters `mode='live'` (a backtest run is absent); detail join; null-safety when state absent; `:id` 404 for unknown.
- SSE: `:id/stream` emits a `live_run_state` frame on upsert.

**Honesty tests (gate):**
- A backtest run does **not** appear in `/api/live/deployments`.
- A paper run serializes `mode:"paper"` / `venue_label:"paper"`; no field ever carries an inferred money value when `live_run_state` is null.

**Frontend (follow-on plans):** `deployment-risk.ts` selector unit tests (threshold colors); strip rendering against a mock `LiveDeploymentSummary` incl. the all-null just-started case and the breached-buffer case.

## 8. Scope boundary & deferred work

**In this spec/plan:** `live_run_state` table + `LiveStateStore` + executor write seam; `LiveDeploymentSummary` type + the three routes + SSE; tests.

**Follow-on implementation plans (unblocked by this contract):** `n0k`, `8s4`, `awm` strips (§5.6).

**Deferred to CT5-real:** `AlpacaLiveSurface` implementation; remove `VenueLabel::Live` rejection; real-money safety/risk review; persisted cross-day daily-loss history (`8wn` budget cap is its own bead).

## 9. Acceptance (contract unblock, per §8.9)

- [ ] `GET /api/live/deployments` returns only actual paper/testnet live runs.
- [ ] `GET /api/live/deployments/:id/stream` streams decision/PnL/risk updates.
- [ ] Dashboard distinguishes paper from (future) live without inferring from `agent_runs`.
- [ ] Capital-risk values come from broker/execution state (+ the persisted accumulator), not eval summaries.
- [ ] New UI labels added to the terminology lock before merge.
