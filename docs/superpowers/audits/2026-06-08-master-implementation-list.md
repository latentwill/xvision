# xvision Design Audit — Master Implementation List
**Date**: 2026-06-08  
**Source**: 3 audit docs × 15 skill/area lenses, de-duplicated and evaluated  
**Total unique items**: 94

---

## ⚠️ READINESS REVIEW (metaswarm, 2026-06-08) — DO NOT HAND OFF AS-IS

**Verdict: NOT ready for direct implementation. Must be reconciled against the active design sweep first.**

A mature, prototype-grounded design sweep is already in flight and **partially merged to `main`**:
`docs/superpowers/specs/2026-06-08-design-improvement-sweep-qa.md` (canonical) + prototypes in
`docs/design/DesignImprovementSweep/`. Handing this 94-item list to agents as-is would cause
duplicate work on already-merged fixes and would **violate the sweep's locked token discipline**.

### Evidence — items already DONE on `main` (verified in live code)

| Master item | Sweep ID / PR | Live-code evidence | Status |
|---|---|---|---|
| 1.1 Optimizer Start no-op | A3 / #862 | no `scrollIntoView` in `OptimizerHome.tsx` | ✅ DONE |
| 2.1 `--text-3` contrast | A1 / #860 (Move 06) | `--text-3: #9aa3b2` (~7:1) in `tokens.css:25` | ✅ DONE (exceeds my proposed `#7a8390`) |
| 2.6 `.caps` legibility | A1 / #860 | part of legibility lift | ✅ DONE (verify) |
| 6.3 eval-detail UUID h1 | A2 / #866 (Move 04) | `<h1 text-5xl font-bold tabular-nums text-pos>` `eval-runs-detail.tsx:282` | ✅ DONE |
| 6.4 eval-detail metric hero | A2 / #866 | same focal-metric header | ✅ DONE |
| 3.6 / 3.7 accent tokens | A5 / #867 area | `accent`/`on-accent` in `tailwind.config.ts:26-27` | ✅ DONE |
| 5.4 optimizer layout/hierarchy | C1 Phase 2a / #870 | CommandBar + Launch button shipped | 🟡 IN PROGRESS (C1 phases 2b–4 pending) |
| 1.2 PhaseStepper state | C1 / #870 | phase spine visible when running | 🟡 PARTIAL |
| (new capability) | A4 / #868 | column picker + scroll affordance | ✅ DONE (beyond audit scope) |

### Conflicts — items that CONTRADICT the locked token discipline

The sweep locks (spec §"Token discipline", and live `eval-runs-detail.tsx` already follows it):
**`--pos`/`--neg` = money (gains/losses) only · `--gold` = running-state / kept-signal · `--accent` = interactive chrome.**

These master-list items must be **rewritten or dropped** before work:

- **Group 6 (all "positive → `text-gold`" / "`border-l-gold` for wins")** → must use **`text-pos` / `border-pos`**, not `--gold`. The just-merged eval-detail header already proves the correct pattern (`text-pos`).
- **Item 3.8 (add new `--profit: #22c55e`)** → **DROP.** The token already exists as `--pos`. Adding `--profit` forks the money-color system.
- **Items 6.1, 6.2, 6.8, 6.10, 10.6** → re-tag colors to `--pos`/`--neg`/`--gold` per discipline.

### Genuinely NET-NEW and additive (the real value of these audits)

The sweep does **not** cover these — they are the items worth folding into new work tracks:

- **Accessibility beyond contrast** (sweep only did contrast): 1.3 `<tr role=link>` invalid ARIA, 1.4 label/select association, 2.4 68× `focus:outline-none` with no ring, 2.5 no global `prefers-reduced-motion`, 2.7 Icon `label` prop, 2.9 `aria-expanded`, 2.10 `aria-live` on ActivityFeed, 2.11 colorblind run dots. **Highest-value net-new cluster.**
- **Motion system**: 3.4 motion tokens, 4.1 row stagger, 4.3 `active:scale` on CTAs, 4.4 SSE row flash, 4.10 stat-number feedback.
- **Loading/empty-state correctness**: 7.1 RecentSessionsList null-on-load, 7.2/7.3/7.4 bare "Loading…" text, 7.7 empty-state conflation, 9.1 ScenarioForm sequential validation.
- **IA flow gaps**: 5.5 eval→optimizer path, 5.6 strategy-detail dead-end, 5.7 compare default sort, 5.9/5.10 ExperimentDetail empty/ordering.
- **Copy** (Group 8): mostly net-new, BUT several (8.1 "cockpit", 8.14 live-capital) reference pre-S1-redesign home; **re-verify against current `home.tsx` before scoping** — the home page was restructured in the Control Tower S1 redesign (see sweep C2 finding).

### Recommended path to "ready for work" (no code yet)

