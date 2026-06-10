# xvision dashboard — full-site UX audit

**Date:** 2026-06-10 · **Auditor:** Claude (senior-design-lead lens)
**Site audited:** `https://xvn.tail2bb69.ts.net/` (live deployment, real data: 50 strategies, 50 eval runs, 20 live runs)
**Method:** every routed page screenshotted with agent-browser (desktop 1440×900, mobile 390×844), console/error capture per page, code-level root-cause tracing for every broken surface, local dev boot to verify fixes against the live API.
**Screenshots:** `assets/` (`desktop-*.png`, `mobile-*.png`; `*-after-fix.png` = verified fixes).

The lens throughout: **can a normal user understand the product, trust it, and finish the core action — pick/produce a profitable strategy and deploy it — without reading docs?** Not whether the UI looks nice.

---

## Executive summary

xvision is two products wearing one shell. The **marketplace** (`desktop-marketplace.png`) is a real product surface: hero copy that states the value proposition in one sentence, social-proof stats, leaderboards, rich listing rows with returns, sparklines, and Buy CTAs. The **operator dashboard** (`desktop-home.png`) is the opposite: a barren page that *contradicts itself* — it announced "46 completed evals" directly above "No completed evals yet. Run an eval →" and "50 strategies have no completed evals yet", with Best return and Median Sharpe rendered as "—" despite 26 completed runs carrying real return data.

That contradiction was not a design choice — it was a data-join bug (runs joined on `run.strategy.id`, which the list endpoint never populates), and it is **fixed in this PR** (frontend falls back to `run.agent_id`; verified in `desktop-home-after-fix.png`). A second P0 — the entire Settings → General page crashing to "Something went wrong" — was a BigInt/number type mismatch, also **fixed and verified** (`desktop-settings-after-fix.png`).

The deeper issue remains: the home page answers none of the three questions an algorithmic-trading operator opens a dashboard to ask — *am I making money, what needs my attention, what should I do next* — and the strongest evidence the product works (the eval engine, the optimizer's experiment history, live runs) is scattered on other pages. The highest-leverage work before the hackathon is concentrating that existing evidence onto the first screen.

**Scorecard** (1–5 on each dimension):

| Page | Understanding | Trust | Conversion | Notes |
|---|---|---|---|---|
| Home / Dashboard | 2 | 1 → 3 (after fix) | 1 | Self-contradiction fixed; still no chart, no next action |
| Strategies list | 3 | 2 | 3 | "Untitled strategy / missing agent" drafts sort first |
| Strategy detail | 3 | 3 | 3 | Dense but coherent inspector |
| Eval list | 3 | 4 | 2 | Outcome columns (Return/Sharpe) dropped at 1440px |
| Eval run detail | 4 | 4 | 4 | Best page in the app; explains itself |
| Live cockpit | 2 | 3 | 2 | 20 anonymous "eval run ACTIVE —" chips; ULID title |
| Optimizer | 4 | 4 | 3 | Dense, alive; weak empty/waiting states |
| Marketplace | 5 | 4 | 5 | The reference standard for the rest of the app |
| Settings | 1 → 4 (after fix) | 1 → 4 | — | Was a hard crash on the default tab |
| Docs | 4 | 4 | — | Solid quickstart |
| Mobile (all) | 2 | 3 | 2 | Home = chat with leftover test messages; hero wrap fixed |

---

## Findings

Each finding: severity (P0 worst), the dimension it hurts (**U**nderstanding / **T**rust / **C**onversion), evidence, and the specific fix.

### P0 — broken or self-contradicting

**F1 · Dashboard contradicts itself — FIXED** (T, C) — `desktop-home.png`
"Completed evals 46" rendered directly above "No completed evals yet. Run an eval →" and "50 strategies have no completed evals yet"; Best return / Median Sharpe showed "—" despite 26 completed runs with nonzero returns. Root cause: `HomeOutcomeStrip`, `StrategyOutcomesSummary`, and `StrategyOutcomesList` join runs to strategies on `run.strategy?.id`, but `/api/eval/runs` (list) never populates `strategy` — only the single-run detail endpoint calls `enrich_run_summary_metadata` (`crates/xvision-engine/src/api/eval.rs:951`). **Fix shipped:** join falls back to `run.agent_id` (the exact key the backend uses for enrichment), with regression tests. Verified: `desktop-home-after-fix.png` shows Strongest recent / Needs review populated with real strategy names. **Follow-up (backend, recommended):** enrich `strategy`/`scenario` metadata in the list endpoint too, so every consumer gets display names.

**F2 · Settings page hard-crashes — FIXED** (T) — `desktop-settings-general.png`
`/settings/general` (the default Settings tab) rendered "Something went wrong. Please refresh the page." Root cause: `MemorySettingsCard.tsx:92` summed `live_observations` into a `0n` BigInt accumulator; ts-rs types the field as `bigint` but JSON.parse delivers plain numbers, so any non-empty memory namespace threw `Cannot mix BigInt and other types`. The unit-test fixture used `12n`, masking the bug. **Fix shipped:** sum as `Number`, fixture corrected to wire reality, regression test added. Verified: `desktop-settings-after-fix.png`.

**F3 · The dashboard is not a dashboard** (U, C) — `desktop-home.png`
Even after F1, ~60% of the first screen is empty black. No equity curve, no recent-run activity, no spend/cost, no optimizer pulse, and no primary CTA. An algorithmic-trading tool's home screen must answer: *am I making money? what needs attention? what do I do next?* Today the user must visit four pages to assemble that picture, while rich, demo-ready modules already exist elsewhere (eval detail's equity chart, optimizer's improvement-over-time plot and experiment table, live cockpit's metric cells). **Recommendation (big change — not done):** promote existing components into a 2×2 home grid below the outcome strip: (a) latest-run equity sparkline (the `Latest run chart` already rendered at the *bottom* of the Eval list — move/clone it here), (b) optimizer digest with its improvement plot, (c) live-trading strip with per-run PnL once available, (d) a persistent "next best action" card (no strategies → create; strategies but no evals → run eval; evals done → review best performer / deploy). Components exist; this is composition work, not new features.

