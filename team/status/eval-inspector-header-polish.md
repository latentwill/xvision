# eval-inspector-header-polish — status

**Contract:** `team/contracts/eval-inspector-header-polish.md`
**Branch:** `task/eval-inspector-header-polish`
**Worktree:** `.worktrees/eval-inspector-header-polish`
**Claimed:** 2026-05-18
**Status:** in-progress (pushed, PR open)

## Disambiguator design

`Run #N · <Month Day, HH:MM>` (or `Run #N/M · …` when more than one run exists
for the same strategy+scenario pair).

- **N** is the 1-indexed position when grouping runs by
  `(agent_id, scenario_id)` and sorting ascending by `started_at` (with `id`
  as a stable tiebreaker).
- **M** is the total run count for that pair, shown only when > 1 so a
  brand-new run reads as a clean `Run #1 · …` rather than `Run #1/1 · …`.
- **Computed entirely from existing `RunSummary` fields** (`agent_id`,
  `scenario_id`, `started_at`, `id`) — no backend contract change.
- The list view passes the rendered `items` array as siblings; the detail
  view fetches via the existing `listRuns({ agent_id })` query and lets the
  helper narrow to same-scenario rows client-side.
- The helper itself (`evalRunDisambiguator`, `evalRunOrdinal`) lives in
  `eval-runs-detail.tsx` and is re-exported into `eval-runs.tsx` so both
  surfaces produce identical strings. It was *not* added to
  `frontend/web/src/lib/run-display.ts` because the contract's
  `allowed_paths` only covers the three eval-runs route files.

Operator sees the same label in the eval-runs list row, the eval inspector
Topbar (`title · subtitle · Run #N · …`), the desktop SummaryCard meta
strip, and the mobile hero meta line — so navigating list → detail confirms
they landed on the right run.

## Changes shipped

- **`eval-runs-detail.tsx`** — fetch sibling runs, compute disambiguator,
  thread it through `Topbar` sub-line + `SummaryCard`. SummaryCard meta
  strip now reads `<disambiguator> · run <shortId> · View agent trace →`
  (strategy/scenario id chips removed; full run id available via
  `title=` hover on the shortened chip). Action buttons grouped in
  `grid grid-flow-col auto-cols-fr` so Stop / Retry / Download all share
  the widest natural label's column width without a hardcoded px floor.
  The status pill stays outside the grid to keep its natural size.
  `evalRunOrdinal` / `evalRunDisambiguator` defined and exported here.
- **`eval-runs-detail-mobile.tsx`** — accepts the disambiguator from the
  desktop route, surfaces it in the SummaryTab hero meta line, drops the
  redundant `strategy <id>` chip, and switches `RunActions` (Retry +
  Download) to the same `grid grid-flow-col auto-cols-fr` treatment.
- **`eval-runs.tsx`** — imports `evalRunDisambiguator` from
  `./eval-runs-detail`, computes the label per row using the full
  rendered list as siblings, and renders it in both the mobile card
  layout and the desktop table. Full id remains in a `title=` tooltip
  on the shortened `run …` chip.
- **`eval-runs-detail.test.tsx`** — mocks `listRuns`, adds two new render
  cases (disambiguator + dropped id chips; equal-width grid wrapper),
  keeps the existing 23 cases green.
- **`eval-runs-detail-mobile.test.tsx`** — mocks `listRuns`, adds a
  mobile-hero disambiguator render case.
- `eval-runs.test.tsx` — touched only by the disambiguator computation
  being driven through the existing render path; existing 14 cases still
  pass without modification.

## Verification

```bash
npm --prefix frontend/web run typecheck       # clean
npm --prefix frontend/web test -- --run \
  eval-runs-detail eval-runs.test             # 45/45 passing
npm --prefix frontend/web run build           # clean
```

Full-suite run shows a single pre-existing failure in
`src/components/chart/RunChart.test.tsx` (looking for an `sma20` label
that doesn't exist on `origin/main`). That file is not in this
contract's allowed_paths and the diff against `HEAD` is empty for it.

## Open follow-ups (out of this contract's scope)

- Same `shortId()` truncation pattern persists in `eval-compare.tsx`,
  `ChatRunListCard.tsx`, `agent/AgentForm.tsx` — the prior
  trace-fullscreen-redesign status already flagged a separate sweep.
- `qa-eval-action-lifecycle` (stacks on this branch) will add a Delete
  button to the same action row; it should pick up the
  `grid-flow-col auto-cols-fr` treatment automatically.

## Notes

Append checkpoints / PR links below.
