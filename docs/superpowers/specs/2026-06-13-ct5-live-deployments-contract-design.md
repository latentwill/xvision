# CT5 — Live-deployments contract (design)

- **Date:** 2026-06-13
- **Status:** Design — passed design-review gate (round 2: 5/5 dimensions, no FAIL); round-2 concerns folded in; pending user approval
- **Epic:** `xvision-s78` (Control Tower) → unblocks `n0k`, `awm`, `8s4` (and `8wn`'s live-cost inputs)
- **Supersedes:** `docs/superpowers/specs/2026-06-07-control-tower-dashboard-evaluation.md` §8.9 (CT5), **including line 765** ("Capital-risk values come from broker/execution state, not eval summaries") — see §3.
- **Register:** product UI (design SERVES the product) — impeccable principles applied to the strip section.

## 1. Problem

The Control Tower home cannot honestly show "what's trading right now." Three beads are gated on this:

- `n0k` — Active-tasks live/paper rows (mode, last decision, running P&L)
- `8s4` — Capital-risk strip (deployed capital, drawdown, daily-loss-limit buffer)
- `awm` — ETA + Stop/cancel-gating + runaway >24h warning for live runs

The 2026-06-07 evaluation deferred all three behind a "live-trading backend contract" assumed not to exist.

## 2. Current state (verified against `main`, 2026-06-13)

Live execution **already exists and runs** — the gating assumption was wrong:

- `xvn eval run --mode live` (and `POST /api/eval/runs`) drives a real `run_inner_live` loop in `crates/xvision-engine/src/eval/executor/backtest.rs` against **Alpaca paper** + **Orderly testnet/mainnet**.
- A live run **is an `eval_run`** with `mode='live'` (`RunMode::Live`, serializes `"live"` for *every* live run incl. paper). No separate deployments table.
- **Capital-risk is tracked per-run by the internal `PortfolioBook`**, not the broker. The hot loop computes equity via `book.mark(asset, bar.close)` + `book.equity(&marks)` (`backtest.rs:3347-3349`); `broker.balance()` is **never** called in the per-bar path. Book equity is per-run and honest; the broker account snapshot is account-wide and not attributable to one run.
- Real money is the only blocked part: `VenueLabel::Live` is rejected by `LiveConfig::validate()` (`live_config.rs:231`, tested), `AlpacaLiveSurface` is a stub. Every live run today is **paper or testnet (simulated)**.
- **Known persistence bug (prerequisite, §4.4):** `eval_runs.create()` (`store.rs:158-192`) does **not** write `venue_label`; the column defaults to `'paper'` (migration 031). Testnet runs are currently mislabeled `paper` in the DB column (the JSON read path returns the right value, but the SQL filter column is wrong). The honesty contract (§5.3) needs this fixed.
- The daily-loss accumulator (`daily_loss_day`, `daily_realized_at_day_start`) is **in-memory only** — invisible to the dashboard, reset on restart.

**The missing seam:** nothing joins `run_id` → its live capital-risk state. There is no typed live-deployments resource.

## 3. Scope decision & §8.9 supersession

**Paper/testnet read layer now, forward-compatible shape.** Build over live runs that already execute. Defer real-money to a **CT5-real** follow-up. `venue_label` distinguishes paper/testnet/live, so the API shape won't need a revision when real-money lands.

**Supersession of §8.9 line 765.** The 2026-06-07 spec required "capital-risk values come from broker/execution state, not eval summaries." This spec **supersedes that specific line**: capital-risk values are **per-run `PortfolioBook`-computed**, *not* broker-state, because `broker.balance()` is account-wide and cannot be attributed to a single run when multiple runs share a broker account. Book values are also not eval summaries — they are the live run's own accounting ledger. Broker-truth reconciliation (a separate async poll) is deferred to CT5-real.

## 4. Architecture — "a deployment *is* a live eval-run + a volatile state row"

Two thin seams; no parallel runtime, no second lifecycle:

1. **Identity reuse.** `deployment_id == run_id`. Listing = query `eval_runs WHERE mode='live'`, enriched.
2. **One new volatile table, `live_run_state`** — exactly one upserted row per live run, written by the executor each bar; carries the capital-risk snapshot plus a denormalized strategy name and a monotonic veto counter, so the API is a single join.

The API joins `eval_runs` (identity/status) ⨝ `live_run_state` → `LiveDeploymentSummary`.

**Rejected alternative:** a separate `deployments` table referencing runs — a second lifecycle to keep in sync for zero benefit while real-money doesn't exist. Premature.

### 4.1 `live_run_state` schema

A new migration under `crates/xvision-engine/migrations/`. **Claim the next available number at branch-creation time** (064 is current highest as of 2026-06-13, but other tracks run concurrently — do not hard-lock a number in the plan).

```sql
CREATE TABLE live_run_state (
  run_id                   TEXT PRIMARY KEY REFERENCES eval_runs(id) ON DELETE CASCADE,
  strategy_id              TEXT,            -- denormalized from live_config at run start (Strategy artifact id)
  strategy_name            TEXT,            -- denormalized display name at run start (§4.4)
  deployed_capital_usd     REAL NOT NULL,   -- static, = scenario.capital.initial (the run's configured capital)
  equity_usd               REAL,            -- BOOK-computed: book.equity(&marks) (NOT broker.balance — see notes)
  unrealized_pnl_usd       REAL,            -- BOOK-computed from bar-close marks (NOT a live broker quote)
  realized_pnl_usd         REAL,            -- book.realized() since run start
  realized_today_usd       REAL,            -- book.realized() − daily_realized_at_day_start (in-memory accum)
  daily_loss_remaining_usd REAL,            -- (daily_loss_kill_pct · deployed_capital_usd) + realized_today,
                                            --   clamped ≥ 0 (0 = breached). Anchored to INITIAL capital, per
                                            --   backtest.rs:3668-3677. NOT current equity.
  drawdown_pct             REAL,            -- (peak_equity − equity)/peak_equity
  peak_equity_usd          REAL,            -- running max
  risk_veto_count          INTEGER NOT NULL DEFAULT 0,  -- server-maintained monotonic counter (see notes)
  last_decision_at         TEXT,            -- RFC-3339
  updated_at               TEXT NOT NULL    -- freshness stamp
);
```

**Value provenance (locked):**

- `equity_usd`, `unrealized_pnl_usd`, `realized_*` are **per-run `PortfolioBook`-computed** (not broker-truth — see §3). `unrealized_pnl_usd` is a **bar-close mark**. In a multi-asset run, `book.equity(&marks)` reprices the *arriving* bar's asset at its close and falls back to each other open leg's stored `last_mark` — i.e. a per-arriving-bar **partial-marks** snapshot (the correct pooled-NAV formula), not a simultaneous all-asset mark. Honest, but note it for any future audit.
- `daily_loss_remaining_usd` = `daily_loss_kill_pct` (`strategy.risk.daily_loss_kill_pct`, `strategies/risk.rs:13`) × `deployed_capital_usd` (initial), plus `realized_today`. **Restart hazard:** the in-memory `daily_realized_at_day_start` resets to 0 on process restart, so after a within-day restart this value is stale until the next day boundary rolls the accumulator. Pre-existing; persisted cross-day history is CT5-real. The strip should treat this as "resets on restart," not authoritative across restarts.
- `risk_veto_count` is a **server-maintained monotonic integer**: `run_inner_live` keeps an in-memory counter, increments it on each risk veto (the same point it calls `record_supervisor_note(role='risk', …)`), and writes it to this column each bar. **There is no per-bar COUNT query** over `supervisor_notes` — that table remains the human-readable veto record; this column is the cheap counter. (Round-1 wrongly sourced this from `eval_decisions`, which has no verdict column — corrected.)
- `deployed_capital_usd` (config) is presented as distinct from `equity_usd`, never blended.

**Writer:** `LiveStateStore::upsert`, from `run_inner_live` once per bar after the decision/fill. Note: `eval_equity_samples` are **batch-flushed at loop end** (`record_equity_upsert_batch`), so this is the **first true per-bar DB write** in the live loop — expect a small per-bar latency add; don't benchmark against a non-existent per-bar write. `strategy_id`/`strategy_name`/`deployed_capital_usd` written once at run start.

**Lifecycle:** terminal upsert on stop/fail; row retained until run deletion (CASCADE). List filters `status='running'`; detail works for any status.

### 4.2 `LiveDeploymentSummary` type

Rust type, ts-rs → `types.gen`. Corrected to the actual wire format; `mode` dropped (every row is `"live"` by construction — it carried no information and invited a paper-as-real-money type-guard footgun):

```ts
type LiveDeploymentSummary = {
  deployment_id: string;            // = run_id
  strategy_id: string | null;
  strategy_name: string | null;     // denormalized; null until first upsert
  venue_label: "paper" | "testnet"; // THE simulated-vs-real discriminator. 'live' is validation-rejected today
                                    //   and excluded from this response (§5.3). // CT5-real will add 'live' here.
  status: "queued" | "running" | "completed" | "failed" | "cancelled";  // actual RunStatus
  paused: boolean;                  // separate bool on Run; awm distinguishes paused-but-running
  started_at: string;
  last_decision_at: string | null;
  deployed_capital_usd: number | null;
  equity_usd: number | null;            // per-run book-computed (§3/§4.1)
  realized_pnl_usd: number | null;
  unrealized_pnl_usd: number | null;    // bar-close mark
  realized_today_usd: number | null;
  drawdown_pct: number | null;
  daily_loss_limit_remaining_usd: number | null;  // anchored to initial capital
  risk_veto_count: number;          // server-maintained monotonic counter (§4.1). The client computes a
                                    //   "since last here" DISPLAY delta from two snapshots (jlm boundary);
                                    //   it never counts supervisor_notes rows itself.
};
```

Every capital-risk field is **nullable** — null when `live_run_state` is absent (just-started run) → UI shows "—", never faked.

### 4.3 Endpoint surface

| Route | Router | Purpose |
|---|---|---|
| `GET /api/live/deployments` | `readonly_router` | list live runs as `LiveDeploymentSummary[]` (default `status=running`; `?status=`) |
| `GET /api/live/deployments/:id` | `readonly_router` | one deployment (`:id` = run_id), any status |
| `GET /api/live/deployments/:id/stream` | `readonly_router` | SSE — new route, shared infra (below) |

- **Router & auth:** all three are read-only in `readonly_router` (matching `GET /api/live/venue-account`, `server.rs:233`), behind the outer `auth_middleware`. Do **not** add `require_auth_middleware` (that lives only in `mutating_router`). Honest caveat: on **loopback binds** (default local/single-user) `auth_middleware` is a no-op, so these capital/PnL routes are reachable without a token by any local process — consistent with every other readonly route, acceptable for the single-user product.
- **SSE is a NEW route, not a modification.** `:id/stream` is a new handler that **reuses the existing `RunEventBus` broadcast infra** (`chart.rs:1328`, per-`run_id` `broadcast::Receiver`, cross-run isolated) and adds a `LiveRunState` SSE event variant emitted on each upsert. The existing `/api/eval/runs/:id/stream` handler is **not** modified.
- **List filtering in SQL.** `eval/store.rs ListFilter` has no `mode` field — extend it (or add `LiveRunStore`) so filtering is `WHERE mode='live'` in SQL, never an O(n) app-level filter.
- Backtest runs **must never** appear here.

### 4.4 Prerequisite code fixes (in scope, land with the contract)

1. **Persist `venue_label` at run creation.** Fix `eval_runs.create()` (`store.rs:158-192`) to write `venue_label` from `live_config.venue_label` (default `'paper'` for backtests). Without this, testnet runs are mislabeled in the filter column and §5.3 honesty can't hold. No backfill (no users; existing rows disposable per project policy).
2. **Denormalize `strategy_name`.** At run start, resolve `live_config.strategy_id` → strategy store `PublicManifest.display_name` and write into `live_run_state`. Avoids per-request filesystem reads.
3. **Extend `ListFilter` with `mode`** (or a `LiveRunStore`) per §4.3.
4. **Add the `risk_veto_count` in-memory counter** to `run_inner_live`.

> `daily_loss_kill_pct` is an existing strategy-risk field this contract only *reads*; no `risk.toml` change is expected. (If one were ever added, mirror the new `[section]` in the `xvision-core` parser per the dual-crate-parsing project memory.)

## 5. UI consumption (impeccable — design SERVES the product)

Strips are **follow-on plans**; this sets their bar + acceptance. Grounded in the existing strip system (`DeployReadinessStrip`, `HomeOutcomeStrip`): full-width inline strips, `Card`, ink ramp, semantic tokens, **glyph + color never color-alone**, null-on-empty.

### 5.1 Layout & placement (CLAUDE.md hard rules)

- **Full-width stacked strips only** (QA30). No `grid-cols-12` 8/4 split, no fourth column / right-side card (chat rail owns the right edge).
- **No popups / modals / sheets / overlays.** Stop-confirm, detail → route or inline-expand.
- Capital-risk strip placement within the home stack is **decided in the strip plan** (explicit deferral).

### 5.2 Color-coding (verified-contrast tokens, paired with glyph + label)

**Color is never the sole signal** — each carries a glyph and a literal value:

| Signal | gold (healthy) | neutral (no tone) | warn (caution) | danger (breach) |
|---|---|---|---|---|
| Daily-loss buffer (`daily_loss_remaining / (kill_pct·capital)`) | > 50% left | 25–50% left (healthy but decaying) | ≤ 25% (and > 0) | ≤ 0 (breached / paused) |
| Drawdown (`drawdown_pct`) | < 5% | — | ≥ 5% and < 15% | ≥ 15% |
| Running P&L (`unrealized + realized_today`) | ≥ 0 (`▲`) | — | — | < 0 (`▼`) |

- **Running P&L is binary** (green/red, no caution tier — no meaningful midpoint).
- **Contrast (corrected):** on the card surface — `--warn` light **5.02:1** (PASS), `--danger` light **4.83:1** (PASS), `--gold` light **3.36:1 (FAILS AA for normal text)**; all pass in dark mode. So `--gold` must not be the sole carrier in light mode: the glyph + literal value label carries it, and **light-mode status text uses the existing `var(--gold-soft)` (`#007a48`, 5.42:1 on white)** — already in `tokens.css`; do **not** introduce a new token. (The pre-existing `DeployReadinessStrip` carries the same false "≥4.5:1 both themes" comment across ~14 usages — broader remediation is a separate track; this spec does not re-certify the claim.)

Thresholds live in a pure selector (`features/live/deployment-risk.ts`) with unit tests against these exact breakpoints (including the 25–50% midband and `drawdown ≥15%`) — components stay presentational.

### 5.3 Honesty (non-negotiable)

- The simulated-vs-real discriminator is **`venue_label`** (`paper`/`testnet`). There is no `mode` field. Every row/strip is **explicitly labeled `paper` or `testnet` (simulated)** — never "live"/"real money"/bare currency.
- **API-layer defense:** list/detail handlers **exclude any `venue_label='live'` row** (impossible today; defends a future leak), covered by an honesty test (§7).
- Null capital-risk values render "—", never `$0.00` or inferred.
- `deployed_capital` (config) and `equity` (book-computed) are presented distinct, never blended.

### 5.4 Motion

- Value updates use the existing `xvn-num-pop` via `key={value}` remount on the changed cell only — not a whole-strip re-entrance. Reduced-motion is globally collapsed (`globals.css`); `key={value}` keeps that catch-all effective (no ref-based transition). (Assumes ≥1-minute bars; sub-minute would thrash — a CT5-real concern.)

### 5.5 AI-slop avoidance

- No hero-metric template. No identical card grid — a compact row list. No side-stripe borders (use glyph + tinted `--*-bg` or a full border). The per-metric `Cell` label (10px uppercase over a mono value) is the established home idiom and is fine — the eyebrow ban targets per-*section* headers.

### 5.6 Per-bead acceptance

- **`n0k`** — Active-tasks live rows from `GET /api/live/deployments?status=running`: **reuse the existing `VenueBadge`** (`components/primitives/VenueBadge.tsx`) for the paper/testnet pill, `last_decision_at` (relative), running unrealized P&L (glyph + sign + simulated label). **Also reconcile the existing `LiveSummaryStrip`** aggregate count, which derives "N live" from `is_live_money` (`agent_runs.rs:99-106`, `mode='live'` only — ignores `venue_label`) and would otherwise still show paper runs as "live" (pre-existing `xvision-9pi`; CT5 makes it more visible).
- **`8s4`** — Capital-risk strip: `deployed_capital`, `drawdown_pct`, `daily_loss_remaining` (each color+glyph+value per §5.2), simulated-labeled.
- **`awm`** — ETA from `live_config.stop_policy` + elapsed (**only when a real limit exists**); Stop/flatten via the existing auth-gated `POST /api/eval/runs/:id/flatten` (sets `flatten_requested`, migration 063) — the UI must **guard the button to `mode='live'` runs** (the route itself doesn't validate mode); runaway >24h warning from `started_at`; uses `status` + `paused` for the right control.

## 6. Terminology lock additions (before merge)

New operator-facing labels — "deployment" (a live/paper run), "running P&L", "deployed capital", "daily-loss buffer", "simulated" (paper/testnet qualifier) — belong to the **live-trading surface**, NOT the autooptimizer domain. Add them to a **new live-trading terminology lock section/doc**, not `2026-05-27-autooptimizer-terminology-lock.md` (avoid conceptual bleed). Developer-surface names stay precise (`live_run_state`, `LiveDeploymentSummary`, `deployment_id`). Note: `live_run_state.strategy_id` is the **Strategy artifact id** (not a marketplace `agent_id` ULID) — label it so a future rename sweep doesn't mislabel it. `ContextScope::Deployment.deployment_id` (`chat_session/context.rs:23`) already uses `deployment_id`; confirm the chat-rail `/live/:id` scope id aligns with `deployment_id = run_id`.

## 7. Testing (TDD)

**Rust:**
- Migration test (table + CASCADE).
- `LiveStateStore` upsert/read (insert, update-in-place, null fields, `risk_veto_count` increment).
- **`venue_label` persistence (regression for §4.4):** a testnet live run has `venue_label='testnet'` **in the DB column AND in the API response** — assert both, not just JSON.
- **Extend the 21 live-loop integration tests:** after N bars a `live_run_state` row has correct `equity_usd` (= `book.equity`), `realized_today_usd`, `daily_loss_remaining_usd` (= `kill_pct·initial + realized_today`); daily-loss resets across a day boundary; `risk_veto_count` increments when a veto fires (assert the `supervisor_notes` veto row **and** `live_run_state.risk_veto_count==1`).
- API handler: list filters `mode='live'` in SQL (backtest absent); detail join; null-safety; `:id` 404.
- SSE: `:id/stream` emits a `LiveRunState` frame on upsert; run-A subscriber gets no run-B events.

**Honesty tests (gate):** a backtest run is absent from `/api/live/deployments`; a forced `venue_label='live'` row is excluded from list/detail; no field carries an inferred money value when `live_run_state` is null.

**Frontend (follow-on):** `deployment-risk.ts` selector unit tests (exact §5.2 breakpoints incl. the 25–50% midband and `drawdown ≥15%`); strip rendering against a mock incl. the all-null just-started case; a `venue_label='testnet'` row renders the simulated label via `VenueBadge`.

## 8. Scope boundary, deliverables & deferred work

**In this spec/plan:** §4.4 prerequisites (`venue_label` INSERT fix, `strategy_name` denormalization, `ListFilter.mode`, `risk_veto_count` counter); `live_run_state` table + `LiveStateStore` + executor write seam; `LiveDeploymentSummary` + three read-only routes + SSE; tests. **Governance deliverables in the same PR:** add `team/OWNERSHIP.md` rows for the touched files (`backtest.rs` — currently unclaimed per `CONFLICT_ZONES.md:25`; `eval/store.rs`; `api/eval.rs`; `dashboard/server.rs`; new migration; new store/route files) and add the §6 live-trading terminology rows. When the contract merges, **update the `xvision-8s4` bead description** (its "no `/api/portfolio`" blocker is resolved via the book-computed seam, a different mechanism than the bead anticipated).

**Follow-on implementation plans (unblocked):** `n0k`, `8s4`, `awm` strips (§5.6); broader `--gold` light-contrast remediation across existing strips.

**Deferred to CT5-real:** `AlpacaLiveSurface` impl; remove `VenueLabel::Live` rejection; real-money safety/risk review; broker-truth equity reconciliation (separate async poll, never in the hot loop); persisted cross-day daily-loss history. (`8wn` budget cap is its own bead.)

## 9. Acceptance (contract unblock, per §8.9 as superseded in §3)

- [ ] `GET /api/live/deployments` returns only paper/testnet live runs (`mode='live'`, `venue_label≠'live'`), filtered in SQL.
- [ ] `GET /api/live/deployments/:id/stream` streams decision/PnL/risk updates over the shared `RunEventBus`.
- [ ] Dashboard distinguishes paper/testnet via `venue_label`, correctly persisted at run creation (DB column + response agree).
- [ ] Capital-risk values are per-run book-computed (honest), `daily_loss_remaining` anchored to initial capital, `risk_veto_count` from the server-maintained counter.
- [ ] New UI labels added to a live-trading terminology lock (not the autooptimizer lock) before merge; OWNERSHIP.md rows added.
- [ ] `--gold` light-mode contrast corrected; healthy state legible via glyph+label using existing `--gold-soft`.
