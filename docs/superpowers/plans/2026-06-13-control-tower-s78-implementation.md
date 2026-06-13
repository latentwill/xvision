# Epic xvision-s78 — Control Tower Dashboard Gaps: Unified Execution Plan

This plan merges nine per-bead implementation plans into one ordered build. The
governing constraint is the **honesty mandate** (§8.1/§8.9): no live-money,
P&L, capital, or budget number may be fabricated from `agent_runs` or eval
summaries. Everything live is gated behind the **CT5 LiveDeploymentSummary
contract** (`GET /api/live/deployments` + `/:id/stream`), which does not exist
anywhere in `crates/` today.

Sequencing follows the operator's intent: resolve nsk → ship frontend-pure wins
(n0k-blocked) → 008 time filters → backend-field beads → 8s4 live SSE last.

---

## Wave 0 — Reconcile the single live home (nsk)

### xvision-nsk — Single home for the at-a-glance live picture
- **Decision:** RECONCILE-VERIFICATION bead, not a fresh removal. **KEEP** `LiveSummaryStrip` as the honest aggregate count strip; designate `ActiveTasksStrip` as the future home for per-RUN live/paper rows (n0k). The spec's literal `LiveStrategiesSection.tsx` / `routes/live-list.tsx` deletions are already done (commits `c1b5bd85` + `4cdb672c`).
- **Files:**
  - `frontend/web/src/routes/home.tsx` — verify-only (no `LiveStrategiesSection` import exists).
  - `frontend/web/src/components/home/AttentionBand.tsx` — verify-only; `LiveSummaryStrip` (line 42) + `ActiveTasksStrip` (line 43) + `CriticalFindingsRow` (line 44) coexist in the `divide-y` stack. No code change.
  - `frontend/web/src/components/home/LiveSummaryStrip.tsx` — add a header-comment boundary note: "aggregate count strip; per-run live ROWS live in ActiveTasksStrip (n0k/CT5)".
  - `frontend/web/src/components/home/ActiveTasksStrip.tsx` — add the matching boundary note.
  - `frontend/web/src/routes/home.test.tsx` — add the missing `expect(screen.queryByText(/Live strategies/i)).toBeNull()` (lines 144-145 already cover `/Real money/i` and `/active live deployments/i`).
- **Backend gap:** none for the reconcile. Per-run live rows are blocked on CT5 (gates n0k, not nsk).
- **UI approach:** no new UI. `LiveSummaryStrip` already complies (full-width in AttentionBand center column, divide-y hairlines, token colors, designed "No live strategies running." empty state).
- **Tests:** add `/Live strategies/i` null assertion to `home.test.tsx`; keep `LiveSummaryStrip.test.tsx` as the spec lock substituting for the non-existent `LiveStrategiesSection.test.tsx`; add an `AttentionBand.test.tsx` assertion that exactly one live-owning surface exists and `ActiveTasksStrip` renders no live-money pill + real-money P&L pre-CT5.
- **Effort:** S.

---

## Wave 1 — Frontend-pure wins (zero backend) — parallelizable

### xvision-e17 — Deploy-readiness checklist
- **Decision:** Build now. Pure composition over existing endpoints. Mount as its **own slim band directly under `SafetyPauseBanner`, above `AttentionBand`** (it is a safety gate, not a nag), collapse-to-one-line ("Ready to deploy") when all checks pass.
- **Files:**
  - NEW `frontend/web/src/features/home/deploy-readiness.ts` — `buildDeployReadiness(input)` → ordered `ReadinessCheck[]` (id, label, status pass|fail|unknown, detail, link). Inputs: providers, brokers, brokerTest, safety, inflightRuns. No JSX/fetch.
  - NEW `frontend/web/src/features/home/deploy-readiness.test.ts`.
  - NEW `frontend/web/src/components/home/DeployReadinessStrip.tsx` — `data-testid='deploy-readiness-strip'`, tone-dot rows + routed fix links.
  - NEW `frontend/web/src/components/home/DeployReadinessStrip.test.tsx`.
  - EDIT `frontend/web/src/routes/home.tsx` — add `testAlpacaConnection` query (enabled only when `brokers.alpaca.configured`), reuse `getSafetyState`/`safetyKeys.state` cache.
