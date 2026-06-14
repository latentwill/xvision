---
title: Dashboard Full-App QA Cleanup (from 2026-06-13 Playwright findings)
status: draft (pending plan-review-gate + scope confirmation)
source: docs/QA/2026-06-13-dashboard-full-app-ui-findings.md (~70 findings, 21 routes)
verified-against: origin/main @ f5c43af6 (worktree .worktrees/qa-app-fixes)
method: 6 read-only Explore sweeps + graphify map + direct grep verification
excludes: G-1 (eval/runs multi-status 400 — owned by chat-rail agent, see 2026-06-13-chat-rail-parity-fixes.md W1)
date: 2026-06-13
---

# Dashboard Full-App QA Cleanup

## Dedup outcome (verified against current origin/main)

The QA ran against a **deployed node at xvn 0.21.0**, which predates fixes already on
`origin/main`. Every finding was re-verified against current code. Categories:

### A. Already fixed / not a code bug — NO action (or doc-only)
| Finding | Verdict | Evidence |
|---|---|---|
| CC-2 eval-run AGENT/SCENARIO pills → list | **Fixed** | `eval-runs-detail.tsx:326,336,343` navigate to `/strategies/:id`, `/agents/:id`, `/scenarios/:id` |
| CC-2 scenario "Runs" rows not clickable | **Fixed** | `scenarios-detail.tsx` RunsTab rows are `<Link to=/eval-runs/:id>` |
| CC-1 dashboard TODAY/7D/30D don't move chart | **By design** | pills scope outcome strips (`home.tsx:93-103`), not `PulseBand` |
| CC-4 /live status tabs don't filter | **Not broken** | `strip-status.ts:177` filters correctly; QA dataset all-STOPPED |
| MKT-1 bundle 503/404 | **Infra, not logic** | `marketplace_read.rs:389` — content_uri unreachable / id not yet indexed |
| Drawdown ≈ Return% | **Data-dependent** | `pulse.ts:39` drawdown computed correctly; flat run → flat curve |
| G-1 eval/runs multi-status 400 | **Owned by chat-rail agent** | excluded from this plan |

### B. Genuine bugs — staged into 6 PRs below.

### C. Decision-needed / larger (file as beads, not auto-fixed) — see "Deferred" section.

---

## Conflict zones (single-writer discipline)
- `frontend/web/src/routes/eval-runs.tsx` — PR2 only (count + columns).
- `frontend/web/src/routes/scenarios.tsx` — PR2 only (source filter + columns + archived).
- `frontend/web/src/routes/agents.tsx` — **PR2 ONLY**. Both the column-gating (W8) and the TOOLS-overflow (W26) edits live in PR2 so a single worktree owns agents.tsx. PR6 is backend/settings only (W25 engine + W27 settings) — it does NOT touch agents.tsx. (Resolves the plan-review contradiction.)
- `frontend/web/src/routes.tsx` — PR4 only (404 + optimizer run route).
- `frontend/web/src/components/lists/*` (shared list primitives) — PR2 only.
PRs run in separate worktrees, merge sequentially.

---

# PR1 — Data-binding & placeholder hygiene (CC-3) · frontend · LOW risk
Visible "undefined"/"$?"/malformed strings reaching users. All nullish-guard / format fixes.

- **W1** `features/live/StrategyStrip.tsx:247-248` — `String(run.model_call_count ?? "—")`, same for `tool_call_count`. Line 208-210: replace raw `objective` fallback (`eval:Backtest:…`) with strategy-name lookup, strip `eval:<kind>:` prefix as last resort.
- **W2** `components/home/OptimizerDigestStrip.tsx:74` — `"$?"` → `"—"` (match the em-dash already used for holdout/suspect).
- **W3** `components/chart/v2/surfaces/GradientHeroDashboard.tsx:149` — `suffix={lead.metrics.return >= 0 ? "is up" : "is down"}`; `primitives/GradientHeadline.tsx:48` — add `{" "}` before the emphasis span (fixes "up-1.02%").
- **W4** marketplace zero-perf: `ListingCard.tsx` / `ListingEntry.tsx` — when `return30dPct===0 && sharpe===0`, render `"—"` not `"0%"`/`"0.00"`. (Seed "Strategy #N" names are humanize() fallback by design — leave; real fix is the manifest resolver, deferred.)
- **W5** `routes/charts/ChartsAnnotated.tsx:27` — the `run_id ?? "demo"` default surfaces 5 hardcoded demo events as if real. Gate behind an explicit "Demo" banner OR add a run selector. Minimum: label the demo state clearly.

