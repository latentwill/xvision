# CT5 — Live-deployments contract (design)

- **Date:** 2026-06-13
- **Status:** Design — revised after design-review gate round 1 (11 blocking findings addressed); pending re-review + user approval
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
- A live run **is an `eval_run`** with `mode='live'` (`RunMode::Live`, serializes `"live"`). There is **no** separate deployments table.
- **Capital-risk is tracked per-run by the internal `PortfolioBook`**, not the broker. The hot loop computes equity via `book.mark(asset, bar.close)` + `book.equity(&marks)` (`backtest.rs:3347-3349`); `broker.balance()` is **never** called in the per-bar path. This matters (see §4.1): book equity is per-run and honest; the broker account snapshot is account-wide and cannot be attributed to one run.
- Real money is the only blocked part: `VenueLabel::Live` is rejected by `LiveConfig::validate()` (`live_config.rs:231`, tested), and `AlpacaLiveSurface` is a stub. Today every live run is **paper or testnet (simulated)**.
- **Known persistence bug (prerequisite, see §4.4):** `eval_runs.create()` (`store.rs:158-192`) does **not** write `venue_label`; the column defaults to `'paper'` (migration 031). So testnet runs are currently mislabeled `paper` in the DB. The honesty contract (§5.3) cannot hold until this is fixed.
- The daily-loss accumulator (`daily_loss_day`, `daily_realized_at_day_start`) lives **only in-memory** in the executor task — invisible to the dashboard, lost on restart.

**The missing seam:** nothing joins `run_id` → its live capital-risk state. There is no typed live-deployments resource; the dashboard would have to `GET /api/eval/runs?status=running` and client-filter `mode='live'`, with no per-run portfolio/risk data.

## 3. Scope decision

**Paper/testnet read layer now, forward-compatible shape.** Build the contract over live runs that already execute. Defer real-money (`AlpacaLiveSurface` impl, `VenueLabel::Live` unblock, real-money safety review) to a **CT5-real** follow-up. `venue_label` already distinguishes paper/testnet/live, so the API shape will not need a revision when real-money lands.

## 4. Architecture — "a deployment *is* a live eval-run + a volatile state row"

Two thin seams; no parallel runtime, no second lifecycle:

1. **Identity reuse.** `deployment_id == run_id`. Listing = query `eval_runs WHERE mode='live'`, enriched. The executor already owns run status.
2. **One new volatile table, `live_run_state`** — exactly one upserted row per live run, written by the executor each bar. This is the only genuinely-new persistence, and it drags the in-memory daily-loss accumulator (and a denormalized strategy name + veto count) into something the dashboard can read with a single join.

The API joins `eval_runs` (identity/status) ⨝ `live_run_state` (capital-risk + denormalized labels) → `LiveDeploymentSummary`.

**Rejected alternative:** a separate `deployments` table referencing runs — adds a second status lifecycle to keep in sync for zero benefit while real-money doesn't exist. Premature.

### 4.1 `live_run_state` schema

Next sequential migration under `crates/xvision-engine/migrations/` (065 at time of writing; co-located with `eval_runs`):

```sql
CREATE TABLE live_run_state (
  run_id                   TEXT PRIMARY KEY REFERENCES eval_runs(id) ON DELETE CASCADE,
  strategy_id              TEXT,            -- denormalized from live_config at run start
  strategy_name            TEXT,            -- denormalized (display name) at run start — see §4.4
  deployed_capital_usd     REAL NOT NULL,   -- static, = scenario.capital.initial (the run's configured capital)
  equity_usd               REAL,            -- BOOK-computed: book.equity(&marks) (NOT broker.balance — see note)
  unrealized_pnl_usd       REAL,            -- BOOK-computed from bar-close marks (NOT a live broker quote)
  realized_pnl_usd         REAL,            -- book.realized() since run start
  realized_today_usd       REAL,            -- book.realized() − daily_realized_at_day_start
  daily_loss_remaining_usd REAL,            -- (daily_loss_kill_pct · deployed_capital_usd) + realized_today
                                            --   (clamped ≥ 0; 0 = breached). Anchored to INITIAL capital, per
                                            --   the executor (backtest.rs:3668-3677), NOT current equity.
  drawdown_pct             REAL,            -- (peak_equity − equity)/peak_equity
  peak_equity_usd          REAL,            -- running max
  risk_veto_count          INTEGER NOT NULL DEFAULT 0,  -- monotonic; executor increments on each risk veto
  last_decision_at         TEXT,            -- RFC-3339
  updated_at               TEXT NOT NULL    -- freshness stamp
);
```

**Honest provenance of the values (corrected after review):**

