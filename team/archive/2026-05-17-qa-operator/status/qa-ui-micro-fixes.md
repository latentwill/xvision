---
track: qa-ui-micro-fixes
status: pr-open
last_update: 2026-05-17
worker: Claude (xvision conductor session)
pr: 229
commits:
  - 8b10ed8 — qa: four UI nits from operator walk-through
---

# Status

## Plan

Four UI nits in a single PR. Working in `.worktrees/qa-ui-micro-fixes`.

## Fix targets

1. **Whitespace** — `routes/authoring.tsx`: `AgentsCard` content `<div>` (around line 265) and `ManifestCard` `<dl>` (around line 722) have no `pt-` after the SectionHeader's `border-b`. Add `pt-4` to mirror the header's `pt-4`.
2. **Run Eval card redundant** — `routes/authoring.tsx`: remove `<RunEvalCard>` (line 91) and the `RunEvalCard` function (lines 959-1048). The topbar `Run eval →` button in `InspectorActions` (line 1102) covers the launch path.
3. **Check icon as delete** — `components/agent/SlotForm.tsx:82`: `<Icon name="check" />` is wired to the "Remove slot" button. Swap for an X or trash glyph from the project's `Icon` primitive.
4. **Tiny trace-strip arrows** — `features/agent-runs/RunStatusStrip.tsx`: Expand button SVG (lines 328-330) and Pop-out button SVG (lines 352-354) render at `width="11" height="11"`. Bump to 14-16px and enlarge the button container to fit.

## Contract amendments made (conductor-side)

- Added `frontend/web/src/routes/authoring.tsx` + `.test.tsx` to allowed_paths (path drift — the operator's "first agent card / manifest card / Run Eval" all live on `authoring.tsx`, not the contract's original `strategies-new.tsx` which is just the new-strategy name+template picker).
- Added `frontend/web/src/components/agent/SlotForm.tsx` to allowed_paths and removed it from forbidden_paths. The checkmark-as-delete bug is in SlotForm. Multi-owner with `qa-remove-agent-max-tokens` (which removes the `max_tokens` field) declared in OWNERSHIP.md.
- Added `frontend/web/src/features/agent-runs/RunStatusStrip.tsx` to allowed_paths. That file (not `StripDockSlot.tsx`) is where the small SVG arrows live.
- All amendments coordinated via OWNERSHIP.md multi-owner rows. board-lint green.

## Out-of-scope reminders

- No popup → accordion refactor (owned by `qa-strategy-popup-to-accordion`).
- No max_tokens removal (owned by `qa-remove-agent-max-tokens`, PR #223).
- No POST-HOC/LIVE toggle removal (owned by `qa-remove-post-hoc-live-toggle`, PR #221).
- No span-content / model-fidelity / event-payload changes — those are deferred Phase B work.
- Dark-mode borders rule (CLAUDE.md): no `border-white` / `border-gray-100/200` / `#fff` on cards.
