# status — eval-running-animation

- Worker: @latentwill
- Claimed: 2026-05-16
- State: claimed
- Worktree: `.worktrees/eval-running-animation`
- Branch: `task/eval-running-animation`
- Base: `origin/main`

## Plan

1. Audit the four `StatusPill` callsites (`eval-runs.tsx`, `eval-runs-detail.tsx`, `eval-compare.tsx`, `home.tsx`) for shared shape.
2. Decide: `animated` prop on `Pill` vs. a `RunningStatusPill` wrapper. Default: keep `Pill` stateless, introduce a single shared `RunningStatusPill` helper.
3. Add a `running-pulse` keyframe (Tailwind config or `globals.css`) guarded by `prefers-reduced-motion`.
4. Wire each callsite to the shared helper.
5. Add render tests: `running` row carries the animation marker, `completed` row does not.
6. Verify: typecheck, lint, eval-runs / eval-runs-detail test suites, `scripts/board-lint.sh`.

## Checkpoints

(empty — none yet)