1. **Canonical backlog = the sweep spec**, not this list. This list is a reviewed *inventory*, not a work plan.
2. **Drop** the ✅ DONE rows above. **Rewrite** the token-conflict rows to `--pos`/`--neg`/`--gold` discipline.
3. **Fold the net-new clusters into the sweep's A/D-series naming** (proposed: **D1 Accessibility hardening**, **D2 Motion system**, **D3 Loading/empty-state correctness**, **D4 IA flow gaps**, **D5 Copy pass**) — one PR per item, same agent instructions as the sweep's Part A.
4. **Re-verify Group 8 copy + Group 5 home-layout items** against current `home.tsx` (post-S1) before scoping.
5. **Worktree discipline (CLAUDE.md):** current checkout is on `main` — no implementation here. Each D-track runs in `.worktrees/<track>` off `origin/main`.
6. **Beads not yet created** — deliberately. Creating ~30 issues now would duplicate merged work. Create the D-series epic + issues only after steps 2–4 trim the list. Recommended first epic: `bd create --title "Design sweep D1: accessibility hardening" --type epic --priority 1`.

**Bottom line:** ~12 of the audit's flagged P0/P1 items are already done; ~6 conflict with token rules; the durable value is the **accessibility, motion, and loading-state net-new clusters** — fold those into the existing sweep rather than running this list standalone.

---

## Scoring key

| Field | Scale |
|---|---|
| **Priority** | P0 broken/critical · P1 high impact · P2 medium polish · P3 low polish · P4 nice-to-have |
| **Effort** | XS < 30 min · S 30 min–2 hr · M 2–4 hr · L 4–8 hr · XL > 1 day |
| **Blast radius** | Isolated (1 file) · Component (1 primitive, many consumers) · Surface (1 page/feature) · App-wide (tokens/globals) |

Blast radius notes flag where a change is likely to break something if done carelessly.

---

## GROUP 1 — Broken / Non-Functional

These are features that literally do not work as labeled.

| # | Item | File | Priority | Effort | Blast radius | Notes |
|---|---|---|---|---|---|---|
| 1.1 | **Optimizer Start button is a no-op** — calls scrollIntoView on itself, never POSTs /sessions | `OptimizerHome.tsx:193` | P0 | M | Surface | Needs strategy dropdown added; no downstream risk once wired |
| 1.2 | **PhaseStepper always shows null** — `currentPhase={null}` passed while session is running; all chips render grey | `OptimizerHome.tsx` | P0 | S | Surface | Needs `current_phase` exposed from session API or hide stepper entirely when null |
| 1.3 | **`<tr role="link">` is invalid ARIA** — conflicts with implicit `row` role; screen readers broken | `eval-runs.tsx:563`, `scenarios.tsx:312`, `agents.tsx:275` | P0 | S | Surface ×3 | Remove `role="link"`, wrap cell content in `<Link>`. Low change risk but touches 3 route files |
| 1.4 | **`<label>` not associated with `<select>`** — no `id`/`htmlFor`; broken screen-reader association for eval start form | `eval-runs.tsx:862-870` | P0 | XS | Isolated | Add `id` to selects + `htmlFor` to labels. Zero risk. |
| 1.5 | **Strategy color assigned nowhere visible** — `ColorPickerRow` lets operators pick a color; it renders on zero list rows or detail pages | `strategies.tsx:429`, `strategies-detail.tsx` | P1 | S | Surface ×2 | Add color dot to list row + `h-0.5` accent line on detail. No data model change needed. |

---

## GROUP 2 — Accessibility (WCAG Failures)

Real failures at AA level. Fix regardless of hackathon.

