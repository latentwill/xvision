# Control Tower dashboard evaluation — design vs implementation vs available value

**Date:** 2026-06-07
**Inputs:** `docs/design/v1-xvisionrender/01-control-tower-*` + `docs/design/gptprompts-v1.md` §1 (the design source of truth), `frontend/web/src/routes/home.tsx` (current implementation), full backend surface inventory (autooptimizer, marketplace, memory/flywheel, eval, agent-runs, safety, optimizations).
**Status:** approved spec; superseded for execution by §8 (2026-06-09). Passed 3-reviewer adversarial gate §7; operator decisions resolved §7.4. §8 reconciles the later reachability audit, PR #878, D-series design sweep ownership, and the live-trading gate.

## 1. The designed Control Tower (gptprompts-v1.md §1, 2026-05-10)

Two-column 8/4 grid. Organizing principle: **"what should I look at first?"** for a returning operator.

| Panel | Content | Key affordance |
|---|---|---|
| Header strip | "Home" + delta subtitle ("5 drafts · 12 runs since you were last here · 18 hours ago") | "Open setup agent" primary CTA, "New strategy" ghost |
| Time-window pills | Today / 7d / 30d / All scoping all panels | — |
| Needs attention (left) | Severity-ranked: perf drop on a strategy, critical eval finding, **proactive agent suggestion** ("3 of your last 5 evals failed in chop — want a chop-aware variant?") | "Open ↗", "Ask the agent →" |
| Eval queue (left) | Queued + running runs with progress %, ETA | Cancel, Open |
| Recent findings (right) | Finding kind + summary + strategy chip | **"Draft variant from this →"** (finding → action loop) |
| In your workshop (right) | Draft strategies with status pills (Eval-ready / Draft / Validating) | "Open in Inspector →" |
| Lineage teaser | "You've forked btc-momentum 4× this week — see lineage →" | Coming-soon chip |

Explicitly out of scope in the design: live-deployments rollup / live cockpit links (deferred at the time).

## 2. The current home page (`routes/home.tsx`)

| Panel | Content |
|---|---|
| CountCard ×3 | Strategy / agent / provider counts + links |
| AttentionCard | Failed eval runs, missing provider keys, unconfigured brokers |
| RecentRunsCard | Last 5 eval runs (Sharpe, return, status) |
| ControlChartCard | Chart snapshot of latest chartable run |

## 3. Gap A — design panels missing from the implementation

The current page is a **setup-completeness panel**; the design is an **operations re-entry panel**. Missing:

1. **Real "needs attention".** Current attention items are config nags (keys, brokers) plus failed runs. The design's items are *operational*: performance regressions, critical findings, and agent-generated pattern observations. All the data exists (`eval_findings`, `eval_reviews` severity, run metrics) — none of it feeds the card.
2. **Eval queue with live progress.** `GET /api/eval/runs?status=queued|running` + the per-run SSE stream exist; the home page shows neither queue depth nor progress/ETA/cancel.
3. **Recent findings with "Draft variant from this".** The judge findings pipeline (`ReviewFinding`, `FindingCard`) is fully built on eval-run detail pages but absent from home. The design's finding→variant action — the core authoring flywheel loop — has no home-page entry point.
4. **Workshop drafts panel.** No surface for in-progress drafts with Eval-ready / Validating status pills.
5. **"Since you were last here" delta subtitle.** No last-visit tracking; header is static.
6. **Time-window scoping** (Today/7d/30d/All).
7. **Proactive agent insight row** wired to the chat rail ("Ask the agent →" seeding a rail session). The rail exists and persists per scope; nothing seeds it from attention items.
8. **Lineage teaser.** Lineage is no longer "coming soon" — it shipped (`/marketplace/lineage/:name`, `optimization_lineage`) — but home links to none of it.

Only items genuinely covered today: recent-runs list (≈ a weaker findings/queue hybrid) and the chart snapshot (not in the design at all, but defensible).

## 4. Gap B — high-value surfaces missing from BOTH design and implementation

The design is dated 2026-05-10. The autooptimizer (landed ~2026-06-01), memory/flywheel, marketplace, safety, and live agent-runs all post-date it. A returning operator's real question set has expanded from "what do my evals say" to "what did my agents do overnight, what's awaiting my approval, what's making money, what's it costing".

### 4.1 Optimizer overnight digest (highest value, fully API-backed)

"What did the Optimizer do last night?" is exactly the control-tower question and has zero surface outside `/optimizer`:

- Last cycle: experiments proposed / kept / dropped (gate verdict distribution), from `/api/autooptimizer/cycles`.
- **Honesty check outcome** (null-result canary) — a trust signal that belongs at the top level.
- Cycle cost (tokens + USD, `/api/autooptimizer/cycles/:id/cost`) and budget-cap proximity.
- Suspect (quarantined) lineage nodes count — these are literally "needs attention" items.
- Experiment-writer accuracy trend (ladder endpoint).

A compact "Last night's run: 12 experiments, 2 kept, 1 suspect, honesty check passed, $4.10" strip with a link to the cycle detail would surface the entire subsystem's value in one row.

### 4.2 Pending operator decisions (approval queue) — CUT (operator decision 2026-06-07)

Rejected by the operator. Rationale: memory promotion runs in the background and shouldn't demand attention; the Optimizer does its own work autonomously; and minting is a deliberate, operator-initiated choice per preferred strategy — live money must never be funneled through a dashboard "approve" queue that nudges minting every strategy. No aggregated approval panel on home.

