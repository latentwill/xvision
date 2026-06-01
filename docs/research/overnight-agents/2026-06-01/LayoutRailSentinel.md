# LayoutRailSentinel

**Date:** 2026-06-01
**Source:** 100x run `.100x/runs/20260601_002720/` and follow-up slice `.100x/runs/20260601_003231/`
**Script:** scripts/layout-rail-sentinel.sh

## Surfaces Inspected

- `frontend/web/src/routes/**` — all routes
- `frontend/web/src/features/**` — feature components
- Router config: `frontend/web/src/routes.tsx` (to determine which routes are inside `<Layout>`)

Rules enforced:
- CLAUDE.md §"Frontend layout rule: no right-side boxes when the chat rail is visible"
- CLAUDE.md §"Frontend UI rule: no popups"

Documented exceptions (never flagged):
- `Toast*` components (transient, non-focus-stealing)
- `FilterDrawer.tsx` (docked right panel, explicitly commented as non-modal)
- `MListSheet.tsx` (mobile list filter, operator-approved 2026-05-20)

## Findings

### VIOLATION — Layout: authoring.tsx right sidebar

**File:** `frontend/web/src/routes/authoring.tsx:85`
**Confirmed inside `<Layout>`:** yes — routes.tsx:134 `{ path: "authoring", element: page(<AuthoringRoute />) }` is a child of the `<Layout />` element at routes.tsx:106.

```tsx
<div className="grid grid-cols-1 lg:grid-cols-[1fr_320px] gap-5">
```

A 320px right column sits next to the chat rail at the large breakpoint. The 320px sidebar compounds with the chat rail's `auto` column, squeezing the center content.

**Migration suggestion:** Move the right-column content (agent configuration card / strategy filter) into a full-width inline strip above the editor. Use `md:grid-cols-[220px_1fr]` inside the strip for the name+type row (already present at line 411) and collapse the card into an accordion if space is constrained.

### VIOLATION — Layout: agent-runs-detail.tsx right SpanInspector panel

**File:** `frontend/web/src/routes/agent-runs-detail.tsx:229`
**Confirmed inside `<Layout>`:** yes — routes.tsx:138 `{ path: "agent-runs/:runId", ... }` is a child of `<Layout />`.

```tsx
<div className="grid grid-cols-1 gap-3 xl:grid-cols-[minmax(0,1fr)_400px] xl:h-[70vh]">
```

A 400px `SpanInspector` panel at xl breakpoint. Like the authoring violation, this pushes into the space the chat rail already occupies.

**Migration suggestion:** Convert to a tabbed panel below the timeline (tabs: Timeline | SpanDetail) or a vertically expanding accordion that opens when a span is selected. The span selection state is already local state, so the transition is encapsulated. This is non-trivial — recommend a separate PR.

### VIOLATION — Popup: MemorySurface.tsx AddPatternDialog

**File:** `frontend/web/src/features/memory/MemorySurface.tsx:1249` (component definition), `:1314` (overlay class)

```tsx
className="fixed inset-0 z-40 flex items-start justify-center pt-24 px-4 bg-bg/80 backdrop-blur-sm"
```

Custom dialog implemented with a `fixed inset-0 z-40` overlay. Steals focus, paints over the primary surface. Used when a user adds a memory pattern.

**Migration suggestion:** Replace with an inline accordion under the patterns list. The form fields (pattern text, examples, strength picker) fit in an inline expand. A `+ Add pattern` button opens the expand in-place; a `Cancel` link collapses it. This aligns with the existing `AlertDialog` component shapes already in the file (line 12 comment).

### VIOLATION — Popup: MemorySurface.tsx ForgetDialog

**File:** `frontend/web/src/features/memory/MemorySurface.tsx:1612` (component definition), `:1636` (overlay class)

```tsx
className="fixed inset-0 z-40 flex items-start justify-center pt-32 px-4 bg-bg/80 backdrop-blur-sm"
```

Confirmation dialog for forgetting a memory pattern. Same `fixed inset-0` overlay pattern.

**Migration suggestion:** Replace with an inline confirmation row: clicking "Forget" replaces the row in-place with `[Confirm: remove? | Keep]`. If the pattern is deleted, show a toast with an undo action (2-second window). This is the standard no-popup pattern for destructive single-row actions.

### VIOLATION — Popup: eval-runs.tsx overlay

**File:** `frontend/web/src/routes/eval-runs.tsx:844`

```tsx
className="fixed inset-0 z-40 flex items-start justify-center pt-24 px-4 bg-bg/80 backdrop-blur-sm"
```

An additional custom dialog on the eval-runs list route, same pattern as the MemorySurface dialogs. This was not in the initial audit scope but the sentinel detected it live.

**Migration suggestion:** Inline expand or route to a detail page; follow the same inline accordion pattern recommended for MemorySurface.

### VIOLATION — Popup: MemoryPanel.tsx backdrop

**File:** `frontend/web/src/features/eval-runs/review/MemoryPanel.tsx:204`

```tsx
className="fixed inset-0 z-30"
```

A `fixed inset-0` backdrop inside the eval-run review memory panel. Context suggests this is a focus-trap backdrop for a docked overlay, but it is structurally identical to the other popup overlays and should be reviewed. If it is a legitimate docked panel backdrop (not stealing focus), add `# layout-ok` to suppress the sentinel.

## Compliant (spot-checked)

- `frontend/web/src/features/marketplace/components/FilterDrawer.tsx` — docked, not modal; explicitly documented as a non-Dialog exception.
- `frontend/web/src/routes/eval-runs-detail.tsx` — single-column layout (migrated in QA30).
- `frontend/web/src/routes/home.tsx` — no right sidebar.
- `frontend/web/src/routes/chart-lab/ChartLabPrimitives.tsx:45` — `xl:grid-cols-2` is inside the chart-lab route which uses `ChartLabLayout`, not the main `<Layout>` shell. Exempt.
- `frontend/web/src/routes/settings/general.tsx:32` — `xl:grid-cols-4` is a stat card row, not a sidebar.

## Files Changed

- `scripts/layout-rail-sentinel.sh` — reusable scanner.
- `docs/research/overnight-agents/2026-06-01/LayoutRailSentinel.md` — this finding.

## Verification

```bash
bash scripts/layout-rail-sentinel.sh
```

Observed exit code: 1 (6 violations across 2 violation types as of 2026-06-01): 2 layout, 4 popup.

## Residual Risks

- `agent-runs-detail.tsx` SpanInspector migration is non-trivial (keyboard shortcut wiring, selection state management). Should be scoped as its own PR with explicit acceptance tests.
- `MemoryPanel.tsx:204` may be a backdrop for a docked panel rather than a focus-stealing dialog; review before migration.
