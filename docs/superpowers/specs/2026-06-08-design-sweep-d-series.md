# Design Sweep — D-Series (net-new from skill audits)

**Date:** 2026-06-08
**Status:** D1 ready-to-work · D2–D5 scoped
**Parent:** extends `docs/superpowers/specs/2026-06-08-design-improvement-sweep-qa.md` (A/C series)
**Source:** reconciled from `docs/superpowers/audits/2026-06-08-master-implementation-list.md`
after the metaswarm readiness review removed already-merged and token-conflicting items.

## Why a D-series

The A/C sweep covered contrast (A1), eval-detail focal metric (A2), phantom Start (A3),
list ergonomics (A4), accent tokens (A5), SSE hook (A6), optimizer redesign (C1), and the
accent picker (C3). The skill audits surfaced clusters the A/C sweep does **not** touch.
Those, de-duplicated and corrected for token discipline, become D1–D5.

## Token discipline (inherited, non-negotiable)

- `--pos` / `--neg` = money only (gains / losses)
- `--gold` = running-state / kept-signal
- `--accent` / `--on-accent` = interactive chrome
- Never introduce a new profit/loss color token — `--pos`/`--neg` already exist and are live.

## Agent instructions

One PR per item. Tests when the change touches logic; none for pure CSS/token changes.
Work in `.worktrees/<track>` off `origin/main` — never the main checkout. Re-verify any
item tagged "VERIFY" against current code before scoping (the home page changed in the
Control Tower S1 redesign).

---

## D1 · Accessibility hardening (READY — first epic)

The A-series did contrast only. These are the remaining WCAG failures. Each is small and
independent; the focus-ring item has app-wide blast radius and must be tested across every
interactive element.

| ID | Item | Files | Effort | Blast radius |
|---|---|---|---|---|
| D1-1 | Remove invalid `role="link"` from `<tr>`; wrap a cell in a real `<Link>` (keep row onClick for mouse) | `routes/eval-runs.tsx:563`, `routes/scenarios.tsx:312`, `routes/agents.tsx:275` | S | Surface ×3 |
| D1-2 | Associate `<label>`↔`<select>` via `id`/`htmlFor` in the eval start form (strategy, scenario, provider, model) | `routes/eval-runs.tsx:862` | XS | Isolated |
| D1-3 | Global focus-ring system: add `--focus-ring` token + `:focus-visible` rule in globals; replace 68× `focus:outline-none`; fix `.input` focus | `styles/tokens.css`, `styles/globals.css` + callsites | M | **App-wide — test every interactive element** |
| D1-4 | Global `prefers-reduced-motion` block in globals.css | `styles/globals.css` | XS | App-wide (safe) |
| D1-5 | `Icon` gains optional `label` prop → `<title>` + `role="img"`; audit icon-only buttons | `components/primitives/Icon.tsx` + callsites | S | Component |
| D1-6 | `aria-expanded` on NagStrip toggle; `aria-live="polite"` on ActivityFeed; A/B/C/D non-color labels on compare run dots | `NagStrip.tsx:87`, `ActivityFeed.tsx:184`, `eval-compare.tsx:287` | S | Surface ×3 |
| D1-7 | `Pill` gains `disabled` prop (`opacity-40 pointer-events-none aria-disabled`) | `components/primitives/Pill.tsx` | XS | Component |

**D1 acceptance (epic-level):**
- [ ] No `role="link"` on any `<tr>`; keyboard + screen-reader navigation works on all three list tables
- [ ] All form controls have associated visible labels
- [ ] Every interactive element shows a visible focus ring on keyboard focus; nothing relies on `outline:none` alone
- [ ] `prefers-reduced-motion` disables non-essential animation app-wide
- [ ] Icon-only buttons have accessible names
- [ ] No color-only information in the compare table

---

## D2 · Motion system (scoped)

