# Handoff: Standard List Component (Desktop + Mobile)

## Overview

A single **standardized list component** to be used across every list in the xvn UI — Strategies, Eval Runs, Decisions, Trade Ledger, Open Positions, Journal, etc. The component enforces a consistent contract:

- **Search** is always available
- **Filters** are domain-specific but use uniform UI
- **Sort** is always available; **"Recently added" is option 1 and the default** everywhere
- **Active filter state** surfaces as removable chips
- Two densities — `full` (primary pages) and `compact` (dashboard mini-lists)
- Two form factors — **desktop** (toolbar above a table) and **mobile** (toolbar above card-style rows, filters in a bottom sheet)

The goal is to replace the ad-hoc search/filter/sort UI scattered across screens with one disciplined component that every list inherits.

---

## About the Design Files

The files in this bundle are **design references created in HTML/JSX with inline Babel transpilation** — they're working prototypes showing intended look and behavior, not production code to copy directly. Open `Lists.html` and `Lists Mobile.html` in a browser to interact with them: search, filter, sort, open the mobile bottom sheet, etc.

The task is to **recreate this component in the target codebase** using its established patterns (React/Vue/SwiftUI/etc.) and bind it to real data sources. The prototype uses inline mock data; the real implementation should accept rows as props and let the host page configure filter and sort options.

If no frontend environment exists yet for xvn, this design assumes **React + TypeScript + CSS variables (or CSS-in-JS / styled-components / tailwind)**. The CSS in `styles.css` defines the token system — port the tokens to whichever styling solution the project uses.

## Fidelity

**High-fidelity (hifi).** Final colors, typography, spacing, interaction states, and component anatomy are all locked. Implementation should be pixel-equivalent. Use the exact tokens, fonts, and measurements documented below.

---

## Files in this bundle

| File | What it is |
|---|---|
| **`Lists.html`** | Desktop design canvas — open in a browser. Shows anatomy, 4 variants, and the component applied to Strategies / Eval runs / Run detail / Home. |
| **`Lists Mobile.html`** | Mobile design canvas — anatomy + 3 applied screens + 2 states. iPhone frame is decorative; the inner 390×844 surface is the production target. |
| `list-toolbar.jsx` | **The desktop component itself.** `<ListCard>`, `<ListToolbar>`, `<ListActiveChips>`, `useListState()`. ~440 lines including styles. |
| `list-toolbar-mobile.jsx` | **The mobile component itself.** `<MListCard>`, `<MListRow>`, `<MListSheet>`. Reuses `useListState` from the desktop file. |
| `list-examples.jsx` | Desktop application examples (Strategies, Eval Runs, Decisions, Recent Runs, Positions). Reference for how to wire `useListState` to a real list. |
| `list-mobile-screens.jsx` | Mobile application examples. Same lists, mobile shape. |
| `list-anatomy.jsx`, `list-variants.jsx`, `list-mobile-anatomy.jsx` | Anatomy / spec diagrams shown in the canvases. Documentation, not part of the component. |
| `list-screens.jsx` | Desktop screens (Strategies V2, Eval Runs V2, Run Detail V2, Home V2) wrapping the lists with sidebar + topbar — context for how lists slot into a full page. |
| `styles.css` | Global design tokens + base styles (buttons, pills, table, sidebar). The list component depends on the CSS variables in `:root`. |
| `mobile-styles.css` | Mobile chrome (topbar, drawer, sheets) — referenced by the mobile canvas only. The list component's own mobile styles are inlined into `list-toolbar-mobile.jsx`. |
| `shared.jsx` | `<Icon>` component used by both the desktop and mobile list — replace with your project's icon library. |
| `design-canvas.jsx`, `ios-frame.jsx` | Canvas / device-frame chrome used only by the preview HTML. Not part of the production component. |

---

## Design tokens

These are defined in `styles.css` as CSS custom properties on `:root`. Port them as constants/SCSS vars/Tailwind config as appropriate.

### Colors