| # | Item | File | Priority | Effort | Blast radius | Notes |
|---|---|---|---|---|---|---|
| 2.1 | **`--text-3` fails 4.5:1 contrast** — `#5f6670` on `#0a0a0a` ≈ 3.6:1; used pervasively as label/meta color | `tokens.css:25` | P0 | XS | App-wide | Change `--text-3: #7a8390`. **Single token, fixes ~80% of contrast issues.** Verify light theme counterpart too. |
| 2.2 | **Placeholder color fails contrast** — `--text-4` on `--surface-elev` ≈ 1.7:1 | `globals.css:27-31` | P0 | XS | App-wide | Change placeholder to `var(--text-3)` after fixing 2.1. Zero risk. |
| 2.3 | **Light mode `--on-accent` fails** — `#000` on `#00a15c` = 4.1:1 (need 4.5:1) | `tokens.css:89` | P0 | XS | Component | Change light mode `--on-accent: #ffffff`. ~5.2:1. Only affects light-mode CTAs. |
| 2.4 | **68× `focus:outline-none` with no ring replacement** — keyboard nav invisible throughout | App-wide | P0 | M | App-wide | Add `:focus-visible { box-shadow: var(--focus-ring); }` to globals + `--focus-ring` token. Then audit callsites. **High blast radius** — test every interactive element after. |
| 2.5 | **No global `prefers-reduced-motion` block** — only `xvn-pill-animated` has override; all other animations fire regardless of OS setting | `globals.css` | P0 | XS | App-wide | Add `@media (prefers-reduced-motion: reduce) { *, *::before, *::after { animation-duration: 0.01ms !important; } }` at bottom of globals. Safe. |
| 2.6 | **`.caps` class: 10.5px + `--text-3` fails both size and contrast** | `globals.css:37-44` | P1 | XS | Component | Raise to 11px, `--text-2` for semantic labels. Reduces tracking 0.10 → 0.08em. Visual-only. |
| 2.7 | **`Icon` has no `label` prop** — icon-only buttons have no accessible name | `components/primitives/Icon.tsx` | P1 | S | Component | Add `label?: string` → `<title>` + `role='img'`. Backward compat: only applies when prop provided. Audit icon-only button callsites. |
| 2.8 | **`Pill` has no disabled state** — no `aria-disabled`, no `pointer-events-none` variant | `components/primitives/Pill.tsx` | P2 | XS | Component | Add `disabled?: boolean` prop. Pure additive. |
| 2.9 | **NagStrip toggle missing `aria-expanded`** | `NagStrip.tsx:87-95` | P2 | XS | Isolated | `aria-expanded={showAll}` + `aria-controls`. Trivial. |
| 2.10 | **ActivityFeed has no `aria-live` region** — SSE updates invisible to AT | `ActivityFeed.tsx:184` | P2 | XS | Isolated | `aria-live='polite'` on scroll container. No visual change. |
| 2.11 | **Color dots in compare table have no non-color fallback** — color-blind users can't distinguish runs | `eval-compare.tsx:287` | P2 | XS | Isolated | Add `A`/`B`/`C`/`D` positional text labels. |
| 2.12 | **`.input` focus indicator: 1px border-color change, not WCAG 2.4.11** | `globals.css:56` | P1 | XS | Component | Replace `focus:border-text-3` with `focus-visible:ring-2 focus-visible:ring-gold`. Handled by 2.4 global but `.input` needs explicit override. |

---

## GROUP 3 — CSS / Design Token Foundations

These unlock everything else. Do once, benefit everywhere.

| # | Item | File | Priority | Effort | Blast radius | Notes |
|---|---|---|---|---|---|---|
| 3.1 | **No shadow tokens** — `shadow-sm` is invisible on dark backgrounds; no `--shadow-card`, `--shadow-float` | `tokens.css` | P1 | XS | App-wide | Add 3 shadow tokens. Then update `Card.tsx` to use `--shadow-card` by default. Visual only. |
| 3.2 | **Surface ladder too narrow** — `--bg:#000` to `--surface-panel:#121212` spans 18 lightness steps; cards invisible on OLED | `tokens.css:11-17` | P1 | XS | App-wide | Widen: `--bg:#050505`, `--surface-card:#0F0F0F`, `--surface-elev:#141414`, `--surface-panel:#1C1C1C`. **Test across all surfaces** — will change perceived depth everywhere. |
| 3.3 | **`--surface-hover` at 4% white below perceptual threshold** — hover states effectively invisible | `tokens.css:17` | P1 | XS | App-wide | Raise to `rgba(255,255,255,0.07)`. Check light mode counterpart. |
| 3.4 | **No motion token system** — every component hardcodes timing independently | `tokens.css` | P1 | S | App-wide | Add `--duration-fast:80ms`, `--duration-base:160ms`, `--duration-slow:240ms`, `--ease-out:cubic-bezier(0.2,0,0,1)`. Wire to tailwind. Additive only. |
| 3.5 | **`--text-3` token lacks semantic alias** — 4 positional tokens, no names for intent | `tokens.css` | P2 | XS | App-wide | Add `--text-body`, `--text-supporting`, `--text-meta`, `--text-muted` as aliases. Additive, zero risk. |
| 3.6 | **`--gold` naming debt** — green token named gold; `--gold` and `--accent` are duplicate identical values | `tokens.css:31`, `tailwind.config.ts` | P2 | XS → L | App-wide | XS: Add `--brand: var(--gold)` alias + remove `--accent` duplicate. **Do not rename `--gold` yet** — that's an L-effort codemod with high blast radius. |
| 3.7 | **No `--action-primary` token** — brand accent doubles as CTA color with no seam | `tokens.css` | P2 | XS | App-wide | Add `--action-primary: var(--gold)` etc. Additive. |
| 3.8 | **No `--success`/`--profit` token** — `--gold` (green) ambiguous: brand AND profit data signal | `tokens.css` | P2 | S | App-wide | Add `--profit: #22c55e` distinct from `--gold: #00e676`. Update `signedToneClass` and `pnlClass` to use `--profit` for positive values. Medium blast radius: touches eval-runs, compare, StrategyOutcomesList. |
| 3.9 | **No `--focus-ring` token** (companion to 2.4) | `tokens.css` | P0 | XS | App-wide | `--focus-ring: 0 0 0 2px var(--bg), 0 0 0 4px var(--gold)`. Do with 2.4. |
| 3.10 | **No `text-wrap: balance` on headings** — orphan words on card titles | `globals.css` | P2 | XS | App-wide | `@layer base { h1,h2,h3 { text-wrap:balance; } p { text-wrap:pretty; } }`. Safe. |
| 3.11 | **Duplicate `:root` and `[data-theme=dark]` blocks** — same 27 tokens in two places | `tokens.css` | P3 | S | App-wide | Remove `:root` block; set `data-theme='dark'` as default in ThemeProvider. Test theme init timing. |
| 3.12 | **`font-serif` alias maps to Geist (not serif)** | `tailwind.config.ts` | P3 | XS | Isolated | Rename to `font-display` or remove. Rename `.serif-i` to `.display-bold`. |
| 3.13 | **Border radius vocabulary: only `rounded-card` (6px)** — all others use ad-hoc Tailwind | `tailwind.config.ts` | P3 | S | App-wide | Add `pill:'3px'`, `input:'4px'`, `lg:'8px'` to tailwind config. Low risk but requires audit of existing usages. |
| 3.14 | **No disabled token set** | `tokens.css` | P3 | XS | Isolated | `--text-disabled`, `--surface-disabled`, `--border-disabled`. Additive. |
| 3.15 | **Chart palette: 3 perceptually similar blue-green hues** — fails deuteranopia | `theme/themes.ts:186` | P2 | XS | Isolated | Replace `#22D3EE` → `#F59E0B` (amber) and `#5EEAD4` → `#E879F9` (fuchsia). Only affects strategy comparison charts. |

