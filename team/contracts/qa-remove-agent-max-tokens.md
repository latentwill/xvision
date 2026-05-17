---
track: qa-remove-agent-max-tokens
lane: leaf
wave: qa-operator-2026-05-17
worktree: .worktrees/qa-remove-agent-max-tokens
branch: task/qa-remove-agent-max-tokens
base: origin/main
status: in-progress
depends_on: []
blocks: []
stacking: none
allowed_paths:
  - frontend/web/src/components/agent/AgentForm.tsx
  - frontend/web/src/components/agent/SlotForm.tsx
  - frontend/web/src/components/agent/agents.test.tsx
  - crates/xvision-engine/src/eval/dispatcher.rs
  - crates/xvision-engine/src/eval/trader_output.rs
  - crates/xvision-engine/src/agent/execute.rs
forbidden_paths:
  - crates/xvision-engine/migrations/**
  - crates/xvision-engine/src/api/**
parallel_safe: false
parallel_conflicts:
  - "qa-strategy-popup-to-accordion: also imports/renders AgentForm.tsx. Coordinate so the popup-to-accordion refactor doesn't bring the removed field back as JSX."
  - "qa-openrouter-pricing-pull: also reads model-library metadata. Independent fields (max_tokens vs pricing) — coordinate via team/queue/ on shared serializer changes."
  - "qa-execute-slot-cap (qa-2026-05-17 wave): also edits crates/xvision-engine/src/agent/execute.rs. Coordinate disjoint regions; the execute_slot cap landing first is fine — this track only adjusts the per-slot max_tokens fallback."
verification:
  - cargo test -p xvision-engine
  - pnpm --dir frontend/web typecheck
  - pnpm --dir frontend/web lint
  - pnpm --dir frontend/web test -- --run agents
  - pnpm --dir frontend/web build
acceptance:
  - The AgentForm and SlotForm surfaces no longer expose a `max_tokens`
    input
  - When the persisted agent record has no `max_tokens`, the engine
    falls back to the model-library cap (landed by #185
    `q15-agent-max-tokens-from-model`)
  - Existing strategies / agents with a previously-set `max_tokens`
    still execute without panic — the engine reads the model-library
    cap regardless, ignoring the persisted field
  - No regression: an eval that ran successfully before this change
    runs successfully after
---

# Scope

Remove the per-agent `max_tokens` input from the dashboard agent-edit
surfaces. Operator request: keeping it editable is a footgun (e.g.
4096 set on a model that can do 384k+). The model-library now persists
the correct cap per model via the closed-out
`q15-agent-max-tokens-from-model` track (PR #185).

Change scope:

1. Remove the `max_tokens` field from `AgentForm.tsx` and `SlotForm.tsx`.
2. Make the engine's eval dispatcher / `execute_slot` ignore any
   persisted `max_tokens` override and always read the cap from the
   model library.
3. Leave the wire schema field in place for backwards-compat on disk —
   do NOT add a migration that drops the column. If the field is fully
   removed from the wire schema, update `frontend/web/src/api/types.gen/**`
   via the standard regen flow (do not hand-edit).

# Out of scope

- DB migration to drop the column. (Pre-launch breaking change is
  acceptable, but only if operator approves in `team/queue/`.)
- Pulling pricing metadata — owned by `qa-openrouter-pricing-pull`.
- Removing other agent config fields (`temperature`, `top_p`, etc.).
- Wider audit of how model caps are propagated to the LLM provider
  call sites beyond the dispatcher fallback.

# Sync-before-work ritual

```bash
cd /Users/edkennedy/Code/xvision
git fetch --prune origin
git worktree add .worktrees/qa-remove-agent-max-tokens \
  -b task/qa-remove-agent-max-tokens origin/main
git -C .worktrees/qa-remove-agent-max-tokens status
```

# Notes

Implementation hints:

- `q15-agent-max-tokens-from-model` (#185, archived under
  `team/archive/2026-05-16-q15/`) is the prior art — read its merged
  PR for how the dispatcher already prefers the model-library value.
  This track just removes the UI input and asserts the fallback is
  the actual code path used.
- If you discover the dispatcher is still preferring the persisted
  value over the model-library cap, that's the bug to fix; the UI
  removal is the user-visible half.
- For `agents-page-v1` resync compatibility, run the agents page
  tests under `frontend/web/src/components/agent/agents.test.tsx`
  before opening the PR.
