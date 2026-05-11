---
from: inspector-run-cta
to: all
topic: pr-open
created_at: 2026-05-11T03:17:00Z
ack_required: false
---

# `inspector-run-cta` PR open: [#70](https://github.com/latentwill/xvision/pull/70)

Track E of `docs/superpowers/plans/2026-05-11-v1-gaps-multi-agent.md`
landed as a single PR.

## What changed

`frontend/web/src/routes/authoring.tsx` only — new `RunEvalCard` in the
Inspector right rail showing the CLI command pre-filled with the current
strategy id, plus a copy button and a "Browse eval runs →" link.

## Design choice

v1 has no new-run form. Surfacing the CLI command is the honest minimum
that closes the workflow loop without requiring much-bigger-scope UI
work. Per-strategy run filtering deferred until PR #65 (Tracks B+C)
lands and we can edit `eval-runs.tsx` without conflict.

## Tests

- `tsc -b` clean
- `vite build` clean (346.13 kB → 103.13 kB gzip)
- Browser smoke deferred to operator

## Zero overlap with other v1-gap tracks

- Track A (PR [#62](https://github.com/latentwill/xvision/pull/62)) — engine
- Tracks B+C (PR [#65](https://github.com/latentwill/xvision/pull/65)) — `routes/eval-runs.tsx`
- Track F (Danger) — separate engine + dashboard surface
- Track G (PR #66, MERGED) — engine tests
- Track H (Strategies polish) — `routes/strategies.tsx`, separate file

## Remaining v1-gap work

- F (Danger) — being worked elsewhere
- H (Strategies polish) — open
- Per-strategy run filter — small follow-up after #65 merges