---

## GROUP 4 — Motion & Animation

CSS-only changes. Big demo impact, low risk.

| # | Item | File | Priority | Effort | Blast radius | Notes |
|---|---|---|---|---|---|---|
| 4.1 | **No row stagger animations on any list** — all rows snap in simultaneously | All list routes | P1 | S | Surface | Add `@keyframes row-in` + index-based `animationDelay`. Cap at `min(i,8)*40ms`. One keyframe, apply everywhere. |
| 4.2 | **No panel enter/exit animations** — StartEvalPanel, OptimizerSections, chart toggles all snap | Multiple | P1 | S | Surface | `@keyframes fade-in` + height-transition pattern. Reuse same keyframe. |
| 4.3 | **`active:scale-[0.96]` missing on all primary CTAs** — clicking feels weightless | All primary buttons | P1 | XS | Component | Add `active:scale-[0.96] transition-[transform,background-color] duration-75` to every primary button. Safe. |
| 4.4 | **ActivityFeed SSE rows appear silently** — new events have no entry flash | `ActivityFeed.tsx:186` | P1 | S | Isolated | `@keyframes flash-in` + row-mounted animation. Pure additive. |
| 4.5 | **ProgressDial arc doesn't animate on fill** — static progress ring | `ProgressDial.tsx:18` | P2 | XS | Isolated | `transition: stroke-dashoffset 600ms cubic-bezier(0.4,0,0.2,1)` on fill circle. |
| 4.6 | **PhaseStepper transitions: no transform, chips pop** | `PhaseStepper.tsx:58` | P2 | XS | Isolated | `transition-[colors,opacity,transform] duration-300` on chips. |
| 4.7 | **RecentSessionRow arrow blinks in (no slide)** | `OptimizerHome.tsx:255` | P3 | XS | Isolated | `translate-x-[-4px] group-hover:translate-x-0 transition-[opacity,transform] duration-150`. |
| 4.8 | **ScheduleStrip accordion snaps open/closed** | `ScheduleStrip.tsx:98` | P3 | XS | Isolated | `max-h-0 overflow-hidden transition-[max-height] duration-200`. |
| 4.9 | **Pill sweep animation uses `ease-in-out` (wrong direction)** | `globals.css:248` | P3 | XS | Isolated | Change sweep to `cubic-bezier(0.4,0,0.6,1)`. |
| 4.10 | **Stat numbers update instantly with no visual feedback** | `eval-runs-detail.tsx:749` | P2 | S | Isolated | `key={value}` + `@keyframes num-pop`. |
| 4.11 | **SafetyPauseBanner dot is static** — critical halt state should pulse | `SafetyPauseBanner.tsx:31` | P2 | XS | Isolated | `animate-ping` outer ring. One of few justified pulse uses. |