### 4.3 Things in motion: live/paper runs

The design explicitly deferred this; it's no longer deferred — `/live/:id`, `agent_runs`, and the RunEvent SSE stream exist. A returning operator needs: active runs count, mode (live/paper), unrealized P&L, last decision timestamp, risk-gate vetoes since last visit. The "Live" nav link is already conditional on an active deployment; home should mirror that state instead of hiding it.

### 4.4 Safety state

`GET /api/safety/state` — if writes are paused, that is the first thing the dashboard should say, above everything else. Currently only a topbar pill. A paused system with a cheerful "all good" dashboard is a correctness bug, not a styling gap.

### 4.5 Marketplace / lineage

Frontend routes exist (browse, leaderboard, lineage, creator) but home has no bridge: forks-of-your-agents count (the design's teaser, now buildable), leaderboard movement for published agents, mint pipeline status. On-chain buy/sell events are still stubbed (Phase 3/5) — keep this panel thin until the adapter is real; don't design panels for mock data.

### 4.6 Flywheel velocity — CUT (operator decision 2026-06-07)

Rejected: a "???" vanity metric. Activity counts (observations captured, patterns promoted) measure motion, not outcomes. Home surfaces only **real, verified success/failure metrics** — PnL, win %, eval pass/fail, per-strategy and per-agent success rates. Flywheel internals stay on the per-agent flywheel page.

### 4.7 Cost rollup (cross-cutting)

Optimizer cycle cost is metered; eval runs and agent runs burn LLM tokens too. No aggregate "spend since you were last here / this week" exists anywhere. For a system that spends money autonomously overnight, this is a conspicuous absence in both design and implementation.

## 5. Frictions to resolve before implementing

1. **Layout rules conflict.** The design's 8/4 two-column grid predates the 2026-05-26 QA30 rule (no right-side boxes where the chat rail renders; default single full-width column). Home renders inside `<Layout>` with the rail. Either get an explicit exception for the home grid or restructure to full-width stacked strips. The design's ⌘K modal also predates the no-popups rule (command palette likely survives as a "native-primitive-like" exception, but it should be ruled on explicitly).
2. **Terminology drift.** The design says "drafts", "Inspector", "New strategy", nav = Setup/Strategies/Eval/Settings. Current reality: Agents are first-class (`Agent` + `AgentRef`), strategies are agent compositions, nav has ~10 more sections (Agents, Memory, Optimizer, Marketplace, Scenarios, Safety, Live, Charts, Docs). Operator-facing names on any new panels must follow the 2026-05-27 terminology lock (Experiment, honesty check, Staged/Active, Suspect/Rejected — never bare "optimizer" in code; codename stays `autooptimizer`).
3. **Wallet settings route is declared but empty** (`/settings/wallet`); live portfolio state lives in the execution layer, not dashboard tables. A positions/P&L panel needs a data path decision (proxy through daemon vs persist snapshots) before any UI work.
4. **Last-visit tracking** doesn't exist; the delta subtitle and "since you were last here" framing need a small persistence decision (localStorage vs server-side).

## 6. Recommended panel set (v3 after operator review 2026-06-07, priority order)

Operator framing: **a cockpit, not a status museum.** At a glance: system health, opportunities, active tasks. Every metric must be a real, verified outcome (success/failure), not activity or vanity. Errors are NOT front and center — they demote to a gentle nag low on the page (config nags lowest of all). Requirement: this panel set must pass a **strong adversarial review** on actual operator usefulness ("would the operator act on this today?") before implementation — not "it's on the dashboard because the endpoint exists."

1. **Verified outcomes header** — the success/failure truth, top of page: PnL (realized/unrealized, live + paper), win %, per-strategy success rates, per-agent success rates (eval pass rate, runs won/lost). Time-window scoped (Today/7d/30d/All).
2. **Active tasks (control tower)** — everything in motion right now: queued/running evals with progress + ETA + cancel, active live/paper runs (mode, last decision, running P&L), Optimizer cycle if running.
3. **Opportunities** — recent high-signal findings with "Draft variant from this"; best-performing strategies worth iterating; suspect results worth re-running.
4. **Last night's Optimizer digest** — outcome-only: kept/dropped counts, honesty check result, holdout deltas, cost. One strip, link to cycle detail. (No approval queue.)
5. **Workshop** — drafts with status pills, re-entry into authoring.
6. **Gentle nag strip (bottom, muted)** — failed runs, missing keys, broker config, safety-pause note if applicable. Low-contrast, dismissible-feeling, never above outcomes.

Cut entirely: approval/decision queue (4.2), flywheel velocity (4.6), count cards (strategies/agents/providers totals are vanity). Safety pause may still warrant a top banner ONLY when actually paused — silent otherwise.

## 7. Adversarial review gate (3 independent reviewers, 2026-06-07)

Verdicts: usefulness **FAIL (reshape)**, feasibility **PARTIAL**, scope/alignment **FAIL (amendments)**. Consolidated outcome below supersedes §6.

### 7.1 Per-panel rulings

| §6 panel | Ruling | Why |
|---|---|---|
| 1 Verified outcomes | **RESHAPE + DEFER (slice 3)** | "+$340 today" is meaningless without risk context: deployed capital, current drawdown, daily-loss-limit buffer (color-coded). "Success rate" undefined → must be operationally defined with a minimum sample floor (below n, show "insufficient data"). Feasibility blocker: live portfolio PnL is NOT in dashboard tables (execution layer + Alpaca only — no `/api/portfolio`); win rate is not a stored RunSummary field; per-strategy/agent rollups need new query routes. Time-window filters exist on NO list endpoint. |
| 2 Active tasks | **KEEP (slice 1)** | The strongest panel; genuinely actionable. Sharpen: show unrealized P&L per live run; warn on stuck queue (>2h no progress) and runaway agents (live >24h without operator check-in); only render Cancel on human-queued evals, never Optimizer-queued. Gaps: no `GET /api/agent-runs` list endpoint (~1d), no active-cycle endpoint (~1d). Don't show ETA unless it's real. |
| 3 Opportunities | **CUT as specced; keep one defined row** | "High-signal" and "best-performing" have no defined selection logic — severity ≠ urgency, and a strategy with one good eval would show as "best" forever. Keep only what's real today: critical-severity findings (`eval_findings`, severity=critical) with "Draft variant from this". "Best performers" returns only when the metric + window + min-n are defined. |
| 4 Optimizer digest | **KEEP w/ reshape (slice 2)** | Fully API-backed (honesty checks, cost, Active/Suspect/Rejected all persisted). Reshape: kept/dropped needs a baseline (30d rolling acceptance rate) to spot degradation; cost needs budget context ("$4.10 / $10 today") + anomaly flag; document what "honesty check passed" verifies. Depends on the 2026-06-07 optimizer-ui-overhaul spec landing. |
| 5 Workshop/Drafts | **CUT (operator, 2026-06-07)** | "Draft" is not a product concept — code check: it exists only as internal API phrasing (`POST /api/strategies` "blank draft", `validate_draft`); no status field, no draft/published distinction. Strategies are just strategies. Folded into the outcomes surface: per-strategy metric rows, with not-yet-evaluated strategies listed plainly as "no evals yet" (which doubles as authoring re-entry). |
| 6 Gentle nag | **KEEP w/ one carve-out** | Correct for keys/brokers/stale failed runs. NOT for: safety pause (top red banner when paused — a paused system with a calm dashboard is a correctness bug), broker unreachable (deploy blocker, not config nag), stuck eval queue (belongs in Active tasks with warning tone). Split `failed` runs into infra errors (nag) vs completed-but-suspicious (findings). |

### 7.2 New requirements from review

- **Capital-risk strip is non-negotiable for live money:** deployed capital · drawdown · daily-loss-limit buffer as the top-line health signal (part of reshaped panel 1).
- **Deploy-readiness check** ("safe to go live? keys ✓ broker ✓ no blocking eval ✗") — candidate panel, slice 3.
- **Cost rollup** across optimizer + evals + runs vs budget cap.
- Layout: full-width stacked strips only (QA30) — no 8/4 grid, no popups; time-window pills as inline row. Mobile: responsive collapse only, no dedicated shell.
- Terminology: all operator labels per the 2026-05-27 lock; any new label ("running P&L" etc.) gets a lock-doc row first.

### 7.3 Sequencing (tracer slices)

1. **Slice 1 (frontend-only PR):** reorder home to cockpit shape — Active tasks strip (existing eval queue API + SSE), critical-findings row, per-strategy outcomes list (metrics from stored eval runs; un-evaluated strategies as "no evals yet" rows), nag strip demoted to bottom, safety-pause top banner, count cards removed.
2. **Slice 2:** Optimizer digest strip (after optimizer-ui-overhaul endpoints land) + agent-runs list + active-cycle endpoints.
3. **Slice 3 (backend-first):** verified-outcomes header — requires two operator decisions: (a) PnL data path (daemon proxy vs persisted snapshots), (b) success-metric definition (win % baseline, window, min-n). Plus time-window filters on list endpoints, then capital-risk strip + deploy-readiness.

### 7.4 Operator decisions (resolved 2026-06-07)

1. **Outcomes data path: LIVE, not snapshots.** Pull live data from running evals and strategies via the existing SSE streams (`/api/eval/runs/:id/stream`, `/api/agent-runs/:id/stream`), and eventually from live-money runs the same way. The outcomes header is a live view of what's running + most recent completed results — not a snapshot-table history product. Persisted snapshot tables are out of scope for now; historical windows come from completed `eval_runs` metrics already stored.
2. **No aggregate "success rate" number.** Show the actual verified metrics per strategy/agent — return, Sharpe, max drawdown — colored against the win threshold (return > 0 AND Sharpe > 1.0) rather than collapsed into a single % figure. Below n=10 runs, no judgement coloring — show the raw metrics plainly. A % rate can be revisited once enough history exists to be honest.
3. **Workshop/Drafts panel CUT.** Not an optimizer term — it was the v1 design's label ("In your workshop") for in-progress strategy drafts. But "draft" is not a real concept in the system (no status field, no draft/published split; only internal API phrasing). Panel removed; strategies appear once, in the outcomes surface, with un-evaluated ones marked "no evals yet" as the authoring re-entry point. No state machine, no derived pills.
4. **Spec only.** No implementation in this pass; slices in §7.3 become the implementation plan when picked up (worktree + TDD + plan-review gate per CLAUDE.md).

## 8. Execution plan after reachability + D-series reconciliation (2026-06-09)

> **For agentic workers:** REQUIRED SUB-SKILL: Use `superpowers:subagent-driven-development` (recommended) or `superpowers:executing-plans` to implement this section task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make `/` a readable Control Tower for the current product: evals, strategies, safety, and Optimizer. Do not imply live trading exists until the live-trading backend contract lands.

**Architecture:** Keep home full-width stacked per QA30. The home page should summarize and route to deeper surfaces; it must not become another long table. Existing eval, strategy, safety, and Optimizer APIs are in scope. True live-trading PnL, capital risk, deployed capital, live/paper run status, and `/live` cockpit semantics are gated behind the live-trading work.

**Related but separate:** `docs/superpowers/specs/2026-06-08-design-sweep-d-series.md` owns accessibility, motion, loading/empty-state polish, IA links, and copy pass. D-series agents may touch home copy, but they do not own Control Tower data selection, live gating, or dashboard information architecture.

### 8.1 Current findings that supersede §7.3

1. **The old plan over-trusts `agent_runs` as live trading.** `agent_runs` are trace/task records, not deployed live strategies. Current `/live` and `LiveStrategiesSection` copy can imply real money even when live trading is not built. This is the source of the false "live" status.
2. **Slice 0 exists but must be treated as a merge/wire-up check.** PR #878 / branch `feat/control-tower-slice0-orphan-wireups` contains useful work: home-mounted `OptimizerDigestStrip`, enriched digest fields, active Optimizer cycle row, and pause/resume controls. Before coding, check whether those commits are already in the execution branch.
3. **D-series does not provide a home-dashboard focal metric header.** The master list's focal metric item is for eval-run detail and is already marked done by A2. The Control Tower still needs its own compact home outcome strip from existing eval + Optimizer data.
4. **The strategy outcomes section is still a list wall.** Rendering every strategy on home violates the "what should I look at first?" principle. Home needs a capped summary, with full strategy lists left to `/strategies` or `/eval-runs`.
5. **Live-money metrics are explicitly gated.** PnL, deployed capital, drawdown, daily-loss-limit buffer, live/paper run status, last live decision, and risk-gate vetoes must not be faked from `agent_runs`.

### 8.2 Ownership boundary with D-series

Do **not** duplicate D-series work in this plan:

| Area | Owner | Control Tower instruction |
|---|---|---|
| Focus rings, ARIA, reduced motion, icon labels | D1 | Do not scope here except when tests fail from CT edits. |
| Motion tokens, row animations, CTA press states | D2 | Do not add CT-specific motion outside existing conventions. |
| Generic skeletons / retry states | D3 | CT may add home-specific skeleton only if it is needed to avoid cascade blink. |
| Eval-to-optimizer links, sidebar grouping, compare sort | D4 | Leave to D4 unless CT edits directly conflict. |
| "cockpit" copy, live-capital copy, status labels | D5 | CT must fix false live-trading claims now; D5 can polish final wording later. |
| Home data selection, live-trading gate, Optimizer reachability, list wall removal | Control Tower | Implement here. |

### 8.3 New sequencing

The remaining implementation should run in this order. Each slice should be its own PR unless the repo owner explicitly asks for one combined PR.

1. **CT0 — Live-trading gate + terminology correction.**
   Remove false live-trading claims from home and `/live`. Do not show `agent_runs` as "Live strategies" or "Real money." Either hide these surfaces until live trading exists or relabel them as agent-run traces / running agent tasks, depending on the current product decision.
2. **CT1 — Slice 0 reachability completion.**
   Merge or re-implement PR #878's useful pieces: Optimizer digest reachable from home, real suspect count, honesty outcome, cost, active Optimizer cycle in Active tasks, and pause/resume controls.
3. **CT2 — Active tasks cleanup.**
   Active tasks should show queued/running evals and active Optimizer cycle only. It should not show a permanent empty card above the fold. It should not show Cancel for runs that are known not to be human-queued once the source field exists.
4. **CT3 — Replace strategy row wall with a home outcomes summary.**
   Replace unbounded `StrategyOutcomesList` on home with a capped, decision-oriented summary from existing completed eval data. Link to full surfaces for long lists.
5. **CT4 — Existing-data home outcome strip.**
   Add a compact top strip using only data available today: completed eval outcome counts, latest return/Sharpe/drawdown summary, active task count, safety paused state, and Optimizer last-run result/cost when available. No PnL or live capital.
6. **CT5 — Live-trading follow-up gate.**
   After live trading lands, add real capital-risk and `/live` cockpit work. This is blocked until the backend exposes a live-trading deployment/portfolio contract.

### 8.4 CT0 — Live-trading gate + terminology correction

**Problem:** There are zero live strategies today, and live trading is not built as a dashboard product surface. Any label saying "Live strategies," "Real money," or "active deployments" from `agent_runs` is misleading.

**Files:**
- Modify: `frontend/web/src/routes/home.tsx`
- Modify or remove from home: `frontend/web/src/components/home/LiveStrategiesSection.tsx`
- Modify: `frontend/web/src/routes/live-list.tsx`
- Tests: `frontend/web/src/routes/home.test.tsx`
- Tests: `frontend/web/src/components/home/LiveStrategiesSection.test.tsx`
- Tests: `frontend/web/src/routes/live-list.test.tsx`

**Implementation decision:** Prefer hiding live sections from home until a real live-trading contract exists. If the team still wants visibility into in-flight agent traces, rename the component to `RunningAgentTasksSection` and link to `/agent-runs`, not `/live`.

- [ ] **Step 1: Add a failing home test proving no live-trading copy appears.**

  In `frontend/web/src/routes/home.test.tsx`, add:

  ```tsx
  it("does not imply live trading exists on the home dashboard", async () => {
    renderRoute();
    await screen.findByRole("heading", { name: "Dashboard" });

    expect(screen.queryByText(/Live strategies/i)).toBeNull();
    expect(screen.queryByText(/Real money/i)).toBeNull();
    expect(screen.queryByText(/active live deployments/i)).toBeNull();
  });
  ```

- [ ] **Step 2: Run the focused test and verify it fails before implementation.**

  Run:

  ```bash
  cd frontend/web
  pnpm test -- home.test.tsx --runInBand
  ```

  Expected before implementation: FAIL because home still renders live-trading copy or imports the live section.

- [ ] **Step 3: Remove `LiveStrategiesSection` from home.**

  In `frontend/web/src/routes/home.tsx`, remove:

  ```tsx
  import { LiveStrategiesSection } from "@/components/home/LiveStrategiesSection";
  ```

  and remove the render line:

  ```tsx
  <LiveStrategiesSection />
  ```

  The home render order after CT0 should be:

  ```tsx
  <SafetyPauseBanner />
  <ActiveTasksStrip />
  <OptimizerDigestStrip />
  <CriticalFindingsRow runs={runs.data ?? []} />
  <StrategyOutcomesList strategies={strategies.data ?? []} runs={runs.data ?? []} />
  <NagStrip items={attentionItems} />
  ```

- [ ] **Step 4: Gate `/live` route copy.**

  In `frontend/web/src/routes/live-list.tsx`, replace the topbar:

  ```tsx
  <Topbar title="Live strategies" sub="Real money · active deployments" />
  ```

  with:

  ```tsx
  <Topbar title="Live trading" sub="Not enabled in this build" />
  ```

  Replace the empty state with:

  ```tsx
  <div className="flex flex-col items-center gap-3 py-16 text-center">
    <p className="text-[15px] font-medium text-text-2">
      Live trading is not enabled yet
    </p>
    <p className="max-w-lg text-[13px] text-text-3">
      This dashboard does not currently have a live-trading deployment surface.
      Use evals and the Optimizer to test strategies before live-trading support lands.
    </p>
  </div>
  ```

  If rows are still rendered from `agent_runs`, label the page "Agent runs" instead of "Live trading"; never mix `agent_runs` with real-money copy.

- [ ] **Step 5: Update tests.**

  In `frontend/web/src/routes/live-list.test.tsx`, replace assertions for "No active live deployments" or "Real money" with "Live trading is not enabled yet" unless the route is renamed to agent runs.

- [ ] **Step 6: Run focused tests.**

  ```bash
  cd frontend/web
  pnpm test -- home.test.tsx live-list.test.tsx --runInBand
  ```

  Expected: PASS.

### 8.5 CT1 — Slice 0 reachability completion

**Problem:** The Optimizer digest and controls were built or partly built, but were not consistently reachable from home. PR #878 should be reused where possible.

**Files:**
- Modify: `frontend/web/src/routes/home.tsx`
- Modify: `frontend/web/src/components/home/OptimizerDigestStrip.tsx`
- Modify: `frontend/web/src/components/home/ActiveTasksStrip.tsx`
- Modify: `frontend/web/src/features/autooptimizer/api.ts`
- Modify: `crates/xvision-dashboard/src/routes/autooptimizer.rs`
- Tests: `frontend/web/src/routes/home.test.tsx`
- Tests: `frontend/web/src/components/home/OptimizerDigestStrip.test.tsx`
- Tests: `frontend/web/src/components/home/ActiveTasksStrip.test.tsx`
- Tests: relevant Rust route tests in `crates/xvision-dashboard/src/routes/autooptimizer.rs`

- [ ] **Step 1: Check whether PR #878 is already present.**

  Run:

  ```bash
  git branch --contains 31690b2
  git log --oneline -- frontend/web/src/components/home/ActiveTasksStrip.tsx frontend/web/src/components/home/OptimizerDigestStrip.tsx crates/xvision-dashboard/src/routes/autooptimizer.rs | head -20
  ```

  If the execution branch already contains `31690b2`, do not duplicate the changes. Continue to the verification steps.

- [ ] **Step 2: Ensure home mounts `OptimizerDigestStrip`.**

  `frontend/web/src/routes/home.tsx` must import:

  ```tsx
  import { OptimizerDigestStrip } from "@/components/home/OptimizerDigestStrip";
  ```

  and render it immediately after active tasks:

  ```tsx
  <SafetyPauseBanner />
  <ActiveTasksStrip />
  <OptimizerDigestStrip />
  <CriticalFindingsRow runs={runs.data ?? []} />
  ```

- [ ] **Step 3: Add route-level reachability test.**

  In `frontend/web/src/routes/home.test.tsx`, mock `useSessionList` and assert the digest appears on home when a session exists:

  ```tsx
  it("mounts OptimizerDigestStrip on the home route when a session exists", async () => {
    const { useSessionList } = await import("@/features/autooptimizer/api");
    vi.mocked(useSessionList).mockReturnValue({
      data: [
        {
          session_id: "sess_01",
          strategy_id: "strategy-alpha",
          state: "finished",
          mode: "explore",
          cycles_completed: 12,
          kept_count: 2,
          suspect_count: 1,
          cost_usd: 4.1,
          honesty_passed: true,
        },
      ],
      isPending: false,
      isLoading: false,
      isError: false,
      isSuccess: true,
    } as never);

    renderRoute();
    expect(await screen.findByTestId("optimizer-digest-strip")).toBeInTheDocument();
  });
  ```

- [ ] **Step 4: Complete the digest type.**

  In `frontend/web/src/features/autooptimizer/api.ts`, `SessionListItem` must include:

  ```ts
  suspect_count?: number;
  cost_usd?: number;
  honesty_passed?: boolean;
  ```

  `StatusResponse` must include:

  ```ts
  active_cycle_id?: string | null;
  ```

- [ ] **Step 5: Render real digest fields.**

  In `OptimizerDigestStrip.tsx`, remove the local `SessionListItemFull` cast and render:

  ```tsx
  const suspectLabel =
    session.suspect_count != null ? `${session.suspect_count} suspect` : "— suspect";

  const honestyLabel =
    session.honesty_passed == null
      ? "Honesty check —"
      : session.honesty_passed
        ? "Honesty check passed"
        : "Honesty check failed";
  ```

  Keep the link to `/optimizer/run/:session_id`.

- [ ] **Step 6: Add backend session-list enrichment.**

  In `crates/xvision-dashboard/src/routes/autooptimizer.rs`, change `list_sessions` to return session rows enriched with:

  ```rust
  #[derive(Serialize)]
  pub struct SessionListRow {
      #[serde(flatten)]
      pub session: OptimizerSession,
      #[serde(skip_serializing_if = "Option::is_none")]
      pub cost_usd: Option<f64>,
      #[serde(skip_serializing_if = "Option::is_none")]
      pub honesty_passed: Option<bool>,
  }
  ```

  Add helper queries that derive session cost from `cycle_cost` via `autooptimizer_events.session_id/cycle_id`, and latest honesty from `cycle_honesty_checks`.

- [ ] **Step 7: Surface active cycle id.**

  In `StatusResponse`, add:

  ```rust
  #[serde(skip_serializing_if = "Option::is_none")]
  pub active_cycle_id: Option<String>,
  ```

  Derive it from the latest `autooptimizer_events` row for the active session with non-null `cycle_id`.

- [ ] **Step 8: Run tests.**

  ```bash
  cd frontend/web
  pnpm test -- home.test.tsx OptimizerDigestStrip.test.tsx ActiveTasksStrip.test.tsx --runInBand
  cd ../..
  cargo test -p xvision-dashboard autooptimizer
  ```

  Expected: PASS. If workspace rules prohibit cargo in the main checkout, run Rust tests in an isolated worktree.

### 8.6 CT2 — Active tasks cleanup

**Problem:** Active tasks is the top actionable dashboard surface. It should not render a permanent empty card, and it should include the active Optimizer cycle after CT1.

**Files:**
- Modify: `frontend/web/src/components/home/ActiveTasksStrip.tsx`
- Tests: `frontend/web/src/components/home/ActiveTasksStrip.test.tsx`

- [ ] **Step 1: Add tests for empty and Optimizer states.**

  Required test cases:

  ```tsx
  it("returns null when there are no active evals and no optimizer cycle", async () => {
    vi.mocked(evalApi.listRuns).mockResolvedValue([]);
    setOptimizerStatus({ active_session: null, last_event_seq: 0 });
    const { container } = renderStrip();
    await waitFor(() => expect(container.firstChild).toBeNull());
  });

  it("renders the active optimizer cycle even when evals are empty", async () => {
    vi.mocked(evalApi.listRuns).mockResolvedValue([]);
    setOptimizerStatus({
      active_session: {
        session_id: "sess_01",
        strategy_id: "strategy-alpha",
        state: "running",
        mode: "explore",
        cycles_completed: 3,
        kept_count: 1,
        suspect_count: 0,
        dropped_count: 2,
      },
      active_cycle_id: "cycle_01",
      last_event_seq: 10,
    });
    renderStrip();
    expect(await screen.findByTestId("active-optimizer-cycle")).toBeInTheDocument();
  });
  ```

- [ ] **Step 2: Return null when no active tasks exist.**

  In `ActiveTasksStrip.tsx`, after computing `total`, add:

  ```tsx
  if (data !== undefined && total === 0) return null;
  ```

  Keep showing the section while evals are loading only if an Optimizer cycle exists.

- [ ] **Step 3: Keep cancel behavior conservative.**

  Until `RunSummary` exposes source/owner, keep the existing cancel behavior only if tests already cover it. Add a code comment:

  ```tsx
  // Cancel ownership is not exposed on RunSummary yet; hide this for
  // Optimizer-owned evals when the API adds a source/owner field.
  ```

- [ ] **Step 4: Run focused tests.**

  ```bash
  cd frontend/web
  pnpm test -- ActiveTasksStrip.test.tsx --runInBand
  ```

### 8.7 CT3 — Replace strategy row wall with a home outcomes summary

**Problem:** The current `StrategyOutcomesList` renders every strategy. Home should show a compact summary that answers "what should I inspect first?"

**Files:**
- Create: `frontend/web/src/components/home/StrategyOutcomesSummary.tsx`
- Create: `frontend/web/src/components/home/StrategyOutcomesSummary.test.tsx`
- Modify: `frontend/web/src/routes/home.tsx`
- Leave available but no longer mounted on home: `frontend/web/src/components/home/StrategyOutcomesList.tsx`

**Selection rules from existing data:**
- Use completed eval runs only.
- Group by `run.strategy?.id`.
- For each strategy, use the most recent completed run.
- Define a provisional "passing" metric only for display: return > 0 and Sharpe > 1.0.
- Do not show judgement coloring below 3 completed evals for that strategy.
- Show at most:
  - 3 strongest recent strategies, sorted by return desc, then Sharpe desc.
  - 3 weakest recent strategies, sorted by return asc, then drawdown desc.
  - one count of strategies with no completed evals.

- [ ] **Step 1: Write tests for summary selection and cap.**

  Use concrete fixture builders in the test file. The assertions must verify both section caps and no-eval aggregation:

  ```tsx
  it("shows at most three strongest and three weakest strategies", () => {
    const strategies = [
      makeStrategy("s1", "Alpha"),
      makeStrategy("s2", "Bravo"),
      makeStrategy("s3", "Charlie"),
      makeStrategy("s4", "Delta"),
      makeStrategy("s5", "Echo"),
      makeStrategy("s6", "Foxtrot"),
      makeStrategy("s7", "Golf"),
      makeStrategy("s8", "Hotel"),
    ];
    const runs = [
      makeRun("r1", "s1", { total_return_pct: 22, sharpe: 2.1 }),
      makeRun("r2", "s2", { total_return_pct: 18, sharpe: 1.7 }),
      makeRun("r3", "s3", { total_return_pct: 11, sharpe: 1.4 }),
      makeRun("r4", "s4", { total_return_pct: 4, sharpe: 1.1 }),
      makeRun("r5", "s5", { total_return_pct: -2, sharpe: 0.6 }),
      makeRun("r6", "s6", { total_return_pct: -8, sharpe: 0.3 }),
      makeRun("r7", "s7", { total_return_pct: -13, sharpe: -0.1 }),
      makeRun("r8", "s8", { total_return_pct: -21, sharpe: -0.4 }),
    ];

    render(<StrategyOutcomesSummary strategies={strategies} runs={runs} />);

    expect(screen.getByText("Alpha")).toBeInTheDocument();
    expect(screen.getByText("Bravo")).toBeInTheDocument();
    expect(screen.getByText("Charlie")).toBeInTheDocument();
    expect(screen.getByText("Foxtrot")).toBeInTheDocument();
    expect(screen.getByText("Golf")).toBeInTheDocument();
    expect(screen.getByText("Hotel")).toBeInTheDocument();
    expect(screen.queryByText("Delta")).toBeNull();
    expect(screen.queryByText("Echo")).toBeNull();
  });

  it("shows no-eval count without rendering every no-eval strategy", () => {
    const strategies = Array.from({ length: 10 }, (_, i) =>
      makeStrategy(`s${i + 1}`, `Strategy ${i + 1}`),
    );
    const runs = [
      makeRun("r1", "s1", { total_return_pct: 7, sharpe: 1.2 }),
      makeRun("r2", "s2", { total_return_pct: -4, sharpe: 0.5 }),
    ];

    render(<StrategyOutcomesSummary strategies={strategies} runs={runs} />);

    expect(screen.getByText("8 strategies have no completed evals yet")).toBeInTheDocument();
    expect(screen.queryByText("Strategy 10")).toBeNull();
  });
  ```

- [ ] **Step 2: Implement `StrategyOutcomesSummary`.**

  Component props:

  ```tsx
  export interface StrategyOutcomesSummaryProps {
    strategies: StrategyListItem[];
    runs: RunSummary[];
  }
  ```

  Render:

  - Heading: `Strategy outcomes`
  - Subheading: `latest completed evals`
  - `Strongest recent` section with capped rows.
  - `Needs review` section with capped rows.
  - No-eval count row with link to `/eval-runs`.
  - Link: `View all strategies` to `/strategies`.

- [ ] **Step 3: Mount summary on home.**

  In `home.tsx`, replace:

  ```tsx
  <StrategyOutcomesList strategies={strategies.data ?? []} runs={runs.data ?? []} />
  ```

  with:

  ```tsx
  <StrategyOutcomesSummary strategies={strategies.data ?? []} runs={runs.data ?? []} />
  ```

- [ ] **Step 4: Update route tests.**

  In `home.test.tsx`, replace `strategy-outcomes-list` assertions with `strategy-outcomes-summary`.

- [ ] **Step 5: Run tests.**

  ```bash
  cd frontend/web
  pnpm test -- StrategyOutcomesSummary.test.tsx home.test.tsx --runInBand
  ```

### 8.8 CT4 — Existing-data home outcome strip

**Problem:** D-series focal metric work applies to eval-run detail, not home. Home still needs a compact existing-data outcome strip, but it must not fake live PnL.

**Files:**
- Create: `frontend/web/src/components/home/HomeOutcomeStrip.tsx`
- Create: `frontend/web/src/components/home/HomeOutcomeStrip.test.tsx`
- Modify: `frontend/web/src/routes/home.tsx`

**Allowed metrics now:**
- completed eval count
- queued/running eval count
- best latest completed eval return among strategies
- median latest completed eval Sharpe, if available
- worst latest completed eval max drawdown, if available
- Optimizer last-run digest, if available through `OptimizerDigestStrip`
- safety paused state remains in `SafetyPauseBanner`, not duplicated

**Forbidden until live-trading gate clears:**
- PnL
- deployed capital
- current drawdown from live capital
- daily-loss-limit buffer
- live/paper run mode
- "real money"

- [ ] **Step 1: Write tests forbidding live-money labels.**

  ```tsx
  it("renders eval and optimizer outcome labels without live-money claims", () => {
    render(<HomeOutcomeStrip runs={runs} strategies={strategies} />);
    expect(screen.getByText(/completed evals/i)).toBeInTheDocument();
    expect(screen.queryByText(/PnL/i)).toBeNull();
    expect(screen.queryByText(/deployed capital/i)).toBeNull();
    expect(screen.queryByText(/real money/i)).toBeNull();
  });
  ```

- [ ] **Step 2: Implement pure derivation helpers.**

  In `HomeOutcomeStrip.tsx`, export pure helpers for tests:

  ```ts
  export function latestCompletedRunsByStrategy(runs: RunSummary[]): RunSummary[] {
    const byStrategy = new Map<string, RunSummary>();
    for (const run of runs) {
      if (run.status !== "completed") continue;
      const strategyId = run.strategy?.id;
      if (!strategyId) continue;
      const existing = byStrategy.get(strategyId);
      const currentCompletedAt = run.completed_at ?? "";
      const existingCompletedAt = existing?.completed_at ?? "";
      if (!existing || currentCompletedAt.localeCompare(existingCompletedAt) > 0) {
        byStrategy.set(strategyId, run);
      }
    }
    return [...byStrategy.values()];
  }

  export function median(values: number[]): number | null {
    const sorted = values
      .filter((value) => Number.isFinite(value))
      .sort((a, b) => a - b);
    if (sorted.length === 0) return null;
    const mid = Math.floor(sorted.length / 2);
    if (sorted.length % 2 === 1) return sorted[mid];
    return (sorted[mid - 1] + sorted[mid]) / 2;
  }
  ```

- [ ] **Step 3: Render compact strip.**

  Render four small cells:

  - `Completed evals`
  - `In flight`
  - `Best return`
  - `Median Sharpe`

  Use `--pos`/`--neg` token classes for return only. Do not introduce new color tokens.

- [ ] **Step 4: Mount below `SafetyPauseBanner`.**

  Home order:

  ```tsx
  <SafetyPauseBanner />
  <HomeOutcomeStrip strategies={strategies.data ?? []} runs={runs.data ?? []} />
  <ActiveTasksStrip />
  <OptimizerDigestStrip />
  <CriticalFindingsRow runs={runs.data ?? []} />
  <StrategyOutcomesSummary strategies={strategies.data ?? []} runs={runs.data ?? []} />
  <NagStrip items={attentionItems} />
  ```

- [ ] **Step 5: Run tests.**

  ```bash
  cd frontend/web
  pnpm test -- HomeOutcomeStrip.test.tsx home.test.tsx --runInBand
  ```

### 8.9 CT5 — Live-trading follow-up gate

Do not implement this slice until a live-trading backend contract exists. The contract must expose a distinct resource from `agent_runs`, for example:

```ts
type LiveDeploymentSummary = {
  deployment_id: string;
  strategy_id: string;
  strategy_name: string;
  mode: "paper" | "live";
  status: "starting" | "running" | "paused" | "stopped" | "failed";
  started_at: string;
  last_decision_at: string | null;
  deployed_capital_usd: number | null;
  realized_pnl_usd: number | null;
  unrealized_pnl_usd: number | null;
  drawdown_pct: number | null;
  daily_loss_limit_remaining_usd: number | null;
  risk_veto_count_since_last_visit: number;
};
```

Acceptance for unblocking CT5:

- [ ] `GET /api/live/deployments` or equivalent returns only actual paper/live trading deployments.
- [ ] `GET /api/live/deployments/:id/stream` or equivalent streams decision/PnL/risk updates.
- [ ] Dashboard can distinguish paper from live money without inferring from `agent_runs`.
- [ ] Capital-risk values come from broker/execution state, not eval summaries.
- [ ] UI labels are added to the terminology lock before merge.

Only after those are true should the dashboard add:

- capital-risk strip
- true `/live` cockpit
- live/paper active tasks rows
- deployed capital / drawdown / daily-loss-limit buffer
- live risk-gate vetoes

### 8.10 Verification checklist for the full CT track

- [ ] Home has no `Live strategies`, `Real money`, or `active deployments` copy unless CT5 is unblocked.
- [ ] Home does not render an unbounded list of strategies.
- [ ] Home shows a compact outcome strip using existing eval data only.
- [ ] Optimizer digest is reachable from home and route-tested.
- [ ] Optimizer digest shows real suspect count, honesty status, and cost when backend data exists.
- [ ] Active tasks shows queued/running evals and active Optimizer cycle only.
- [ ] Empty active tasks does not consume above-fold space.
- [ ] Critical findings row remains capped and actionable.
- [ ] Nag strip remains bottom-mounted and only contains config/infra nags.
- [ ] D-series ownership is respected; CT PRs do not sprawl into accessibility/motion/copy beyond false live-trading claims.