| ID | Item | Files | Effort |
|---|---|---|---|
| D2-1 | Motion tokens: `--duration-fast/base/slow`, `--ease-out`; wire to tailwind | `tokens.css`, `tailwind.config.ts` | S |
| D2-2 | List row stagger: one `@keyframes row-in` + index `animationDelay` (cap `min(i,8)·40ms`), applied across list routes | globals + list routes | S |
| D2-3 | `active:scale-[0.96]` on all primary CTAs | all primary buttons | XS |
| D2-4 | ActivityFeed SSE row entrance flash (`@keyframes flash-in`) | `ActivityFeed.tsx` | S |
| D2-5 | Stat-number update feedback (`key={value}` + `num-pop`) on eval detail | `eval-runs-detail.tsx` | S |

Depends on D1-4 (reduced-motion) landing first so all new motion respects it.

---

## D3 · Loading & empty-state correctness (scoped)

| ID | Item | Files | Effort |
|---|---|---|---|
| D3-1 | RecentSessionsList: replace `return null` on load with skeleton + `isError` branch | `OptimizerHome.tsx:262` | XS |
| D3-2 | Shape-matched skeletons + retry for CycleDetail / ExperimentDetail / strategy-detail bare "Loading…" text | `CycleDetail.tsx:32`, `ExperimentDetail.tsx:44`, `strategies-detail.tsx:301` | S |
| D3-3 | Eval empty-state: split zero-ever (onboarding CTA) from zero-filtered (clear-filters) | `eval-runs.tsx:399` | XS |
| D3-4 | ScenarioForm: collect all validation errors before return (no submit-fix-submit loop) | `ScenarioForm.tsx:145` | XS |

---

## D4 · IA flow gaps (scoped — some VERIFY)

| ID | Item | Files | Effort | Note |
|---|---|---|---|---|
| D4-1 | Add eval→optimizer path: "Optimize →" on completed run rows / detail | `eval-runs.tsx`, `eval-runs-detail.tsx` | XS | |
| D4-2 | Strategy detail "Run eval on this strategy →" CTA once readiness resolves | `strategies-detail.tsx` | XS | |
| D4-3 | Compare default sort → `net_return` desc + winner-row emphasis | `eval-compare.tsx:118` | XS | |
| D4-4 | ExperimentDetail: gate empty "What happened" section on data; move Decision above numbers | `ExperimentDetail.tsx` | XS | |
| D4-5 | Sidebar grouping + stronger active state | `Sidebar.tsx` | XS | VERIFY vs S1 |

---

## D5 · Copy pass (scoped — VERIFY against post-S1 home)

Mostly one-line string fixes (Group 8 of the master list). **Re-read `home.tsx` first** — the
S1 redesign may have already changed "cockpit" and live-capital copy. In scope: status-badge
operator labels (`QUEUED`→`WAITING`), "Tonight's run"→"Status and recent runs", remove em-dash
in template-picker copy, plain-language ScenarioForm help text, `decisionMode` label rename,
"Venue (Alpaca)"→"Simulation settings", empty Topbar subtitles.

---

## Dropped from the master list (do not work)

- **Already merged:** master items 1.1, 2.1, 2.6, 3.6, 3.7, 6.3, 6.4 (sweep A1/A2/A3/A5).
- **Token-conflict (contradict `--pos`/`--neg`/`--gold` discipline):** master item 3.8 (new `--profit` token — DROP; use `--pos`). Group 6 color recs re-tagged to `--pos`/`--neg` and folded where still needed.
- **In active C1:** master 5.4 (optimizer layout), 1.2 (PhaseStepper) — owned by the C1 phases, not D.

---

## Execution order

1. **D1** now (this epic) — accessibility is the highest-value net-new cluster and unblocks nothing else, so it can run fully parallel.
2. **D2** after D1-4 (reduced-motion guard) lands.
3. **D3**, **D4**, **D5** independent; D5 only after a home-page re-verify.
