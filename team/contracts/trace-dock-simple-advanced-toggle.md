---
track: trace-dock-simple-advanced-toggle
lane: leaf
wave: harness-observability-audit
worktree: .worktrees/trace-dock-simple-advanced-toggle
branch: task/trace-dock-simple-advanced-toggle
base: origin/main
status: pr-open
depends_on:
  - harness-span-attrs-populate          # F-2 — provides the typed SpanAttributes bag the Simple-mode one-liner reads from
  - harness-span-taxonomy-extension      # F-4 — adds the validate / state.transition spans Simple mode hides
blocks: []
stacking: none
allowed_paths:
  - frontend/web/src/stores/trace-dock.ts
  - frontend/web/src/features/agent-runs/TraceDock.tsx
  - frontend/web/src/features/agent-runs/AgentRunIndentedTimeline.tsx
  - frontend/web/src/features/agent-runs/SpanInspector.tsx
  - frontend/web/src/features/agent-runs/FlameGraph.tsx
  - frontend/web/src/features/agent-runs/span-colors.ts
  - frontend/web/src/features/agent-runs/use-span-filter.ts
  - team/contracts/trace-dock-simple-advanced-toggle.md
  - team/status/trace-dock-simple-advanced-toggle.md
  - team/board.md
forbidden_paths:
  - crates/**                              # pure frontend; no backend change
  - frontend/web/src/api/**                # types-agent-runs already carries the F-4 SpanKind variants
  - migrations/**
interfaces_used:
  - useTraceDock (zustand store, frontend/web/src/stores/trace-dock.ts)
  - SpanKind (frontend/web/src/api/types-agent-runs.ts) — including the F-4 additions
  - localStorage key `xvision.trace-dock.advanced-view`
parallel_safe: true
parallel_conflicts: []
verification:
  - cd frontend/web && pnpm build
  - cd frontend/web && pnpm typecheck
  - cd frontend/web && pnpm test --run   # if vitest exists for any touched file
acceptance:
  - The trace-dock zustand store gains one boolean field `advanced_view: boolean` plus an action `setAdvancedView(v: boolean)`. Persisted under localStorage key `xvision.trace-dock.advanced-view` using the same try/catch-and-fall-back pattern as `DOCK_HEIGHT_STORAGE_KEY` (lines 113–133 of the current store). Default is `false` (Simple mode is the default, per intake).
  - The trace-dock header gains a two-button segmented control labelled `Simple` / `Advanced`, inserted into the existing header (`TraceDock.tsx` lines 178–236) before the `ml-auto` button row. Styling matches the existing `FilterBar` kind buttons (FilterBar.tsx:66–88) — `h-6 px-1.5 text-[10px] font-mono tracking-[0.14em]`, active state uses `var(--surface-card)` background and the active border color. No new popup / dialog / sheet (frontend popup rule).
  - **Simple mode hides** spans of kind `tool.validate_input`, `tool.validate_output`, `state.transition` from the timeline and the flamegraph. The other intake-listed kinds (`context.assemble`, `prompt.render`) don't exist yet — they're noted in the audit as nice-to-haves, hidden defensively if/when they're added. **Recovery spans (`recovery.attempt`) are visible in both modes** per the intake — they always matter.
  - Hiding happens at the render boundary in `AgentRunIndentedTimeline.tsx` and `FlameGraph.tsx` (lines that iterate `spans`/`filtered`). No refetch; the hidden spans are still in the in-memory list, just not rendered. Selected-span behaviour: if the currently selected span is hidden by Simple mode, the SpanInspector renders a one-line summary (see below) and a clearly-worded "switch to Advanced to see full details" affordance — does NOT auto-clear `selectedSpanId` (so the operator can flip Advanced back on and stay on the same span).
  - In Simple mode, `SpanInspector` collapses its `FIELDS` block (currently a Row-per-key grid at SpanInspector.tsx:357–388) to a single line of the form `<span-id-prefix> · <kind> · <model?> · <tool?> · retry=<n?>`. The summary draws from `span.attributes_json` parsed as the F-2 `SpanAttributes` shape (`run_id`, `agent_id`, `stage`, `model`, `provider`, `tool_name`, `retry_count`, `prompt_version`). Missing fields are omitted from the summary, not rendered as `null`. Advanced mode is unchanged — keeps the full FIELDS grid.
  - The four new F-4 SpanKind variants (`tool.validate_input`, `tool.validate_output`, `recovery.attempt`, `state.transition`) get explicit `categoryOf` arms in `span-colors.ts` so they render with a stable color rather than falling through to the supervisor-default. Proposed mapping: validate kinds → `"tool"` category (they're tool-adjacent), `state.transition` and `recovery.attempt` → `"supervisor"` category (they're observability infrastructure).
  - The toggle is persisted across page reloads via localStorage. Two browser tabs do NOT need to stay in sync (no storage-event listener) — that's a separate, lower-priority polish.
  - No CSS / Tailwind utility classes from outside the existing design system (no new color tokens). Reuse `--surface-card`, `--border`, `--text-2`, `--text-3` and the existing FilterBar pattern.
  - `pnpm build` and `pnpm typecheck` both succeed cleanly. No new ESLint warnings beyond what's already on `origin/main`.
  - No backend changes. No migration. No new API endpoint. No new tracker / analytics call.
---

# Scope

Implement F-7 from the 2026-05-18 harness observability audit
(`team/intake/2026-05-18-harness-observability-audit.md`).

Now that F-2 (typed `SpanAttributes` bag) and F-4 (four new span
kinds — `tool.validate_input/output`, `recovery.attempt`,
`state.transition`) have both landed on `origin/main`, the trace dock
will show materially more spans per run plus a populated attribute
bag per span. An operator triaging a failure does not want to wade
through `state.transition` markers, validate brackets, or 8-field
attribute grids — they want the agent / model / tool / retry-count
view.

This contract adds a `Simple | Advanced` segmented control to the
trace-dock header. Simple is default: hides instrumentation kinds at
the render boundary and collapses the `SpanInspector` attribute bag
to a one-line summary derived from F-2's typed fields. Advanced is
the existing behaviour — every span, full attribute grid.

Persistence is via `localStorage` under
`xvision.trace-dock.advanced-view`, matching the dock-height slider
pattern already in `trace-dock.ts` (lines 99 / 113–133).

The intake explicitly notes recovery spans
(`recovery.attempt` — owned by F-5) stay visible in both modes
because they always matter. F-7 implements that exception.

This is pure frontend. No backend, no migration, no API change.

Reference: 2026-05-18 harness audit intake, finding F-7.

# Out of scope

- Cross-tab sync. If a user opens two tabs and flips the toggle in
  one, the other doesn't update until reload. Storage-event listening
  is a polish for a separate track if anyone asks.
- The currently-hypothetical `context.assemble` / `prompt.render`
  spans the intake mentions. Those don't exist as `SpanKind` variants
  yet — F-7's hide list defensively includes their string identifiers
  but no real span uses them.
- Backend payload-size optimisation. Simple mode hides spans at
  render time; the wire still carries them. That's intentional — the
  toggle is a UX surface, not a transport optimisation.
- A keyboard shortcut for the toggle. Welcome polish but out of scope.
- Migrating existing `Dialog` / `Sheet` / `Popover` usages elsewhere
  in the app to the no-popup convention. Separate track per
  `docs/superpowers/specs/2026-05-17-agent-run-observability-ui-design.md`.

# Sync-before-work ritual

```bash
cd /Users/edkennedy/Code/xvision
git fetch --prune origin
git -C .worktrees/trace-dock-simple-advanced-toggle status
git -C .worktrees/trace-dock-simple-advanced-toggle log --oneline -3 origin/main..HEAD
# Confirm:
#   - clean working tree
#   - branch is task/trace-dock-simple-advanced-toggle
#   - base is up to date with origin/main (which now has F-2 #294 and F-4 #297)
```

If the worktree does not exist:

```bash
git fetch --prune origin
git worktree add .worktrees/trace-dock-simple-advanced-toggle \
  -b task/trace-dock-simple-advanced-toggle origin/main
```

# Notes

Append checkpoints / PR links below.