- `equity_usd`, `unrealized_pnl_usd`, `realized_*` are **book-computed (the run's own `PortfolioBook`)**, not broker-truth. This is *deliberate and correct*: the book is per-run, whereas `BrokerSurface.balance()` / Orderly `venue_snapshot` are **account-wide** and cannot be attributed to one run when multiple runs share a broker account. `unrealized_pnl_usd` is a **bar-close mark**, not a live broker quote. The spec makes no "broker truth" claim. (`GET /api/live/venue-account` remains the separate account-level view; it is *not* this contract.)
- `daily_loss_remaining_usd` uses the executor's actual formula: `daily_loss_kill_pct` (from `strategy.risk.daily_loss_kill_pct`, `strategies/risk.rs:13`) × `deployed_capital_usd` (the run's `scenario.capital.initial`), plus `realized_today` — anchored to **initial capital**, not current equity.
- `deployed_capital_usd` is config-stated; it is presented as distinct from `equity_usd`, never blended.

**Writer:** `LiveStateStore::upsert`, called from `run_inner_live` once per bar right after the decision/fill. Note: `eval_equity_samples` are **buffered and batch-flushed at loop end** (`record_equity_upsert_batch`), so this upsert is the **first true per-bar DB write** in the live loop — expect a small per-bar latency add; do not benchmark against a (non-existent) per-bar write. `risk_veto_count` increments in the same write path when the executor records a veto. `strategy_id`/`strategy_name`/`deployed_capital_usd` are written once at run start and left unchanged.

**Lifecycle:** terminal upsert on stop/fail; row retained until the run is deleted (CASCADE). The list endpoint filters `status='running'`; detail works for any status.

### 4.2 `LiveDeploymentSummary` type

Rust type, ts-rs → `types.gen`. Corrected to the actual wire format:

```ts
type LiveDeploymentSummary = {
  deployment_id: string;            // = run_id
  strategy_id: string | null;
  strategy_name: string | null;     // denormalized in live_run_state; null until first upsert
  mode: "live";                     // ALL rows are RunMode::Live; this endpoint filters mode='live'.
                                    //   The paper/testnet/real distinction is NOT here — see venue_label.
  venue_label: "paper" | "testnet"; // the simulated-vs-real discriminator. 'live' is impossible today
                                    //   (validation-rejected) and is filtered out of this response (§5.3).
  status: "queued" | "running" | "completed" | "failed" | "cancelled";  // actual RunStatus
  paused: boolean;                  // separate bool on Run; awm uses it to distinguish paused-but-running
  started_at: string;
  last_decision_at: string | null;
  deployed_capital_usd: number | null;
  equity_usd: number | null;            // book-computed (per-run); see §4.1
  realized_pnl_usd: number | null;
  unrealized_pnl_usd: number | null;    // bar-close mark
  realized_today_usd: number | null;
  drawdown_pct: number | null;
  daily_loss_limit_remaining_usd: number | null;  // anchored to initial capital
  risk_veto_count: number;          // from live_run_state (executor-maintained); sourced from risk vetoes
                                    //   recorded as supervisor_notes(role='risk', content LIKE 'risk veto%'),
                                    //   NOT eval_decisions. "since last here" delta computed client-side (jlm).
};
```

Every capital-risk field is **nullable** — null when `live_run_state` is absent (just-started run) → UI shows "—", never faked.

### 4.3 Endpoint surface

| Route | Router | Purpose |
|---|---|---|
| `GET /api/live/deployments` | `readonly_router` | list live runs as `LiveDeploymentSummary[]` (default `status=running`; `?status=` filter) |
| `GET /api/live/deployments/:id` | `readonly_router` | one deployment (`:id` = run_id), any status |
| `GET /api/live/deployments/:id/stream` | `readonly_router` | SSE — see below |

- **Router placement:** all three are **read-only**, mounted in `readonly_router` (matching `GET /api/live/venue-account` at `server.rs:233`), behind the outer `auth_middleware`. They are NOT in `mutating_router` and must not bypass both routers.
- **SSE is a NEW route, not a modification.** `:id/stream` is a *new* handler at the new path that **reuses the existing `RunEventBus` broadcast infra** (`chart.rs:1328`, per-`run_id` `broadcast::Receiver`) and adds a new `LiveRunState` SSE event variant emitted on each `live_run_state` upsert. The existing `/api/eval/runs/:id/stream` handler is **not** modified (it serves backtests too).
- **List filtering:** `eval/store.rs ListFilter` has no `mode` field — extend it (or add a dedicated `LiveRunStore` query) so the filter is `WHERE mode='live'` **in SQL**, never an O(n) app-level filter over the full runs table.
- Backtest runs **must never** appear here (`mode='live'` filter is the contract boundary).

### 4.4 Prerequisite code fixes (in scope, land before/with the contract)

