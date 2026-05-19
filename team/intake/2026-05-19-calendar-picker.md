# Intake — 2026-05-19 — Calendar picker (date-range + calendar-type select)

Frontend track to implement the calendar-picker component design package
landed at `docs/design/calendar-picker/` and to retire the free-form
date/calendar inputs on the scenario form. Component-level only — this
intake does **not** touch page layout, route shells, or surrounding
chrome on the scenario routes.

## Source

- Operator request, 2026-05-19 session: "I added the docs/design
  calendarpicker zip file in and want to implement that ui for dates.
  Preferably compact, no pop ups."
- Component design package: `docs/design/calendar-picker/` (extracted
  from `docs/design/calendarpicker.zip` on 2026-05-19).
- Operator constraint, same session: design references must be
  component-only — no page layout. Implementation must not reach into
  the host route's chrome to satisfy the design.

## Why now — the structural fix that retires model-specific normalizer hacks

The wizard's `normalize_create_scenario_input`
(`crates/xvision-dashboard/src/wizard_loop.rs`) has accumulated
model-specific repair logic for the `calendar` field because Qwen
keeps emitting invented variant names (`calendar`, `UserGenerated`,
`{type: "Continuous24x7"}` tag-wrappers). PR #272 added unwrap logic;
PR #314 added `coerce_to_one_of` fallback. Each repair is dead code
the day we swap that model out, and per
[[feedback-systematic-over-model-specific]] this is the wrong layer.

The structural fix: replace the operator-facing free-form `calendar`
text input + `<input type="date">` From/To pair on
`frontend/web/src/components/scenario/ScenarioForm.tsx` with the
component-package picker. The calendar select becomes a typed enum
control (canonical set only); date inputs become a real range picker.
The model can no longer invent variant names because the values that
reach the tool call come from a constrained UI, not free text — and
the wizard's `create_scenario` tool can drop its per-model repair
shims in a follow-up.

Note: the **tool-call schema** for `create_scenario` is separate from
the UI. This intake only changes the operator-facing UI. The follow-up
to retire the normalizer hacks is to (a) ship a stricter JSON Schema
description for the `calendar` field in the tool definition so the
model is told the canonical set up front, and (b) once the UI lands,
audit whether the normalizer's coerce-fallback can be deleted. Track
(b) under a separate `wizard-normalizer-cleanup` contract after this
one merges — not bundled here.

## Component design package — what to use

`docs/design/calendar-picker/`:

| File | Use? | Notes |
|---|---|---|
| `README.md` | Yes — read first | Component API, props, behavior notes |
| `calendar-core.jsx` | Port | `MonthGrid`, `MonthHeader`, `MonthsView`, `YearsView`, `CalendarView`, date utils |
| `calendar-desktop.jsx` | Port — primary | `<InlineRangeBar>` is the recommended pattern (disclosure bar that swings open in the page flow, NO overlay) |
| `calendar-mobile.jsx` | Port | `MobileInlineCard` for narrow viewports |
| `shared.jsx` | **Reference only — do NOT port** | Page chrome (`Sidebar`, `Topbar`) — out of scope |
| `strategies-context.jsx` | **Do NOT port** | `StrategiesPageWithCalendar` is a page-layout demo, out of scope |
| `design-canvas.jsx` | **Do NOT port** | Canvas wrapper for the design preview, not production |
| `styles.css` | Reference — adapt tokens | Map design tokens to existing xvn Tailwind theme; don't ship the file wholesale |
| `Calendar Picker.html` | Reference only | Design-canvas entry point |

The `InlineRangeBar` is the load-bearing component. From the README:

> Disclosure bar that swings open in the page flow, no overlay.
> Props: `initialOpen`, `initialStart`, `initialEnd`, `initialAnchor`,
> `width`, `label`.