- **Backend gap:** none. Composes `listProviders`, `getBrokers`, `testAlpacaConnection`, `getSafetyState`, `listRuns`.
- **UI approach:** full-width stacked strip, no popup. Three checks min: provider keys, broker (configured AND reachable; **broker-unreachable = explicit FAIL/deploy-blocker** per §7.1 panel 6), no-blocking-eval (not safety-paused AND no stuck >2h in-flight run). Glyphs ✓ text-gold / ✗ text-danger / — text-text-4; routed fix Link per failing check.
- **Tests:** selector pass/fail/unknown cases incl. broker-unreachable-as-blocker; component renders the literal "keys ✓ broker ✓ no blocking eval ✗", asserts no `role='dialog'`.
- **Effort:** S.

### xvision-jlm — "Since you were last here" header delta
- **Decision:** Build now. **localStorage persistence for v1** (key `xvn.home.last_visit` via existing `safeStorageGet/Set` in `lib/storage.ts`). Export `LAST_VISIT_LS` so xvision-8wn windows off the SAME key.
- **Files:**
  - NEW `frontend/web/src/features/home/last-visit.ts` — `LAST_VISIT_LS`, `readLastVisit()`, `writeLastVisit(nowIso)`, pure `computeSinceDelta({runs, findings, lastVisitIso, now})`.
  - NEW `frontend/web/src/features/home/last-visit.test.ts`.
  - NEW `frontend/web/src/components/home/HomeDeltaSubtitle.tsx` — "X runs / Y findings since you were last here · Nh ago"; neutral first-visit line.
  - NEW `frontend/web/src/components/home/HomeDeltaSubtitle.test.tsx`.
  - EDIT `frontend/web/src/routes/home.tsx` — read-before-write on mount (useRef/useEffect), pass `<HomeDeltaSubtitle>` as Topbar `sub`.
  - `frontend/web/src/components/shell/Topbar.tsx` — no change (`sub` is already ReactNode).
- **Backend gap:** none. Counts derive from already-fetched `RunSummary.completed_at` + `CriticalFinding.created_at`.
- **UI approach:** inline subtitle through Topbar `sub` slot, font-mono tabular-nums counts, `· Nh ago` in text-text-4. **No live-money phrasing** — runs/findings are eval facts.
- **Tests:** `computeSinceDelta` strict-after-lastVisit boundary, first-visit zeros + null, storage-unavailable tolerance; subtitle renders populated + neutral states; asserts no "Real money"/"Live strategies"; home read-then-write so a second mount shows the new delta.
- **Effort:** M.

### xvision-zn2 — Optimizer digest reshape (FE-derivable slices)
- **Decision:** **SPLIT** — land the three FE-derivable reshapes now (30d rolling acceptance rate, holdout deltas, client-side cost-anomaly flag from trailing-cycle median) + honesty-check documentation. **Gate ONLY the literal `$X / $Y today` budget denominator** behind the new `GET /api/autooptimizer/budget` endpoint (tracked with 8wn).
- **Files:**
  - EDIT `frontend/web/src/components/home/OptimizerDigestStrip.tsx` — add holdout-Δ segment, 30d acceptance-rate segment w/ degradation tone, cost-anomaly tint, `title`/`aria` documenting the honesty check ("a sabotaged null-result canary correctly degraded").
  - EDIT `frontend/web/src/components/home/OptimizerDigestStrip.test.tsx`.
  - EDIT `frontend/web/src/features/home/optimizer-summary.ts` — `rollingAcceptanceRate`, `bestHoldoutDelta`, `costAnomaly` selectors.
  - EDIT `frontend/web/src/features/home/optimizer-summary.test.ts`.
  - (budget-gated, deferred to 8wn wave) `frontend/web/src/features/autooptimizer/api.ts` + `crates/xvision-dashboard/src/routes/autooptimizer.rs` — `OptimizerBudget` type + `GET /api/autooptimizer/budget`.
