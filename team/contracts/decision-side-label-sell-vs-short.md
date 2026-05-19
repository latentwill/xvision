---
track: decision-side-label-sell-vs-short
lane: leaf
wave: qa-operator-2026-05-19-round-4
worktree: .worktrees/decision-side-label-sell-vs-short
branch: task/decision-side-label-sell-vs-short
base: origin/main
status: claimed
depends_on: []
blocks: []
stacking: none
allowed_paths:
  - frontend/web/src/routes/eval-runs-detail.tsx
  - frontend/web/src/routes/eval-runs-detail.test.tsx
  - frontend/web/src/routes/eval-runs-detail-mobile.tsx
  - frontend/web/src/routes/eval-runs-detail-mobile.test.tsx
  - frontend/web/src/features/decisions/positions.ts
  - frontend/web/src/features/decisions/positions.test.ts
  - frontend/web/src/styles/globals.css  # only the .dec-pill--{kind} colour-mix rules
forbidden_paths:
  - crates/**
  - frontend/web/src/api/types.gen/**
interfaces_used:
  - DecisionRowDto
  - derivePositionsByDecision
  - OpenPosition / PositionSide
parallel_safe: true
parallel_conflicts: []
verification:
  - pnpm --filter web exec tsc -p tsconfig.app.json --noEmit
  - pnpm --filter web exec vitest run src/routes/eval-runs-detail.test.tsx src/routes/eval-runs-detail-mobile.test.tsx src/features/decisions/positions.test.ts
  - pnpm --filter web build
acceptance:
  - DecisionSignal pill renders five distinct labels driven by `(action, prior_side_for_asset)`:
      `long_open`           → `BUY`
      `short_open`          → `SHORT`
      `flat` after `long`   → `SELL`
      `flat` after `short`  → `COVER`
      `hold` (and `flat` when already flat — defensive) → `HOLD`
  - Filter tabs above the decisions table expose six buckets: `All / Buy / Short / Sell / Cover / Hold`. Counts reflect the position-aware mapping (a `flat` row's bucket depends on prior side).
  - Mobile decisions view (`eval-runs-detail-mobile.tsx`) applies the same five-label mapping; existing `ActionPill` typing widens accordingly.
  - `derivePriorSideByDecision` (new export from `features/decisions/positions.ts`) returns the per-asset side held *before* each `decision_index`, with a unit test covering: open-from-flat, flat-after-long, flat-after-short, hold preserves prior side, and reverse via short_open while long.
  - Existing `derivePositionsByDecision` semantics unchanged (still returns *after* state). No on-the-wire schema changes.
  - Existing tests in `eval-runs-detail.test.tsx` and `eval-runs-detail-mobile.test.tsx` keep passing; new assertions cover at least one `flat-after-long → SELL` row and one `flat-after-short → COVER` row.
---

# Scope

Operator complaint (QA22, round-4 intake item #4): a `short_open` decision
renders as a generic `SELL` pill, and a `flat` decision renders as `CLOSE`
regardless of which side was being closed. The two-way collapse hides
direction context — operators reading the table see "SELL" on the row that
opened a short and have to cross-reference the Open-positions cell to
disambiguate.

This track makes the action pill direction-aware:

| Action     | Prior side | Pill |
|------------|------------|------|
| long_open  | (any)      | BUY    |
| short_open | (any)      | SHORT  |
| flat       | long       | SELL   |
| flat       | short      | COVER  |
| flat       | flat       | HOLD (defensive — flat-from-flat is a no-op)|
| hold       | (any)      | HOLD   |

Prior-side is computed client-side from the existing decision sequence;
no schema change. `derivePositionsByDecision` already walks the full
ordered sequence to produce *after* state — adding a parallel
*before*-state walk in the same module keeps the position-derivation
logic in one place. Filter tabs above the table widen to match the five
display labels so the "Sell" count means "actually sold a long" rather
than "opened a short or closed something".

Implements finding #4 in `team/intake/2026-05-19-qa-operator-round-4.md`.

# Out of scope

- Engine-side schema or persistence changes. `DecisionRowDto.action` stays
  at the four backend values (`long_open`, `short_open`, `flat`, `hold`).
- Open-positions cell rendering. It already shows side via the existing
  `OpenPositionsCell`; only the action pill changes.
- Cross-asset or multi-leg positions. The engine sim is single-asset
  long-or-short-or-flat per decision; this track inherits that model.
- Trace/agent-run views. Action-pill labelling on the trace surface is a
  separate track if it surfaces a regression.

# Sync-before-work ritual

```bash
cd /Users/edkennedy/Code/xvision
git fetch --prune origin
git -C .worktrees/decision-side-label-sell-vs-short status
git -C .worktrees/decision-side-label-sell-vs-short log --oneline -3 origin/main..HEAD
```

If the worktree does not exist:

```bash
git fetch --prune origin
git worktree add .worktrees/decision-side-label-sell-vs-short \
  -b task/decision-side-label-sell-vs-short origin/main
```

# Notes

- The operator's verbatim wording was "Sell always says short" — the
  literal rendered label is `SELL` (not `SHORT`), but the underlying
  collapse the operator was pointing at — `short_open` and `flat` both
  losing direction context in the pill — is the real defect. The intake
  table's prescribed five-label mapping captures the fix correctly.
- `flat`-from-`flat` is technically reachable if the engine ever emits a
  redundant close; the defensive `HOLD` mapping keeps the pill neutral
  rather than minting a misleading SELL/COVER on a no-op row.
