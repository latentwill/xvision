---
track: agent-cli-press-audit
lane: integration
wave: agent-cli-press-audit-2026-05-25
worktree: /Users/edkennedy/Code/xvision-cli-press
branch: task/agent-cli-press-audit
base: origin/main
status: ready          # operator-authorized track; worked via worktree + subagent fan-out
depends_on: []
blocks: []
stacking: none
allowed_paths:
  - crates/xvision-cli/tests/cli_surface.rs
  - crates/xvision-cli/tests/cli_surface_snapshot.json
  - crates/xvision-cli/tests/agent_workbench.rs
  - crates/xvision-cli/src/commands/agent.rs
  - crates/xvision-cli/src/json/list_shapes.rs
  - crates/xvision-cli/src/json/mod.rs
  - crates/xvision-dashboard/src/cli_jobs/allowlist.rs
  - crates/xvision-dashboard/wiki/cli-reference.md
  - crates/xvision-dashboard/wiki/remote-cli.md
  - crates/xvision-mcp/src/parity.rs
  - docs/superpowers/evidence/2026-05-25-agent-cli-press-audit/**
  - README.md
  - scripts/xvn-remote.py
  - team/contracts/agent-cli-press-audit.md
  - team/status/agent-cli-press-audit.md
forbidden_paths:
  - crates/xvision-engine/migrations/**
  - crates/xvision-core/migrations/**
  - crates/xvision-engine/src/api/strategy.rs
  - crates/xvision-engine/src/strategies/store.rs
  - crates/xvision-engine/src/agents/store.rs
interfaces_used:
  - xvision_cli::Cli (clap::CommandFactory)
  - xvision_engine::api::agents::{list, validate}
  - xvision_dashboard::cli_jobs::allowlist
  - xvision_cli::json::{emit_object, ObjectFormat}
  - xvision_cli::exit::{XvnExit, CliError, ResultExt}
parallel_safe: false
parallel_conflicts:
  - cli-strategy-clone-model-override   # owns commands/strategy.rs + api/strategy.rs (live leaf)
verification:
  - cargo test -p xvision-cli --test cli_surface
  - cargo test -p xvision-cli --test agent_workbench
  - cargo test -p xvision-cli
  - bash scripts/board-lint.sh
acceptance:
  - "Checked-in CLI surface inventory generated from Cli::command() (verbs, subcommands, aliases, output flags, mutation markers); a drift test fails when the clap tree changes without regenerating it."
  - "Regression test fails when a new top-level xvn verb is added without documenting it in wiki/cli-reference.md (or explicitly exempting it)."
  - "Regression test fails when the remote CLI allowlist references a command/subcommand path that does not exist in the clap tree."
  - "xvn agent ls --format table|json|json-compact lists library agents; xvn agent lint [--json] surfaces validation diagnostics."
  - "Object/list commands expose --format json|json-compact (legacy --json kept as alias); JSON-stdout contract + typed-exit integration tests cover usage/auth/not-found/upstream/conflict."
  - "One --dry-run convention applied to mutating strategy/scenario/provider/agent verbs; remote policy keeps rejecting mutation paths."
  - "One canonical remote-agent doc (create/poll/output/SSE/cancel) across README + wiki + scripts/xvn-remote.py."
  - "Checked-in MCP-vs-engine-API parity matrix; new workbench fns must declare MCP posture."
---

# Scope

Implements the **2026-05-25 Agent CLI Press Audit Amendment** of
`docs/superpowers/plans/2026-05-10-xvn-scheduling-and-agent-cli.md`. The
original plan is stale — `xvision_engine::api`, `api_audit`, typed exit
codes (`exit.rs`), remote CLI jobs, and `xvn run inspect` already shipped.
This track delivers the amendment's six batches: freeze the CLI surface
with regression tests, complete the `xvn agent` workbench (`ls`, `lint`),
normalize the output/error contract, add a single `--dry-run` mutation
convention, consolidate the remote-agent docs, and produce an MCP/engine-API
parity matrix. `schedule`, `deploy` mutation verbs, and binary renames are
**explicitly deferred** by the amendment and out of scope here.

# Out of scope

- `xvn schedule` / cron scheduler / `agent_runner` (deferred — needs fresh
  current-state design against agentd + remote CLI jobs).
- `deploy` mutation verbs and deployment-risk knob mutation.
- Any new DB migration (audit + api schema already exist).
- `commands/strategy.rs` and `api/strategy.rs` mutation logic — owned by the
  live `cli-strategy-clone-model-override` leaf. Batches 3/4 only normalize
  *output* on the strategy list path and must rebase on that leaf if it lands
  first; the parallel Wave-1 fan-out does not touch these files.
- Binary renames (`xvn` / `xvn-mcp` stay).

# Sync-before-work ritual

```bash
cd /Users/edkennedy/Code/xvision
git fetch --prune origin
git -C /Users/edkennedy/Code/xvision-cli-press status
git -C /Users/edkennedy/Code/xvision-cli-press log --oneline -3 origin/main..HEAD
```

# Notes

- 2026-05-25: track opened by operator directive (outside a conductor sweep).
  Worked from a sibling worktree (`/Users/edkennedy/Code/xvision-cli-press`,
  off `origin/main` == amendment commit `e38f615`) and via subagent fan-out.
- Wave 1 (parallel, disjoint files): Batch 1 (surface inventory), Batch 2
  (agent workbench), Batch 5 (remote docs), Batch 6 (MCP parity).
- Wave 2 (sequential, cross-cutting command files): Batch 3 (output/error
  contract), Batch 4 (dry-run mutation safety).
