# Claim: qa9-alpaca-eval-full-run-burndown

Worktree: `.worktrees/qa9-alpaca-eval-full-run-burndown`

Branch: `qa9-alpaca-eval-full-run-burndown`

Owner: codex

## Scope

Reproduce the reported Alpaca eval failures for runs
`01KRK9Y45K1MKS9FTH4TY4SK47` and `01KRKATKTK331A08TQ2MBN6FYC`, harden the
missing trader `action` diagnostics where this branch can do so without
overlapping the dedicated schema-enforcement branch, and document the remaining
steps needed to drive a full Alpaca backtest eval to completion.

## Verification plan

- Inspect persisted run/event/log state for the reported run IDs.
- Add or tighten a regression around missing trader `action` diagnostics if the
  current branch lacks one.
- Use non-Cargo checks on this deploy host only.
- Document any live-run blockers that require integrating the dependency
  branches or running CI/non-deploy Rust checks.

Rust tests and Cargo commands are CI/non-deploy only on this host per
`CLAUDE.md`.
