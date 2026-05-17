---
from: v2a-example-artifacts
to: all
topic: claim
created_at: 2026-05-16T23:39:26Z
ack_required: false
---

# `v2a-example-artifacts` track claimed

Picking up V2A item 3 from `team/contracts/v2a-example-artifacts.md` and
`docs/superpowers/plans/2026-05-13-v2-v4-action-plan.md`.

Branch `task/v2a-example-artifacts` from `origin/main`. Worktree at
`.worktrees/v2a-example-artifacts`.

## Scope (per contract)

- `xvn example seed --reset` populates a curated set of strategies,
  scenarios, and tutorial artifacts into the active `XVN_HOME`.
- Idempotent; items labelled `source=example` so operator data is not
  overwritten.
- Example artifacts referenced by `v2a-driver-tour` available after seed.

## Allowed paths (per contract)

- `crates/xvision-cli/src/commands/example/**`
- `crates/xvision-cli/src/commands/mod.rs` — single-line subcommand registration
- `data/examples/**`
- `crates/xvision-engine/src/strategies/templates.rs` — add example template ids

## Non-conflicts

- `v2a-driver-tour` is frontend-only (`frontend/web/**`) — no overlap.
- `v2a-in-app-docs` touches dashboard + frontend docs — no overlap.
- `agent-run-observability-*` lives in `xvision-observability` + new CLI
  subtree `commands/obs/**` — no overlap with `commands/example/**`.

The single-line registration in `crates/xvision-cli/src/commands/mod.rs`
is a multi-owner exemption (same pattern as `obs` retention CLI per its
contract notes). Will keep the edit to one line.

## Smoke plan

- `cargo test -p xvision-cli example` — green
- `cargo test -p xvision-engine strategies::templates` — green
- Manual: `xvn example seed --reset` against a scratch `XVN_HOME`,
  followed by `xvn strategy ls` to confirm seeded items.