1. **Persist `venue_label` at run creation.** Fix `eval_runs.create()` (`store.rs:158-192`) to write `venue_label` from `live_config.venue_label` (default `'paper'` for backtests). Without this every testnet run is mislabeled `paper` and §5.3 honesty is unachievable. Backfill is unnecessary (no users; existing rows are disposable per project policy).
2. **Denormalize `strategy_name`.** At run start, resolve `live_config.strategy_id` → strategy store `display_name` (`strategies/manifest.rs` `PublicManifest.display_name`) and write it into `live_run_state`. Avoids a per-request filesystem read; keeps the API a single join.
3. **Extend `ListFilter` with `mode`** (or a `LiveRunStore`) per §4.3.

> Risk-config note: `daily_loss_kill_pct` lives in the strategy risk config. If any new `[section]` is added to `risk.toml`, mirror it in the `xvision-core` parser (see the project memory on `risk.toml` dual-crate parsing). This contract only *reads* the existing field, so no config change is expected.

## 5. UI consumption (impeccable — design SERVES the product)

The strips are **follow-on implementation plans**; this section sets their design bar and acceptance. Grounded in the project's existing strip system (`DeployReadinessStrip`, `HomeOutcomeStrip`): full-width inline strips, `Card` primitive, ink ramp, semantic tokens, **glyph + color never color-alone**, null-on-empty.

### 5.1 Layout & placement (CLAUDE.md hard rules)

- **Full-width stacked strips only** (QA30). No `grid-cols-12` 8/4 split, no fourth column / right-side card (the chat rail owns the right edge).
- **No popups / modals / sheets / overlays.** Stop-confirm, deployment detail → route or inline-expand, never a dialog.
- Capital-risk strip placement within the home stack is **decided in the strip plan** (explicit deferral, not a gap).

### 5.2 Color-coding (verified-contrast tokens, paired with glyph + label)

Risk semantics use the real token palette. **Color is never the sole signal** — each carries a glyph and a literal value:

| Signal | gold (healthy) | warn (caution) | danger (breach) |
|---|---|---|---|
| Daily-loss buffer (`daily_loss_remaining / (kill_pct·capital)`) | > 50% left | ≤ 25% left | ≤ 0 (breached / paused) |
| Drawdown (`drawdown_pct`) | < 5% | ≥ 5% and < 15% | ≥ 15% |
| Running P&L (`unrealized + realized_today`) | ≥ 0 (`▲`) | — | < 0 (`▼`) |

**Contrast (corrected — my round-1 "all ≥4.5:1" claim was false):** measured on the card surface — `--warn` light **5.02:1** (PASS), `--danger` light **4.83:1** (PASS), `--gold` light **3.36:1 (FAILS AA for normal text)**; all pass in dark mode. Therefore the healthy-state **`--gold` color must not be the sole carrier of meaning in light mode**: the glyph (`▲`/`✓`) plus the literal value label carries it, and the strip plan adds a light-mode override `--gold-accessible-light: #007a48` (≥4.5:1 on white) for any text-gold that conveys status. (The pre-existing `DeployReadinessStrip` carries the same false "≥4.5:1 both themes" comment across ~14 usages — broader token remediation is a separate track; this spec must not re-certify the claim.)

Thresholds live in a pure selector (`features/live/deployment-risk.ts`) with unit tests against these exact breakpoints — components stay presentational.

### 5.3 Honesty (non-negotiable)

- The simulated-vs-real discriminator is **`venue_label`** (`paper`/`testnet`), **not** `mode` (which is always `"live"`). Every row/strip is **explicitly labeled `paper` or `testnet` (simulated)** — never "live," "real money," or bare currency without the simulated qualifier. A frontend type-guard on `mode` would be a bug.
- **API-layer defense:** the list/detail handlers **reject/exclude any `venue_label='live'` row** (impossible today, but defends against a future leak), and this is covered by an honesty test (§7) — not left to the UI alone.
- Null capital-risk values render as "—", never `$0.00` or an inferred number.
- `deployed_capital` (config) and `equity` (book-computed) are presented as distinct, never blended into one vanity number.

### 5.4 Motion

- Value updates use the existing `xvn-num-pop` via a `key={value}` remount on the changed cell only — not a whole-strip re-entrance. Reduced-motion is globally collapsed (`globals.css`), and `key={value}` keeps that catch-all effective (no ref-based transition).
- No reveal that gates content visibility on a class-triggered transition.

### 5.5 AI-slop avoidance

- No hero-metric template (big number + gradient). No identical card grid of deployments — a compact row list. No side-stripe borders for risk state (use glyph + tinted `--*-bg` fill or a full border). The per-metric `Cell` label (10px uppercase over a mono value) is the established home idiom and is **fine** — the eyebrow ban targets per-*section* headers, not metric labels.

