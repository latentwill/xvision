---
track: scenario-form-date-range-picker
lane: leaf
wave: calendar-picker-2026-05-19
worktree: .worktrees/scenario-form-date-range-picker
branch: task/scenario-form-date-range-picker
base: origin/main
status: pr-open
depends_on: []
blocks: []
stacking: none
allowed_paths:
  - frontend/web/src/components/calendar-picker/**             # NEW component dir
  - frontend/web/src/components/scenario/ScenarioForm.tsx
  - frontend/web/src/components/scenario/ScenarioForm.test.tsx # existing tests fireEvent against the From/To inputs being replaced; needs update
  - frontend/web/src/components/chart/WizardPreviewChart.tsx
  - team/contracts/scenario-form-date-range-picker.md
  - team/status/scenario-form-date-range-picker.md
  - team/board.md
  - team/OWNERSHIP.md
forbidden_paths:
  - crates/**                                                  # pure frontend track
  - crates/xvision-dashboard/src/wizard_loop.rs                # normalizer cleanup is a SEPARATE follow-up
  - frontend/web/src/api/types.gen/**                          # no Rust type changes
  - frontend/web/src/routes/**                                 # no page-layout changes
  - frontend/web/src/components/scenario/RegimeRangePresets.tsx # keep the existing presets row untouched on this pass
  - frontend/web/src/components/chart/ScenarioChart.test.tsx   # fixture only — must keep working unchanged
  - frontend/web/src/routes/scenarios-detail.test.tsx          # fixture only — must keep working unchanged
  - frontend/web/src/routes/eval-runs.test.tsx                 # fixture only — must keep working unchanged
interfaces_used:
  - frontend/web/src/api/types.gen/CalendarRef.ts              # CalendarRef = "Continuous24x7" | "UsEquities" | { "Custom": string }
  - frontend/web/src/api/types.gen/CreateScenarioRequest.ts    # time_window + calendar fields on the submit payload
parallel_safe: true
parallel_conflicts: []
verification:
  - cd frontend/web && pnpm install --frozen-lockfile
  - cd frontend/web && pnpm typecheck
  - cd frontend/web && pnpm test -- calendar-picker scenario
  - cd frontend/web && pnpm build
acceptance:
  - New directory `frontend/web/src/components/calendar-picker/` ports the COMPONENT layer of `docs/design/calendar-picker/` to TypeScript + Tailwind. Files mirror the design package's component split: `calendar-core.tsx` (date math + `MonthGrid` / `MonthHeader` / `MonthsView` / `YearsView` / `CalendarView`), `calendar-desktop.tsx` (`InlineRangeBar` — primary; supporting helpers it actually uses), `calendar-mobile.tsx` (`MobileInlineCard`). Page-chrome demos (`strategies-context.jsx`, `design-canvas.jsx`, `shared.jsx` Sidebar/Topbar) are NOT ported. `styles.css` is NOT shipped wholesale — token mapping converts to existing xvn Tailwind classes.
  - `<InlineRangeBar>` is the load-bearing component used by the form. It is an inline disclosure bar (in-flow, no overlay) per `docs/design/calendar-picker/README.md`. **No** `Dialog`, `Popover`, `Sheet`, or `DropdownMenu` imports introduced in any file this track edits. `DualMonthRangePopover` from the design package is explicitly NOT ported — popover ≠ inline per the workspace no-popup rule.
  - `ScenarioForm.tsx` no longer contains `<input type="date">`. The From/To `<Row><Field>` block at lines 250–275 is replaced by `<InlineRangeBar>` (desktop) / `<MobileInlineCard>` (sm: breakpoint). The bar's `onChange` writes back into the existing `from` / `to` state in the `YYYY-MM-DD` shape the submit handler already serializes at line 179 (`${from}T00:00:00Z`). No change to the submit payload schema.
  - `ScenarioForm.tsx` no longer hardcodes `const CALENDAR: CalendarRef = 'Continuous24x7';` (currently line 45). A new `<select>` (or segmented control if the design package shows one — pick the one that's smaller) exposes the three canonical `CalendarRef` shapes: `Continuous24x7`, `UsEquities`, and `Custom`. Picking `Custom` reveals a single inline text input bound to the inner string, producing the typed `{ "Custom": <string> }` payload that matches the Rust enum. Form state replaces the constant; the submit handler at line 183 reads from state, not a constant.
  - `WizardPreviewChart.tsx:105` (the other `calendar: 'Continuous24x7'` hardcode) consumes the form's draft via the existing `onDraftChange` if a draft value is available; otherwise it falls back to `'Continuous24x7'` (preview-only — does not block submit). No new prop wiring outside the existing `onDraftChange` channel.
  - Dark-mode borders compliant with workspace rule: no `border-white`, `border-gray-100`, `border-gray-200`, `#fff` / `#ffffff` borders on cards / containers. Use `border-border` or muted tones (`border-muted-foreground/20`) with `dark:` variants. The design package's Folio dark + gold accents map to existing xvn theme tokens — do NOT add Folio-specific CSS variables that don't already exist.
  - Date math pinned to UTC to match the existing serialization (`${from}T00:00:00Z` at line 179). The bar must not silently shift dates to the user's local timezone on display. Where the design package uses `new Date()`, the port uses a UTC-safe equivalent.
  - Component tests under `frontend/web/src/components/calendar-picker/*.test.tsx`:
    - `InlineRangeBar` range select: first click sets start, second sets end, clicking earlier-than-start resets start.
    - `InlineRangeBar` view drill-down: clicking the month name opens the months view; clicking the year opens the years view; picking a year drills into the months view.
    - `InlineRangeBar` UTC discipline: a range selected at a non-UTC local timezone serializes to the expected `YYYY-MM-DD` strings.
    - Calendar select: picking `Custom` reveals an inline string input; the form's `calendar` state matches `{ "Custom": <typed-string> }`. Picking `Continuous24x7` / `UsEquities` produces the bare string.
    - Mobile path: `MobileInlineCard` renders below the form's `sm:` breakpoint; range-select behavior parity with desktop.
  - Existing scenario form tests (`ScenarioChart.test.tsx`, `scenarios-detail.test.tsx`, `eval-runs.test.tsx`) continue to pass without rewrite. Default form state matches today's submit shape (`calendar: 'Continuous24x7'`); fixtures that hardcode this value keep working.
  - `pnpm typecheck` clean. `pnpm test -- calendar-picker scenario` green. `pnpm build` succeeds (no chunk size regression > 5% on the scenario-route chunk).
  - This contract does NOT delete or modify `wizard_loop.rs::normalize_create_scenario_input`. The Qwen-specific repair shims stay in place; retiring them is a separate follow-up contract (`wizard-normalizer-cleanup`) that lands AFTER this UI is live, once the model-facing tool-call schema is also tightened. The two layers (UI + tool schema) close the loop together — this track is one half.
---

# Scope

Implements the calendar-picker intake at `team/intake/2026-05-19-calendar-picker.md`.

Two findings, one track because they share a single file and would
otherwise need coordinated edits across two contracts:

1. The scenario form's From/To dates are two native `<input type="date">`
   controls (`ScenarioForm.tsx:252-274`). Replace with the
   `<InlineRangeBar>` component from the design package at
   `docs/design/calendar-picker/`.
2. The form's `calendar` field is hardcoded to `'Continuous24x7'`
   (`ScenarioForm.tsx:45`). Replace with a `<select>` exposing the
   three canonical `CalendarRef` shapes, with `Custom` revealing an
   inline string input.

Together these constitute the structural fix that removes the
operator-facing path for invalid `calendar` values, which is the
root cause that necessitated the Qwen-specific repair logic in
`normalize_create_scenario_input`. Retiring that logic is a separate
follow-up contract — this track is the UI half.

# Out of scope

- The wizard tool-call schema (`create_scenario` tool definition in
  `crates/xvision-dashboard/src/wizard_loop.rs`). Tightening the JSON
  Schema description for `calendar` so the MODEL knows the canonical
  set is a separate concern. Track: `wizard-create-scenario-schema-tighten`
  (not yet contracted).
- Deleting the `coerce_to_one_of` fallback and the lowercase aliases
  in `normalize_create_scenario_input`. Track: `wizard-normalizer-cleanup`
  (not yet contracted; depends on both this track AND the schema-tighten
  track landing first).
- Replacing the existing `<RegimeRangePresets>` row with the design
  package's preset-chip header. Future enhancement once the inline
  bar is live and we can see whether the two preset surfaces compete
  or complement.
- Any page layout, route shell, sidebar, topbar, or header changes.
  Component-only per the design-package-components-only rule (see
  `[[feedback-design-package-components-only]]`).
- The TradingView chart customization pass (V3 item 16) — separate
  track, separate design source.
- A new design-token CSS file. Adapt to existing xvn theme tokens;
  do not introduce Folio-specific CSS variables.
- Mobile route layout changes outside the scenario form's own
  `sm:` breakpoint behavior. The picker switches to `MobileInlineCard`
  at the existing form's responsive boundary.

# Sync-before-work ritual

```bash
cd /Users/edkennedy/Code/xvision
git fetch --prune origin
git -C .worktrees/scenario-form-date-range-picker status
git -C .worktrees/scenario-form-date-range-picker log --oneline -3 origin/main..HEAD
# Confirm:
#   - clean working tree
#   - branch is task/scenario-form-date-range-picker
#   - base is up to date with origin/main
```

If the worktree does not exist:

```bash
git fetch --prune origin
git worktree add .worktrees/scenario-form-date-range-picker \
  -b task/scenario-form-date-range-picker origin/main
```

# Notes

## 2026-05-19 scope amendment

`ScenarioForm.test.tsx` was incorrectly listed under `forbidden_paths`
on first contract write. The existing tests `fireEvent.change` against
`screen.getByLabelText("From")` / `getByLabelText("To")` directly — those
labels go away when the `<input type="date">` pair is replaced by
`<InlineRangeBar>`. Updating those tests is in scope and unavoidable;
the prior wording was an over-broad "don't touch test files" written
before the input replacement was concrete in the worker's head.

The OTHER scenario-related tests (`ScenarioChart.test.tsx`,
`scenarios-detail.test.tsx`, `eval-runs.test.tsx`) DO stay green
unchanged — they use fixture objects with hardcoded
`calendar: "Continuous24x7"` and never touch the form's input
behaviour. Those files remain in `forbidden_paths`.

Append checkpoints / PR links below.
