---
track: bar-history-limit-surface
lane: leaf
wave: eval-honesty-tail-2026-05-22
worktree: .worktrees/bar-history-limit-surface
branch: task/bar-history-limit-surface
base: origin/main
status: ready
depends_on: []
blocks: []
stacking: none
allowed_paths:
  - frontend/web/src/components/agent/SlotForm.tsx
  - frontend/web/src/components/agent/SlotForm.test.tsx
  - frontend/web/src/components/agent/AgentForm.tsx
  - frontend/web/src/components/agent/AgentForm.test.tsx
  - frontend/web/src/api/types.gen/**
  - docs/v2d-memory-overview.md
forbidden_paths:
  - crates/xvision-engine/**
  - crates/xvision-cli/**
  - crates/xvision-dashboard/**
interfaces_used:
  - xvision_engine::agents::model::AgentSlot::bar_history_limit (shipped via PR #372 — runner cap exists)
  - Existing SlotForm field layout
parallel_safe: true
parallel_conflicts:
  - trader-noop-skip
verification:
  - pnpm -C frontend/web test -- SlotForm AgentForm
  - pnpm -C frontend/web typecheck
acceptance:
  - SlotForm exposes `bar_history_limit` as an editable number field with min/max guidance and an explanatory tooltip ("how many recent bars the agent sees per decision; lower = cheaper + faster, higher = more context")
  - Default value displayed matches the runner default
  - Validation rejects negative numbers / non-integers
  - Saving the form persists the value through to the existing API path (already wired by PR #372)
  - AgentForm Memory selector layout unchanged
---

# Scope

Surface `AgentSlot.bar_history_limit` in the agent editor. The
runner cap shipped in PR #372 (`eval-prompt-cache-and-rolling-window`)
but operators can't set the value from the UI today.

Frontend-only contract — the field already exists, the API already
accepts it, the runner already honors it; this just adds the input.

Source intake: `team/intake/2026-05-21-eval-honesty-and-agent-graph.md`
row "Surface and respect `AgentSlot.bar_history_limit` in the agent
editor; default-respect on the runner."

# Out of scope

- Runner-side behavior changes (already shipped)
- Per-strategy override of slot's limit (defer)
- Provider-specific cap UX (defer)

# Sync-before-work ritual

```bash
cd /Users/edkennedy/Code/xvision
git fetch --prune origin
git worktree add .worktrees/bar-history-limit-surface -b task/bar-history-limit-surface origin/main
```

# Notes

`max_tokens` override was intentionally removed from SlotForm
2026-05-17 (see comment at SlotForm.tsx:5–10). Do not bring it back
under cover of this contract.
