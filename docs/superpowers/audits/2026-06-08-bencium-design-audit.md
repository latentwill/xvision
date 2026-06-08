# xvision Bencium Design Audit — Full Surface Fan-out
**Date**: 2026-06-08  
**Skill**: `bencium-innovative-ux-designer` (all 5 sub-agents)  
**Areas covered**: Home/Dashboard · Eval Runs · Optimizer · Design System · Strategies/Agents/Scenarios  
**Constraint**: No code changes — findings only. Concrete CSS/Tailwind/React fixes per item.

---

## AREA 1 — Home / Dashboard

### D-H1 — Missing hero stat row (high)
**Location**: `routes/home.tsx` — no component exists  
**Issue**: No at-a-glance summary. Best return, strategy count, active evals, optimizer runs — none are surfaced above the fold. The operator must scroll and read every row to form a mental model.  
**Fix**: Add `<StatHeroRow>` immediately after `<SafetyPauseBanner>`. 3-4 stat tiles in `flex gap-4`. Each: `text-4xl font-mono font-bold` number + `text-[11px] uppercase tracking-widest text-muted-foreground` label. Best-return number at full `#00e676` saturation.

### D-H2 — Performance table buried at position 5 of 7 (high)
**Location**: `routes/home.tsx:77` — section ordering  
**Issue**: StrategyOutcomesList (the performance table) renders after LiveStrategies, OptimizerDigest, ActiveTasks, and CriticalFindings. The answer to "how am I doing?" is the 5th thing on the page.  
**Fix**: Reorder: SafetyPauseBanner → StatHeroRow → StrategyOutcomesList → CriticalFindingsRow → [right rail: Live + ActiveTasks + OptimizerDigest] → NagStrip.

### D-H3 — Metric numbers at 13px (high)
**Location**: `components/home/StrategyOutcomesList.tsx:57-65` — MetricCell  
**Issue**: Return %, Sharpe, Max DD are `text-[13px] font-mono font-medium` — same size as labels. The financial verdict is invisible.  
**Fix**: `text-2xl font-mono font-bold tabular-nums` (24px) for values. Labels: `text-[10px] uppercase tracking-widest text-muted-foreground`.

### D-H4 — Win/loss row coloring is invisible at 5% opacity (high)
**Location**: `components/home/StrategyOutcomesList.tsx:42-43`  
**Issue**: `bg-green-500/5` on `#000000` = completely transparent. Win/loss state conveys nothing.  
**Fix**: Win: `border-[#00e676]/60 bg-[#00e676]/[0.08] border-l-2 border-l-[#00e676]`. Loss: `border-amber-500/50 bg-amber-500/[0.07] border-l-amber-500`.

### D-H5 — Live badge uses diluted green (high)
**Location**: `components/home/LiveStrategiesSection.tsx:75`  
**Issue**: `bg-green-500/15 text-green-600` — muted gray-green, not a live signal.  
**Fix**: Pulsing dot + label: `animate-ping` background ring at full `#00e676` opacity + `text-[#00e676] uppercase tracking-widest font-semibold text-[11px]`.

### D-H6 — Flat `space-y-5` layout, no visual hierarchy (high)
**Location**: `routes/home.tsx:77`  
**Issue**: All 7 sections identical visual weight via `space-y-5`. No focal point.  
**Fix**: Two-zone layout: command strip (banner + hero row) then body grid `grid grid-cols-[1fr_320px] gap-6`. Left: StrategyOutcomesList. Right rail: Live + ActiveTasks + OptimizerDigest.

### D-H7 — All section headers identical weight (high)
**Location**: CriticalFindingsRow, StrategyOutcomesList, LiveStrategiesSection headers  
**Issue**: `text-sm font-semibold tracking-tight` everywhere — no hierarchy between section labels and content.  
**Fix**: `text-[11px] font-semibold uppercase tracking-[0.12em] text-muted-foreground` for all section labels (Bloomberg terminal style).

### D-H8 — ActiveTasksStrip renders "No active tasks" permanently (medium)
**Location**: `components/home/ActiveTasksStrip.tsx:131-133`  
**Fix**: `return null` when `inflight.length === 0`. Remove the dead-weight card entirely.

### D-H9 — CriticalFindings danger tint invisible (medium)
**Location**: `components/home/CriticalFindingsRow.tsx:20`  
**Issue**: `border-danger/30 bg-danger/5` — 5% danger on black. "Critical" findings visually quieter than standard cards.  
**Fix**: `border-danger/50 bg-danger/[0.08] border-l-2 border-l-danger`.

