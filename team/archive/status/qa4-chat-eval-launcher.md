# qa4-chat-eval-launcher

Status: completed checkpoint

Branch: `qa4-chat-eval-launcher`
Worktree: `/root/deploy/xvision/.worktrees/qa4-chat-eval-launcher`

Summary:
- Start eval now reads scenarios from the scenario registry instead of the stale eval scenario list.
- The launcher defaults to backtest mode.
- The dialog performs provider/model and Alpaca paper preflight before queueing runs.
- Preflight and backend launch errors render inline and keep the dialog open.

Verification:
- `corepack pnpm --dir frontend/web test -- eval-runs.test.tsx`
- `corepack pnpm --dir frontend/web typecheck`
- `corepack pnpm --dir frontend/web test`
- `git diff --check`

Not run:
- Cargo/Rust tests, per deploy-host instruction.
