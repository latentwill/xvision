# strategy-agent-backend

## Goal

Finish the backend half of the strategy-agent refactor and make it the new
authoritative strategy shape. Old slot-shaped strategy compatibility is not
required.

## Primary source

- `docs/superpowers/plans/2026-05-12-strategies-refactor-agent-composition.md`
- `team/execution-board-2026-05-13.md`

## Worktree

- `/root/deploy/xvision/.worktrees/strategy-agent-backend-core`
- Branch: `strategy-agent-backend-core`

The earlier `/root/deploy/xvision/.worktrees/strategy-agent-backend` branch
contains out-of-scope frontend/eval-runs work. Treat it as source-only unless
there is an explicit cherry-pick decision.

## Scope

- `crates/xvision-engine/src/authoring/*`
- `crates/xvision-engine/src/api/strategy.rs`
- strategy store / migration command files
- `crates/xvision-cli/src/commands/strategy.rs`

## Explicitly in

- `add-agent`, `remove-agent`, `set-pipeline`
- strategy migration command if still useful
- strategy API surface for agent refs + pipeline
- CLI subcommands and tests

## Explicitly out

- Inspector rebuild
- chat rail / wizard prompt work
- broad QA pass 4 UI cleanup

## Constraints

- Prefer the simplest target shape.
- Do not spend time preserving the old slot format unless a live caller forces
  it.
- If an old caller breaks, either update it now if local to this scope or note
  the exact downstream consumer in status.

## Verification

```bash
cargo test -p xvision-engine
cargo test -p xvision-cli strategy -- --nocapture
```
