# Claim: qa9-strategy-create-agent-guardrail

Worktree: `.worktrees/qa9-strategy-create-agent-guardrail`

Branch: `qa9-strategy-create-agent-guardrail`

Owner: codex

## Scope

Make the strategy creation path explicit that a strategy is not eval-ready until
it has a complete attached agent, and tighten validation so zero-agent drafts do
not validate as ready.

## Verification plan

- Strategy creation UI regression for the strategy-agent checklist.
- Strategy validation regression for zero attached AgentRefs.
- `corepack pnpm --dir frontend/web test -- strategies-new`
- `corepack pnpm --dir frontend/web typecheck`
- `git diff --check`

Rust tests are CI/non-deploy only on this host per `CLAUDE.md`.