### D-H10 — Sidebar active state: 6% opacity is imperceptible (medium)
**Location**: `components/shell/Sidebar.tsx:54-58`  
**Fix**: Active: `bg-gold/[0.10] border-l-[3px] border-gold font-medium`. Hover (inactive): add `hover:bg-white/[0.03]`.

### D-H11 — Sidebar: 10 equal-weight nav items (medium)
**Location**: `components/shell/Sidebar.tsx:13-24`  
**Fix**: Two groups with `<hr className="mx-6 my-2 border-border/30">` divider. Group 1: core workflow. Group 2: utility items at slightly reduced visual weight (`text-[12.5px] text-text-3`).

### D-H12 — `--gold` token semantic mismatch (low)
**Location**: `styles/tokens.css`  
**Issue**: `--gold` = `#00e676` (neon green). The name implies warmth. Long-term semantic debt.  
**Fix**: Introduce `--accent` as canonical; deprecate `--gold` via find-replace. New code uses `text-accent`.

---

## AREA 2 — Eval Runs (list + detail + compare)

### D-E1 — Detail page h1 is a 36-char UUID (high)
**Location**: `routes/eval-runs-detail.tsx:263-264`  
**Issue**: First readable element on the most important page is a UUID in `font-mono text-[28px]`. No return number visible above fold.  
**Fix**: h1 → strategy name + disambiguator: `text-[22px] font-semibold`. UUID → `<button className="font-mono text-[11px] text-text-4 select-all">` (copy on click). Move Net % / Return to `text-[72px] font-mono font-semibold tabular-nums` hero position immediately below topbar.

### D-E2 — Stat values too modest for financial verdicts (high)
**Location**: `routes/eval-runs-detail.tsx:749`  
**Issue**: Primary stats at `text-[24px] fontWeight:500`. A +18.4% return should demand the eye.  
**Fix**: TOTAL PNL and NET % → `text-[36px] fontWeight:700 tracking-[-0.03em]`. SHARPE and MAX DD → `text-[24px] fontWeight:600`.

### D-E3 — Gold ≠ profit (green = profit is universal) (high)
**Location**: `routes/eval-runs-detail.tsx:739-744`, `routes/eval-runs.tsx:629`  
**Issue**: `var(--gold)` (green) used for profit — ambiguous, reads as "caution" in financial context.  
**Fix**: Introduce `--profit` CSS variable → `#22c55e`. Map `pos` tone → `var(--profit)` in `pnlClass`, `signedToneClass`, `MetricCell`. Reserve `--gold` for status badges only.

### D-E4 — Stat grid renders BELOW the chart (high)
**Location**: `routes/eval-runs-detail.tsx:682-717`  
**Issue**: TOTAL PNL / NET % / SHARPE / MAX DD appear after the equity chart. The verdict is item 3, not item 1.  
**Fix**: Move the stat grid `<div className="mt-4 grid grid-cols-2 md:grid-cols-4">` to render BEFORE the chart. Verdict → chart as supporting evidence.

### D-E5 — Return column at position 9 of 13 in runs table (high)
**Location**: `routes/eval-runs.tsx:330-343`  
**Issue**: Most important column is far right. Equal column weight buries profitability.  
**Fix**: Reorder: select, run, strategy, scenario, status, **RETURN** (col 6, `text-[14px] font-bold`), sharpe, drawdown, tokens, started, actions. Add `border-l-[3px]` in profit color to positive-return `<tr>`.

### D-E6 — SummaryCard "Summary" heading is generic (high)
**Location**: `routes/eval-runs-detail.tsx:582-595`  
**Issue**: 'Summary' in `text-[22px]` — doesn't communicate verdict. PASS badge at `text-[9px]` is a footnote.  
**Fix**: Replace heading with strategy name at `text-[18px] font-semibold`. Verdict badge → `px-3 py-1 text-[13px] font-mono` with semantic background: completed+profit → `bg-profit/15 border-profit/40 text-profit`.

### D-E7 — Compare: default sort by call_order, not performance (medium)
**Location**: `routes/eval-compare.tsx:118-145`  
**Fix**: Default sort → `net_return` descending. Add winner indicator to top row: `✦ BEST` badge + `border-l-2 border-profit`.

