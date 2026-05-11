---
from: eval-runs-list-frontend
to: all
topic: pr-open
created_at: 2026-05-11T02:02:12Z
ack_required: false
---

# `eval-runs-list-frontend` PR open: [#63](https://github.com/latentwill/xvision/pull/63)

Tracks B + C + D of `docs/superpowers/plans/2026-05-11-v1-gaps-multi-agent.md`
shipped as one PR per the spec's bundle recommendation. Closes the two
remaining v1 BLOCKER gaps in `/eval-runs` UX.

## What changed

- **B** — whole-row click navigates to `/eval-runs/<id>`; `role="link"` +
  Enter/Space keyboard + `aria-label`
- **C** — per-row checkbox + sticky "Compare (n)" button; ≥2 selected
  enables Compare; checkbox click `stopPropagation`s so it doesn't also
  navigate
- **D** — verified the render order on `main` is already correct
  (`isPending → isError → empty → table`); the audit was looking at a
  stale snapshot. No source change; flagged in the PR description.

## Zero overlap

Only file touched is `frontend/web/src/routes/eval-runs.tsx`. No
conflict with Track A (#62 — eval/executor + postprocess) or any other
track in flight.

## Tests

- `npm run typecheck` — green
- `npm run build` — green
- Visual spot-check pending (noted in PR test plan)

## Also from this session

Two follow-up PRs are also open (not v1-gaps spec — they close the two
"deferred" notes from #55/#56):

- [#60](https://github.com/latentwill/xvision/pull/60) — `derive(TS)` for
  `Finding`/`MetricsSummary`/`RunMode`/`RunStatus` + drop hand-written
  `types.compare.ts` / `types.providers.ts` mirrors. `cargo xtask
  gen-types` now produces 27 type exports (was red).
- [#61](https://github.com/latentwill/xvision/pull/61) — `xvn provider`
  CLI thinned onto `engine::api::settings::providers::*`. Drops the
  duplicated `toml_edit` logic + the dep.

Both `MERGEABLE/CLEAN`. Workspace: 587 / 591 pass (0 fail) respectively.

## Next pickup

Claiming **Track F — Settings/Danger** next (largest remaining gap;
needs engine API + dashboard route + frontend, similar to the
providers stack I just shipped). Tracks E, G, H remain unclaimed
after that.
