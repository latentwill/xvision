---
from: inspector-run-cta
to: all
topic: claim
created_at: 2026-05-11T03:14:12Z
ack_required: false
---

# `inspector-run-cta` track claimed (v1 gaps Track E)

Implements Track E of `docs/superpowers/plans/2026-05-11-v1-gaps-multi-agent.md`:
the Inspector's right rail gets a "Run eval" action card so the user has
a clear next step after editing a draft.

Branch `feature/inspector-run-cta` based on `origin/main` @ `b74b657`
(post-merge of Tracks G and typed-exit-codes).

## Scope

`frontend/web/src/routes/authoring.tsx` — adds a new `RunEvalCard` to
the right rail (between ValidationCard and BackLinkCard) showing:

- The CLI command to launch a run for the current strategy
  (`xvn eval run --strategy <id> --scenario crypto-bull-q1-2025 --mode backtest`)
- A "copy" button (uses `navigator.clipboard`; gracefully no-ops in
  non-secure contexts)
- A short note about scenario substitution and `--mode paper`
- A "Browse eval runs →" link to `/eval-runs`

## Why CLI command rather than a "new run" form

The spec acknowledges no new-run form exists in v1. Two options were:
ship a form (much bigger scope) or surface the CLI command (small,
honest, matches operator workflow today). Picked the latter.

Per-strategy filtering of the runs list is also out of scope here —
it would touch `routes/eval-runs.tsx` which is also touched by PR #65
(Tracks B+C). That follow-up can land after both merge.

## Non-conflicts

- Track A (PR #62) — engine, no overlap
- Tracks B+C (PR #65) — eval-runs.tsx, this PR doesn't touch it
- Track F (Settings Danger) — separate files
- Track G (audit + health tests, already merged #66) — engine, no overlap
- Track H (Strategies polish) — separate file

## Tests

- `tsc -b` clean
- `vite build` clean
- Browser smoke deferred to operator (session can't drive a browser)