### D-E8 — MetaCard duplicates status, shows low-signal data at equal weight (medium)
**Location**: `routes/eval-runs-detail.tsx:800-849`  
**Fix**: Collapse to 3 chips only (mode, duration, started). Remove token cost and status (both shown elsewhere). Render as a slim `text-[11px] font-mono text-text-3` footnote strip.

### D-E9 — Table column headers font-normal, indistinguishable from cells (medium)
**Location**: `routes/eval-compare.tsx:253-265`, `routes/eval-runs.tsx:330-343`  
**Fix**: `<th className="font-medium text-[11px] uppercase tracking-[0.08em] text-text-3 py-2.5 px-3 text-right">` for all metric headers.

### D-E10 — SummaryCard no elevation over secondary cards (medium)
**Location**: `routes/eval-runs-detail.tsx:576`  
**Issue**: Primary result card identical style to MetaCard and AssetRollupPanel.  
**Fix**: For completed profitable runs: `border-profit/40` border + 3px left accent: `style={{ borderLeft: '3px solid var(--profit)' }}`. For failed: `border-danger/40`. Add top gradient bar: `<div className="h-[3px] rounded-t-card" style={{ background: 'linear-gradient(90deg, var(--profit), transparent 60%)' }} />`.

### D-E11 — Row hover only ghost tint, no directional signal (low)
**Location**: `routes/eval-runs.tsx:562-572`  
**Fix**: `hover:bg-surface-hover hover:shadow-[inset_3px_0_0_var(--gold)]` — flash of left border on hover reinforces terminal aesthetic.

### D-E12 — Return cell same 13px as Tokens/Duration (low)
**Location**: `routes/eval-runs.tsx:631-633`  
**Fix**: Return column cell → `text-[14px] font-medium` vs all others at `text-[13px]`. One size step = clear visual anchor.

---

## AREA 3 — Optimizer (home + cycle + experiment)

### D-O1 — Active phase chip whispers, should shout (high)
**Location**: `features/autooptimizer/ui/PhaseStepper.tsx:58-65`  
**Issue**: Current phase gets `bg-gold/10 border-gold/20` — same size and padding as completed and pending chips.  
**Fix**: Active chip → `border-transparent bg-gold text-black font-semibold px-3.5 py-1.5 text-[12px] scale-105 shadow-[0_0_12px_2px_rgba(0,230,118,0.3)] transition-all duration-300`. Completed → `opacity-40 line-through`.

### D-O2 — Running state has zero ambient motion (high)
**Location**: `features/autooptimizer/ui/PhaseStepper.tsx`, `features/autooptimizer/ui/ActivityFeed.tsx:186-209`  
**Issue**: UI looks identical running vs paused. No visual urgency.  
**Fix**: Active phase chip: wrap in `relative` + add `<span className="absolute inset-0 rounded-sm animate-ping opacity-20 bg-gold" />`. ActivityFeed: add `border-l-2 border-gold animate-pulse` on container when live rows incoming.

### D-O3 — Kept vs Dropped experiments share identical hero card (high)
**Location**: `features/autooptimizer/screens/ExperimentDetail.tsx:51-63`  
**Issue**: Gate verdict is a small inline badge. Page-level color signal = none.  
**Fix**: Conditionally color hero section by bucket: Kept → `border-gold/50 bg-gold/[0.04]`, Dropped → `border-danger/40 bg-danger/[0.03]`, Pending → `border-warn/40 bg-warn/[0.03]`.

### D-O4 — ExperimentDetail: 9 equal-weight sections, EmptyPanels at full weight (high)
**Location**: `features/autooptimizer/screens/ExperimentDetail.tsx:65-117`  
**Issue**: Future EmptyPanel sections ('Flight recorder', 'Sign-off receipts', 'Attester activity') render at full visual weight alongside real content.  
**Fix**: Collapse all 3 future EmptyPanel sections into `<details className="text-[11px] text-text-3 mt-4"><summary>Upcoming sections (3)</summary>…</details>`. Give 'Decision' section outcome-colored full-bleed accent: Kept → `bg-gold/[0.06] border-l-4 border-gold`.

