---
track: integration-next-build
worktree: /root/deploy/xvision/.worktrees/integration-next-build
branch: integration-next-build
phase: integrated-for-next-build
last_updated: 2026-05-13T02:42:00Z
owner: codex-cli
---

# What changed

The `integration-next-build` branch merges all ten completed execution-board
tracks from `team/execution-board-2026-05-13.md` in board order.

Merged branch heads:

- `remote-cli-orphan-recovery-clean` at `db8baec`
- `agent-docs-cli-truth-clean` at `34b8d00`
- `ghcr-build-optimization` at `ef76621`
- `strategy-agent-backend-core` at `5a0153f`
- `pr94-chart-stabilization-clean` at `ff56036`
- `qa4-settings-zero-provider` at `1ed7c45`
- `qa4-scenarios-4h-bars-ui` at `9a20224`
- `qa4-surface-consistency` at `350d462`
- `qa4-chat-eval-launcher` at `4fbabaa`
- `strategy-agent-inspector` at `cd5687d`

# Conflict resolution

- Kept the latest execution-board closeout and MCP/subagent runtime notes.
- Replaced stale `strategy-agent-backend` status with the
  `strategy-agent-backend-core` PR #97/backend checkpoint status.
- Preserved backend AgentRef execution/token-estimation behavior while merging
  the inspector frontend changes.
- Kept `migrate-agents` tests on an explicit legacy-slot fixture because
  `xvn strategy new` now emits AgentRef-shaped drafts.

# Verification

Passed on this deploy host:

- `git diff --check`
- conflict marker scan outside `scripts/setup_runpod.sh`
- `bash scripts/check_agent_docs.sh`
- Python YAML parse for `.github/workflows/*.yml`
- `corepack pnpm --dir frontend/web install --frozen-lockfile`
- `corepack pnpm --dir frontend/web test`
- `corepack pnpm --dir frontend/web typecheck`
- `corepack pnpm --dir frontend/web build`

Not run here because `CLAUDE.md` forbids Rust tooling on this deploy host:

- `cargo test -p xvision-dashboard cli_jobs -- --nocapture`
- `cargo test -p xvision-cli help_cli -- --nocapture`
- `cargo test -p xvision-engine`
- `cargo test -p xvision-cli strategy -- --nocapture`
- `cargo test -p xvision-core -p xvision-engine`
- `cargo test -p xvision-engine scenario -- --nocapture`

# Next up

Run the Rust verification in CI or a non-deploy workspace, then use
`integration-next-build` as the next GHCR build candidate if those checks pass.