Acceptance: no literal "undefined"/"$?"/"up-1.02%" in rendered output; `vitest` for touched components green.

# PR2 — List filters & counts (CC-4) · frontend · LOW–MED risk
- **W6** `routes/scenarios.tsx:108-116` — Source=Optimizer returns 0 because filter requires `source=Generated` AND tag; ec-day-* rows in DB don't satisfy both. Fix: drop the `source` requirement for the optimizer token (rely on `tags:[OPTIMIZER_SCENARIO_TAG]` alone → `source: null`). *(Decision A: tag-only, confirmed.)* Note (plan-review): confirm live-config scenarios (`scenario_from_live_config`) never set `source:autooptimizer` in `cfg.tags`, else they'd leak into the Optimizer filter.
- **W7** `routes/eval-runs.tsx` — Mode filter doesn't change the "N of M" count. Verify the actual `subtitleFor(q, M, N)` call site first: **N (visible) must be the post-filter `list.rows.length`** (so it drops when Mode filters), **M (denominator) the server total `q.data?.total ?? 0`**. The bug is N tracking `list.totalRows` (= unfiltered `opts.rows.length`) instead of the filtered length — do NOT just swap M to `q.data.total` (that leaves N wrong). Fix the `ResponsiveListCard count` prop consistently with whichever value it's meant to show. (Plan-review correction.)
- **W8** `routes/agents.tsx` ALSO lives in PR2 — see W26. Columns picker shows checked cols with no `<th>`/misaligned `<td>`: `routes/eval-runs.tsx:412` (+ `scenarios.tsx:262`, `agents.tsx:238`) — plumb the `visibleKeys` 3rd arg into `DesktopRow` and gate each `<td>`. Distinguish auto-hidden vs user-unchecked in the picker badge (`components/lists/ListToolbar.tsx`).
- **W9** marketplace segment tabs (Trending/New/Mine) no-op: `features/marketplace/data/filter.ts:28-49` — add a `segment` predicate. mine = viewer.createdListingIds; new = sort newest. **Trending: confirm a server-side velocity/score field exists FIRST; if absent, fall back to `newest` for Trending and note the limitation — do NOT invent a client-side velocity proxy.** Sort label stale: `routes/browse/Toolbar.tsx:71` — when a perf-sort becomes unavailable, reset `filter.sort` to `"newest"`. (Plan-review note.)
- **W10** Hide-archived feedback: `routes/scenarios.tsx` + `agents.tsx` `subtitleFor` — show "N of M (K archived hidden)" when exclude is active.

Acceptance: each filter/tab demonstrably changes rendered rows or count (Mode filter reduces N); list-state vitest green.

# PR3 — Charts (CC-1 /live, STR-1, STR-2, Layers) · frontend · MED risk
- **W11** `components/chart/v2/surfaces/LiveChartV2.tsx:48` — title ignores selected range; append `range` so "1h"/"4h" reflect in the label.
- **W12** `components/chart/v2/surfaces/StrategyHistoryChartV2.tsx:116-141` — `applyRange` applies intraday windows (4h=14400s) to multi-day run timestamps → single diagonal line. Filter `ChartFrame` presets for this surface to calendar/day-scale (drop 1h/4h/6h/12h) or slice by run-count. Mirror the preset-filter pattern at `RunChartV2.tsx:44-49`.
- **W13** `StrategyHistoryChartV2.tsx:51-55` — "Layers ▾" opens to a static text node (no toggles). Either pass `layersPanel={undefined}` to hide the button, or supply a real `<LayerPanel>`.
- **W14** STR-1 Y-axis "00.0%": the ACTIVE root cause (confirmed by plan-review) is `usePlot.ts:116` — `JSON.stringify(opts)` silently drops the `axes[1].values` formatter **function**, so the uPlot instance is not recreated when the formatter/data changes and a stale/missing formatter persists. Fix the dep array: stable opts hash or an explicit data-identity dep that doesn't rely on JSON-stringifying functions. This is shared across ALL uPlot chart surfaces — **add a regression test** asserting `usePlot` re-renders when only `opts.axes[].values` (a function) changes. Then confirm `MultiStrategyEquityPane.tsx:134-138` Y-axis renders real percentages on a non-flat run.