---

## GROUP 5 — Information Architecture & Navigation

Layout and flow decisions. Medium blast radius (page-level).

| # | Item | File | Priority | Effort | Blast radius | Notes |
|---|---|---|---|---|---|---|
| 5.1 | **Sidebar: 10 equal-weight items, no grouping** — core workflow indistinguishable from utility | `Sidebar.tsx:13` | P1 | XS | Isolated | Add `<hr>` dividers + optional group labels. Pure CSS/JSX. |
| 5.2 | **Sidebar: active state at 6% opacity — imperceptible** | `Sidebar.tsx:54` | P1 | XS | Isolated | Active: `bg-gold/[0.10] border-l-[3px] font-medium`. Inactive hover: `hover:bg-white/[0.03]`. |
| 5.3 | **Home: StrategyOutcomesList buried at section 5 of 7** | `home.tsx:77` | P1 | S | Surface | Reorder sections. No data change. |
| 5.4 | **Optimizer: 9 equal-weight sections, no idle/running layout split** | `OptimizerHome.tsx` | P1 | M | Surface | Conditional rendering based on `isActive`. Medium effort but contained to one page. |
| 5.5 | **Eval list → Optimizer: no path exists** — completed run has no "Optimize →" action | `eval-runs.tsx`, `eval-runs-detail.tsx` | P1 | XS | Surface ×2 | Add link to `/optimizer?strategy=<id>` on completed rows. Additive. |
| 5.6 | **Strategy detail dead-ends** — no "run eval" next step after create | `strategies-detail.tsx` | P1 | XS | Isolated | Add `Run eval on this strategy →` CTA once agent-readiness resolves ready. |
| 5.7 | **Compare defaults to call_order sort** — winner not obvious | `eval-compare.tsx:118` | P1 | XS | Isolated | `useState<CompareSortKey>('net_return')`. One-line change. |
| 5.8 | **Eval detail: stat grid renders BELOW chart** — verdict is item 3 | `eval-runs-detail.tsx:682` | P1 | XS | Isolated | Move stat grid before chart. JSX reorder, no logic change. |
| 5.9 | **ExperimentDetail: "What happened" always empty/visible** — placeholder section pollutes layout | `ExperimentDetail.tsx:85` | P1 | XS | Isolated | Gate on `detail?.events?.length > 0`. One conditional. |
| 5.10 | **ExperimentDetail: Decision section renders AFTER numbers** — verdict should lead | `ExperimentDetail.tsx:24` | P2 | XS | Isolated | JSX reorder. Zero risk. |
| 5.11 | **Agents: Memory/Skills nav at same weight as "New agent" CTA** | `agents.tsx:163` | P2 | S | Surface | Move Memory/Skills to tab strip or Topbar sub. |
| 5.12 | **Strategies list: no eval count or performance data visible** | `strategies.tsx:280` | P2 | M | Surface | Add "Evals" column + "Run eval →" in row action menu. Requires passing count through API. |
| 5.13 | **"Latest run chart" shows wrong data when filters active** | `eval-runs.tsx:451` | P2 | XS | Isolated | Hide when filters active. One conditional. |
| 5.14 | **"Draft variant →" links to read-only run detail** (misleading CTA label) | `CriticalFindingsRow.tsx:37` | P2 | XS | Isolated | Rename to `View run →`. |

---

## GROUP 6 — Financial Data Display

The app is about profitable strategies. These make that visible.

| # | Item | File | Priority | Effort | Blast radius | Notes |
|---|---|---|---|---|---|---|
| 6.1 | **Win/loss row coloring invisible** — `bg-green-500/5` on black = 5% opacity = nothing | `StrategyOutcomesList.tsx:42` | P0 | XS | Isolated | `bg-[#00e676]/[0.08] border-l-2 border-l-[#00e676]` for win. One className change. |
| 6.2 | **Return % values at 13px body weight** — same size as metadata labels | `StrategyOutcomesList.tsx:57`, `eval-runs.tsx:631` | P1 | XS | Surface ×2 | `text-[20px] font-bold tabular-nums` for wins. `text-[14px] font-medium` for table column. |
| 6.3 | **Eval detail page h1 is a UUID** — first thing user sees is a 36-char hex string | `eval-runs-detail.tsx:263` | P1 | XS | Isolated | h1 → strategy name. UUID → secondary `font-mono text-[11px] text-text-3 select-all`. |
| 6.4 | **Eval detail: no hero return number above fold** | `eval-runs-detail.tsx` | P1 | M | Isolated | Add `MetricHero` component: Return % at `text-[56px] font-bold tabular-nums` immediately below topbar. |
| 6.5 | **Return column position 9 of 13 in eval list** | `eval-runs.tsx:330` | P1 | XS | Isolated | Move Return to column 6 (after status). JSX reorder. |
| 6.6 | **Compare: winner row has no visual emphasis** | `eval-compare.tsx:118` | P1 | XS | Isolated | Gold ring + `✦ BEST` badge on top row after sort. Additive. |
| 6.7 | **SummaryCard no elevation over secondary cards** — all cards identical depth | `eval-runs-detail.tsx:576` | P2 | XS | Isolated | `border-profit/40` + `border-l-2` accent on SummaryCard for completed profitable runs. |
| 6.8 | **Optimizer stat strip treats "kept" same as "dropped"** | `OptimizerHome.tsx` | P1 | XS | Isolated | `kept_count` → `text-gold font-semibold`, `dropped_count` → `text-text-3`. |
| 6.9 | **Win activation threshold requires 10 evals** — never shows for most users | `StrategyOutcomesList.tsx:26` | P1 | XS | Isolated | Lower to 3. One constant change. |
| 6.10 | **`text-emerald-400`/`text-red-400` bypass design tokens** | `FlywheelStrip.tsx:43`, `LiveStrategiesSection.tsx` | P2 | XS | Isolated | `text-emerald-400` → `text-gold`, `text-red-400` → `text-danger`. Two grep-replaces. |

