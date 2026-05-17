---
track: strategy-agent-card-collapse
worktree: .worktrees/strategy-agent-card-collapse
branch: task/strategy-agent-card-collapse
phase: pr-open
last_updated: 2026-05-16T23:35:00Z
owner: claude-opus
---

# What I'm doing right now

PR open: https://github.com/latentwill/xvision/pull/194

Each attached AgentRef on the strategy authoring page (`AgentsCard` in
`authoring.tsx`) now renders as a collapsible row:

- Collapsed bar shows `role · agent name · provider / model` so the model
  remains visible without expanding.
- Chevron toggle (`▶` / `▼`) flips between collapsed and expanded.
- Collapse state is persisted per `(strategy_id, role)` via
  `safeStorageGet/Set` from `@/lib/storage` (mobile-Safari safe).
- "Open in window" button pops a modal dialog (`role="dialog"
  aria-modal="true"`) showing agent_id, provider/model, system prompt,
  plus the same Rename role / Edit agent / Remove actions.
- Row rendering extracted into an `AttachedAgentRow` subcomponent.

# Blocked on

Nothing. Waiting on review.

# Next up

- Conductor merge.
- Conductor archives this contract per CONDUCTOR.md daily checklist.

# Notes

- `pnpm --dir frontend/web typecheck` clean.
- `pnpm --dir frontend/web test -- authoring` — 12/12 passing
  (3 new in `authoring.test.tsx` + 9 existing in
  `authoring-risk.test.tsx` — collapsed/expanded bar continues to
  surface `agent.name` + `provider / model` so the existing
  "shows attached agent name and provider/model when available"
  assertion still passes).
- Used a state-driven overlay rather than the native `<dialog>` element
  to avoid jsdom-`showModal` polyfill churn in tests; matches the
  modal patterns used elsewhere in the route (eval-runs.tsx).
