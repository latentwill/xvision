---
track: mobile-eval-run-detail
lane: leaf
wave: mobile-polish
worktree: .worktrees/mobile-eval-run-detail
branch: task/mobile-eval-run-detail
base: origin/main
status: in-progress
depends_on: []
blocks: []
stacking: none
allowed_paths:
  - frontend/web/src/routes/eval-runs-detail.tsx
  - frontend/web/src/routes/eval-runs-detail.test.tsx
  - frontend/web/src/routes/eval-runs-detail-mobile.tsx
  - frontend/web/src/routes/eval-runs-detail-mobile.test.tsx
  - team/board.md
  - team/contracts/mobile-eval-run-detail.md
  - team/OWNERSHIP.md
  - team/CONFLICT_ZONES.md
forbidden_paths:
  - frontend/web/src/components/chart/**
  - frontend/web/src/api/**
  - crates/**
interfaces_used:
  - RunDetail, RunSummary, DecisionRowDto, EquityPoint
  - useViewportMode
  - MobileShell (host)
parallel_safe: true
parallel_conflicts: []
verification:
  - cd frontend/web && pnpm typecheck
  - cd frontend/web && pnpm vitest run src/routes/eval-runs-detail
acceptance:
  - On viewport <768px, /eval-runs/:runId renders the tabbed observability surface matching docs/design/mobile/XVN/Eval Run Detail · Mobile.html — sticky LIVE strip (LIVE / COMPLETED / WARN / ERROR state with halt button when active), SUMMARY/DECISIONS/TRACE/REVIEW tab bar, Summary (hero + activity card when live + 2×2 KPI grid + equity sparkline + META + run actions), Decisions (action pill + conviction bar + justification card list), Trace (deep-link to /agent-runs/:agentRunId until the backend wires summary.agent_run_id), Review (mounts the existing ReviewPanel keyed by run id).
  - On viewport >=768px, the existing desktop layout is unchanged.
  - Existing eval-runs-detail.test.tsx still passes (data hooks unchanged).
  - No new dependencies; no backend or types.gen changes.
---

# Scope

Implement the mobile version of the Eval Run Detail screen as defined by
the Anthropic Design bundle at
`docs/design/mobile/XVN/Eval Run Detail · Mobile.html` + `mobile-screens.jsx`
(the user added the bundle to the repo after the initial pass).

The design ports the desktop observability surface (Strip + Dock + Inspector)
to a phone-appropriate pattern:
  - persistent LIVE status row replaces the floating desktop strip
  - sticky tab bar with TRACE replaces the bottom dock
  - bottom sheets replace the inline inspector

Mobile is gated on `useIsPhone()` and reuses the existing `RunDetail` query
+ cancel/retry mutations + SSE stream from the route, so live updates,
retry, and cancel work identically across viewports.

Trace tab is rendered as a deep-link to `/agent-runs/:agentRunId` until
`summary.agent_run_id` lands on RunSummary (see TODO comments —
implementation depends on `agent-run-observability-ipc-emission`). The
full span tree + span/filter bottom sheets are scoped out of this contract
on purpose.

# Out of scope

- No backend API changes. `RunDetail` shape is taken as given.
- No new chart engine. The mobile equity card uses an inline SVG
  sparkline computed from `RunDetail.equity_curve`; `RunChart`
  (lightweight-charts) is desktop-only this wave.
- No "trade ledger" backend join — the design's ledger column is rendered
  from the existing `decisions` array filtered to rows with a non-null
  `pnl_realized`.
- The design's "Critical / Warning / Insight" filter taxonomy is for
  qualitative findings we don't emit yet; mobile keeps the existing
  buy/sell/hold/close filter taxonomy from the desktop.

# Sync-before-work ritual

```bash
cd /Users/edkennedy/Code/xvision
git fetch --prune origin
git -C .worktrees/mobile-eval-run-detail status
git -C .worktrees/mobile-eval-run-detail log --oneline -3 origin/main..HEAD
```

# Notes

- Worktree created from origin/main at e05770b.
- Conflict-zone claim on `frontend/web/src/routes/eval-runs-detail.tsx`
  registered in `team/CONFLICT_ZONES.md` (was `(none)`).
