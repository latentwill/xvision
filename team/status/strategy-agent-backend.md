---
track: strategy-agent-backend
worktree: /root/deploy/xvision/.worktrees/strategy-agent-backend-core
branch: strategy-agent-backend-core
phase: implementation-progress
last_updated: 2026-05-13T02:05:12Z
---

# What I'm doing right now

Committed the first scoped backend/CLI checkpoint on
`strategy-agent-backend-core`:

- `2ae9828 feat(cli): expose strategy agent composition commands`
- Adds `xvn strategy add-agent`, `remove-agent`, and `set-pipeline`
- Adds CLI round-trip coverage for add/set/remove
- Adds exit-code coverage for missing agent and invalid pipeline kind
- `f2786a3 feat(strategy): migrate legacy slots to agent refs`
- Adds `xvn strategy migrate-agents [--dry-run]`
- Migrates legacy strategy slots into Agent records, writes AgentRefs +
  PipelineDef, clears old slot fields, and validates the new shape
- Allows `validate_bundle` to accept agent-ref strategies without requiring a
  legacy trader slot
- `fd1fc0e feat(strategy): execute resolved agent pipelines`
- Threads resolved AgentRefs through eval executors and `xvn strategy run`
- Executes single/sequential agent-ref pipelines; graph pipelines explicitly
  return a runtime error until graph execution semantics are implemented
- Uses role `trader` as the decision output, falling back to the last
  sequential agent if no role is named `trader`

The underlying engine/API agent-ref substrate was already present on `main`;
these checkpoints expose it through the CLI and provide a salvage path for
existing local slot-shaped bundles.

# Blocked on

Nothing.

# Next up

- Decide and execute the deeper legacy-slot cut:
  - either remove old strategy slot fields/callers now, or
  - keep them only where a live caller still forces it and document that caller
- Decide whether to make templates create Agent records directly. Current
  shipped `xvn strategy new` still creates slot-shaped drafts; `migrate-agents`
  converts them to the new shape.
- Decide whether graph pipeline execution is in v1 scope. Types and validation
  exist; runtime intentionally rejects graph execution for now.
- Token estimation still reads legacy slot fields only; agent-ref estimates are
  low until that helper resolves attached agents.
- Run the required Rust verification in CI or a non-deploy workspace:
  - `cargo test -p xvision-engine`
  - `cargo test -p xvision-cli strategy -- --nocapture`

# Verification so far

- `git diff --check` passed in the worktree.
- Rust tooling was not run locally because this deploy host is governed by the
  `CLAUDE.md` no-cargo guardrail.