### D-O5 — HashSigil looks like a gray debug thumbnail (high)
**Location**: `features/autooptimizer/ui/HashSigil.tsx:19-63`  
**Issue**: `bg-surface-elev border-border` — no visual character for a unique cryptographic identity artifact.  
**Fix**: `style={{ background: 'radial-gradient(circle at center, ${fg}15 0%, var(--surface-card) 70%)' }}`. Add `shadow-[0_0_0_1px_var(--border)]`. Bump default to 80px on ExperimentDetail. Add colored border matching fill accent.

### D-O6 — ProgressDial too small and too quiet (high)
**Location**: `features/autooptimizer/ui/ProgressDial.tsx:1-38`  
**Issue**: 64px default, track at `--border-strong` (mid-grey), percentage at 13px. Mission-critical progress indicator looks like a badge.  
**Fix**: Default 96px, stroke 8px, track → `var(--surface-elev)` (darker), percentage → `text-[17px] font-bold`. Glow: `filter: drop-shadow(0 0 8px rgba(0,230,118,0.5))`. Place on its own row above stat grid in CycleDetail.

### D-O7 — ActivityFeed: new events appear silently (high)
**Location**: `features/autooptimizer/ui/ActivityFeed.tsx:186-209`  
**Fix**: Track newly-added row IDs in a Set ref. Apply `animate-[flash-in_0.6s_ease-out]` to new rows. `@keyframes flash-in { 0% { background: rgb(0 230 118 / 0.15); } 100% { background: transparent; } }`. Each incoming SSE event gets a 600ms gold flash.

### D-O8 — ActivityFeed: all event types look identical (medium)
**Location**: `features/autooptimizer/ui/ActivityFeed.tsx:180-213`  
**Fix**: Left-accent by event keyword: 'accepted'/'kept' → `border-l-2 border-l-gold/70`; 'rejected'/'dropped' → `border-l-2 border-l-danger/50`; 'cycle' → `border-l-2 border-l-info/50`. Bump label to `text-[13px]`, shrink time to `w-[72px] text-[10px]`.

### D-O9 — CycleDetail / ExperimentDetail: 16-char hash as h1 (medium)
**Location**: `features/autooptimizer/screens/CycleDetail.tsx:37-38`, `ExperimentDetail.tsx:58`  
**Fix**: Break hash into groups: `{hash.slice(0,4)} {hash.slice(4,8)} {hash.slice(8,12)} {hash.slice(12,16)}` with `tracking-[0.15em]`. Size: `text-[24px] font-bold`. Stat labels: raise 8.5px → `text-[10px]`.

### D-O10 — GateBadge 'Kept' feels like a neutral tag (low)
**Location**: `features/autooptimizer/ui/GateBadge.tsx:12-17`  
**Fix**: Kept → `font-semibold` + `✓ Kept` prefix, `bg-gold/[0.15] border-gold/60`. Dropped → `✗ Dropped`. Symbol carries the emotional weight; size stays.

### D-O11 — Resume/Pause/Cancel buttons undifferentiated (medium)
**Location**: `features/autooptimizer/screens/RunDetail.tsx:88-134`  
**Fix**: Resume → `bg-gold text-black font-semibold shadow-[0_2px_8px_rgba(0,230,118,0.3)] hover:shadow-[0_4px_14px_rgba(0,230,118,0.4)]`. Pause → `border-border font-medium`. Cancel → demoted `text-[12px] text-danger/70 border-danger/30`.

### D-O12 — ImprovementChart empty state is generic (medium)
**Location**: `features/autooptimizer/ui/ImprovementChart.tsx:76-79`  
**Fix**: `<div className="flex flex-col items-center gap-2 py-10"><span className="text-[32px] opacity-20">⟳</span><span className="text-[13px] font-medium text-text-2">No cycles yet</span><span className="text-[11px] text-text-3">Start a run to begin tracking improvement.</span></div>`.

---

## AREA 4 — Design System (tokens + primitives + chart palette)

### D-DS1 — `--gold` and `--accent` are duplicate tokens with identical values (high)
**Location**: `styles/tokens.css:31`, `tailwind.config.ts:29`  
**Issue**: Both resolve to `#00e676`. Components randomly use one or the other. Palette change requires two search-replace passes.  
**Fix**: Collapse to `--gold` as canonical. Remove `--accent` from tokens + tailwind, or alias: `--accent: var(--gold)`.

