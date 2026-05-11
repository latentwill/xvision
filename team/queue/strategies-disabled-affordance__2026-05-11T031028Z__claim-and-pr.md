---
from: strategies-disabled-affordance
to: all
topic: claim-and-pr
created_at: 2026-05-11T03:10:28Z
ack_required: false
---

# `strategies-disabled-affordance` track claimed + shipped (v1 gaps Track H)

Track H (🟢 NIT) of `docs/superpowers/plans/2026-05-11-v1-gaps-multi-agent.md`.
Closes the visual gap where the FilterBar's disabled inputs and
buttons looked active because they had no disabled styling, only a
plain `disabled` attribute.

Branch `feature/strategies-disabled-affordance` based on `origin/main`
@ `0fff672`.

## What changed

`frontend/web/src/routes/strategies.tsx#FilterBar`:

- Three disabled inputs (search input + 2 selects) — added
  `disabled:opacity-50 disabled:cursor-not-allowed`. The search wrapper
  div carries a static `opacity-50` so the icon mutes alongside the
  input (Tailwind's `disabled:` variant doesn't reach a child element).
- Two disabled buttons ("New from template", "New strategy") — added
  the same disabled utilities plus `disabled:hover:*` overrides so
  the hover state doesn't fight the disabled state.
- Added `title` tooltips on the two filters that previously had none,
  pointing at Plan 5 (Findings + Polish) which is where the audit
  said this surface ships.

## v1 QA value

Closes the last remaining v1-gaps spec track. After this and Tracks A
(#62), B/C/D (#63), F (#67), G (#66), and E (#68) land, the v1-gaps
audit is fully addressed.

## Non-conflicts

Only file: `frontend/web/src/routes/strategies.tsx`. Zero overlap
with anything else in flight.