| Token | Hex | Used for |
|---|---|---|
| `--bg` | `#000000` | App background (warm near-black) |
| `--surface-sidebar` | `#000000` | Sidebar |
| `--surface-card` | `#0A0A0A` | Cards, list row default |
| `--surface-elev` | `#0E0E0E` | Inputs, selects, sort dropdowns |
| `--surface-panel` | `#121212` | Avatar, sheet handle bg |
| `--surface-hover` | `#121212` | Row hover |
| `--border` | `#1A1A1A` | Default border |
| `--border-strong` | `#2A2A2A` | Buttons, kbd shortcut chip |
| `--border-soft` | `#141414` | Table separators, list dividers |
| `--text` | `#FFFFFF` | Primary text |
| `--text-2` | `#9CA3AF` | Secondary text |
| `--text-3` | `#5F6670` | Muted labels |
| `--text-4` | `#3A3F47` | Disabled |
| `--gold` | `#00E676` | **Primary accent**, active filter highlight, active sort, chip border |
| `--gold-soft` | `#00B85F` | Focus border, secondary gold |
| `--gold-bg` | `rgba(0, 230, 118, 0.1)` | Gold tint background |
| `--gold-bg-strong` | `rgba(0, 230, 118, 0.18)` | Count pill background |
| `--warn` | `#FFB020` | Running, Warnings |
| `--danger` | `#FF4D4D` | Failed, negative returns, Archived close |
| `--info` | `#5FA8FF` | Sell action, paper mode |

### Typography

Three font families, loaded from Google Fonts:

```css
@import url('https://fonts.googleapis.com/css2?family=Geist:wght@400;500;600;700;800&family=Inter:wght@400;500;600&family=Geist+Mono:wght@400;500;600;700&display=swap');
```

| Family | Used for |
|---|---|
| **Inter** | Default UI text, buttons, body |
| **Cormorant Garamond** (serif) | Headings, list titles, hero metrics, big numbers |
| **JetBrains Mono** | IDs, timestamps, numeric values, tokens, keyboard shortcuts, micro-labels |

### Radii

| Token | Value |
|---|---|
| `--radius-card` | `6px` |
| `--radius-sm` | `4px` |
| Mobile pill controls | `100px` (full pill) |
| Mobile sheet | `18px` top-only |
| Mobile row card | `8px` |

### Spacing

The component uses a loose 4/6/8/10/12/14/16/18/20px scale. Toolbar internal gaps are 6–10px; card padding is 14–20px depending on density.

---

## Component spec — Desktop

### `<ListCard>` — top-level wrapper

A `.card` (1px border, 6px radius, `--surface-card` background) containing:

```
┌────────────────────────────────────────────────────┐
│  Header (optional)                                 │
│  ─ title (serif, 22px) ─ count pill ─ subtitle    │  16px 20px 8px
│                                ─ right actions ─   │
├────────────────────────────────────────────────────┤
│  Toolbar                                           │
│  [🔍 search]  [filter 1] [filter 2]  [↕ sort]      │  4px 20px 14px
│  Active: search "btc" × │ status: Validated × │ Clear all
├────────────────────────────────────────────────────┤
│  Table body (caller-rendered rows)                 │
└────────────────────────────────────────────────────┘
```

**Props**
- `title?: string` — header label, rendered in Cormorant Garamond italic via `.serif` class (22px, weight 500)
- `count?: number` — small `.pill` next to the title (11px JetBrains Mono inside a 1px border)
- `subtitle?: string` — small muted text right of count
- `density?: "full" | "compact"` — default `"full"`
- `toolbar` — `{ search, filters, sort, actions, showSearch?, showSort?, showActiveChips? }`
- `columns: { key, label, align?, width? }[]`
- `rows: T[]` (already filtered + sorted by caller using `useListState`)
- `renderRow: (row, i) => JSX.Element` — caller renders `<tr>` with `<td>` cells
- `actions?: JSX.Element` — right-side header buttons (e.g. "New strategy")
- `footer?: JSX.Element` — optional bottom strip with counts and "View all →"
- `empty?: string` — message when `rows.length === 0`

### `<ListToolbar>`

The toolbar row, usable on its own. Children:

#### Search input (`.lt-search`)
- 32px tall pill with leading `search` icon, trailing `/` keyboard shortcut hint
- Background `--surface-elev`, 1px `--border`, 4px radius
- Focus ring: border becomes `--gold-soft`
- `×` clear button appears when value is non-empty
- Default width 280px; placeholder uses `--text-3`

