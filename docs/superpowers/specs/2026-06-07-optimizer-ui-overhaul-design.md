# Optimizer UI & observability overhaul — design

**Date:** 2026-06-07
**Status:** Approved design (spec only; implementation plan to follow)
**Surfaces:** `crates/xvision-engine/src/autooptimizer/`, `crates/xvision-dashboard/`, `frontend/web/src/features/autooptimizer/`
**Terminology:** governed by `docs/superpowers/specs/2026-05-27-autooptimizer-terminology-lock.md`; new rows required (§8).

## 1. Problem

Operator feedback on the Optimizer surface (2026-06-07):

1. **No signs of life.** Once a run starts the UI shows "Running" with no activity until
   an experiment completes. Two root causes, confirmed in code:
   - `deriveCycleState()` in `LiveCycleView.tsx` infers run state from SSE events
     received *since page load*. A refresh mid-run shows "No cycle running" and hides
     the existing F28 Cancel button. There is no run-status endpoint.
   - Event granularity is coarse: between `mutation_proposed` and `mutation_gated`
     sits the entire expensive phase (writer LLM call, day-window backtest,
     untouched-window backtest, reverse-mutation check, reviewer) with zero events.
2. **Experiments are "kept and lost" with no visible why.** The experiment page
   (`/optimizer/experiment/:hash`) exists but is thin. The evidence mostly exists and
   is not surfaced: writer rationale (`MutationDiff.rationale`, blob store), gate
   numbers (`delta_day`, `delta_holdout`, epsilon, baseline vs candidate), structured
   gate reason. Judge findings are SSE-only; `GET /findings/:hash` returns `[]`.
3. **No charts.** Tables and cards only. `uplot` is the house chart library (eval-runs).
4. **No controls beyond start.** Cancel exists (F28) but is hidden by (1). No
   pause/resume, no continuous mode, no scheduling ("Next: No scheduled run" is
   hardcoded).
5. **Cluttered IA.** Launch config, live telemetry, and history compete on one page.

## 2. Goals

- Run state survives page refresh and is visible at a glance (status endpoint + replayable feed).
- Continuous narrated activity during a run via phase-level events.
- An "experiment researcher" page answering: why tested, what happened, the numbers, the decision, the reviewer's notes.
- Charts showing the optimizer making progress: improvement curve, outcome mix, spend, writer ladder.
- Standard controls: start (with mode), pause, resume, cancel; plus an evening-run schedule.
- IA: mission control home, run detail page, enriched experiment page.

## 3. Non-goals

- No rename of any locked dev-surface term; no contact with the DSPy `optimize`/`Optimizer*` namespace.
- No changes to gate semantics, mutation logic, attestation/seal chain, or eval engine.
- No CLI parity work in this wave (`xvn optimizer` gains nothing here; noted as follow-up).
- No mobile-specific layouts beyond existing responsive behavior.

## 4. Backend design

### 4.1 First-class run entity ("Run" / `OptimizerSession`)

The backend already mints a `session_id` (ULID) per run for the attestation
`SessionCommitment`. We promote it to a queryable entity rather than inventing a
parallel id. New engine-DB table:

```sql
CREATE TABLE autooptimizer_session_state (
  session_id        TEXT PRIMARY KEY,          -- existing attestation ULID
  strategy_id       TEXT NOT NULL,
  config_json       TEXT NOT NULL,             -- models, budget, windows, mode
  state             TEXT NOT NULL,             -- queued|running|paused|cancelling|cancelled|finished|failed
  mode              TEXT NOT NULL,             -- once | n_experiments | until_budget
  cycles_planned    INTEGER,                   -- N for n_experiments; NULL otherwise
  cycles_completed  INTEGER NOT NULL DEFAULT 0,
  kept_count        INTEGER NOT NULL DEFAULT 0,
  suspect_count     INTEGER NOT NULL DEFAULT 0,
  dropped_count     INTEGER NOT NULL DEFAULT 0,
  error             TEXT,
  created_at        TEXT NOT NULL,
  started_at        TEXT,
  finished_at       TEXT
);
CREATE INDEX idx_aoss_state ON autooptimizer_session_state(state);
CREATE INDEX idx_aoss_created ON autooptimizer_session_state(created_at);
```

