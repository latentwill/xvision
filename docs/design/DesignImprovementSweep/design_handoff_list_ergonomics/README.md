# Handoff: Eval List Ergonomics — Column Picker, Scroll Affordance & Responsive Columns

## Overview

The eval-runs list (and every other data table built on `ListCard`) currently renders a
`min-w-max` table inside an `overflow-x-auto` wrapper. With 9+ columns this **never
compresses** — it always overflows horizontally behind a scrollbar that is invisible until
hover on macOS. Users can't tell there's more content, and the horizontal scroll is the
single most-complained-about ergonomic issue in the app.

This handoff specifies a fix in five composable parts, all demonstrated working in the
attached prototype:

1. **Persisted column picker** — show/hide columns; choice saved to `localStorage`.
2. **Edge-fade scroll affordance** — a gradient + nudge-arrow that *announces* overflow.
3. **Sticky identity + status columns** — pin the first and last columns while the middle scrolls.
4. **ULID folded into the name cell** — remove the widest, least-scanned column.
5. **Priority-based responsive auto-hide** — drop `min-w-max`; retire low-priority columns when the track is too narrow.

The unifying idea: a single shared hook `useListColumns(listId, columns)` + one `<ColumnMenu>`
component serve **every** list in the app (eval runs, decisions, marketplace, settings, …) from
column metadata.

---

## About the Design Files

The file in this bundle — **`XVN List Ergonomics — Column Picker.html`** — is a **design
reference created in HTML**. It is a working prototype demonstrating the intended look and
behavior (the column picker actually persists to `localStorage`; the table actually auto-hides
on resize). **It is not production code to copy directly.**

Your task is to **recreate these behaviors in the existing `frontend/web` React + Tailwind
codebase**, using its established patterns — specifically by extending the existing
`ListCard` / `useListState` / `ResponsiveListCard` system rather than introducing a parallel one.
The prototype's plain-JS column model and `computeRender()` logic map almost 1:1 onto a React
hook; treat them as a behavioral spec, not source to port verbatim.

---

## Fidelity

**High-fidelity.** Colors, typography, spacing, radii, and interaction details below are final
and match the prototype. Recreate pixel-faithfully using the codebase's existing primitives
(Tailwind tokens, `Card`, `SignalMenu`/popover, `Icon`). Where the prototype hardcodes a hex,
prefer the equivalent existing CSS variable / Tailwind token (mapping table in **Design Tokens**).

---

## Target Files in the Codebase

These existing files are the integration surface:

| File | Role | Change |
|---|---|---|
| `src/components/lists/ListCard.tsx` | Renders the table, header, toolbar, footer | Main change: filter `columns`, add scroll affordance + sticky, render auto-hide note |
| `src/components/lists/useListState.ts` | List sort/filter/search state + `ListColumn` type | Extend `ListColumn` type with `key`-level metadata; add `useListColumns` hook (new) or co-locate |
| `src/components/lists/ListToolbar.tsx` | Toolbar (search + filter chips) | Add the **Columns** menu button to the actions cluster |
| `src/components/lists/ResponsiveListCard.tsx` | Thin wrapper passing `columns` through | Pass column-picker props through |
| `src/routes/eval-runs.tsx` | Defines the eval-runs `columns` array + `renderRow` | Add metadata to each column; fold the ULID into the name cell |
| `src/components/primitives/SignalMenu.tsx` | Existing popover primitive | Reuse for the column menu (don't hand-roll a popover) |

**Reuse the existing density-persistence pattern** already in `ListCard.tsx`:
`densityKey(listId) → "xvn:list:${listId}:density"`, `readPersistedDensity`, `useResolvedDensity`.
The column picker mirrors this exactly.

---

## Part 1 — Column metadata (extend `ListColumn`)

The current type:

```ts
export type ListColumn = {
  key: string;
  label: ReactNode;
  align?: "left" | "right" | "center";
  width?: number | string;
};
```

Extend it with three optional fields (all backward-compatible — existing lists work unchanged):

```ts
export type ListColumn = {
  key: string;
  label: ReactNode;
  align?: "left" | "right" | "center";
  width?: number | string;
  /** Locked ON, never offered for hiding (identity, status, row actions). Default false. */
  essential?: boolean;
  /** Higher number = retired FIRST under responsive auto-hide. Essential cols ignore this. Default 5. */
  priority?: number;
  /** Hidden by default but available in the picker (e.g. Mode, Cost). Default false. */
  defaultOff?: boolean;
  /** Estimated rendered px width, used by the auto-hide fit calculation. */
  estWidth?: number;
  /** Pin while scrolling: "left" or "right". Typically the identity col + status col. */
  sticky?: "left" | "right";
};
```

### eval-runs column metadata (from `eval-runs.tsx`)

| key | label | essential | priority | defaultOff | sticky | estWidth |
|---|---|---|---|---|---|---|
| `sel` | (checkbox) | ✅ | — | — | — | 42 |
| `id` | Run | ✅ | — | — | left | 210 |
| `scenario` | Scenario | — | 6 | — | — | 150 |
| `started` | Started | — | 7 | — | — | 130 |
| `mode` | Mode | — | 8 | ✅ | — | 96 |
| `sharpe` | Sharpe | — | 4 | — | — | 84 |
| `return` | Return | — | 2 | — | — | 96 |
| `dd` | Max DD | — | 5 | — | — | 92 |
| `cost` | Cost | — | 9 | ✅ | — | 84 |
| `status` | Status | ✅ | — | — | right | 128 |
| `act` | (row actions) | ✅ | — | — | right | 46 |

Priority rationale: **Return (2) is retired last** among optionals because it's the headline
metric; **Cost (9) / Mode (8) / Started (7) first** because they're least scanned.

---

## Part 2 — `useListColumns` hook (new)

Mirror `useResolvedDensity`. Storage key: `xvn:list:${listId}:columns` (array of visible keys).

```ts
// src/components/lists/useListColumns.ts
function columnsKey(listId: string) { return `xvn:list:${listId}:columns`; }

export function useListColumns(listId: string | undefined, columns: ListColumn[]) {
  // 1. default visible set = essential ∪ (not defaultOff)
  // 2. read persisted visible-keys from localStorage (if present), always re-add essentials
  // 3. expose: visibleKeys: Set<string>, toggle(key), reset(), isEssential(key)
  // 4. persist on every toggle/reset (try/catch — private mode safe, like readPersistedDensity)
}
```

Behavior contract (verified in prototype):
- Essential keys are **always** in the visible set, even if absent from storage.
- `reset()` clears the key and falls back to defaults.
- Toggling persists immediately.
- Storage parse failures fall back to defaults silently (never throw).

---

## Part 3 — Responsive auto-hide (`computeRender` logic)

Drop `min-w-max` from the table. In `ListCard`, after resolving the user-visible columns,
measure the scroll track and progressively retire the **highest-priority-number, non-essential**
columns until the row fits (or only essentials remain):

```
candidates = columns.filter(c => c.essential || visibleKeys.has(c.key))
autoHidden = []
if (autoHideEnabled) {
  available = scrollTrack.clientWidth
  total = sum(candidates.estWidth)
  while (total > available) {
    victim = candidates.filter(!essential).sort(by priority desc)[0]
    if (!victim) break
    remove victim from candidates; push to autoHidden; total -= victim.estWidth
  }
}
```

- Recompute on container resize via `ResizeObserver` on the scroll track (the app already uses
  `ResizeObserver` elsewhere — see responsive shells).
- Auto-hidden columns remain **in the picker**, tagged `auto` (amber), so nothing is lost.
- Surface a footer note: `"{n} columns auto-hidden · widen to restore"`.
- The **Columns** button badge shows `"{visibleOptionalCount}"` and, when auto-hiding,
  `"{visibleOptionalCount}+{autoHiddenCount}"` (e.g. `1+4`).

---

## Part 4 — Edge-fade scroll affordance

Wrap the scroll track in a `position: relative` outer. Add two gradient overlays and one
nudge-arrow, toggled by scroll position:

- **Left fade**: `linear-gradient(90deg, var(--surface-card), transparent)`, 46px wide, shown
  when `scrollLeft > 1`.
- **Right fade**: `linear-gradient(270deg, var(--surface-card), transparent)`, 46px wide, shown
  when not at end (`scrollLeft < scrollWidth - clientWidth - 1`).
- **Nudge arrow**: 22px circle, `--surface-panel` bg, `--border-strong` border, chevron-right,
  bottom-right; shown with the right fade; clicking scrolls `+240px` smoothly.
- Recompute on `scroll` and on resize.

Fades are `pointer-events: none` and sit above cells (`z-index: 6`); the arrow is clickable
(`z-index: 7`). Sticky cells use `z-index: 4`.

---

## Part 5 — Sticky columns + ID fold

**Sticky:** cells with `sticky: "left"` get `position: sticky; left: 0`, `sticky: "right"` get
`right: 0`. Both need an opaque cell background (`var(--surface-card)`, and `--surface-hover`
on row hover) so scrolled content passes *under* them. Add a directional shadow only while
actually overflowing on that side:
- left-stuck shadow: `box-shadow: 14px 0 16px -12px rgba(0,0,0,.7)` when `scrollLeft > 1`
- right-stuck shadow: `box-shadow: -14px 0 16px -12px rgba(0,0,0,.7)` when not at end

**ID fold (highest impact, lowest effort):** instead of a standalone full-ULID column, render
the identity cell as a two-line stack:

```
<div class="idcell">
  <span class="nm">{strategyName}</span>          // Geist Mono 12.5px, --text, 500
  <span class="rid">{runId}<CopyIcon/></span>     // Geist Mono 10.5px, --text-4, copy-on-click
</div>
```

This reclaims the most horizontal space of any single change. The mobile surfaces already do
this via `evalRunLabels` / `evalRunDisambiguator` (`src/lib/run-display.ts`) — reuse those
label helpers for consistency. Copy-on-click writes `runId` to clipboard; show a brief
"copied" affordance.

---

## Part 6 — Compact density goes horizontal

The existing density toggle (`useResolvedDensity`) only changes vertical padding today. When
`density === "compact"`, also tighten cell horizontal padding (`px-3.5 → px-2.5`) and reduce
the id/meta secondary line, so power users on 100-row pages fit more columns without scrolling.

---

## Interactions & Behavior (summary)

| Trigger | Behavior |
|---|---|
| Click **Columns** button | Toggle the picker popover (reuse `SignalMenu`). Outside-click closes. |
| Toggle a column in picker | Add/remove from visible set; persist to `localStorage`; re-render. Essentials are locked (not togglable). |
| Click **Reset** in picker | Clear stored keys; revert to defaults. |
| Resize container | `ResizeObserver` → recompute auto-hide + fades. |
| Scroll the track | Recompute left/right fades + sticky shadows. |
| Click nudge arrow | Smooth-scroll `+240px`. |
| Click ULID copy icon | Copy run id to clipboard. |
| Toggle density | Vertical **and** horizontal padding change; persists (existing pattern). |

Respect `prefers-reduced-motion` for the smooth-scroll (fall back to instant).

---

## State Management

- **`visibleKeys: Set<string>`** — from `useListColumns`, persisted at `xvn:list:${listId}:columns`.
- **`density: "full" | "compact"`** — existing, persisted at `xvn:list:${listId}:density`.
- **`autoHidden: ListColumn[]`** — derived per-render from track width + `estWidth`; not persisted.
- **`scrollState: { atStart, atEnd, overflowing }`** — derived from scroll events; drives fades/shadows.

No server/data-fetching changes. This is presentation-layer only.

---

## Design Tokens

Prototype hexes map to the **proposed Signal-dark token set** (from the contrast/depth work).
Prefer the codebase's CSS variables; values given for reference.

| Token (proposed) | Value | Use |
|---|---|---|
| `--surface-card` | `#111318` | Card + cell background (opaque sticky cells) |
| `--surface-elev` | `#15171D` | Toolbar inputs, buttons |
| `--surface-panel` | `#1A1D24` | Popover background |
| `--surface-hover` | `rgba(255,255,255,.035)` | Row hover |
| `--border` | `#2C313B` | Card border, header rule |
| `--border-strong` | `#3A4150` | Picker border, scrollbar thumb |
| `--border-soft` | `#20242C` | Row dividers |
| `--text` | `#F4F6F8` | Primary text |
| `--text-2` | `#AEB6C2` | Secondary (values, meta) |
| `--text-3` | `#9AA3B2` | Tertiary (headers, ids) — **lifted from `#5f6670`** |
| `--text-4` | `#5C6573` | ULID line, row-action dots |
| `--accent` | `#3B82F6` | Brand/interactive: badge, links, picker checks, focus |
| `--pos` | `#00E676` | Positive return |
| `--neg` | `#FF5C5C` | Negative return / failed |
| `--info` | `#5FA8FF` | Running status |

> ⚠️ **Accent note:** the prototype uses `#3B82F6` (azure) for *interactive* accent, deliberately
> **separate** from `--pos` green. This is part of the broader "accent split" direction
> (brand ≠ money). If that token split hasn't landed yet, coordinate — don't reintroduce green
> for the Columns badge/links.

**Typography:** Geist (sans) for labels/values; **Geist Mono** for ids, numerics, column
headers (10px, `letter-spacing: .07em`, uppercase). Radii: card 13px, buttons/inputs 7px,
pills/checkboxes 4px, popover 10px.

**Popover elevation** (floating surface): `box-shadow: 0 18px 44px rgba(0,0,0,.6), 0 3px 10px
rgba(0,0,0,.5), inset 0 1px 0 rgba(255,255,255,.06)`.

---

## Assets

- **Icons** — all inline 24×24 line SVGs in the prototype (rows, columns, search, filter,
  chevron, copy, dots). Use the codebase's existing `Icon` component / icon set; do not ship the
  prototype SVGs. Names map obviously.
- No images or fonts beyond **Geist / Geist Mono**, already loaded by the app.

---

## Files in This Bundle

| File | What it is |
|---|---|
| `XVN List Ergonomics — Column Picker.html` | Working hi-fi prototype. Plain-JS `COLUMNS` model + `computeRender()` = the behavioral spec for the React hook. Drag the dashed right edge to see auto-hide; toggle switches for fold/sticky/auto-hide; the picker persists to `localStorage`. |
| `README.md` | This document. |

---

## Suggested Implementation Order

1. **ID fold** (Part 5) + **edge-fade/sticky** (Parts 4–5) — high impact, low effort, no new state.
2. **`useListColumns` hook + picker** (Parts 1–2) — the unifying layer; mirrors density pattern.
3. **Responsive auto-hide** (Part 3) — drop `min-w-max`; wire `ResizeObserver`.
4. **Compact-horizontal density** (Part 6) — small polish.

Land 1 first; it removes most of the pain on its own. 2–3 make it systematic across all tables.
