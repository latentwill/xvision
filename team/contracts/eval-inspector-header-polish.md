---
track: eval-inspector-header-polish
lane: leaf
wave: agent-run-observability-followups
worktree: .worktrees/eval-inspector-header-polish
branch: task/eval-inspector-header-polish
base: origin/main
status: claimed
depends_on: []
blocks: []
stacking: none
allowed_paths:
  - frontend/web/src/routes/eval-runs-detail.tsx
  - frontend/web/src/routes/eval-runs-detail-mobile.tsx
  - frontend/web/src/routes/eval-runs.tsx
  - frontend/web/src/routes/eval-runs-detail.test.tsx
  - frontend/web/src/routes/eval-runs-detail-mobile.test.tsx
  - frontend/web/src/routes/eval-runs.test.tsx
forbidden_paths:
  - crates/**
  - frontend/web/src/api/types.gen/**
  - frontend/web/src/features/agent-runs/**
interfaces_used:
  - EvalRunSummary (existing)
  - useEvalRunLabels (existing)
parallel_safe: true
parallel_conflicts: []
verification:
  - npm --prefix frontend/web run typecheck
  - npm --prefix frontend/web test -- eval-runs-detail
  - npm --prefix frontend/web test -- eval-runs-detail-mobile
  - npm --prefix frontend/web test -- eval-runs
acceptance:
  - The Stop / Retry / Download JSON buttons in the eval inspector header
    render at the same visual width on a given run regardless of label
    length (e.g. "Stop eval" vs "Retry"). Pick the widest natural label
    in the row as the floor (min-width or grid), not a hardcoded px.
  - The redundant `run <id> · strategy <id> · scenario <id> · View agent
    trace →` metadata strip no longer duplicates the strategy/scenario
    already shown in the title block. Keep one source of truth: the
    title block keeps the human names; the metadata strip is reduced to
    the eval run identifier + the agent-trace link. Long IDs collapse
    to a copyable short form (existing helper) on hover/click.
  - Eval runs gain a stable, user-visible disambiguator independent of
    the random ULID, so a user with N runs of the same strategy/scenario
    can tell them apart in the eval-runs list AND the detail header.
    The chosen disambiguator (sequence number per
    strategy+scenario pair, or relative start timestamp, or both) must
    be derivable from existing `EvalRunSummary` fields — no backend
    contract change. Document the choice in the contract Notes section
    before opening the PR. The detail header shows the same label as
    the list row so users can confirm they navigated to the right run.
  - Mobile detail route (`eval-runs-detail-mobile.tsx`) receives the
    same metadata-strip cleanup and disambiguator label so the two
    views stay in sync.
  - Existing eval-runs route tests still pass; add at least one render
    test that asserts the disambiguator is present and the redundant
    metadata is gone.
  - No backend, schema, or API-types changes. If a derivation truly
    needs a new field, push a contract-update PR first.
---

# Scope

User feedback (2026-05-18, eval inspector QA):

1. The Stop eval button renders wider than the Retry / Download
   buttons on the same row because the buttons size to their label.
   Make every button in that action row share a width.
2. Below the strategy / scenario title the inspector reprints the
   IDs as `run X · strategy Y · scenario Z · View agent trace →`,
   which duplicates what's already in the title block. Keep only the
   eval run id (since the title hides it) and the agent-trace link.
3. Evals do not currently expose a user-controllable name. With many
   runs of the same strategy + scenario pair, users can only tell
   them apart by random ULID, which is painful UX. Add an automatic
   disambiguator (e.g. "Run #3 — 2026-05-18 14:02") derived from
   existing fields, visible in both the eval-runs list and the
   detail-header.

# Out of scope

- No backend changes. The disambiguator is computed client-side from
  existing `EvalRunSummary` fields (status timestamps, sort order).
- No rename of the underlying eval or strategy/scenario records.
- No agent-runs surface changes — `frontend/web/src/features/agent-runs/**`
  stays locked to the existing observability tracks.
- No restyling of the rest of the eval inspector body.

# Sync-before-work ritual

```bash
cd /Users/edkennedy/Code/xvision
git fetch --prune origin
git -C .worktrees/eval-inspector-header-polish status
git -C .worktrees/eval-inspector-header-polish log --oneline -3 origin/main..HEAD
```

If the worktree does not exist:

```bash
git fetch --prune origin
git worktree add .worktrees/eval-inspector-header-polish \
  -b task/eval-inspector-header-polish origin/main
```

# Notes

Append checkpoints / PR links below. Worker must record the chosen
disambiguator scheme (sequence-per-pair, timestamp, or hybrid) here
before opening the PR so the conductor can confirm it matches the
eval-runs list view.