#### Filter / Sort select (`.lt-select`)
- 32px tall, 1px `--border`, 4px radius, `--surface-elev` bg
- Two-line content: 11.5px `--text-3` label on left ("Status"), 12.5px `--text` value on right ("All status")
- Chevron icon (size 11) trailing
- **When non-default**: border becomes `rgba(0, 230, 118, 0.45)`, background becomes `rgba(0, 230, 118, 0.06)`, value text turns `--gold`
- Native `<select>` is absolutely positioned over the whole pill with `opacity: 0` — preserves OS-native dropdown behavior
- Width: filters auto-size; sort is 180px (full) / 120px (compact)

#### Actions (`.lt-actions`)
- Right-aligned via `margin-left: auto`
- Houses page-level CTAs ("New strategy", "Compare selected (0)", etc.)
- Uses existing `.btn`, `.btn.ghost`, `.btn.primary` classes

### `<ListActiveChips>` — Active filter pills row

Renders below the toolbar when search or any filter is non-default.

- Leading 10.5px uppercase `--text-3` label "Active"
- Each chip: 22px tall, 1px `rgba(0, 230, 118, 0.35)` border, `rgba(0, 230, 118, 0.08)` bg, gold text
  - Format: `[muted key]  [bold value]  [× close]`
  - Click anywhere → resets that filter to its default option (option index 0)
- Trailing "Clear all" link in muted text-3, underlined

### `useListState({ rows, filters, sortOptions, filterFn, sortFn, initialSort })` hook

Manages all state and returns derived rows.

**Returns** `{ search, filters, sort, rows }` ready to spread into `<ListCard toolbar={...} rows={...}>`.

```jsx
const list = useListState({
  rows: STRATEGIES_DATA,
  filters: [
    { id: "status", label: "Status", options: [
      { value: "all", label: "All status" }, { value: "Validated", label: "Validated" }, …
    ]},
  ],
  sortOptions: [
    { value: "added", label: "Recently added" }, // ALWAYS first
    { value: "added-asc", label: "Oldest first" },
    { value: "name", label: "Name A → Z" },
    // domain-specific options follow
  ],
  filterFn: (row, query, filterValues) => boolean,
  sortFn: (rows, sortKey) => sortedRows,
});

return (
  <ListCard
    title="Strategies"
    count={STRATEGIES_DATA.length}
    toolbar={list}
    columns={[...]}
    rows={list.rows}
    renderRow={(r) => <tr>...</tr>}
  />
);
```

### Compact density rules
- Search input collapses to a 32×32px icon button until clicked; expanding shows a 200px-wide input
- Select labels (the gray prefix) are hidden via CSS — only the current value shows
- Active-chips row is suppressed
- `/` keyboard hint is hidden

---

## Component spec — Mobile

### `<MListCard>` — mobile wrapper

Full-height column layout with sticky header + scrollable body.

```
┌──────────────────────────────────────┐
│ Strategies   8   (header, 26px serif)│
├──────────────────────────────────────┤
│ 🔍 Search strategies, templates…     │  ← 38px pill, always visible
│ [⚙ Filter (2)]  [Sort: Recent ▾   ]  │  ← 32px pill row
│ status: Validated × │ tpl: trend × Clear │  ← active chips (optional)
├──────────────────────────────────────┤
│ ┌──────────────────────────────────┐ │
│ │ ETH-MR-V3      [VALIDATED]       │ │
│ │ mean_reversion                   │ │
│ │ 53.5k tok · 14m ago        1.62  │ │ ← MListRow card
│ │                            Sharpe│ │
│ └──────────────────────────────────┘ │
│  …more rows…                          │
└──────────────────────────────────────┘
```

**Props** (delta from desktop):
- No `columns` — rows are free-form via `renderRow`
- No `footer`
- `rightAction?` — right-side header icon button
- `pad?: boolean` — when `false`, removes inner padding so rows can be flush

### `<MListRow>` — convenience row component

Flex card with body on left, hero metric on right.

