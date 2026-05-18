---
track: qa-budget-cost-precision
lane: leaf
wave: qa-operator-2026-05-18
worktree: .worktrees/qa-budget-cost-precision
branch: task/qa-budget-cost-precision
base: origin/main
status: ready
depends_on: []
blocks: []
stacking: none
allowed_paths:
  - frontend/web/src/features/budget/**
  - frontend/web/src/routes/budget.tsx
  - frontend/web/src/routes/budget.test.tsx
  - frontend/web/src/utils/cost-format.ts
  - frontend/web/src/utils/cost-format.test.ts
  - frontend/web/src/api/budget.ts
forbidden_paths:
  - crates/**
  - frontend/web/src/features/agent-runs/**
  - frontend/web/src/features/eval-runs/**
interfaces_used:
  - Budget / per-call cost API
  - cost-format utility
parallel_safe: true
parallel_conflicts: []
verification:
  - pnpm --dir frontend/web typecheck
  - pnpm --dir frontend/web lint
  - pnpm --dir frontend/web test -- --run budget cost-format
  - pnpm --dir frontend/web build
acceptance:
  - Per-call cost cells no longer display `$0.0000` for cheap models
    whose real cost is in the $1e-6 .. $1e-4 range. Smart formatting:
    `<$0.0001` with the full precision on hover, OR
    4-significant-figures with scientific notation for very small
    values. Picked rule is documented in the contract Notes.
  - Investigation note in `team/status/qa-budget-cost-precision.md`
    confirms that token prices and token counts are actually flowing
    from the model library into the per-call cost computation. If a
    gap exists (e.g. OpenRouter pricing pulled but the cost calc
    doesn't read it), file a queue note to the upstream owner and
    document the gap. The display fix lands in any case.
  - Tooltip on small-value cells shows the full underlying number to
    at least 8 significant figures so an operator can confirm "yes,
    this is $0.00000123" rather than guess.
  - Aggregate totals (per-run, per-eval) use the same formatter so the
    list and detail surfaces match.
  - Unit tests on the formatter cover: zero, $1e-7, $1e-5, $0.001,
    $0.1, $1.23, $123.45, $12_345.67.
  - No regression on the normal $0.01+ display.
---

# Scope

Operator reported (2026-05-18): "Need to add more decimal places for
low cost API models in budget — validate that token prices are flowing
because cost just shows $0.0000."

Two halves:

1. **Display fix** (P2 leaf). Replace the 4-decimal hard floor with a
   smart formatter that handles values from $1e-7 to $10k+ legibly.
   This is the bulk of the contract.
2. **Validation** (investigative). Confirm token prices are flowing
   end-to-end: model library has prices → per-call event carries
   token counts → cost calc multiplies and surfaces. If broken,
   document and surface via a queue note to the upstream owner
   (likely `qa-openrouter-pricing-pull` had the pricing pull; check
   whether the eval cost calc consumes it).

# Out of scope

- Fetching new pricing data (owned by the prior
  `qa-openrouter-pricing-pull` track).
- Backend changes to the per-call cost computation unless the
  validation half finds a missing read. If a backend fix is needed,
  file a contract update or hand off via `team/queue/`.
- Currency conversion (USD-only).

# Sync-before-work ritual

```bash
cd /Users/edkennedy/Code/xvision
git fetch --prune origin
git -C .worktrees/qa-budget-cost-precision status
git -C .worktrees/qa-budget-cost-precision log --oneline -3 origin/main..HEAD
```

If the worktree does not exist:

```bash
git fetch --prune origin
git worktree add .worktrees/qa-budget-cost-precision \
  -b task/qa-budget-cost-precision origin/main
```

# Notes

Worker should record the chosen formatter rule here before opening
the PR. Operator suggestion: `<$0.0001` placeholder with full
precision on hover; sig-fig fallback for slightly larger values.

Append checkpoints / PR links below.