State machine: `queued → running ⇄ paused → finished | cancelled | failed`.
`cancelling` is a transient state set when the cancel flag is raised.

Pause is a cooperative flag mirroring the F28 cancel flag, registered per session in
`AppState` and checked at the same safe checkpoints: before the next candidate within
a cycle, and between cycles. While paused the loop polls the flag (1s interval), the
cost ticker suspends, and `SessionStateChanged` is emitted on each transition.

At most one session may be `running|paused` at a time (existing single-run lock
extends to sessions). Crash recovery: on dashboard startup, any session left in
`running|paused|cancelling|queued` is marked `failed` with `error = "interrupted"`.

### 4.2 Continuous mode

`mode` in session config:

- `once` — one propose → gate → commit (today's behavior; `evening-cycle` POST becomes a thin wrapper creating a `once` session).
- `n_experiments` — loop cycles until `cycles_completed == cycles_planned`.
- `until_budget` — loop until the budget cap is exhausted (budget required for this mode).

All modes additionally stop on cancel and on the existing sustained-no-pass loosening
logic reaching its floor. Each loop iteration is a normal cycle with its own
`cycle_id`, so all existing cycle-scoped tables, seals, and pages apply unchanged.

### 4.3 Phase-level progress events

Additive `CycleProgressEvent` variants (no breaking changes; existing consumers read
unknown kinds as no-ops):

```rust
PhaseStarted  { session_id, cycle_id, parent_hash: Option<String>, phase: Phase, detail: String }
PhaseFinished { session_id, cycle_id, parent_hash: Option<String>, phase: Phase, duration_ms: u64 }
SessionStateChanged { session_id, state: String }
```

`Phase` enum (wire snake_case): `writer_proposing`, `eval_day_window`,
`eval_untouched_window`, `reverse_check`, `gate_evaluating`, `reviewer_running`,
`honesty_check`. Operator labels via the existing `autooptimizer_labels.rs` map:
"Writer drafting experiment", "Backtesting today's window", "Backtesting untouched
period", "Reverse-mutation check", "Applying decision gate", "Reviewer writing
notes", "Honesty check". Existing events gain a `session_id` field (additive,
`#[serde(default)]`).

### 4.4 Event persistence + replay

Every progress event is appended to:

```sql
CREATE TABLE autooptimizer_events (
  seq         INTEGER PRIMARY KEY AUTOINCREMENT,
  session_id  TEXT NOT NULL,
  cycle_id    TEXT,
  kind        TEXT NOT NULL,
  payload_json TEXT NOT NULL,
  ts          TEXT NOT NULL
);
CREATE INDEX idx_aoe_session ON autooptimizer_events(session_id);
```

The SSE endpoint (`GET /api/autooptimizer/events`) honors `Last-Event-ID` (and
`?since_seq=` as a query fallback) by replaying rows with `seq > N` before switching
to the live broadcast; each SSE frame carries `id: <seq>`. A page refresh therefore
reconstructs the full feed instead of starting blind. Retention: on session start,
prune events belonging to all but the most recent 50 sessions.

### 4.5 Decision-evidence persistence

Two gaps closed, keyed by `bundle_hash`:

```sql
CREATE TABLE autooptimizer_findings (
  id           INTEGER PRIMARY KEY AUTOINCREMENT,
  bundle_hash  TEXT NOT NULL,
  severity     TEXT NOT NULL,      -- info|warn|risk
  code         TEXT NOT NULL,
  summary      TEXT NOT NULL,
  detail       TEXT,
  model        TEXT,
  created_at   TEXT NOT NULL
);
CREATE INDEX idx_aof_hash ON autooptimizer_findings(bundle_hash);

CREATE TABLE autooptimizer_gate_records (
  bundle_hash           TEXT PRIMARY KEY,
  parent_day_score      REAL, child_day_score      REAL,
  parent_holdout_score  REAL, child_holdout_score  REAL,
  gate_epsilon          REAL,
  delta_day             REAL, delta_holdout        REAL,
  drawdown_ratio        REAL,
  verdict               TEXT NOT NULL,             -- kept|suspect|dropped
  reason                TEXT,                      -- structured failure reason
  created_at            TEXT NOT NULL
);
```

