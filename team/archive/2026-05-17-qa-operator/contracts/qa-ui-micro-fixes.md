---
track: qa-ui-micro-fixes
lane: leaf
wave: qa-operator-2026-05-17
worktree: .worktrees/qa-ui-micro-fixes
branch: task/qa-ui-micro-fixes
base: origin/main
status: pr-open
depends_on: []
blocks: []
stacking: none
allowed_paths:
  - frontend/web/src/routes/authoring.tsx
  - frontend/web/src/routes/authoring.test.tsx
  - frontend/web/src/routes/authoring-risk.test.tsx
  - frontend/web/src/routes/strategies-new.tsx
  - frontend/web/src/routes/strategies-new.test.tsx
  - frontend/web/src/routes/agents.tsx
  - frontend/web/src/routes/agents-edit.tsx
  - frontend/web/src/components/agent/SlotForm.tsx
  - frontend/web/src/features/agent-runs/TraceDock.tsx
  - frontend/web/src/features/agent-runs/StripDockSlot.tsx
  - frontend/web/src/features/agent-runs/RunStatusStrip.tsx
forbidden_paths:
  - crates/**
  - frontend/web/src/api/**
  - frontend/web/src/components/agent/AgentForm.tsx
parallel_safe: true
parallel_conflicts:
  - "qa-strategy-popup-to-accordion: also edits strategies-new.tsx (popup → accordion). Coordinate file regions via team/queue/."
verification:
  - pnpm --dir frontend/web typecheck
  - pnpm --dir frontend/web lint
  - pnpm --dir frontend/web test -- --run strategies-new agents
  - pnpm --dir frontend/web build
acceptance:
  - Strategy detail's first agent card and the manifest card both have
    consistent top whitespace matching sibling cards on the surface
  - The "Run Eval" container card is removed; the inline run button
    remains and is rendered without surrounding chrome
  - On the Agents page's Add Agent flow, the delete control uses an
    `x` icon or trash glyph — not a checkmark
  - Trace-strip arrow icons (next-span / prev-span) are readable at
    normal viewport zoom (~14–16px or `text-base`-sized in Tailwind
    units); no other trace-strip behavior changes
  - No regression to keyboard navigation or focus rings on either page
  - No `border-white`/`border-gray-100`/`border-gray-200`/`#fff` on dark
    mode (CLAUDE.md rule)
---

# Scope

Four small UI nits in one PR. None require state shape changes; all are
CSS / JSX cleanup. The trace-strip arrow icon resize was absorbed here
from `qa-eval-trace-fidelity` (now blocked on Phase B observability) so
the visual nit ships immediately rather than waiting on Phase B.

1. **Whitespace.** The first agent box and the manifest box on the
   strategy detail page (`strategies-new.tsx`) have insufficient top
   padding/margin compared to sibling cards. Bring to parity.
2. **Run Eval card removal.** The eval-launch surface on the strategy
   detail wraps a single button in a `Card`-style container. The card
   adds visual noise without information. Remove the wrapper; keep the
   button inline with the surrounding rail.
3. **Delete icon.** On the Agents page Add Agent flow, the
   delete-row control uses a `Check` icon (likely a copy-paste of a
   confirm icon). Swap for `X` or `Trash2` from `lucide-react` (already
   used elsewhere on the surface).
4. **Trace-strip arrow icons.** The next-span / prev-span arrows on
   the trace strip are illegibly small. Bump to a readable size
   (~14–16px). Pure visual change — no behavior, no event payload, no
   Phase-B dependency.

# Out of scope

- Popup → accordion refactor (owned by `qa-strategy-popup-to-accordion`).
- Removing per-agent `max_tokens` setting (owned by
  `qa-remove-agent-max-tokens`).
- Removing the `POST-HOC⇄LIVE` toggle (owned by
  `qa-remove-post-hoc-live-toggle`).
- Restyling sibling cards or refactoring the strategy detail layout
  beyond the named fixes.
- Any other change to `TraceDock.tsx` / `StripDockSlot.tsx` beyond the
  icon-size bump. Span content / model rendering / event payloads
  remain blocked on Phase B observability (`qa-eval-trace-fidelity`).

# Sync-before-work ritual

```bash
cd /Users/edkennedy/Code/xvision
git fetch --prune origin
git worktree add .worktrees/qa-ui-micro-fixes \
  -b task/qa-ui-micro-fixes origin/main
git -C .worktrees/qa-ui-micro-fixes status
```

Coordinate with `qa-strategy-popup-to-accordion` if both are active on
`strategies-new.tsx`. Touch disjoint regions; rebase the smaller diff
onto the larger.

# Notes

Implementation hints:

- The Tailwind spacing scale in this repo is `space-y-4` / `space-y-6`
  for card stacks. Mirror sibling cards rather than picking a new value.
- The Run Eval button likely lives near the bottom of the strategy
  detail; check whether it's emitting an `onClick={mutation.mutate}` —
  the inline form is fine, the surrounding `Card` / `CardContent` is
  what to drop.
- Lucide icon import: `import { Trash2, X } from "lucide-react"`. Pick
  `Trash2` if the row truly deletes the persisted agent; pick `X` if it
  just removes an unsaved attach from the in-progress strategy form.
