---
track: wire-run-summary-into-eval-detail
lane: leaf
wave: qa-operator-2026-05-19
worktree: .worktrees/wire-run-summary-into-eval-detail
branch: task/wire-run-summary-into-eval-detail
base: origin/main
status: ready
depends_on: []   # PR #320 (the RunSummary component) already merged
blocks: []
stacking: none
allowed_paths:
  - frontend/web/src/routes/eval-runs-detail.tsx
  - frontend/web/src/routes/eval-runs-detail.test.tsx
  - frontend/web/src/routes/eval-runs-detail-mobile.tsx
  - frontend/web/src/routes/eval-runs-detail-mobile.test.tsx
forbidden_paths:
  - frontend/web/src/features/eval-runs/RunSummary.tsx
  - frontend/web/src/features/eval-runs/RunSummary.test.tsx
  - frontend/web/src/routes.tsx
  - frontend/web/src/api/**
interfaces_used:
  - RunSummary (component, from frontend/web/src/features/eval-runs/RunSummary)
  - RunSummary (TypeScript type, from @/api/types.gen — distinct from the component)
  - RunDetail / failure-rendering path on eval-runs-detail.tsx
verification:
  - pnpm --dir frontend/web typecheck
  - pnpm --dir frontend/web test -- --run eval-runs-detail
  - pnpm --dir frontend/web build
acceptance:
  - `routes/eval-runs-detail.tsx` imports `RunSummary` as a component
    from `@/features/eval-runs/RunSummary` (in addition to the
    existing type import from `@/api/types.gen`). The component import
    and the type import will share a name — alias one of them if
    needed for clarity (e.g. `import { RunSummary as RunSummaryPanel }
    from "@/features/eval-runs/RunSummary";`).
  - The existing inline failure-rendering block (the red-bordered
    code-block displaying `summary.error` or equivalent) is replaced
    by a single `<RunSummaryPanel error={...} />` invocation.
  - A failed run with the `[repeated_broker_error]` prefix renders
    the classified banner (component behavior from PR #320).
  - A failed run without the prefix renders the legacy red
    code-block (no regression to runs failing for other reasons).
  - A successful / queued / running run renders no failure panel
    (no regression).
  - If `eval-runs-detail-mobile.tsx` has the same inline failure block,
    wire it the same way. If it doesn't, leave it alone — do not add a
    failure panel where none existed before.
  - Existing `eval-runs-detail.test.tsx` (and the mobile test if
    touched) updated to assert the new component renders. If existing
    tests directly inspect the inline DOM, refactor those assertions
    to assert against `RunSummary`'s observable rendering instead.
  - No new tests required beyond keeping the existing suite green —
    the component's own behavior is covered by `RunSummary.test.tsx`
    (out of scope here).
  - `pnpm --dir frontend/web build` clean.
  - No `try/catch` silencing (`feedback_alpha_root_cause`).
parallel_safe: true
parallel_conflicts: []
---

# Scope

PR #320 (`eval-broker-error-circuit-breaker`) shipped a `RunSummary`
component at `frontend/web/src/features/eval-runs/RunSummary.tsx` that
parses the `[repeated_broker_error]` prefix produced by
`xvision_engine::eval::executor::format_failure_reason` and renders a
classified one-liner above the raw error text. It also keeps the
legacy red code-block rendering for runs without the prefix.

That component is currently dead code — `routes/eval-runs-detail.tsx`
was outside #320's `allowed_paths`, so the inline failure block was
never replaced. This track does the one-line wire-up.

Anchor reading:

- `team/intake/2026-05-19-test-drift-and-wiring.md` (finding 3).
- `frontend/web/src/features/eval-runs/RunSummary.tsx` (component to
  use — read for its prop shape).
- `frontend/web/src/routes/eval-runs-detail.tsx` (current inline
  failure block — find it via `summary.error` references around the
  failed-state branch).

# Out of scope

- Changes to `RunSummary.tsx` itself (forbidden by contract).
- Adding new failure classification categories to the component.
- Restyling the legacy red code-block path.
- Refactoring the eval-runs-detail route beyond the failure block.

# Sync-before-work ritual

```bash
cd /Users/edkennedy/Code/xvision
git fetch --prune origin
git -C .worktrees/wire-run-summary-into-eval-detail status
git -C .worktrees/wire-run-summary-into-eval-detail log --oneline -3 origin/main..HEAD
```

If the worktree does not exist:

```bash
git fetch --prune origin
git worktree add .worktrees/wire-run-summary-into-eval-detail \
  -b task/wire-run-summary-into-eval-detail origin/main
```

# Notes

Append checkpoints / PR links below.