Findings are written at emit time (same place `JudgeFinding` is broadcast);
`GET /findings/:hash` stops returning `[]`. Gate records are written where the gate
verdict is computed. Writer rationale needs no new storage — `MutationDiff` is
already in the blob store via `diff_hash`; the detail endpoint joins it in.

### 4.6 Scheduling

```sql
CREATE TABLE autooptimizer_schedules (
  id           INTEGER PRIMARY KEY AUTOINCREMENT,
  enabled      INTEGER NOT NULL DEFAULT 1,
  time_local   TEXT NOT NULL,        -- "HH:MM", dashboard-host local time
  strategy_id  TEXT NOT NULL,
  config_json  TEXT NOT NULL,        -- same shape as session config
  last_run_at  TEXT,
  next_run_at  TEXT
);
```

A tokio ticker (60s) checks enabled schedules and creates a session when due,
skipping (and logging a persisted event) if a session is already active. v1 supports
daily schedules only; one schedule per strategy.

### 4.7 API surface

New/changed routes (all under `/api/autooptimizer/`):

| Route | Method | Purpose |
|---|---|---|
| `/sessions` | POST | Start a run. Body: strategy_id, mode, cycles_planned?, budget_usd?, models, windows. 202 + session_id. |
| `/sessions` | GET | List sessions (limit/offset), newest first. |
| `/sessions/:id` | GET | Session detail: state, config, counts, cost, cycle_ids. |
| `/sessions/:id/pause` | POST | Raise pause flag. 200 if accepted, 409 if not running. |
| `/sessions/:id/resume` | POST | Clear pause flag. 409 if not paused. |
| `/sessions/:id/cancel` | POST | Raise cancel flag (works from running or paused). |
| `/status` | GET | `{ active_session: SessionSummary \| null, last_event_seq }` — cheap poll for nav badge + refresh-proof page state. |
| `/experiments/:hash/detail` | GET | One call: lineage node + MutationDiff (incl. rationale) + gate record + findings + regime results. |
| `/stats` | GET | Per-cycle aggregates for charts: `{ cycle_id, session_id, ts, kept, suspect, dropped, best_delta_holdout, cost_usd, cum_cost_usd }[]`, filter `?strategy_id=&since=`. |
| `/schedule` | GET/POST/DELETE | Read/upsert/remove the schedule. |
| `/events` | GET (SSE) | Unchanged path; gains `id:` frames + `Last-Event-ID` replay (§4.4). |

Existing routes unchanged. `POST /evening-cycle` and `POST /cycles/:id/cancel` remain
as compatibility wrappers over sessions.

## 5. Frontend design

All pages single-column (no `grid-cols-12` sidebars), no popups, operator-surface
labels throughout. Routes:

### 5.1 `/optimizer` — mission control

Top to bottom:

1. **Status hero** — driven by `GET /status` (poll 5s while active) + SSE. Run state
   pill, headline (`Run <id> · <strategy> · <mode>`), controls row (Start scrolls to
   configure; Pause/Resume/Cancel shown per state machine), and during a run the
   **phase stepper**: the seven phases as a horizontal chip strip, completed phases
   dimmed, current phase highlighted with elapsed seconds. The existing live-spend
   strip merges into the hero. Because state comes from the server, refresh-proof.
2. **Improvement hero chart** (uplot) — best kept Δ untouched-period score per cycle
   from `/stats`, optionally filtered by strategy. Empty state explains what will
   appear after the first kept experiment.
3. **Configure & run** — existing launch form (strategy, budget, windows, models)
   in a collapsed inline accordion, plus the **mode picker**: Single experiment /
   N experiments (count input) / Until budget (budget required).
4. **Recent runs** — MListCard: state, strategy, mode, experiments, kept (gold),
   cost, finished-at; rows link to `/optimizer/run/:id`.