### D-DS2 — Surface tokens span only 18 lightness steps (imperceptible depth) (high)
**Location**: `styles/tokens.css:11-17`  
**Issue**: `--bg:#000000` to `--surface-panel:#121212` — 18 steps total. Cards and panels are visually flat on OLED/calibrated displays.  
**Fix**: Widen the ladder: `--bg:#050505, --surface-card:#0F0F0F, --surface-elev:#141414, --surface-panel:#1C1C1C`. Border at `#1A1A1A` now reads as a distinct ring.

### D-DS3 — No shadow, glow, or hero font-size tokens (high)
**Location**: `styles/tokens.css` — missing  
**Issue**: No way to express elevation or emphasis without ad-hoc raw CSS every time.  
**Fix**: Add:
- `--glow-accent: 0 0 12px rgba(0,230,118,0.35)`
- `--shadow-card: 0 1px 3px rgba(0,0,0,0.6)`
- `--shadow-float: 0 8px 24px rgba(0,0,0,0.8)`
- `--fs-hero: clamp(2rem, 4vw, 3.5rem)` (wire into tailwind `fontSize`)

### D-DS4 — No `--focus-ring` token; 68 `focus:outline-none` without replacement (high)
**Location**: `styles/tokens.css` — missing; app-wide  
**Fix**: `--focus-ring: rgba(0,230,118,0.6)`. Update `.input` in globals.css to `focus:border-gold focus:ring-2 focus:ring-[var(--focus-ring)] focus:outline-none`. Add to globals: `:focus-visible { outline: 2px solid var(--gold); outline-offset: 2px; }`.

### D-DS5 — Base font 13px squashes all hierarchy into 11-14px band (high)
**Location**: `styles/globals.css:17`, `tailwind.config.ts:47`  
**Fix**: Raise base to 14px. Recalibrate scale: `xs:11, sm:12, base:14, md:16, lg:18, xl:22, 2xl:28, 3xl:36, hero:clamp(2.5rem,5vw,4rem)`.

### D-DS6 — `--surface-hover: rgba(255,255,255,0.04)` below perceptual threshold (medium)
**Location**: `styles/tokens.css:17`  
**Issue**: 4% white on `#0a0a0a` ≈ `#171717` — nearly indistinguishable from resting state on most displays.  
**Fix**: `--surface-hover: rgba(255,255,255,0.07)`. Add `--surface-hover-accent: rgba(0,230,118,0.06)` for rows that should hint at brand accent on hover.

### D-DS7 — Card primitive has no elevation, no variants (high)
**Location**: `components/primitives/Card.tsx`  
**Issue**: No shadow, no variant system. Every surface looks the same.  
**Fix**: Add `shadow-[var(--shadow-card)]` to base card. Add `variant` prop: `'default' | 'elevated' | 'accent' | 'ghost'`. `accent` variant: 1px top border in `--gold` + inner `--glow-accent`. `elevated`: heavier shadow.

### D-DS8 — Pill tones all look identical (medium)
**Location**: `components/primitives/Pill.tsx`  
**Issue**: Six tones, all border-only with matching text. Dense dashboards — pills blur together.  
**Fix**: Add subtle fill to semantic tones: danger → `bg-danger/8 border-danger/40 text-danger`, warn → `bg-warn/8 border-warn/40 text-warn`, info → `bg-info/8 border-info/40 text-info`. Add `muted` tone: `bg-transparent border-text-4/30 text-text-4`.

### D-DS9 — Single font system: no typographic personality (high)
**Location**: `tailwind.config.ts:38-40`  
**Issue**: Both `font-sans` and `font-serif` → Geist. No display font, no drama, no contrast.  
**Fix**: Introduce `font-display` → Instrument Serif or Fraunces (Google Fonts, editorial contrast). Use on page headings, hero numbers, and detail page titles for one tier of genuine typographic drama.

### D-DS10 — Chart palette: teal/cyan/sky cluster fails color-blindness (medium)
**Location**: `theme/themes.ts:186-195`  
**Issue**: `#5EEAD4` (teal) and `#22D3EE` (cyan) and `#38BDF8` (sky) — 3 perceptually close blue-greens in an 8-slot palette.  
**Fix**: Replace `#22D3EE` → `#F59E0B` (amber), `#5EEAD4` → `#E879F9` (fuchsia). Test result palette through deuteranopia simulator.

### D-DS11 — `caps` class at 10.5px fails readability floor (medium)
**Location**: `styles/globals.css:38-44`  
**Issue**: 10.5px uppercase with `#5f6670` on `#0a0a0a` ≈ 3.2:1 contrast ratio (below WCAG AA 4.5:1).  
**Fix**: Raise to 11px, reduce tracking 0.10 → 0.08em. Raise `--text-3` to `#707880` (≥4.5:1 on card surface).