---

## GROUP 7 — Loading & Empty States

These cause layout shift and telegraph "unpolished" loudly.

| # | Item | File | Priority | Effort | Blast radius | Notes |
|---|---|---|---|---|---|---|
| 7.1 | **RecentSessionsList returns `null` on load** — section disappears, then snaps back | `OptimizerHome.tsx:262` | P1 | XS | Isolated | Replace `return null` with 3-row skeleton. Add `isError` branch. |
| 7.2 | **CycleDetail loading: bare "Loading cycle…" text** | `CycleDetail.tsx:32` | P1 | XS | Isolated | Shape-matched skeleton + error panel with retry. |
| 7.3 | **ExperimentDetail loading: bare "Loading experiment…" text** | `ExperimentDetail.tsx:44` | P1 | XS | Isolated | Same fix as 7.2. |
| 7.4 | **Strategy detail loading: bare text** | `strategies-detail.tsx:301` | P1 | XS | Isolated | Width-graded skeleton (title + subtitle + 4 dl rows). |
| 7.5 | **ActiveTasksStrip renders "No active tasks" permanently** — dead-weight above-fold card | `ActiveTasksStrip.tsx:131` | P1 | XS | Isolated | `return null` when `inflight.length === 0`. |
| 7.6 | **Home: 7 parallel queries with no coordinated loading gate** — cascade blink-in | `home.tsx:27` | P2 | S | Surface | Gate page render on `runs.isPending && strategies.isPending` — show single skeleton. |
| 7.7 | **Eval empty state conflates zero-evals-ever with zero-filter-results** | `eval-runs.tsx:399` | P1 | XS | Isolated | Split: when `totalRows === 0` → onboarding copy + CTA; when filters zero → "clear filters" button. |
| 7.8 | **Zero-strategy empty state is one sentence with a link** — highest-stakes empty state | `StrategyOutcomesList.tsx:200` | P1 | S | Isolated | Add 3-step onboarding inline card. |
| 7.9 | **Strategy detail error state has no retry** | `strategies-detail.tsx:308` | P2 | XS | Isolated | Add `<button onClick={() => query.refetch()}>Try again</button>`. |
| 7.10 | **Compare loading: non-structural single-bar skeleton** | `eval-compare.tsx:88` | P2 | S | Isolated | Structural skeleton matching MetricsTable column layout. |

---

## GROUP 8 — Copy & Microcopy

Many are one-line fixes with outsized legibility impact.

