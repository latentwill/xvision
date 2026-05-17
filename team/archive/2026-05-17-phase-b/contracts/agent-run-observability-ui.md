---
track: agent-run-observability-ui
lane: leaf
wave: agent-run-observability
worktree: .worktrees/agent-run-observability-ui
branch: task/agent-run-observability-ui
base: origin/main
status: ready
depends_on: []
blocks: []
stacking: none
allowed_paths:
  - frontend/web/src/api/agent-runs.ts
  - frontend/web/src/api/types-agent-runs.ts
  - frontend/web/src/features/agent-runs/**
  - frontend/web/src/routes/agent-runs-detail.tsx
  - frontend/web/src/routes/agent-runs-detail.test.tsx
  - frontend/web/src/routes.tsx
  - frontend/web/src/stores/trace-dock.ts
  - frontend/web/src/stores/trace-dock.test.ts
  - frontend/web/src/components/responsive/DesktopThreePaneShell.tsx
  - frontend/web/src/components/responsive/TabletSplitShell.tsx
  - frontend/web/src/components/mobile/MobileShell.tsx
  - frontend/web/src/routes/eval-runs-detail.tsx
  - frontend/web/src/routes/live.tsx
  - frontend/web/CLAUDE.md
forbidden_paths:
  - crates/**
  - xvision-agentd/**
  - frontend/web/src/themes/**
  - frontend/web/src/routes/eval-runs.tsx
  - frontend/web/src/routes/eval-runs-detail-mobile.tsx
  - frontend/web/src/routes/home.tsx
  - frontend/web/package.json
  - frontend/web/pnpm-lock.yaml
interfaces_used:
  - GET /api/agent-runs/:id (mocked behind env flag until export-cli lands)
  - SSE channel for event.assistant_text_delta (mocked)
parallel_safe: true
parallel_conflicts:
  - mobile-eval-run-detail (frontend/web/src/routes/eval-runs-detail.tsx — coordinate; mobile-eval-run-detail is in-progress and will rebase first; this track only adds a "View agent trace" link + strip mount)
  - qa-eval-trace-fidelity / qa-trace-json-download / qa-trace-error-surfacing / qa-eval-running-status-streaming / qa-remove-post-hoc-live-toggle (frontend/web/src/features/agent-runs/** — multi-owner; this track creates the surfaces those QA contracts then modify, ordering is "ui lands first, qa tracks stack")
verification:
  - (cd frontend/web && pnpm install --frozen-lockfile)
  - (cd frontend/web && pnpm test)
  - (cd frontend/web && pnpm typecheck)
  - (cd frontend/web && pnpm build)
acceptance:
  - New route `/agent-runs/:runId` renders per `docs/superpowers/plans/2026-05-17-agent-run-observability-ui-implementation-plan.md` Phase 3 (dedicated route).
  - Three independently working surfaces: RunStatusStrip (Layer 1, floating bottom-center pill), TraceDock (Layer 2, mounted at AppShell), AgentRunIndentedTimeline + AgentRunRailTree (Layer 3, dedicated route).
  - Mock-backed API shim (`frontend/web/src/api/agent-runs.ts`) honors `VITE_USE_MOCK_AGENT_RUNS=1` and returns canned `AgentRun` data so the UI lands before the backend track is wired.
  - Type module `frontend/web/src/api/types-agent-runs.ts` mirrors the spec data model; matches the keys the export-cli track produces in `xvn_run.json` (schema `xvn.agent_run.v1`).
  - Retention-mode badge in the header; `full_debug` banner when the run was recorded under that mode.
  - Span-tree timeline supports kind colors (5-kind palette per design prototype), expand/collapse, click-to-open SpanInspector drawer with hash + retained-payload preview.
  - Tests pass: every new `*.test.tsx` / `*.test.ts` listed in the UI plan §File structure; `pnpm test` exits 0; `pnpm typecheck` exits 0; `pnpm build` exits 0.
  - F12 summon, minimize-to-strip, and stubbed rerun/halt buttons work per the plan (stubs emit toast "checkpoint design pending").
  - Pixel-perfect match against `docs/superpowers/designs/2026-05-17-agent-run-observability/Eval Run Detail.html` for typography, spacing, color, hover states. Where this contract and the design differ, the design wins.
---

# Scope

Three-layer agent-run observability UI: floating status-line strip, bottom
dock with span tree + inspector, and a dedicated `/agent-runs/:runId`
route. Detailed task-by-task plan lives at
`docs/superpowers/plans/2026-05-17-agent-run-observability-ui-implementation-plan.md`
and is the authoritative checklist for this contract — follow it phase by
phase.

Mock-backed API shim lets the UI land before the export-cli backend
arrives. Same shim swaps to the real endpoints once
`agent-run-observability-export-cli` lands; types stay stable because
both tracks mirror the spec data model.

# Out of scope

- Backend: no `agent_runs` / `spans` reads. The shim is the entire backend
  surface this track touches.
- Actual checkpoint/rerun execution — button surface is wired; the action
  is a toast saying "checkpoint design pending."
- Project-wide popup audit (spec FU-3) — separate track.
- Mobile-eval-run-detail's redesign of `eval-runs-detail.tsx` body — that
  track owns the body; this track only adds a "View agent trace" link +
  strip mount and must coordinate region edits via team/queue/.

# Sync-before-work ritual

```bash
cd /Users/edkennedy/Code/xvision
git fetch --prune origin
git -C .worktrees/agent-run-observability-ui status
git -C .worktrees/agent-run-observability-ui log --oneline -3 origin/main..HEAD
```

If the worktree does not exist:

```bash
git fetch --prune origin
git worktree add .worktrees/agent-run-observability-ui \
  -b task/agent-run-observability-ui origin/main
```

# Notes

Append checkpoints / PR links below.