Acceptance: range/label reflect selection; 4h on strategy detail renders a sane window; no orphan Layers button; usePlot regression test green; Y-axis shows real % on a non-flat run.

# PR4 — Routing & navigation · frontend · LOW–MED risk
- **W15** CC-5 (safety, HIGH): `routes/strategies.tsx:298-300` — "New Strategy" calls `create.mutate()` and persists "Untitled strategy" immediately. Change `onClick` to `navigate("/strategies/new")` (the existing `strategies-new.tsx` name form is orphaned). Remove the now-unused `create` mutation from the list view.
- **W16** G-2: `routes.tsx:264` — catch-all `<Navigate to="/">` → new `<NotFoundRoute/>` (`routes/not-found.tsx`) with a clear message + home link.
- **W17** DOC-1: `routes/docs/index.tsx:162-176` — nav `<button>` → `<a href="?slug=…">` with `onClick preventDefault` for SPA nav (bookmarkable). And :63-65 — unknown slug silently renders Quickstart; render an explicit "page not found" when `slug!=null && !slugInIndex` instead of falling back to `pages[0]`. (The "agent-daemon"/"mcp-surface" QA slugs are wrong — real slugs are `agentd`/`mcp`; the fallback fix covers the silent-redirect class.)
- **W18** OPT-2: `features/autooptimizer/ui/LineageRiver.tsx:57` — `keptCount` counts edges (`points.length-1`); a single active Pass node → 0 → "nothing kept yet". Count nodes, or relabel so one active node isn't reported as nothing kept.
- **W19** OPT-1 (scoped): `routes.tsx:99-105,240` — `/optimizer/run/:id` is redirect-only → `/optimizer?session=`, which has no session-scoped detail. Minimum: ensure the session chip view shows session-scoped content (stats already filter by session_id) with a clear "session view" header. (Full session-detail page = deferred feature → **bead**, Decision B.)
- **W28** (Med, plan-review GAP-1): `routes/eval-runs-detail.tsx:~888` — failed runs show "This run hasn't recorded any decisions yet — likely still queued or running" even when `status=failed`, burying the real error (e.g. `stop_loss_pct must be between …`). Gate that copy on `!isTerminalStatus(status)`; for failed runs hoist the error message to the top. (Also `eval-runs-detail-mobile.tsx` if it mirrors the copy.)
- **W29** (Med, plan-review GAP-2): `routes/eval-compare.tsx` (`/charts/compare`) — removing a roster pill (×) updates only internal state, not the `?ids=` URL (subtitle claims "Roster changes update the URL"). Make pill removal write `?ids=` like the add path does.

Acceptance: New Strategy no longer persists on click; unknown route shows 404; docs links are anchors; optimizer single-node shows a non-misleading state; failed runs show the real error not "still queued"; compare roster `×` updates `?ids=`.

