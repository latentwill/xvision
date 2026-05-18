---
track: qa-ui-polish-round2
lane: leaf
wave: qa-operator-2026-05-18
worktree: .worktrees/qa-ui-polish-round2
branch: task/qa-ui-polish-round2
base: origin/main
status: ready
depends_on: []
blocks: []
stacking: none
allowed_paths:
  - frontend/web/src/routes/index.tsx
  - frontend/web/src/routes/index.test.tsx
  - frontend/web/src/routes/agents.tsx
  - frontend/web/src/routes/agents.test.tsx
  - frontend/web/src/features/home/LatestRunChart.tsx
  - frontend/web/src/features/home/LatestRunChart.test.tsx
  - frontend/web/src/features/agents/**
  - frontend/web/src/features/agent-runs/SpanInspector.tsx
  - frontend/web/src/features/agent-runs/SpanInspector.test.tsx
  - frontend/web/src/features/charts/**
  - frontend/web/src/features/settings/RetentionCard.tsx
  - frontend/web/src/features/settings/RetentionCard.test.tsx
forbidden_paths:
  - crates/**
  - frontend/web/src/features/agent-runs/TraceDock.tsx
  - frontend/web/src/features/agent-runs/FlameGraph.tsx
  - frontend/web/src/features/agent-runs/RunStatusStrip.tsx
  - frontend/web/src/routes/eval-runs-detail.tsx
  - frontend/web/src/routes/eval-runs-detail-mobile.tsx
interfaces_used:
  - Existing chart, agents, span-inspector, settings primitives
  - lucide-react icons
parallel_safe: false
parallel_conflicts:
  - "v2a-driver-tour: single-writer claim on frontend/web/src/routes/index.tsx. Coordinate or stack."
  - "model-call-streaming-text-passthrough: edits SpanInspector.tsx. The duplicate-streaming-icon fix here may overlap. Stack and rebase."
  - "agent-run-observability-blob-fetch-route: single-writer claim on SpanInspector.tsx. Coordinate region; the icon dedupe is small and isolated."
  - "ux-polish-eval-list-and-snapshot: previously touched Home chart snapshot. Confirm the latest-run-chart-name nit is NOT already covered there before editing LatestRunChart.tsx. If covered, drop #3 from this bundle and document in Notes."
verification:
  - pnpm --dir frontend/web typecheck
  - pnpm --dir frontend/web lint
  - pnpm --dir frontend/web test -- --run LatestRunChart agents SpanInspector RetentionCard charts
  - pnpm --dir frontend/web build
acceptance:
  - **Latest run chart eval name (#3).** The Home page's latest-run
    chart shows the eval name as the chart title (e.g. "Run #4 —
    Strategy X on Scenario Y"). Confirm this is not already covered
    by a prior track before editing; if it is, drop from the bundle.
  - **Agents Show-archived delete (#4).** The Agents page's "Show
    archived" view exposes a Delete affordance per archived row
    (trash icon, confirmation via existing inline confirm pattern,
    no popup per the no-popups rule). Hard-delete calls the existing
    DELETE route.
  - **Duplicate streaming icon (#9).** The trace span inspector renders
    the streaming-active icon exactly once per active span. Today it
    renders twice (likely once in the header and once in the body
    pulse — pick one canonical location).
  - **Retention warning removal (#10).** The loud retention warning
    surface in the trace dock / span inspector / wherever it is
    rendering today is removed. The Settings → Retention card stays
    as-is (minimal, file-backed picker per `feedback_no_privacy_overkill`
    memory). The warning toast / banner / inline note in the run
    surface goes away. Operators read the retention mode from the
    Settings card; they don't need a warning splashed on every run.
  - **TradingView chart titles (#13).** Chart panes that consume
    TradingView (or the in-repo TradingView wrapper) show their
    title overlay again. Today the title is missing — confirm the
    cause (config prop dropped, CSS hiding it, or upstream API
    change) and restore.
  - Each item lands as a separate commit in the same PR so the
    operator can validate them one at a time.
  - No `border-white` / `border-gray-100` / `border-gray-200` / `#fff`
    on dark mode (CLAUDE.md rule).
---

# Scope

Five small visual / polish fixes bundled into a single PR. None are
worth their own track. All are frontend-only and isolated from the
larger trace / observability redesign tracks.

1. Latest run chart on Home needs the evaluation name.
2. Agents page "Show archived" view needs a delete affordance.
3. Trace span inspector duplicates the streaming icon — dedupe.
4. Remove the loud retention warning surface (keep Settings card
   untouched).
5. TradingView chart titles missing — restore.

The bundle is intentionally tight: if any item turns out to be larger
than expected (e.g. the TradingView title regression is upstream and
needs a patch), the worker should split it into a follow-up contract
rather than expand this one.

# Out of scope

- Trace dock layout, flame graph, run-status strip (owned by other
  tracks).
- Eval inspector header / metadata strip (`eval-inspector-header-polish`).
- Settings → Retention card content (`feedback_no_privacy_overkill`
  memory — keep minimal).
- Strategy detail surface (`qa-strategy-popup-to-accordion` has
  merged; further polish is its own follow-up).
- Backend / API changes.

# Sync-before-work ritual

```bash
cd /Users/edkennedy/Code/xvision
git fetch --prune origin
git -C .worktrees/qa-ui-polish-round2 status
git -C .worktrees/qa-ui-polish-round2 log --oneline -3 origin/main..HEAD
```

If the worktree does not exist:

```bash
git fetch --prune origin
git worktree add .worktrees/qa-ui-polish-round2 \
  -b task/qa-ui-polish-round2 origin/main
```

Multiple in-flight contracts touch overlapping files:
- `v2a-driver-tour` claims `routes/index.tsx`.
- `agent-run-observability-blob-fetch-route` claims `SpanInspector.tsx`.
- `model-call-streaming-text-passthrough` writes `SpanInspector.tsx`.

Coordinate via `team/queue/` or stack via `stacking:` before claiming.
Worker may also choose to land items in pieces if the file claims
make a single PR unworkable.

# Notes

Append checkpoints / PR links below. If item #3 (latest-run chart
name) is already covered, document the determination here and drop
from scope.