- Min-height ~64px, 12px×14px padding
- 1px `--border`, 8px radius, `--surface-card` bg
- Tap state: bg becomes `--surface-hover`
- **Left column:** title (13.5px JetBrains Mono, weight 500) + badge pill + subtitle (12px mono `--text-2`) + meta (11px mono `--text-3`)
- **Right column:** `rightTop` in 15px Cormorant 500 weight + `rightSub` in 11px mono muted
- Badges via `badgeColor`: `gold | warn | danger | info | muted`
  - 18px tall, 7px padding, 3px radius, 9.5px mono uppercase with 0.08em tracking

### `<MListSheet>` — bottom sheet

Triggered by tapping Filter or Sort. Slides up over the content with backdrop blur.

```
┌──────────────────────────────────────┐
│      ━━━━ (drag handle)              │
│  Filter & sort        CLEAR ALL      │  serif italic 22px
├──────────────────────────────────────┤
│  STATUS                              │  group label, 10.5px mono uppercase
│  [✓ All]  [Validated]  [Draft] …     │  single-select pill group
│                                       │
│  TEMPLATE                            │
│  [✓ All] [mean_reversion] …          │
│                                       │
│  ▢ SORT BY                           │
│  ┌──────────────────────────────┐    │
│  │ ●  Recently added            │    │ radio list, current = gold + bg
│  │ ○  Oldest first              │    │
│  │ ○  Recently updated          │    │
│  └──────────────────────────────┘    │
├──────────────────────────────────────┤
│  [ Show 4 results ]                  │ 46px gold pill, full width
└──────────────────────────────────────┘
```

- Sheet animates `translateY(100%) → 0` over 220ms cubic-bezier(.2,.7,.3,1)
- Backdrop: `rgba(0,0,0,0.55)` + 2px blur; tapping it closes the sheet
- Sheet max-height `88%`, body scrolls
- Apply button shows live `resultCount` so users know what they're committing to
- **Sort-focused mode**: tapping the Sort pill (not Filter) opens the same sheet but hides the filter groups, leaving only the sort radio list

### Mobile control row

- **Filter pill** — 32px, pill-shaped, leading `sliders` icon. Shows a small gold badge with the count of non-default filters. When any filter is active, the whole pill turns gold (border + bg + text).
- **Sort pill** — 32px, `flex: 1` (takes remaining width). Format: `Sort: <current label> ▾`. Always opens the sheet in sort-focused mode.

---

## Sort options — the standard set

Every list MUST offer at least these in the sort dropdown, in this order, with "Recently added" as the default:

1. **Recently added** ← default
2. **Oldest first**
3. **Recently updated**
4. **Name A → Z**
5. **Name Z → A**

Domain-specific sort options follow (e.g. *Sharpe high → low*, *Return high → low*, *PnL*, *Conviction*). They may replace tail items but never the first one.

---

## Applied screens — how the component slots in

See `list-examples.jsx` and `list-mobile-screens.jsx` for working code. Summary:

| Screen | Density | Filters | Sort options |
|---|---|---|---|
| **Strategies** (`/strategies`) | full | Status, Template | + Tokens (high → low) |
| **Eval runs** (`/eval/runs`) | full | Strategy, Mode, Status | + Sharpe, + Return, + Max DD |
| **Decisions** (Run detail) | compact | Action, Conviction | + Conviction |
| **Trade ledger** (Run detail) | compact | Outcome (wins / losses) | + PnL (asc / desc) |
| **Recent runs** (Home) | compact | Mode | + Sharpe, + Return |
| **Open positions** (Home) | compact | — | + PnL, + Symbol |

Notes on the column layouts (table-based, desktop only):
- Checkbox column when bulk actions exist (Strategies, Eval runs)
- Numeric columns right-aligned, `.mono` font
- Status column: `<span className="dot {color}"/>` + label
- Three-dot menu `⋯` in the last column for row actions

---

## Interactions & behavior