### P1 — major friction on core flows

**F4 · Navigation alone creates junk data** (T) — `desktop-strategies.png`, `mobile-strategies.png`
Visiting `/strategies/new` immediately persists an "Untitled strategy" draft — the list count went 95 → 96 during this audit's read-only screenshot sweep. These drafts then sort first under the default "Recently added" sort, each wearing a red **missing agent** badge, so the first thing a visitor sees atop 95 strategies is broken-looking rows. Fix: defer persistence until the user makes any edit (or names the strategy); separately, exclude `display_name == "Untitled strategy" && agent_count == 0` drafts from the default list view or group them under a "Drafts" section with a bulk-delete affordance.

**F5 · Eval list hides the outcomes** (C) — `desktop-eval-runs.png`
At 1440×900 the run list shows only RUN / STRATEGY / SCENARIO; Return, Sharpe, Max DD, Status are responsively dropped (`useListColumns` priorities at `routes/eval-runs.tsx:331-345`) even though ample horizontal space remains. The page that exists to compare strategy performance shows no performance. Fix: make `return` and `sharpe` `essential: true` (or raise their priorities above `scenario`), and audit the width estimation — at this viewport the drop is premature.

**F6 · Live cockpit runs are anonymous** (U, C) — `desktop-live.png`
20 active runs render as identical chips: "eval run ACTIVE —". The page title's subtitle is a raw ULID (`01KTQNW57MWPZP14Y9R0XTKZQD`), which on mobile wraps mid-character. A trader cannot tell which strategy is which, which is live-money vs paper, or which is winning. Fix: chip = strategy display name (truncated) + mode badge + current PnL; subtitle = strategy name with the ULID demoted to a copyable mono chip. (Strategy names require F1's backend follow-up or a strategies-list join the page already has access to.)

**F7 · Mobile home is a chat window with someone else's test messages** (U, T) — `mobile-home.png`
On a phone, `/` renders the chat rail as the whole page — including persisted scratch messages ("test") and an assistant reply admitting *"I don't have access to a history of previous strategy evaluations"*. A first-run mobile user gets no dashboard at all, and the AI's first demonstrated behavior is failure. Fix: phone home should render the outcome strip + strategy outcomes (they're responsive already), with chat reachable via the dock; and the workspace-chat tool needs read access to eval history before it's the front door (see F12).

**F8 · Optimizer's waiting states look broken** (T) — `desktop-optimizer.png`
The strongest evidence the product works (experiment table with accept/reject deltas, improvement-over-time chart, lineage history) sits behind "Waiting for connection…", "Waiting for the cycle…", and empty plots that throw repeated uPlot `createLinearGradient: non-finite` console warnings when data is empty/degenerate. Fix: explicit idle-state copy ("No cycle running — start one with `xvn optimizer run` or ▶ Start"), and guard the sparkline data range before constructing the gradient (also silences F15).

### P2 — polish that moves trust

**F9 · Onboarding tour: wrong copy, popup pattern — copy FIXED** (T)
The driver.js welcome modal said "A quick **three-step** tour" while paginating "1 of **4**" (`onboarding/steps.ts:8`). **Copy fixed** to step-count-agnostic. The overlay itself also contradicts the project's own no-popups rule (CLAUDE.md) — consider converting the first-run tour to an inline dismissible strip on the dashboard; recommendation only.

