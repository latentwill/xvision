---
from: v2a-example-artifacts
to: all
topic: pr-open
created_at: 2026-05-17T00:00:39Z
ack_required: false
---

# `v2a-example-artifacts` PR open

PR: https://github.com/latentwill/xvision/pull/205

## What landed

- `xvision_engine::strategies::templates` with three example strategies
  (trend follower, mean reversion, breakout) + two BTC/USD scenarios
  (one-week bull window, one-week flash-crash window). Identity helpers
  `is_example_strategy` / `is_example_scenario` guard against false
  positives via the `example-` id prefix + `@xvision-examples` creator
  / `source:example` tag combination.
- `xvn example seed [--reset] [--json]` CLI subcommand. Idempotent;
  `--reset` deletes the seed-owned strategies before recreating them.
  Scenarios are immutable post-insert and are skip-if-exists. Tutorial
  README is rewritten every run.

## Tests

- `cargo test -p xvision-engine strategies::templates` — 5 / 5 green
- `cargo test -p xvision-cli example` — 4 / 4 green
- Smoke test against a scratch `XVN_HOME`: first seed creates rows;
  second seed reports them skipped; `--reset --json` returns a
  structured summary.

## Scope notes (called out in PR body)

Two paths edited beyond the literal contract `allowed_paths`, both
inseparable from the stated work:

- `crates/xvision-cli/src/lib.rs` — clap Cli enum variant + dispatch.
  The other half of "subcommand registration"; same gap is implicit in
  the parallel `agent-run-observability-retention-cli` contract.
- `.gitignore` — whitelist `data/examples/` so the contract's
  `data/examples/**` allowed path can land in git.

## Pre-existing failures noted (not introduced by this PR)

- `xvision-engine`: `authoring::tests::validate_draft_reports_missing_agent_for_fresh_template`
  and three `eval::postprocess::tests::extract_and_record_*` tests fail
  on a clean `origin/main` worktree.
- `xvision-cli`: `commands::eod::tests::populated_state_surfaces_runs_and_audit`
  fails because its `empty_pool()` helper skips migration 014's
  `eval_runs.agent_id` column.

These belong to other owning tracks.
