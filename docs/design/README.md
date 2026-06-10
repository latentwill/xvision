# xvision dashboard redesign — design README

**Status:** living document. Drafted 2026-06-10 as the executable brief for the
home-dashboard redesign; finalized alongside the implementation on
`feat/dashboard-redesign`. Before/after screenshots live in
`docs/design-audit/assets/` (`desktop-home*.png`, `*-after-redesign.png`).

## Why

Design-audit finding **F3: "the dashboard is not a dashboard."** ~60% of the
first screen was empty; the page was a stack of list rows with em-dash
metrics, a mistagged "Live trading · 20 active" banner, and a nagging
"44 strategies have no completed evals yet" footer. A trading operator landing
here must answer three questions without leaving the page:

1. **Am I making money?** (equity, PnL, win rate — with honest uncertainty)
2. **Is the machine doing good work?** (optimizer experiments, accepted
   improvements, eval throughput)
3. **What needs my attention next?** (critical findings, stale runs,
   strategies awaiting first eval)

## Data-correctness foundations (shipped with this redesign)

A beautiful dashboard on top of wrong numbers is worse than a list. Two fixes
land before any pixels:

- **"Live" means live.** The live strip previously counted *any non-terminal
  agent run* as ACTIVE — 100 orphaned `agent_runs` rows stuck in
  `status=running` (their parent eval runs long completed) rendered as "20
  active live strategies." `/api/agent-runs` now exposes the parent eval-run
  mode + status; liveness = live-money mode AND non-terminal parent. Orphans
  render as *stale*, never live.
- **Eval coverage counts CLI and optimizer evals.** Runs created via the CLI
  carry only the strategy *bundle hash* (`agent_id`), not the workspace ULID
  (`agents_agent_id`); the old join missed them, inflating "no eval" counts.
  Optimizer-generated strategies (lineage descendants) are now segmented from
  user-created ones — they are evaluated inside the optimizer loop and never
  nag.

## Aesthetic direction: **quant mission-control, calm density**

The app already has a voice — the `[ X V N ]` terminal mark, near-black
surfaces, neon-signal green `#00e676`, Geist + Geist Mono. The redesign
amplifies that voice instead of importing a new one:

- **Dense but legible bento.** Compact cards in a deliberate grid; clear
  hierarchy; generous *internal* spacing, tight *external* gutters. No
  full-screen voids, no wall-of-rows.
- **Numbers are the typography.** Tabular Geist Mono numerals at display
  sizes for the KPIs; labels stay small, uppercase, tracked, muted.
- **Color is signal, not decoration.** Green = realized gain / accepted
  experiment / genuinely live. Amber = suspect / stale / needs review.
  Red = drawdown / veto / failure. Everything else stays in the gray ramp.
  Low-opacity tints for fills; no white borders in dark mode (theme tokens
  only).
- **Atmosphere, not gimmicks.** Subtle panel gradients and a faint grain on
  the hero band; one orchestrated load stagger; micro-interactions on
  hover/drilldown. No popups, no modals — everything routes or
  inline-expands (house rule).
- **Honesty is a design feature.** Every metric ships with its sample size,
  data freshness, and simulated/paper/live-money state. Weak sample → explicit
  low-confidence chip. Drawdown is shown *with* return, never return alone.
  "No live capital deployed" is a first-class, well-designed state — not an
  apologetic em-dash.

## Page composition (home, desktop)

Single center column inside `DesktopThreePaneShell` (chat rail stays on the
right — no fourth column, per house rule). Top to bottom:

1. **Pulse band (hero).** Equity area chart of the latest meaningful run with
   drawdown band overlay, flanked by big-numeral KPIs: total return, max
   drawdown, win rate, evals completed / in flight. Each KPI carries a
   micro-sparkline and freshness stamp. Execution state chip
   (paper · localhost / live-money) is loud and honest.
2. **Live & attention strip.** Honest live counts (live-money / paper /
   stale) + critical findings + "awaiting first eval" segmented by origin
   (user vs optimizer). Each item is a routed drilldown, phrased as the next
   action — not a nag.
3. **Optimizer panel ("is the machine working?").** Experiments
   accepted / rejected-overfit ratio, avg ΔSharpe by writer model (the
   ladder), kept/suspect/dropped per recent cycle, cumulative spend, honesty
   check status. Idle state says *when* it last worked and what it found —
   never "Waiting for connection…".
4. **Strategy leaderboard.** Top user strategies by recent eval outcome as
   compact cards: return, Sharpe, max DD, trade count (sample size), origin
   chip, regime tag where known, low-confidence warning when n is small.
   Links to strategy detail / eval run.

Mobile: same sections stacked, chat reachable via dock (audit F7).

## Components & tech

- Charts: existing **uPlot** (equity + drawdown band + sparklines); guard all
  gradient construction against non-finite data (audit F8 console noise).
- New primitives live under `frontend/web/src/components/home/` and reuse
  `Card`/`Pill`/tokens. Data selectors live in `features/` with vitest
  coverage; components consume selectors, never raw API joins.
- Tokens: extend `styles/tokens.css` only when a value will be reused; no
  inline hex.

## Non-goals (this pass)

- No backend portfolio-aggregation endpoint (client-side aggregation over
  existing endpoints; candidate for follow-up — see ce-plan).
- No redesign of strategies/eval/optimizer detail pages (leaderboard and
  panels link into them as-is).
- No new charting library.