- **Backend gap:** PARTIAL — acceptance rate / holdout deltas / cost anomaly are all served by the existing `GET /api/autooptimizer/stats?since=` (StatsRow already carries kept/suspect/dropped + best_delta_holdout + cost_usd). The ONLY gap is the persisted daily budget cap (shared with 8wn).
- **UI approach:** single full-width strip inside OptimizerPanel Card; new facts as inline ` · `-separated segments; degradation/anomaly via saturated-text tint (text-warn/text-gold), never side-stripe; honesty doc on `title`/`aria`; em-dash placeholders for missing data; "canary" never appears in visible copy.
- **Tests:** acceptance-rate from windowed fixtures + degradation tone; holdout-Δ gold/warn; cost-anomaly tone+aria; honesty `title` mentions "sabotage/null result" while visible label stays "Honesty check"; preserve all existing passing tests; selector unit tests incl. 0-denominator → null and null-walkback.
- **Effort:** M (FE slices); budget denominator rolls into 8wn.

---

## Wave 2 — Time-window filters, backend-first (008)

### xvision-008 — Time-window pills + list time filters
- **Decision:** Backend-first per §7.3. Thread a single validated `since` (RFC-3339) end-to-end on BOTH list endpoints, copying the proven `/api/autooptimizer/ladder?since=` validation ladder. FE pills default to **'All'** so first paint is unchanged.
- **Files (backend, BLOCKING):**
  - `crates/xvision-dashboard/src/routes/eval_runs.rs` — add `since: Option<String>` to `ListParams`, parse/validate RFC-3339 → `DashboardError::Validation` on bad input.
  - `crates/xvision-engine/src/api/eval.rs` — add `since: Option<DateTime<Utc>>` to `ListRunsRequest`, map into `ListFilter`.
  - `crates/xvision-engine/src/eval/store.rs` — add `since` to `ListFilter`; push `started_at >= ?` into `list()` + `count()` WHERE (RFC-3339 string compare is lexicographically correct).
  - `crates/xvision-dashboard/src/routes/agent_runs.rs` — add `since` to `ListAgentRunsParams`; `WHERE ar.started_at >= ?` (or Rust post-filter alongside the existing status pass — lowest-risk).
- **Files (frontend):**
  - NEW `frontend/web/src/components/home/TimeWindowPills.tsx` — 4 controlled pills (Today/7d/30d/All), `data-testid='time-window-pills'`.
  - NEW `frontend/web/src/features/home/time-window.ts` — `TimeWindow`, `windowStartIso`, `filterRunsByWindow` (keys off `completed_at ?? started_at`).
  - EDIT `frontend/web/src/api/eval.ts` — add `since?` to `ListRunsParams`, `buildRunsListUrl`, AND `evalKeys.runs` cache key.
  - EDIT `frontend/web/src/api/agent-runs.ts` — add `since?` to params + `agentRunKeys.list` key.
  - EDIT `frontend/web/src/routes/home.tsx` — `useState<TimeWindow>('all')`, render pills inline under `HomeOutcomeStrip` (single column, no grid), pass windowed runs into `HomeOutcomeStrip` + `AttentionBand`.
  - EDIT `frontend/web/src/components/home/HomeOutcomeStrip.tsx` + `AttentionBand.tsx` — consume the windowed runs.
- **Backend gap:** REAL/blocking for the historical-window guarantee (`hasTimeFilter:false` on both endpoints). The home outcomes panel can be windowed client-side from the already-fetched 100-run page immediately, but the spec's historical-window acceptance is only met once the list endpoints filter server-side.
- **UI approach:** inline pill row, no popup/dropdown, no 4th column, no 8/4 grid. Active pill = low-opacity tone fill + saturated text ≥4.5:1; `aria-pressed`; `data-testid='time-window-pills'`.
- **Tests:** `time-window.test.ts` boundary inclusivity + `completed_at`-then-`started_at` keying; `TimeWindowPills.test.tsx` exactly 4 pills, aria-pressed, onChange, no Dialog/Sheet/Popover; home integration narrowing; backend store-level test `ListFilter.since` drops out-of-window rows + dashboard `?since=<bad>` → 400.
- **Effort:** L.

