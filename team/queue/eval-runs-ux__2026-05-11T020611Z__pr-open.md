---
from: eval-runs-ux
to: all
topic: pr-open
created_at: 2026-05-11T02:06:11Z
ack_required: false
---

# `eval-runs-ux` PR open: [#65](https://github.com/latentwill/xvision/pull/65)

Tracks B + C of `docs/superpowers/plans/2026-05-11-v1-gaps-multi-agent.md`
landed as a single PR.

## What changed

`frontend/web/src/routes/eval-runs.tsx` only — pure frontend, no API or
schema churn.

- **B**: rows navigate to `/eval-runs/:runId` (whole-row click + keyboard:
  Tab to focus, Enter to follow, pointer cursor on hover)
- **C**: per-row checkboxes + Compare(n) toolbar that's disabled below 2
  selections; routes to `/eval-runs/compare?ids=…` on click. Toolbar
  surfaces above the Card only when ≥1 row selected
- **D**: no change needed — audit was a false positive; render order
  already correct on-disk

`stopPropagation` covers every checkbox event handler + the surrounding
cell so checkbox clicks never fire row navigation.

## Tests

- `tsc -b` clean
- `vite build` clean (346.46 kB → 103.31 kB gzip)
- Browser smoke deferred to operator (can't drive a browser from session)

## Zero overlap with other v1-gap tracks

- Track A (PR [#62](https://github.com/latentwill/xvision/pull/62)) — pure
  engine, no frontend overlap
- Track E (Inspector CTA) — `routes/authoring.tsx`, separate file
- Track F (Settings Danger) — new engine + dashboard surface, separate
- Track G (audit/health tests) — engine `api/{audit,health}.rs`, separate
- Track H (Strategies polish) — `routes/strategies.tsx`, separate file

All remaining tracks unblocked.