| # | Item | File | Priority | Effort | Blast radius | Notes |
|---|---|---|---|---|---|---|
| 8.1 | **"cockpit · N strategies" subtitle** — jargon that means nothing | `home.tsx:72` | P1 | XS | Isolated | `{n} strategies · {m} eval runs`. One string. |
| 8.2 | **"Tonight's run…" subtitle on Optimizer** — wrong 23 hrs/day | `OptimizerHome.tsx:322` | P1 | XS | Isolated | "Status and recent runs". |
| 8.3 | **Optimizer idle h2: "No run in progress"** in `text-3` color — dismissive | `OptimizerHome.tsx:323` | P1 | XS | Isolated | "Ready to optimize" in `text` color + two-line context sentence. |
| 8.4 | **"Experiment writers" → operator-facing jargon** | `ExperimentWritersPanel` | P2 | XS | Isolated | Panel heading → "Variation approaches". |
| 8.5 | **Status badge shows `QUEUED`/`RUNNING` (internal tokens)** | `eval-runs-detail.tsx:693` | P1 | XS | Isolated | Map: `queued → WAITING, running → IN PROGRESS, cancelled → STOPPED`. |
| 8.6 | **"Compare needs two or more runs" — passive system copy** | `eval-compare.tsx:489` | P2 | XS | Isolated | "Pick runs to compare". |
| 8.7 | **ScenarioForm context-bars help text uses `t=0`, `≥`, EMA jargon** | `ScenarioForm.tsx:326` | P2 | XS | Isolated | Plain-language rewrite. |
| 8.8 | **"Venue (Alpaca)" fieldset — internal implementation label** | `ScenarioForm.tsx:338` | P2 | XS | Isolated | "Simulation settings". |
| 8.9 | **`decisionMode` labels: "filter-gated agent", "agent-direct"** | `strategies.tsx` | P2 | XS | Isolated | → "AI + filter", "AI only", "Rules only", "Setup needed". |
| 8.10 | **"from 3 most recent reviews" — wrong terminology** (not "reviews") | `CriticalFindingsRow.tsx:78` | P3 | XS | Isolated | "from 3 most recent eval runs". |
| 8.11 | **AgentEdit subtitle is raw ULID in edit mode** | `agents-edit.tsx:31` | P2 | XS | Isolated | "Edit agent configuration" or resolve to agent name. |
| 8.12 | **Template picker copy uses em-dash** — banned in project | `agents-edit.tsx:82` | P2 | XS | Isolated | Rewrite without em-dash. |
| 8.13 | **`scenarios-new.tsx` Topbar `sub=''`** — empty subtitle wastes slot | `scenarios-new.tsx:36` | P3 | XS | Isolated | "Define a backtest window and market context". |
| 8.14 | **"Live strategies trade real capital." reads as alarm** | `LiveStrategiesSection.tsx:48` | P2 | XS | Isolated | "No strategies are running live. Connect a broker to enable live deployment." |
| 8.15 | **ScenarioForm "Create →" — arrow glyph on button label** | `ScenarioForm.tsx:410` | P3 | XS | Isolated | "Create scenario". Remove `→`. |

---

## GROUP 9 — Form UX

| # | Item | File | Priority | Effort | Blast radius | Notes |
|---|---|---|---|---|---|---|
| 9.1 | **ScenarioForm: sequential validation shows one error at a time** | `ScenarioForm.tsx:145` | P1 | XS | Isolated | Run all checks, set all errors, then `return`. |
| 9.2 | **InlineEditField: edit entrypoint invisible** — no hover cue the field is editable | `strategies-detail.tsx:330` | P1 | XS | Isolated | Pass `displayClassName="hover:underline decoration-dashed cursor-text"`. |
| 9.3 | **TemplateCard: no `active:scale` on the most important click in agents flow** | `agents-edit.tsx:93` | P1 | XS | Isolated | `active:scale-[0.96] transition-[transform,…]`. |
| 9.4 | **ScenarioForm Name `required` attr + JS validation = double error surfaces** | `ScenarioForm.tsx:218` | P2 | XS | Isolated | Remove `required`, add `placeholder`. |
| 9.5 | **ColorPickerRow swatches: 24×24px below 40px hit target** | `strategies-detail.tsx:115` | P2 | XS | Isolated | `p-2` wrapper. |
| 9.6 | **ScenarioForm Advanced section: no accordion animation, Unicode arrows** | `ScenarioForm.tsx:344` | P3 | XS | Isolated | `max-h` transition + chevron icon with `rotate-90`. |

---

## GROUP 10 — Visual Identity / Boldness

Higher effort, higher reward. Require design judgment.