### Desktop
- Search debounces 1 frame (uses React's batching via `setState`)
- `/` key focuses search input (host page wires this; the component renders the hint)
- Filter selects use native `<select>` for OS-correct dropdown behavior on every platform
- Active chip click: resets that filter to its default option (option index 0)
- "Clear all" link: resets search to "" and all filters to defaults

### Mobile
- Search has no debounce — typing filters in real-time
- Tapping Filter or Sort pill opens the same sheet (different scroll focus)
- Sheet closes on: backdrop tap, Apply button, or X (Apply pattern preferred — the sheet acts as a non-destructive draft until Apply, except in this design the changes apply live and Apply just dismisses)
- Active filter chips below the controls are individually tap-to-remove

### Both
- Empty state: rows render a single full-width row with the `empty` message in muted text, padded 28px on desktop / 36px on mobile
- Hover on rows (desktop): bg becomes `--surface-hover`
- Active press on rows (mobile): bg becomes `--surface-hover` via `:active`

---

## Implementation notes

1. **Token system first.** Port `:root { --bg, --gold, --text, … }` from `styles.css` into whatever the project uses (CSS vars, Tailwind theme, design-tokens JSON). Every other style references these.

2. **Replace `<Icon>`.** The component uses an inline-SVG `<Icon name="…">` helper from `shared.jsx`. Use the project's icon library — the icon names referenced are: `search`, `sliders`, `plus`, `chevR` (chevron-right). All other glyphs are inline SVGs in component code.

3. **`useListState` should accept generic types.** In TypeScript:
   ```ts
   function useListState<T>(opts: {
     rows: T[],
     filters?: FilterDef[],
     sortOptions?: SortOption[],
     filterFn?: (row: T, query: string, filters: Record<string, string>) => boolean,
     sortFn?: (rows: T[], sortKey: string) => T[],
     initialSort?: string,
   }): { search, filters, sort, rows: T[] }
   ```

4. **Server-side filtering.** For large datasets, the host should pass already-filtered/sorted `rows` and skip `filterFn`/`sortFn`. The component itself is presentation-only when those are omitted. Consider adding a `loading` prop for skeleton rows.

5. **Persisting state.** Search / filter / sort state should optionally sync to URL query params so deep links work (`?q=eth&status=Validated&sort=sharpe`). Implement this in the host page, not the component.

6. **Keyboard accessibility.**
   - `Tab` should walk: search → each filter → sort → actions → rows
   - `/` focuses search (host-level shortcut)
   - `Esc` clears search if focused
   - On mobile, sheet should trap focus while open

7. **Performance.** `useListState` memoizes derived rows. For lists >1000 rows, add virtual scrolling (react-virtual or similar) inside the table body.

8. **Bottom sheet on mobile.** If the codebase already has a sheet primitive (e.g. Radix Dialog with a sheet variant, or a native bottom-sheet on iOS), use that instead of the hand-rolled one in `list-toolbar-mobile.jsx`. The CSS-driven version is fine but doesn't handle: focus trap, swipe-to-dismiss, scroll-lock on body. Production should have all three.

---

## Open follow-ups (not in this design, worth adding later)

These came up during design discussion but are not implemented in this handoff. Mention to PM before scoping.

- **Multi-select filters** — currently single-select dropdowns. The mobile sheet's pill-group UI already accommodates multi-select visually; the data model just needs to become `string[]` per filter.
- **Saved views** — persist a search+filter+sort combo with a name, list them in a dropdown above the toolbar.
- **Column-header sorting** — clicking a sortable column header (`columns[i].sortable`) sets the sort key. Visual: a small arrow next to the active header. Coexists with the sort dropdown.
- **Server-side pagination** — `useListState` hook should accept a `pageSize` and emit `{ page, setPage, total }` for paginated APIs.
- **Density toggle in toolbar** — let users switch between full/compact themselves (persisted to localStorage).

---

## Recreating in the target codebase

1. Define the token system in your styling solution.
2. Create the `useListState` hook (~50 lines, framework-agnostic React).
3. Build `<ListCard>` + `<ListToolbar>` + `<ListActiveChips>` + `<ListSelect>` + `<ListSearch>`.
4. Build the mobile counterparts: `<MListCard>` + `<MListRow>` + `<MListSheet>`.
5. For each existing list screen, replace its bespoke search/filter/sort UI with `<ListCard>` (or `<MListCard>`), passing the screen-specific filter/sort options and `filterFn`/`sortFn`.
6. Verify against the open canvas files (`Lists.html`, `Lists Mobile.html`) — visual diff each applied screen.

Once shipped, **no list in xvn should have its own search/filter/sort UI**. If a list page can't be expressed via `ListCard`, that's a sign the component needs to grow — file an issue, extend the component, then migrate the offending list.