---

## Wave 3 — Backend-field beads (need new columns/endpoints) — n0k unblocks awm runaway warning

### xvision-n0k — Live/paper rows in ActiveTasksStrip
- **Decision:** Add live/paper rows sourced EXCLUSIVELY from the new CT5 `LiveDeploymentSummary` contract (never `agent_runs`/`RunSummary`). **Data path: LIVE via SSE for per-row P&L/last-decision; short-interval poll (~5s) for list membership** — mirrors how `api/agent-runs.ts` splits list-poll from per-run SSE. Gate the entire live block: pre-CT5, render NO live rows and behave exactly as today.
- **Files (backend, BLOCKING — CT5):**
  - NEW `crates/xvision-engine/src/api/live.rs` — `LiveDeploymentSummary` + `GET /api/live/deployments` + `GET /api/live/deployments/:id/stream`, sourced from broker/execution state.
  - `docs/superpowers/specs/2026-05-27-autooptimizer-terminology-lock.md` — add operator-label rows ("running P&L"/"unrealized P&L"/"last decision") before merge.
- **Files (frontend, gated on CT5):**
  - NEW `frontend/web/src/api/live-deployments.ts` — typed fetcher + query keys + `openLiveDeploymentStream(:id)` SSE.
  - NEW `frontend/web/src/api/types.gen/LiveDeploymentSummary.ts` (ts-rs generated).
  - NEW `frontend/web/src/features/live/deployment-status.ts` — active filter, P&L formatting/sign, last-decision relative time selectors.
  - EDIT `frontend/web/src/components/home/ActiveTasksStrip.tsx` — add `LiveDeploymentRow`; header "N in flight" count includes live rows.
  - EDIT `frontend/web/src/components/home/ActiveTasksStrip.test.tsx`.
- **Backend gap:** BLOCKING. No `LiveDeploymentSummary`, no `/api/live/deployments`, no SSE stream. `RunSummary` lacks `last_decision_at` + realized/unrealized P&L; `agent_runs` are trace records, not deployments.
- **UI approach:** `LiveDeploymentRow` reuses `RunRow` layout; mode Pill (info 'live' / neutral 'paper', never inferred); unrealized P&L mono tabular-nums (text-gold ≥0, text-danger <0, em-dash for null); last-decision relative time. No popups; pre-CT5 renders nothing (no permanent empty card).
- **Tests (TDD, red first):** live-row mode/last-decision/P&L; paper vs live distinct; P&L sign colors + null em-dash; NO live rows when endpoint unavailable; live values come from deployment fetcher not `listRuns`; `deployment-status.test.ts` selectors.
- **Effort:** L.

### xvision-awm — Cancel-gate, real ETA, runaway-live warning
- **Decision:** Three sub-items at different readiness. (A) ETA/PROGRESS: keep honest behavior (real elapsed only; render ETA only from a real progress fraction, never fabricated) — shippable now as a test-locked invariant. (B) CANCEL gating: blocked on backend `RunSummary.source`; interim keep cancel as-is + upgrade the line-93 TODO to the spec comment. When source lands, **default Cancel SHOWN when source is absent/undefined; HIDE only on explicit `source === 'optimizer'`** (legacy human runs predate the migration and must stay cancellable). (C) RUNAWAY-LIVE >24h: amber pill on LIVE rows — gated behind n0k/CT5.
- **Files (backend, BLOCKING for cancel-gate):**
  - `crates/xvision-engine/src/api/eval.rs` — add `source`/`queued_by` to `RunSummary` (near line 235) + `summarise()`.
  - `crates/xvision-engine/migrations/` — NEW migration adding `eval_runs.source` ('human' via `POST /api/eval/runs` vs 'optimizer' via the cycle path); follow the **cycle-migration skill**.
