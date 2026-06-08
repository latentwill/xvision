# Control Tower dashboard evaluation — design vs implementation vs available value

**Date:** 2026-06-07
**Inputs:** `docs/design/v1-xvisionrender/01-control-tower-*` + `docs/design/gptprompts-v1.md` §1 (the design source of truth), `frontend/web/src/routes/home.tsx` (current implementation), full backend surface inventory (autooptimizer, marketplace, memory/flywheel, eval, agent-runs, safety, optimizations).
**Status:** approved spec (spec only — implementation plan to follow). Passed 3-reviewer adversarial gate §7; operator decisions resolved §7.4.

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
