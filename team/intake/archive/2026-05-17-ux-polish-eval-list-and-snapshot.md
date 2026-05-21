# Intake — 2026-05-17 — UX polish: chart snapshot title + eval-list friendly labels + scroll indicator

Three small UX nits raised by the operator during 2026-05-17 conductor walk-through.
All three live on the dashboard surface; all three are independent of any
backend change.

## Source

Operator request, 2026-05-17, during conductor handoff.

## Items

### 1. Chart snapshot on Home shows eval title + date and "latest eval" framing

`frontend/web/src/routes/home.tsx` — `ControlChartCard`
(`frontend/web/src/routes/home.tsx:306-353`) currently renders the heading
`"Chart snapshot"` and an `open eval →` link, with no indication of which
eval run the chart represents.

Expected:

- Heading or sub-line shows the latest eval run's title (strategy name +
  scenario name) and `started_at` date.
- Make it visually obvious this is the **latest** eval (e.g. "Latest eval ·
  <strategy> on <scenario> · <date>" as a sub-line beneath the heading).

The latest-run row is already fetched by the same page. The data should not
require any new API call — pipe the selected `RunSummary` (the one the chart
is sourced from) into `ControlChartCard` and render strategy/scenario
display names + `fmtTime(started_at)`.

Names: the data model uses `agent_id` (strategy id) and `scenario_id`; the
human-friendly display names come from `/api/strategies` and
`/api/scenarios` lookups (see item 2 — same data plumbing).

### 2. Eval list "Scenario" and "Strategy" columns show display name, not raw id

`frontend/web/src/routes/eval-runs.tsx` — desktop table and mobile card both
print raw ids:

- `eval-runs.tsx:353` (mobile): `{row.agent_id.slice(0, 8)} · {row.scenario_id}`
- `eval-runs.tsx:472-475` (desktop): `Strategy` column prints
  `row.agent_id.slice(0, 8)`; `Scenario` column prints `row.scenario_id`.

Expected:

- Strategy column shows strategy `display_name`.
- Scenario column shows scenario `title` (or whatever the scenario object's
  human-friendly label is).
- Fallback to a short id only when the strategy/scenario has been deleted
  and the lookup misses.

Data sources already wired into the same page:

- `listStrategies()` from `@/api/strategies` (already imported at
  `eval-runs.tsx:35-37`) — returns `StrategyListItem[]` with
  `display_name`.
- `listScenarios()` from `@/api/scenarios` (already imported at
  `eval-runs.tsx:25-27`) — returns `Scenario[]`.

Both queries are already used by the "Start new eval" panel on the same
route, so adding lookup `Map`s keyed by id is a single hook addition.

### 3. Scroll indicator on Eval list horizontal axis

The desktop eval table sits inside an `overflow-x-auto` wrapper
(`eval-runs.tsx:416`). On wider columns or compressed viewports it
horizontally scrolls, but there is no visual hint a scrollbar / scroll
area exists; users miss columns beyond the visible width.

Expected:

- A visible affordance that the table scrolls horizontally — options:
  - Persistent custom scrollbar (CSS scrollbar styling so it's always
    visible, not just on hover).
  - Fade/shadow gradient on the right edge when content overflows.
  - A small "scroll →" caption rendered above the table when overflow is
    detected.
- Pick the lowest-noise option that survives the dark/light theme contract.
  Either persistent scrollbar styling or an edge fade gradient is preferred
  over a caption.

## Out of scope

- Refactoring the eval list filtering, pagination, or row layout.
- Changing the `agent_id` → display-name mapping on the backend.
- Backfilling display names for orphaned runs whose strategy has been
  deleted — fallback to short id is acceptable.

## Decomposition

One leaf contract is enough: `ux-polish-eval-list-and-snapshot`. All three
items touch frontend-only files, no migrations, no API changes.