**F10 · Mobile marketplace hero wrapped one word per line — FIXED** (U) — `mobile-marketplace.png`
`HeaderStrip`'s `flex justify-between` kept both Share buttons beside the headline at 390px, squeezing "Buy a strategy…" into a ~140px column. **Fixed:** stacks vertically below `sm:`.

**F11 · ULIDs as primary identity everywhere** (U)
Run titles ("Run #6/6"), live-cockpit subtitles, agent names ("clone C71QYKTP-1"), breadcrumbs all lead with 26-char ULIDs. Humans navigate by names; machines by ids. Fix pattern: name first, ULID as a small copyable mono chip; never let a ULID be a heading.

**F12 · The workspace AI can't answer the product's core question** (T) — `mobile-home.png`
"Tell me what happened with the latest strategy" → the assistant says it has no access to eval history. If the chat rail ships in the hackathon demo, wire it to `listRuns`/run-detail tools or scope its affordance copy to what it *can* do ("Ask about your current draft…").

**F13 · "paper · localhost" environment badge on a non-localhost deployment** (T)
The topbar badge says `paper · localhost` while served from `xvn.tail2bb69.ts.net`. Show actual host/profile, or drop the host segment and keep `paper`/`live` only — the mode is the part that matters for trust.

**F14 · Auth gate is theater** (T)
`/login` says "Dashboard access requires a session token… server validates XVN_DASHBOARD_TOKEN" — but `POST /api/auth/session` with `{}` returns a valid token on this deployment (env unset). Acceptable for a tailnet-only hackathon box, but either set the token on this host or have the login page detect open mode and say so plainly. A security-savvy judge probing the demo will find this in one curl.

**F15 · Console noise: repeated uPlot gradient warnings** (T)
Empty/degenerate series reach `createLinearGradient` with non-finite bounds on several pages (home, optimizer, settings). Anyone who opens DevTools during a demo sees a wall of warnings. Fix with F8's data guard.

### P3 — minor

**F16 · Setup page placeholder reads "Streaming…" while idle** (`desktop-setup.png`) — looks stuck before the user types anything; placeholder should be "Describe a strategy…" and only show Streaming during generation.
**F17 · Eval run detail leads with "0.00% TOTAL RETURN"** without explaining *why* (no trades). The mobile variant already shows "WIN RATE — / no trades" (`mobile-eval-run-detail.png`); surface the same "no trades executed" note next to the desktop hero number.
**F18 · Stats separators on marketplace mobile** dangle mid-wrap ("· " orphans) — minor; stacking from F10 mostly resolves it.

---

## Top 5 issues hurting conversion

1. **F3 — the home page doesn't sell the product it sits on.** Judges and new users form their model in the first 30 seconds; today that model is "empty tool". Composing existing modules onto the first screen is the single highest-ROI change before the hackathon.
2. **F1 — (fixed) the dashboard denied that any work had succeeded.** Until this PR deploys, the live site still tells every visitor the product has produced nothing. **Deploy the fix.**
3. **F5 — the eval list hides Return/Sharpe**, so "find the profitable strategy" — the core action — requires opening 50 detail pages one at a time.
4. **F6 — live trading (the money surface) is unreadable**: 20 identical anonymous chips and no PnL at a glance.
5. **F4 — broken-looking "Untitled / missing agent" rows greet every visitor** at the top of the strategies list, undermining trust right where buying/cloning decisions start.

## 5 quick wins (fixable today)

1. ✅ **Done** — dashboard join fallback (`run.strategy?.id ?? run.agent_id`) in the three home components; dashboard now shows real outcomes.
2. ✅ **Done** — Settings BigInt crash; whole settings area restored.
3. ✅ **Done** — tour copy "three-step" → step-count-agnostic.
4. ✅ **Done** — marketplace mobile hero stacks instead of one-word-per-line.
5. **~30 min** — F5: mark `return` + `sharpe` columns `essential` in `routes/eval-runs.tsx` so outcomes are always visible on the eval list.

---

## Method appendix

- Route inventory from `frontend/web/src/routes.tsx`; live entity ids pulled from `/api/strategies`, `/api/eval/runs`, `/api/scenarios`.
- Tour suppressed via `localStorage["xvn.onboarding.first-run-tour.completed"]="1"` before capture.
- **Capture gotcha:** `screenshot --full` (captureBeyondViewport) does not paint below-fold list rows in this app — rows exist in the DOM with no paint-skipping CSS, but full-page stitching misses them. Workaround used: set the viewport to the page's full height (`set viewport 1440 3600`) and take a normal screenshot. `desktop-strategies.png` retains the artifact deliberately as evidence; `desktop-eval-runs.png` was re-captured tall.
- Fixes verified by booting Vite dev (`npx vite --port 5180`) with `/api` proxied to the live deployment, then re-screenshotting (`*-after-fix.png`).
- The reusable flow for this audit is saved as the `design-audit` skill (`.claude/skills/design-audit/SKILL.md`).
