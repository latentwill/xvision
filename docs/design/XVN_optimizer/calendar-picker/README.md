# xvn — Calendar picker

Inline date-range picker for the Strategies page. Folio dark, gold accent,
Cormorant display + JetBrains Mono numerics.

## Files

| File | Role |
|---|---|
| `Calendar Picker.html` | Entry point — design canvas with every artboard |
| `calendar-core.jsx` | Date utilities, `MonthGrid`, `MonthHeader`, `MonthsView`, `YearsView`, `CalendarView` |
| `calendar-desktop.jsx` | `InlineRangeBar` (the recommended pattern), `DualMonthRangePopover`, `CompactPresetCalendar`, `FilterBarTrigger` |
| `calendar-mobile.jsx` | `MobileBottomSheet`, `MobileInlineCard`, `PhoneFrame` |
| `strategies-context.jsx` | `StrategiesPageWithCalendar` — shows the inline bar in its natural home |
| `shared.jsx` | `Icon`, `Sidebar`, `Topbar`, `Sparkline` — lifted from the xvn prototype |
| `styles.css` | xvn Folio dark theme (tokens, base components) |
| `design-canvas.jsx` | Canvas wrapper — pan/zoom, focus mode, drag-reorder artboards |

## Open it

Just open `Calendar Picker.html` in a browser. No build step.

## Components at a glance

- **`<InlineRangeBar>`** — the primary pattern. Disclosure bar that swings open
  in the page flow, no overlay. Props: `initialOpen`, `initialStart`,
  `initialEnd`, `initialAnchor`, `width`, `label`.
- **`<MonthHeader>`** — month + year, each independently clickable with a
  dotted-underline affordance and a small caret. Pass `view` (`"days"`,
  `"months"`, or `"years"`) and the matching `onMonthClick` / `onYearClick`.
- **`<CalendarView>`** — convenience wrapper that owns the view-switching
  state. Used by the compact and mobile variants. Pass `initialView` to
  open it on a specific panel.

## Behavior notes

- Click the **month name** → 4×3 month grid; chevrons step ±1 year.
- Click the **year** (italic) → 4×3 year grid; chevrons step ±12 years.
- Picking a year drills into the month picker; picking a month drills back
  into days. Today's year + the anchor's current month get a quiet gold tint.
- Range selection: first click sets start, second click sets end. Clicking a
  date earlier than the current start resets start. Hover preview between
  clicks.
- Picking a preset fills both ends and pops the active-preset indicator in
  the header.
