---
track: qa-strategy-popup-to-accordion
lane: leaf
wave: qa-operator-2026-05-17
worktree: .worktrees/qa-strategy-popup-to-accordion
branch: task/qa-strategy-popup-to-accordion
base: origin/main
status: pr-open
depends_on: []
blocks: []
stacking: none
allowed_paths:
  - frontend/web/src/routes/authoring.tsx
  - frontend/web/src/routes/authoring.test.tsx
  - frontend/web/src/routes/strategies-new.tsx
  - frontend/web/src/routes/strategies-new.test.tsx
  - frontend/web/src/components/strategy/**
  - frontend/web/src/components/agent/AgentForm.tsx
forbidden_paths:
  - crates/**
  - frontend/web/src/api/**
  - crates/xvision-engine/migrations/**
interfaces_used:
  - "@/api/agents — listAgents, agentKeys, Agent"
  - "@/api/strategies — Strategy, AgentRef"
parallel_safe: true
parallel_conflicts:
  - "qa-ui-micro-fixes: also edits strategies-new.tsx (whitespace + Run Eval card removal). Coordinate file regions via team/queue/."
  - "qa-remove-agent-max-tokens: also edits AgentForm.tsx (removes max_tokens field). Coordinate ordering."
verification:
  - pnpm --dir frontend/web typecheck
  - pnpm --dir frontend/web lint
  - pnpm --dir frontend/web test -- --run strategies-new agents
  - pnpm --dir frontend/web build
acceptance:
  - No popup/modal/sheet/dialog is used for the agent-attach flow on the strategy detail surface
  - "Create and Attach Agent" and "Attach Existing Agent" are presented as one inline accordion panel
  - The accordion exposes a dropdown listing existing library agents AND an inline "create new" affordance
  - Selecting an existing agent attaches it as `AgentRef { agent_id, role }` without leaving the page
  - Creating a new agent inline persists it via the existing `agents` mutation and immediately attaches it
  - No regression: existing agent-detach, role-edit, and reorder controls continue to work
  - No `border-white`/`border-gray-100`/`border-gray-200`/`#fff` on dark mode (CLAUDE.md rule)
---

# Scope

The strategy detail page (`strategies-new.tsx` — name preserved from the
agents-page-v1 refactor) currently opens a popup window for the
agent-attach flow. This violates the dashboard no-popups rule adopted
2026-05-17 (`docs/superpowers/specs/2026-05-17-agent-run-observability-ui-design.md`,
codified in `/CLAUDE.md`).

Replace the popup with an inline accordion / flip-down panel. While
there, merge the two attach surfaces ("Create and Attach Agent" +
"Attach Existing Agent") into one accordion containing:

- A dropdown that lists existing library agents (`listAgents()`).
- An "inline create" affordance for authoring a new agent without
  leaving the page.

After this track, the strategy detail surface has zero popup usage and
one consolidated attach UX.

# Out of scope

- Removing per-agent `max_tokens` (owned by `qa-remove-agent-max-tokens`).
- Whitespace / Run Eval / delete-icon nits (owned by `qa-ui-micro-fixes`).
- Schema changes to `AgentRef` or `Strategy`.
- Touching engine code or migrations.
- Auditing other popup usage in the repo. (A separate audit track is
  planned per the no-popups spec; this contract only fixes the strategy
  attach flow.)

# Sync-before-work ritual

```bash
cd /Users/edkennedy/Code/xvision
git fetch --prune origin
git worktree add .worktrees/qa-strategy-popup-to-accordion \
  -b task/qa-strategy-popup-to-accordion origin/main
git -C .worktrees/qa-strategy-popup-to-accordion status
```

If `qa-ui-micro-fixes` or `qa-remove-agent-max-tokens` is in-progress on
overlapping files, sync via `team/queue/` and stack as needed.

# Notes

Implementation hints:

- The popup is probably a `Dialog` / `Sheet` / `Popover` from
  shadcn/ui in `strategies-new.tsx`. Grep for `Dialog` / `Sheet` /
  `Popover` to find it.
- shadcn/ui `Accordion` is already vendored; `frontend/web/src/components/`
  has prior usage on the eval-runs detail surface.
- The dropdown wants `Combobox` semantics (filter-as-you-type) since
  the agents library can grow long. shadcn doesn't ship one — there's
  prior in-repo usage you can mirror (grep `Command` / `cmdk`).
- The inline-create should reuse `AgentForm.tsx` rather than reproduce
  field-by-field.