# PR5 — A11y & responsive (G-3, RWD-1) · frontend · LOW risk
- **W20** G-3: `components/shell/CommandPalette.tsx` — add explicit `role="dialog"`; wrap results in `role="listbox"`; result buttons `role="option" aria-selected`; input `role="combobox" aria-expanded aria-controls aria-activedescendant`.
- **W21** RWD-1a: `components/mobile/MobileShell.tsx:39-48` — mobile `/` renders ONLY the chat rail (dashboard hidden). Render `<Outlet/>` as primary content and demote chat to the existing slide-up overlay.
- **W22** RWD-1c: `components/lists/ListToolbar.tsx:268` — fixed 280px search overflows at 390px; use `max-w-[280px] w-full flex-1 min-w-0`.
- **W23** RWD-1d: `components/responsive/TabletSplitShell.tsx:17` — `grid-cols-[360px_…]` is 47% at 768px; use `min(360px,45vw)` or 280px.
- **W24** RWD-1b: verify `routes/eval-runs.tsx` / `routes/docs/index.tsx` don't override the shell's `pb-24`; add bottom padding ≈ rail height where a nested container eats it. (Padding only — NO side panel; chat-rail column rule.)

Acceptance: Cmd-K screen-reader navigable; mobile home shows dashboard; no toolbar overflow / bottom-bar overlap at 390px.

# PR6 — Backend attribution & settings · backend+frontend · MED risk
**(PR6 does NOT touch agents.tsx — that's PR2. W26 moved to PR2.)**
- **W25** Agents "RECENT RUNS" always empty: `crates/xvision-engine/src/api/agents.rs:485-503,538-572` — `referencing_strategy_ids` relies on new `Strategy.agents: Vec<AgentRef>`; legacy strategies have it empty → no runs found. **Dual-path fix (plan-review correction — `agents_agent_id` alone is NULL for pre-migration-022 runs):** (a) keep the filesystem strategy-hop for legacy strategies, AND (b) also union runs where `eval_runs.agents_agent_id = ?` for new runs — add `agents_agent_id: Option<String>` to `ListFilter` + its SQL builder, or use raw SQL. Document that runs created before migration 022 with a legacy strategy may still be empty. Acceptance must verify against an agent whose runs are reachable by at least one path.
- **W27** Settings broker error: `frontend/web/src/api/settings.ts:158-164` logs `settings.broker.load.error` on every load while content still renders. Trace to `brokers::get` in engine; narrow the catch / return defaults for missing `brokers.toml` instead of throwing. Acceptance: cold load with no `brokers.toml` produces NO `settings.broker.load.error` in console.

Acceptance: agents with completed backtests show recent runs (via at least one attribution path); no spurious broker error on cold load.

## W26 (moved into PR2) — Agents TOOLS column overflow
- **W26** `routes/agents.tsx:395` — TOOLS `<select>` needs `overflow-hidden text-ellipsis` and consistent `__custom__` option presence when multi-tool. **Executes in PR2's worktree** (same file as W8/W10's agents.tsx column work), applied sequentially in that one worktree.

---

## Deferred (file as beads, require a decision or are larger features — NOT in this sweep)
- **MKT-2** marketplace detail uses sequential int id (`/marketplace/lineage/<int>`) not ULID — non-portable links; needs route+backend ULID lookup. (design change)
- **Marketplace metric wiring** — `return30dPct`/`sharpe`/`clones`/`buyers.agents` hardcoded 0; needs the off-chain eval seam. (feature)
- **Manifest resolver** — real marketplace listing names (replace "Strategy #N"). (infra)
- **OPT-1 full session detail page** — a real `/optimizer/run/:id` candidate table/timeline/events. (feature)
- **Dashboard pills → PulseBand** — wire `windowedRuns` into the hero chart IF that's the intended behavior. (product decision)
- Misc Low/cosmetic & "Improvement" items from the QA doc (sort options, combobox for agent ULIDs, friendly date formatting, tooltips, CTA styling) — batch as a polish bead.

## Resolved decisions (operator, 2026-06-13)
- **Scope**: PRs 1–6 (genuine High/Med bugs) now. Low/cosmetic + larger features → follow-up beads.
- **Decision A (scenarios Source=Optimizer)**: **frontend tag-only filter** (W6, no migration). Match on `tags:[OPTIMIZER_SCENARIO_TAG]` alone; set `source: null` for the optimizer token.
- **Decision B (OPT-1)**: **cheap session-view polish now** (W19) — ensure `?session=` view is clearly session-scoped — **plus a bead** for the full session-detail page (candidate table/timeline/events) as a deferred feature.