5. **Next scheduled run strip** — real data from `/schedule`, with inline enable/
   disable and edit (time, strategy). Replaces the hardcoded "No scheduled run".

The genealogy and ladder tabs remain reachable; the Active-lineages card grid moves
to the genealogy tab to declutter the home page.

### 5.2 `/optimizer/run/:sessionId` — run detail (new)

Works for live and historical sessions:

1. Header: state pill + controls + config chips (strategy, writer model, reviewer
   model, mode, budget, experiments count, spend `$x / $cap`).
2. Phase stepper for the in-flight experiment (live sessions only).
3. **Activity feed** — replayed from `autooptimizer_events` on load (complete after
   refresh), then live via SSE. Rows: time, operator label, target hash link,
   duration for phase rows ("Backtesting untouched period · 42s"). Auto-scroll with
   a "jump to latest" affordance when scrolled up.
4. Session charts (uplot): cumulative spend vs budget cap; kept/suspect/dropped
   stacked per cycle.
5. Experiments table for the session: hash, kind pill, outcome badge, Δ day,
   Δ untouched, reviewer-note count; rows link to the experiment page.

`/optimizer/cycle/:cycleId` remains for cycle-scoped views; run pages link to their
cycles.

### 5.3 `/optimizer/experiment/:hash` — experiment researcher page (enriched)

Backed by the single `/experiments/:hash/detail` call. Sections in reading order:

1. **Why tested** — writer rationale (`MutationDiff.rationale`) + the diff itself
   (existing ParentDiffPanel, moved here).
2. **What happened** — phase timeline with durations, derived from persisted events
   for this experiment's cycle.
3. **The numbers** — gate scorecard: paired horizontal bars (baseline vs candidate)
   for today's window and the untouched period, with the minimum-improvement
   threshold drawn as a marker line; drawdown-ratio readout.