This matches the no-popup rule (`/CLAUDE.md`: "no popups, modals,
sheets, popovers"). The inline-disclosure pattern is the right shape;
the `DualMonthRangePopover` variant in `calendar-desktop.jsx` is NOT
the pattern to ship — popover ≠ inline.

## Findings → tracks

| # | Severity | Finding | Track |
|---|---|---|---|
| 1 | P1 | Two native `<input type="date">` controls for From/To on `ScenarioForm.tsx` (lines 252-274). No range semantics, no preset chips beyond the existing `RegimeRangePresets`, no visual confirmation of the picked range. Wizard models invent date strings; UI has no constraint beyond browser-level date validation. | `scenario-form-date-range-picker` |
| 2 | P1 | Calendar field is hardcoded to `'Continuous24x7'` (line 45, `const CALENDAR: CalendarRef = 'Continuous24x7'`). Operator and model cannot pick between the three canonical variants (`Continuous24x7`, `UsEquities`, `{Custom: string}`). The free-form input lives in the wizard tool-call path only, which is why the normalizer accumulated Qwen-specific repairs. | bundled into the same track as #1 — they're one form |

Two findings, one track. Splitting would force coordinated edits on
the same file across two contracts.

## Track summary — `scenario-form-date-range-picker` (P1, frontend)

Replace the From/To `<input type="date">` pair AND the hardcoded
`CALENDAR` constant on `ScenarioForm.tsx` with:

1. **`<InlineRangeBar>`** ported from `calendar-desktop.jsx`. Compact,
   no popover. Slots into the existing `<Row><Field label="From">...
   <Field label="To">` block — same vertical real estate, no page
   shell changes. Wired to the existing `from` / `to` state
   (currently `YYYY-MM-DD` strings; the bar emits the same).
2. **Calendar-type `<select>`** alongside it. Three options matching
   `CalendarRef`: `Continuous24x7`, `UsEquities`, `Custom`. Picking
   `Custom` reveals a single inline text input bound to the inner
   string (matching the `{ "Custom": string }` enum payload). State
   replaces the `const CALENDAR` literal.
3. **Mobile path:** swap to `<MobileInlineCard>` from
   `calendar-mobile.jsx` at the existing form's `sm:` breakpoint —
   do NOT introduce new responsive scaffolding outside the form.
4. **Preset chips:** keep the existing `<RegimeRangePresets>`
   row. The bar's preset header (per README) is a future enhancement
   — don't compete with the existing presets on first ship.

### Concrete scope

- New `frontend/web/src/components/calendar-picker/` directory with
  ported `.tsx` versions of `calendar-core.jsx`, `calendar-desktop.jsx`,
  `calendar-mobile.jsx`. Strip out demo wrappers
  (`PhoneFrame`, `FilterBarTrigger` is borderline — port only if used
  by `InlineRangeBar` itself; otherwise skip).
- TypeScript types for props from the JSX docstrings; pin `Date` math
  to UTC to match the existing `${from}T00:00:00Z` serialization on
  line 179.
- Style adaptation: convert `styles.css` tokens to Tailwind classes
  using the existing xvn theme variables. Dark-mode borders per
  workspace rule — no `border-white` / `border-gray-100/200`. Folio
  dark + gold accent comes from existing tokens; do not introduce new
  CSS variables.
- Replace lines 251-275 of `ScenarioForm.tsx` with `<InlineRangeBar>`.
- Replace line 45 (`const CALENDAR: CalendarRef = 'Continuous24x7';`)
  with form state: `const [calendar, setCalendar] = useState<CalendarRef>('Continuous24x7')`.
  Add the calendar select right after the From/To row.
- Update the submit handler (line 183) to use the new `calendar` state
  instead of the constant.
- Component tests for `InlineRangeBar`: range select, preset, anchor
  navigation (year → months → days), keyboard nav.
- Component tests for the calendar `<select>`: switching to `Custom`
  reveals the inline string input; payload shape matches `CalendarRef`.
- Update existing `ScenarioChart.test.tsx` / `scenarios-detail.test.tsx`
  fixtures that hardcode `calendar: "Continuous24x7"` — they should
  keep working unchanged (default state matches today's behavior).
- Update `WizardPreviewChart.tsx:105` (same hardcoded calendar string)
  to read from the new state if it consumes the form's draft via
  `onDraftChange`; otherwise leave it.

### Out of scope

- The wizard's tool-call schema (`create_scenario`) and the
  normalizer cleanup in `wizard_loop.rs`. A follow-up
  `wizard-normalizer-cleanup` contract picks that up after this UI
  lands. The UI change alone does NOT delete the normalizer — it
  removes the operator path that creates the typed-wrong values, but
  the model can still emit them via the wizard tool call until the
  schema is tightened.
- Any page layout, route shell, sidebar, topbar, or header changes.
  The intake is component-only. If the design package's
  `strategies-context.jsx` or `shared.jsx` is consulted, it's reference
  only — porting the page wrapper is out of scope.
- The TradingView chart customization pass (V3 item 16) — separate
  track, separate design source.
- A new design-token file. Adapt to the existing theme; don't ship
  Folio-specific CSS variables that don't already exist in the xvn
  theme tokens.
- Replacing the existing `<RegimeRangePresets>` with the design
  package's preset chips. Future enhancement.

### Acceptance

- `ScenarioForm.tsx` no longer contains `<input type="date">`. From/To
  is rendered by `<InlineRangeBar>` (or `<MobileInlineCard>` on
  narrow viewports).
- `ScenarioForm.tsx` no longer hardcodes `const CALENDAR: CalendarRef
  = 'Continuous24x7'`. A `<select>` exposes the three canonical
  values; `Custom` reveals an inline string input.
- No popover, modal, sheet, or overlay introduced. `InlineRangeBar`
  is the disclosure-bar pattern (in-flow, not overlay). No new
  `Dialog` / `Popover` / `Sheet` imports in any file this track
  edits.
- Dark-mode borders compliant with `/CLAUDE.md` (no
  `border-white`/`border-gray-100/200`).
- Component tests for range select, preset, view-switching, mobile
  variant, and the calendar-type select green.
- `cargo test -p xvision-dashboard wizard_loop` continues to pass —
  the wizard tool-call path is unchanged (normalizer hacks stay until
  the follow-up contract retires them).
- No regression in `ScenarioChart.test.tsx`, `scenarios-detail.test.tsx`,
  or `eval-runs.test.tsx`.

## Verbatim findings

> oh. well I added the docs/design calendarpicker zip file in and want
> to implement that ui for dates. Preferably compact, no pop ups.
> Design docs Should only reference the component not overall page
> layout. Move review agent to this board from v2 and then intake the
> calendar picker.

> add this as part of calendar picker (frontend track: replace the
> free-form time_window + calendar text fields in ScenarioForm.tsx
> with a real date-range picker and a calendar-type select, so the
> model and operator can't type invented values — directly addresses
> the "fix it system-side not per-model" frustration).