### D-DS12 — No border radius vocabulary beyond 6px `rounded-card` (medium)
**Location**: `tailwind.config.ts:42-44`  
**Fix**: Extend: `sm:'4px', card:'6px', lg:'10px', pill:'999px'`. Update Pill → `rounded-sm`, full-pill elements → `rounded-pill`. Consider raising card radius to 8px for more perceived character.

### D-DS13 — Warm chart palette key names are semantic lies (low)
**Location**: `theme/themes.ts:221-232`  
**Issue**: `warm.gold=#00E676` (green), `warm.amber=#38BDF8` (sky blue), `warm.bronze=#5EEAD4` (teal). None match their names.  
**Fix**: Rename to hue-accurate keys: gold→emerald, amber→sky, bronze→teal, ember→orange, copper→rose, plum→violet. Or use positional keys: `series1` through `series8`.

---

## AREA 5 — Strategies / Agents / Scenarios

### D-S1 — Strategy `color` field assigned nowhere visible (high)
**Location**: `routes/strategies.tsx:429-435`, `routes/strategies-detail.tsx:319-392`  
**Issue**: Operators assign strategy colors via ColorPickerRow, but the color appears on zero list rows and zero detail pages. The entire color subsystem is orphaned.  
**Fix**: List `DesktopRow`: prepend `<td className="w-1 p-0"><div style={{ background: row.color ?? 'var(--text-3)' }} className="h-full w-1 rounded-l" /></td>`. Detail header: `<div style={{ background: m.color ?? undefined }} className="w-full h-0.5 rounded mb-4" />`.

### D-S2 — All three list surfaces render as identical CRUD tables (high)
**Location**: `routes/strategies.tsx:412-508`, `routes/agents.tsx:262-320`, `routes/scenarios.tsx:303-377`  
**Issue**: Every row identical visual weight. No entity identity. Strategies, agents, scenarios all look like DB audit logs.  
**Fix**: Name cell → `text-[14px] font-semibold text-text`. Sub-metadata (cadence, model) → `text-[11px] text-text-3`. Strategy row: colored left bar (D-S1). Agent row: slot-name chips inline (see D-S4).

### D-S3 — Strategy detail page is unstyled HTML (high)
**Location**: `routes/strategies-detail.tsx:319-392`  
**Issue**: Bare `<main>`, `<dl>`, `<dt>`, `<dd>` with browser UA defaults. `<h1>` wrapping `InlineEditField` has no size applied.  
**Fix**: `<main className="space-y-6">`. `<h1 className="text-[24px] font-semibold text-text tracking-tight mt-1">`. `<dl className="grid grid-cols-[120px_1fr] gap-x-4 gap-y-2 text-[13px]">`. `<dt className="text-text-3 font-medium">`. ColorPickerRow → its own labeled section outside the `<dl>`.

### D-S4 — Agent slot roles invisible in agents list (high)
**Location**: `routes/agents.tsx:287-292`  
**Issue**: Slot composition (intern/trader/critic/router) shown only as a count string `1 (main)`. The most important identity signal for agents is hidden.  
**Fix**: Inline slot-name chips below the agent name: `row.slots.slice(0,3).map(s => <span className="font-mono text-[10px] text-text-3 bg-surface-elev border border-border-soft rounded px-1.5 py-0.5">{s.name}</span>)`.

### D-S5 — Primary "New Strategy" CTA is undersized (high)
**Location**: `routes/strategies.tsx:284`  
**Issue**: 13px gold button top-right. The entry point to the entire configuration layer is visually a secondary action.  
**Fix**: `px-4 py-2 text-[13.5px] rounded-md` + `<Icon name="arrowRight" size={12} />` suffix. Move to left of the toolbar row (first, most prominent element).

### D-S6 — ScenarioForm submit = cancel visually (high)
**Location**: `features/scenarios/ScenarioForm.tsx:216-416`  
**Issue**: Submit button `border-border bg-surface-elev` identical to cancel. No visual hierarchy.  
**Fix**: Submit → `bg-gold text-bg px-4 py-1.5 font-medium hover:bg-gold-soft disabled:opacity-50`. Cancel stays as secondary. Advanced toggle → `<Icon name="chevronDown/Right" size={12}>` instead of Unicode `▸`.