4. **Decision** — Kept/Suspect/Dropped badge + the structured reason rendered in
   plain language (e.g. "untouched-period score improved 0.04, below minimum
   improvement 0.10").
5. **Reviewer notes** — persisted findings (severity, code, summary, detail, model).
6. Existing regime cards and lineage links. The two `EmptyPanel` placeholders
   (flight recorder, sign-off receipts) stay, below the new sections.

### 5.4 Charts

uplot everywhere, styled to match eval-runs:

| Chart | Where | Data |
|---|---|---|
| Improvement curve (best kept Δ untouched per cycle) | Home hero; per-run variant on run page | `/stats` |
| Kept/suspect/dropped stacked per cycle | Run page; aggregate on home behind a toggle | `/stats` |
| Cumulative spend vs budget cap | Run page | `/stats` + session config |
| Writer ladder pass-rate over time | Ladder tab (replaces text-only panel; table stays below) | `/ladder` |

### 5.5 Client state rules

- Run state always derives from `/status` reconciled with SSE — never from
  in-memory event history alone. `deriveCycleState()` is deleted.
- Controls are optimistic with reconciliation: clicking Pause shows "Pausing…" until
  `SessionStateChanged` or the next `/status` poll confirms.
- The feed component requests replay with `Last-Event-ID` on (re)connect.

## 5.6 Flywheel / prompt-tuning surfacing (DSPy bridge)

The autooptimizer already feeds the pre-existing DSPy prompt optimizer via
`autooptimizer/dspy_flywheel.rs` (cycle findings → memory observations → compile at
`dspy_pattern_cohort_threshold` → prompt Pattern). DSPy has its own dashboard
surface (`/api/optimizations`, `optimizations-detail` page). We surface the bridge
as linked summaries only — never merged UI, and never the word "DSPy"/"optimize" on
the autooptimizer surface (terminology firewall):

1. **Flywheel progress strip** (mission control, below the improvement chart):
   "Observations toward next prompt compile: N/threshold · M patterns compiled."
   Data: new `GET /api/autooptimizer/flywheel` returning cohort count, threshold,
   compiled-pattern count, and the latest optimization run id. Hidden when
   `dspy_enabled = false`.
2. **Prompt-tuning gate metric** (same strip): latest memory-demo gate result from
   `agent_slot_optimizations` — "Last prompt compile: dev +0.06, untouched +0.04 ·
   kept" — linking to the existing `/optimizations/:id` detail page.
3. **Feed event**: new persisted event kind `flywheel_compiled { session_id,
   cycle_id, optimization_run_id, pattern_id }`, operator label "Findings compiled
   into prompt pattern", rendered in the activity feed with a link to
   `/optimizations/:id`.

Operator-surface labels: "Prompt tuning" / "prompt compile" / "pattern". Lock-doc
rows added in §8. Phasing: the feed event lands in P1 (one more event kind); the
strip + metric land in P3 alongside the other stats work.

## 6. Testing

Per repo policy (TDD mandatory; `.coverage-thresholds.json` is the blocking gate):

- **Rust:** session state-machine unit tests (all legal/illegal transitions, crash
  recovery marking); cooperative pause/cancel flag checkpoints; event append +
  replay-from-seq; findings/gate-record writes; schedule ticker due/skip logic;
  route tests for every new endpoint including 409 paths, the back-compat
  wrappers, and `/flywheel` (incl. the `dspy_enabled = false` hidden case).
- **Frontend:** status-driven hero state incl. refresh-mid-run (mock `/status`);
  feed replay + live append + lag handling; phase stepper transitions; gate
  scorecard rendering from a fixture detail payload; controls
  optimistic/reconciliation behavior; chart components smoke-tested with fixture
  series.

## 7. Rollout phasing

Each phase is independently shippable and reviewable:

1. **P1 — Signs of life:** session entity (mode `once` only) + `/status` +
   phase events + event persistence/replay + home status hero, phase stepper, and
   replayable feed; `/optimizer/run/:id` ships in reduced form (header + feed only).
   Fixes complaints 1 and the worst of 5.
2. **P2 — Evidence:** findings + gate-record persistence + `/experiments/:hash/detail`
   + experiment researcher page. Fixes complaint 2.
3. **P3 — Charts:** `/stats` + the four uplot charts. Fixes complaint 3.
4. **P4 — Controls:** pause/resume + continuous modes + mode picker + full run
   detail page (controls, session charts, experiments table). Fixes complaint 4.
5. **P5 — Scheduling:** schedule table/ticker/API + home strip.

## 8. Terminology lock additions

New rows to append to `2026-05-27-autooptimizer-terminology-lock.md`:

| Dev-surface | Operator-surface |
|---|---|
| `OptimizerSession` / `autooptimizer_session_state` | **Run** |
| `state: paused` / pause flag | **Paused / Pause / Resume** |
| `mode: once` | **Single experiment** |
| `mode: n_experiments` | **N experiments** |
| `mode: until_budget` | **Until budget** |
| `PhaseStarted/PhaseFinished` | (phase labels below) |
| `writer_proposing` | **Writer drafting experiment** |
| `eval_day_window` | **Backtesting today's window** |
| `eval_untouched_window` | **Backtesting untouched period** |
| `reverse_check` | **Reverse-mutation check** |
| `gate_evaluating` | **Applying decision gate** |
| `reviewer_running` | **Reviewer writing notes** |
| `autooptimizer_schedules` | **Scheduled run** |
| `flywheel_compiled` event | **Findings compiled into prompt pattern** |
| dspy_flywheel surface (strip) | **Prompt tuning** |

The codename remains `autooptimizer`; nothing here touches the DSPy
`optimize`/`Optimizer*` namespace.

## 9. Decisions taken (so they don't reopen)

- Reuse the attestation `session_id` as run identity; no parallel id. "Session" the
  attestation concept is unrenamed; the operator-facing word for the new entity is
  "Run".
- Event replay via persisted table + `Last-Event-ID`, not client-side caching.
- One active session at a time (extends the existing single-run lock).
- Schedules are daily-only, one per strategy, host-local time, in v1.
- `evening-cycle` and `cycles/:id/cancel` routes stay as wrappers (no breaking API
  change pre-launch, but no reason to break them either).
- Charts read from `/stats` server aggregates, not client-side joins over lineage.