### 5.6 Per-bead acceptance

- **`n0k`** — Active-tasks renders live rows from `GET /api/live/deployments?status=running`: `venue_label` pill (paper/testnet), `last_decision_at` (relative), running unrealized P&L (glyph + sign + simulated label).
- **`8s4`** — Capital-risk strip: `deployed_capital`, `drawdown_pct`, `daily_loss_remaining` (each color+glyph+value per §5.2), simulated-labeled.
- **`awm`** — ETA from `live_config.stop_policy` + elapsed (**only when a real limit exists**); Stop/flatten via the existing auth-gated `POST /api/eval/runs/:id/flatten` (sets `flatten_requested`, column exists, migration 063); runaway >24h warning from `started_at`; uses `status` + `paused` to render the right control.

## 6. Terminology lock additions (before merge)

Per §7.2 + the 2026-05-27 lock, add lock-doc rows in `docs/superpowers/specs/2026-05-27-autooptimizer-terminology-lock.md` for: "deployment" (operator surface for a live/paper run), "running P&L", "deployed capital", "daily-loss buffer", "simulated" (paper/testnet qualifier). Developer-surface names stay precise (`live_run_state`, `LiveDeploymentSummary`, `deployment_id`). The follow-on plan template carries this as an explicit checklist item. Note: `ContextScope::Deployment.deployment_id` (`chat_session/context.rs:23`) already uses `deployment_id`; confirm the chat-rail `/live/:id` scope id aligns with `deployment_id = run_id`.

## 7. Testing (TDD)

**Rust:**
- Migration test (table + CASCADE).
- `LiveStateStore` upsert/read (insert, update-in-place, null fields, `risk_veto_count` increment).
- **`venue_label` persistence:** a testnet live run serializes `venue_label='testnet'` end-to-end (regression for the §4.4 fix).
- **Extend the existing 21 live-loop integration tests:** assert a `live_run_state` row appears after N bars with correct `equity_usd` (= `book.equity`), `realized_today_usd`, and `daily_loss_remaining_usd` (= `kill_pct·initial + realized_today`); assert daily-loss resets across a day boundary; assert `risk_veto_count` increments when a veto is recorded (drive a veto, assert the `supervisor_notes` veto row AND `live_run_state.risk_veto_count==1`).
- API handler: list filters `mode='live'` in SQL (a backtest run is absent); detail join; null-safety when state absent; `:id` 404 for unknown.
- SSE: `:id/stream` emits a `LiveRunState` frame on upsert; run-A subscriber gets no run-B events.

**Honesty tests (gate):**
- A backtest run does **not** appear in `/api/live/deployments`.
- A `venue_label='live'` row (forced via direct insert) is **excluded** from the list/detail response.
- No field carries an inferred money value when `live_run_state` is null.

**Frontend (follow-on plans):** `deployment-risk.ts` selector unit tests (the exact §5.2 breakpoints, incl. the breached-buffer and `drawdown ≥15%` cases); strip rendering against a mock `LiveDeploymentSummary` incl. the all-null just-started case; a `venue_label='testnet'` row renders the simulated label.

## 8. Scope boundary & deferred work

**In this spec/plan:** the §4.4 prerequisites (`venue_label` INSERT fix, `strategy_name` denormalization, `ListFilter.mode`); `live_run_state` table + `LiveStateStore` + executor write seam (incl. `risk_veto_count` increment); `LiveDeploymentSummary` type + the three read-only routes + SSE; tests.

**Follow-on implementation plans (unblocked by this contract):** `n0k`, `8s4`, `awm` strips (§5.6); broader `--gold` light-contrast token remediation across existing strips.

**Deferred to CT5-real:** `AlpacaLiveSurface` implementation; remove `VenueLabel::Live` rejection; real-money safety/risk review; optional broker-truth equity reconciliation (a separate async poll, never in the hot loop); persisted cross-day daily-loss history. (`8wn` budget cap is its own bead.)

## 9. Acceptance (contract unblock, per §8.9)

- [ ] `GET /api/live/deployments` returns only actual paper/testnet live runs (`mode='live'`, `venue_label≠'live'`), filtered in SQL.
- [ ] `GET /api/live/deployments/:id/stream` streams decision/PnL/risk updates over the shared `RunEventBus`.
- [ ] Dashboard distinguishes paper/testnet via `venue_label` without inferring from `agent_runs`, and the `venue_label` is correctly persisted at run creation.
- [ ] Capital-risk values are per-run book-computed (honest), with `daily_loss_remaining` anchored to initial capital; `risk_veto_count` sourced from risk-veto `supervisor_notes`.
- [ ] New UI labels added to the terminology lock before merge.
- [ ] `--gold` light-mode contrast claim corrected; healthy state legible via glyph+label (not color alone).