- **Files (frontend):**
  - EDIT `frontend/web/src/components/home/ActiveTasksStrip.tsx` — gate Cancel; real ETA only when real progress present; runaway >24h warning on live rows (after n0k).
  - EDIT `frontend/web/src/components/home/ActiveTasksStrip.test.tsx`.
  - `frontend/web/src/api/types.gen/RunSummary.ts` — regenerated to add `source`.
- **Backend gap:** BLOCKING for cancel-gate (`hasSourceField:false`; eval.rs only has unrelated retry `source_run_id`). Runaway-live (C) additionally blocked on CT5. ETA/progress (A) and the existing >2h stuck-queue warning need NO backend work.
- **UI approach:** no new primitives. Real ETA as existing text-text-3 font-mono meta ("42% · ~3m"); absent progress renders nothing. Cancel stays the bordered text button. Runaway warning reuses the stuck-pill pattern (amber bg-amber-500/15, "⚠ running >24h").
- **Tests (TDD):** ETA absent for elapsed-only run; Cancel shown on human-source / absent-source, hidden on optimizer-source; runaway >24h shown/hidden; preserve >2h stuck spec. Backend-gated cases written but skipped/xfail until migration + CT5 land.
- **Effort:** M.

### xvision-1zs — Failed-run infra-vs-suspicious split *(buildable in Wave 1; placed here to share `ActiveTasksStrip`/findings context — see Decisions)*
- **Decision:** Build now (string-pattern classification works against today's wire shape). **Merge suspicious failed runs INTO the existing `CriticalFindingsRow`** (the singular "findings surface"), ordered after human-reviewed criticals. **'Stale' = failed runs older than 2 hours** (named const, reuses `formatRelativeTime` from `pulse.ts`).
- **Files:**
  - NEW `frontend/web/src/features/home/failed-runs.ts` — `classifyFailedRun`, `failedRunNags`, `failedRunFindings`, INFRA_ERROR_PATTERNS table + staleness threshold.
  - NEW `frontend/web/src/features/home/failed-runs.test.ts`.
  - EDIT `frontend/web/src/routes/home.tsx` — extend `buildAttention()` to append `failedRunNags(runs)` BEFORE the config nags (config stays lowest priority per §6).
  - EDIT `frontend/web/src/components/home/CriticalFindingsRow.tsx` — render `failedRunFindings` as danger chips alongside review findings.
  - EDIT `frontend/web/src/components/home/CriticalFindingsRow.test.tsx`, `frontend/web/src/components/home/NagStrip.test.tsx`.
- **Backend gap:** PARTIAL but NON-blocking. `status:'failed'` + `error:string` already on `RunSummary`. No structured `error_kind` discriminator exists (errors are free-form anyhow chains), so FE classifies by regex table — brittle but fully unit-testable. A clean engine-side `error_kind` enum is a separable follow-up, NOT required to ship.
- **UI approach:** infra errors flow through the unchanged `NagStrip` as `AttentionItem` rows (tone dot, routed "view run"); suspicious runs as danger chips in `CriticalFindingsRow` matching `FindingChip` exactly. Safety-pause aborts (`aborted: safety_paused`) excluded from both lists.
- **Tests (TDD):** `classifyFailedRun` infra vs suspicious table; staleness boundary via injected `nowMs`; excludes safety_paused; CriticalFindingsRow renders suspicious chip + routes, excludes infra; NagStrip renders infra item with config items after it.
- **Effort:** M.

---

## Wave 4 — Cross-source cost rollup (8wn)

### xvision-8wn — Cost rollup vs budget
- **Decision:** **HOLD the strip until the backend rollup + persisted cap land** (mark backend-blocked). A page-truncated client estimate is the exact vanity total the scope ruling rejected. Depend on **xvision-jlm's `LAST_VISIT_LS` localStorage helper** for the "since you were last here" boundary — do NOT invent a parallel mechanism.
- **Files (backend, BLOCKING):**
  - NEW cost-rollup route in `crates/xvision-engine/src/api/` — `GET /api/cost/rollup?window=since_last_visit|this_week` returning `{window, optimizer_usd, eval_usd, agent_run_usd, total_usd, budget_cap_usd|null, unpriced_calls}`. Time-windowed `SUM(model_calls.cost_usd)` over eval_runs + agent_runs + `SUM(cycle_cost.cost_usd)`.
  - NEW migration in `crates/xvision-core/migrations/` — persisted budget cap (shared with zn2's budget denominator). Use **cycle-migration skill** + terminology lock.
- **Files (frontend):**
  - NEW `frontend/web/src/api/cost.ts` — fetcher + query keys.
  - NEW `frontend/web/src/components/home/CostRollupStrip.tsx` + `.test.tsx`.
  - EDIT `frontend/web/src/routes/home.tsx` — mount + wire last-visit boundary.
  - EDIT `docs/.../2026-05-27-autooptimizer-terminology-lock.md` — operator labels ("Spend since last visit", "This week", "Budget cap", "over budget").
- **Backend gap:** BLOCKING and substantial — no cross-source aggregation exists (only `session_cost_usd` over one session's cycles), no persisted budget cap (`budget_usd` is request-time only), no since-timestamp query.
- **UI approach:** full-width stacked strip, big tabular-mono "$spend / $cap" per window; anomaly = tone shift on the spend numeral (text-warn near / text-danger over); `formatSpendUsd` renders "—" for unknown; null cap shows "no cap set" + route, never a fake denominator.
- **Tests:** both windows render; over-cap danger flag; null cap "no cap set"; unknown total "—" not "$0.00"; no `role='dialog'`; backend Rust tests for windowing + three-source SUM + unpriced handling (isolated `CARGO_TARGET_DIR` per stale-rmeta memory note).
- **Effort:** L.

---

## Wave 5 — Live capital-risk strip (8s4) — last

### xvision-8s4 — Capital-risk strip (deployed capital, drawdown, daily-loss buffer)
- **Decision:** Fully backend-blocked; do NOT start frontend until the CT5 contract lands. **Data path resolved = LIVE via SSE** (operator-resolved per §7.4(1)). Reconcile the no-snapshot-tables rule by allowing minimal in-memory/per-session peak + day-start-equity tracking inside the execution layer (execution state, not a persisted snapshot product). Home strip shows an **AGGREGATE** (total deployed capital, worst/aggregate drawdown, tightest daily-loss buffer) and routes to `/live` for per-deployment detail.
- **Files (backend, BLOCKING — the gating dependency):**
  - NEW live-deployments route in `crates/xvision-engine/src/api/` (shares the CT5 `LiveDeploymentSummary` contract with n0k) — `GET /api/live/deployments` + `/:id/stream` exposing `deployed_capital_usd`, `drawdown_pct`, `daily_loss_limit_remaining_usd`, realized/unrealized P&L, mode, status — from broker/execution state (`crates/xvision-execution/src/alpaca.rs` account/positions and/or Orderly), NEVER `agent_runs`. New engine logic to compute drawdown (peak tracking) + daily-loss buffer (vs `daily_loss_kill_pct`).
- **Files (frontend, gated on CT5):**
  - EDIT/NEW `frontend/web/src/api/live.ts` — `LiveDeploymentSummary` DTO + `getLiveDeployments()` + SSE consumer (`useLiveDeploymentsStream`).
  - NEW `frontend/web/src/features/home/capital-risk.ts` + `.test.ts` — aggregate + per-metric `RiskTone` (drawdown reddens; daily-loss buffer amber→red vs `daily_loss_kill_pct`).
  - NEW `frontend/web/src/components/home/CapitalRiskStrip.tsx` + `.test.tsx`.
  - EDIT `frontend/web/src/routes/home.tsx` — mount in CT5 position; show no live-capital copy until CT5 unblocked (§8.10).
  - EDIT terminology-lock doc — "Deployed capital", "Drawdown", "Daily-loss buffer".
- **Backend gap:** BLOCKING. No `/api/portfolio`; existing `/api/live/venue-account` is Orderly-only, polled (not SSE), lacks deployed-capital/drawdown/daily-loss-buffer.
- **UI approach:** full-width stacked strip, three large tabular-mono KPIs; color = signal only on the numeral; designed "No live capital deployed" empty state (first-class, not an apologetic em-dash); null → "—" never fabricated 0; freshness "updated {relative}" + live/paper chip; SSE rows animate mount-only with reduced-motion fallback.
- **Tests (written against the contract while backend lands):** selector aggregation + tone thresholds + null→"—"; component three color-coded KPIs, empty state, live/paper chip, no `role='dialog'`; backend Rust integration tests asserting values derive from broker/execution state not `agent_runs` (isolated `CARGO_TARGET_DIR`); `home.test.tsx` asserts strip absent until CT5, present after (lines 306-307 already assert no `/PnL/i` / `/deployed capital/i` pre-CT5).
- **Effort:** L.

---

## Decisions made (operator declined to choose)

1. **nsk single home (RESOLVED):** KEEP `LiveSummaryStrip` (honest aggregate count strip) as the at-a-glance live picture; `ActiveTasksStrip` becomes the home for per-RUN live/paper rows once CT5 lands. They answer different questions (how many are live? vs. what is each doing now?) and are not duplicates. The spec's literal `LiveStrategiesSection`/`live-list` deletions are already done (commits `c1b5bd85` + `4cdb672c`); the only remaining acceptance gap is the `/Live strategies/i` test assertion. Boundary documented in both component headers + a terminology-lock note so no future agent re-introduces a duplicate live-row list.

2. **jlm last-visit persistence (RESOLVED):** localStorage for v1 (key `xvn.home.last_visit` via existing `safeStorageGet/Set`). Lightest, ships now, no server column. Windows are per-operator/per-browser, not a server fact. `LAST_VISIT_LS` exported so 8wn's cost rollup shares the exact same boundary — the home delta subtitle and the cost strip must agree.

3. **8s4 data path (RESOLVED = operator pre-resolved):** LIVE via SSE. No persisted snapshot tables; minimal in-memory peak + day-start-equity tracking lives in the execution layer (execution state, not a snapshot product).

4. **Cross-cutting — CT5 is the single live contract:** `LiveDeploymentSummary` (`GET /api/live/deployments` + `/:id/stream`) is built ONCE and shared by n0k (rows), awm (runaway warning), and 8s4 (capital aggregate). Build it once in Wave 3; 8s4's strip consumes the same route. No live number is ever faked from `agent_runs`.

5. **Cross-cutting — persisted budget cap is shared:** zn2's `$X / $Y today` denominator and 8wn's `$spend / $cap` rollup both need the same persisted daily-budget config. Build the cap migration + `GET /api/autooptimizer/budget` (or the unified `/api/cost/rollup` cap field) once; zn2 ships its FE-derivable slices first and upgrades the cost denominator when the cap lands.

6. **Cross-cutting — `since` filter pattern:** 008 copies the validated `/api/autooptimizer/ladder?since=<rfc3339>` ladder (DashboardError::Validation on bad input) verbatim for both list endpoints. RFC-3339 lexicographic compare is correct because `started_at` is stored as an RFC-3339 string.

7. **awm absent-source default (RESOLVED):** Cancel SHOWN when `source` is absent/undefined; HIDE only on explicit `source === 'optimizer'`. Protects legacy human-queued runs (the pre-migration majority) from losing the control.

8. **1zs findings home + staleness (RESOLVED):** merge suspicious failed runs into the existing `CriticalFindingsRow` (no new sibling strip); staleness threshold = 2 hours (named const).