### D-S7 — Template picker cards look like content cards (medium)
**Location**: `routes/agents-edit.tsx:112-148`  
**Issue**: No selected state, no role icon, no hover ring. Looks like card content, not a selection interface.  
**Fix**: Add role icon: `<div className="w-8 h-8 rounded-lg bg-gold/10 border border-gold/20 flex items-center justify-center mb-3"><Icon name={iconForTemplate(t.id)} size={16} className="text-gold" /></div>`. Hover ring: `hover:border-gold/60 hover:shadow-[0_0_0_2px_rgba(0,230,118,0.12)]`.

### D-S8 — Empty states: absence announcements, not invitations (medium)
**Location**: `routes/strategies.tsx:324`, `routes/agents.tsx:207`, `routes/scenarios.tsx:244`  
**Fix**: Zero-state: two-level hierarchy — `text-[14px] font-medium text-text` title + `text-[12px] text-text-3` description with context about what to create and why. Filtered-empty: add clearAll action button.

### D-S9 — Strategy/agent loading = bare "Loading…" text (medium)
**Location**: `routes/strategies-detail.tsx:301-307`, `routes/agents-edit.tsx`  
**Fix**: Replace with `<main className="space-y-4 animate-pulse">` skeleton (3 height-graduated gray rectangles). Error state: title + description + `<button onClick={() => query.refetch()}>Retry</button>`.

### D-S10 — Strategies table: equal-width column squash (medium)
**Location**: `routes/strategies.tsx:272-279`  
**Fix**: Add explicit column widths: name `min-w-[180px] w-auto`, shape `w-[130px]`, tags `w-[150px]`, model `w-[160px]` (truncated with `title` tooltip), created `w-[100px]`, actions `w-[48px]`.

### D-S11 — ColorPickerRow swatches below minimum touch target (medium)
**Location**: `routes/strategies-detail.tsx:119-227`  
**Issue**: 24×24px swatches. Minimum 32×32 for desktop, 44×44 for mobile.  
**Fix**: `className="w-8 h-8 rounded-md cursor-pointer transition-transform hover:scale-110"`. Custom color input: wrap in `<label>` with `<span className="text-[9px] text-text-3">custom</span>` below.

### D-S12 — Agent edit subtitle leaks implementation detail (low)
**Location**: `routes/agents-edit.tsx:31`  
**Issue**: New agent subtitle: "Single-slot draft". Edit subtitle: raw agentId ULID.  
**Fix**: New → `"Define name, system prompt, model, and tools"`. Edit → `"Edit agent configuration"`.

### D-S13 — Scenario new page: empty subtitle (low)
**Location**: `routes/scenarios-new.tsx:36`  
**Fix**: `sub="Define a market window, asset class, and venue settings for backtesting"`.

---

## Cross-Area Synthesis

### Biggest single ROI change
**Show the strategy color everywhere.** A strategy's `color` field is stored, editable, and rendered nowhere. Adding a 4px left-bar to every strategy row and a `h-0.5` accent line on the detail page header costs ~10 lines and gives every strategy a distinct visual fingerprint across the entire app. This one change makes the strategies surface feel like a curated portfolio rather than a database table.

### Systemic fixes with the widest impact
1. **Raise `--surface-hover` to 7%** — fixes hover legibility across all interactive surfaces simultaneously
2. **Introduce `--profit` token (green) separate from `--gold`** — fixes profit/loss color vocabulary in eval-runs, compare, StrategyOutcomesList, and FlywheelStrip in one grep-replace
3. **Add `--shadow-card` token + apply to Card primitive** — gives every card in the app depth for free
4. **Widen surface ladder (#050505 → #0F0F0F → #141414 → #1C1C1C)** — fixes the invisible-card problem app-wide without touching any component

### Page that needs the most work
**Eval Runs Detail** — hero moment for the entire product (did the strategy make money?) but opens with a UUID as h1, stats below the chart, gold used for profit instead of green, and SummaryCard indistinguishable from MetaCard. 6 of the top 18 eval-runs findings apply to this single page.

---

*Generated by 5 parallel `bencium-innovative-ux-designer` agents: home-dashboard · eval-runs · optimizer · design-system · strategies-agents-scenarios*  
*Total findings: ~85 raw → 68 de-duplicated across 5 areas*