| # | Item | File | Priority | Effort | Blast radius | Notes |
|---|---|---|---|---|---|---|
| 10.1 | **Card primitive has no depth (no shadow, no variants)** | `Card.tsx` | P1 | S | Component | Apply `--shadow-card` token (from 3.1). Add `variant='elevated'/'accent'` props. **Highest blast radius in this group** — Card is used everywhere; test all surfaces. |
| 10.2 | **Pill tones all look identical** — border-only with matching text | `Pill.tsx` | P2 | XS | Component | Add 8% fill to `danger`/`warn`/`info` tones. Add `muted` tone. Visual-only change. |
| 10.3 | **Pill radius 2px (rounded-sm) looks square** at 11px text | `Pill.tsx` | P3 | XS | Component | `rounded-[3px]`. Visual-only. |
| 10.4 | **ProgressDial: 64px default, arc track too light, % too small** | `ProgressDial.tsx` | P1 | XS | Isolated | Default 96px, stroke 8px, % text `text-[17px] font-bold`. |
| 10.5 | **HashSigil: flat grey box has no visual character** | `HashSigil.tsx:19` | P2 | S | Isolated | Radial gradient from accent color, colored border, default 80px. Fix `--violet` token too. |
| 10.6 | **GateBadge "Kept" looks like a neutral tag** — success feels like nothing | `GateBadge.tsx:12` | P2 | XS | Isolated | `font-semibold` + `✓ Kept` prefix, `bg-gold/[0.15] border-gold/60`. |
| 10.7 | **Atmospheric depth: pure flat black reads as unfinished** | `globals.css` / main shell | P2 | S | App-wide | Add hairline radial gradients to sidebar and content: `radial-gradient(ellipse at 50% 0%, rgba(0,230,118,0.025) 0%, transparent 60%)`. Visual-only but **test across light mode too**. |
| 10.8 | **Typography: single font system, no display face** | `tailwind.config.ts` | P3 | M | App-wide | Add `Instrument Serif` or `Fraunces` as `font-display` for page titles and result heroes. Font load risk — test FOUT. |
| 10.9 | **ActivityFeed: event types all look identical** | `ActivityFeed.tsx:180` | P2 | XS | Isolated | Left-accent by event keyword: accepted → `border-l-2 border-l-gold/70`, rejected → `border-l-2 border-l-danger/50`. |
| 10.10 | **Optimizer running state has no ambient motion** | `PhaseStepper.tsx`, `ActivityFeed.tsx` | P1 | S | Isolated | `animate-ping` ring on active chip. Feed pulse border. Makes "it's working" legible. |

---

## GROUP 11 — Component Primitives (additive improvements)

Low risk, cumulative polish.

| # | Item | File | Priority | Effort | Blast radius | Notes |
|---|---|---|---|---|---|---|
| 11.1 | **`CardHeader` h2: no `text-wrap: balance`, uses `truncate`** | `Card.tsx:37` | P2 | XS | Component | Add `tracking-tight text-wrap:balance`. Replace `truncate` with `line-clamp-2`. |
| 11.2 | **`font-smoothing` + `tabular-nums` on `body` only** — portal/shadow DOM misses it | `globals.css:15` | P2 | XS | App-wide | Move to `*` selector in `@layer base`. Safe. |
| 11.3 | **Icon size vocabulary: raw pixel numbers, no semantic scale** | `Icon.tsx` | P3 | S | Component | Add `size?: 'xs'|'sm'|'md'|'lg'` → `[12,16,20,24]` with proportional strokeWidth. Backward compat via raw number still supported. |
| 11.4 | **`Card` has no loading/error state** | `Card.tsx` | P2 | S | Component | `state?: 'loading'|'error'` prop. Loading → shimmer. Error → danger-tinted border. |
| 11.5 | **Pill `solid` tone missing glow** — signal pills don't feel like signal lights | `Pill.tsx` | P3 | XS | Component | `shadow-[0_0_8px_rgba(0,230,118,0.25)]` on `solid` tone. |
| 11.6 | **`transition-all` in LiveCycleView connection dot** | `LiveCycleView.tsx:1074` | P3 | XS | Isolated | `transition-[background-color,opacity] duration-200`. |

---

## Implementation sprints

### Sprint 0 — Accessibility fixes (P0, ~2-3 hrs total, ship before anything else)
2.1 → 2.3 → 2.5 → 2.12 → 3.9 → 2.4 (in that order — tokens first, then focus ring, then audit callsites)

### Sprint 1 — Broken things (P0, ~4-6 hrs)
1.1 (Start button) · 1.3 (tr role=link) · 1.4 (label/select) · 1.2 (PhaseStepper null)

### Sprint 2 — Maximum hackathon demo impact (P1, ~1 day)
6.1 · 6.2 · 6.3 · 6.5 · 6.8 · 6.9  (financial data visual — show the profit)  
5.7 · 5.8 · 6.6  (eval wins obvious at a glance)  
5.3 · 7.5 · 8.1 · 8.2  (home narrative)  
4.3  (all button press states — 30 min, everywhere)  
7.1 · 7.2 · 7.3 · 7.4  (no more bare "Loading…" text)

### Sprint 3 — Token & motion system (P1, ~half day)
3.1 · 3.2 · 3.3 → then 10.1 (Card shadows unlock automatically)  
3.4 · 4.1 · 4.2  (motion system + row animations)  
3.8  (--profit token → fixes color ambiguity in 4 places)

### Sprint 4 — IA & copy (P1-P2, ~1 day)
5.4 · 5.5 · 5.6 · 5.1 · 5.2  (nav + optimizer layout)  
8.1–8.9  (all one-line copy fixes)  
9.1 · 9.2 · 9.3  (form UX)

### Sprint 5 — Polish (P2-P3, ongoing)
Remaining groups 3, 4, 7, 10, 11 items as time permits.

---

*Sources: `2026-06-08-hackathon-design-audit.md` · `2026-06-08-bencium-design-audit.md` · `2026-06-08-multiskill-area-audit.md`*
