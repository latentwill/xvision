# trace-fullscreen-redesign — status

**Contract:** `team/contracts/trace-fullscreen-redesign.md`
**Branch:** `task/trace-fullscreen-redesign`
**Worktree:** `.worktrees/trace-fullscreen-redesign`
**Claimed:** 2026-05-18
**Status:** in-progress (commit pushed, awaiting review/merge)

## Current state

- Worktree created from `origin/main` (HEAD c7941c7).
- Commit `3f6ed64` pushed to `task/trace-fullscreen-redesign`:
  - Rewrote `AgentRunIndentedTimeline.tsx` as a Logfire-style row:
    indent + colored dot + kind chip + full span name + inline
    per-row waterfall bar + right-aligned duration + status pip.
  - Long span names (e.g. `openrouter/deepseek/deepseek-v4-pro`) now
    wrap with `break-all` instead of CSS-truncating, so the full ID
    stays visible.
  - Deleted `AgentRunRailTree.tsx` + `AgentRunRailTree.test.tsx`.
  - Updated `routes/agent-runs-detail.tsx` to a two-column grid
    (`xl:[minmax(0,1fr)_400px]`) — the waterfall column claims the
    main width; SpanInspector stays 400px on the right.
  - Updated tests: `AgentRunIndentedTimeline.test.tsx` gains 2 new
    cases (waterfall bar positioning + kind chip vs. name separation);
    `agent-runs-detail.test.tsx` drops the rail-node assertion and
    adds a waterfall-bar/row-count parity assertion.
- Verification (in worktree's `frontend/web/`): `pnpm typecheck`
  clean; `pnpm test` 284/284 passing; `pnpm build` green.

## Open follow-ups (out of this contract's scope)

- Cross-app `shortId()` audit: `eval-runs-detail.tsx`,
  `eval-runs-detail-mobile.tsx`, `eval-runs.tsx`, `eval-compare.tsx`,
  `chat/cards/ChatRunListCard.tsx`, and `agent/AgentForm.tsx` all
  call `shortId(...)` to display truncated `run`/`strategy`/`scenario`
  IDs. The user has asked for full IDs everywhere. A separate
  follow-up contract should sweep those call sites — this track only
  covers the trace fullscreen surface.

## Notes

- Pop-out test that filters to MODEL still works: FilterBar's MODEL
  chip is unchanged, the inspector still falls back to the first
  filtered span (`s3`).
- No edits to `SpanInspector.tsx` — that file remains claimed by
  `agent-run-observability-blob-fetch-route`.
- Build deleted `crates/xvision-dashboard/static/.gitkeep` as a side
  effect; restored before committing.
