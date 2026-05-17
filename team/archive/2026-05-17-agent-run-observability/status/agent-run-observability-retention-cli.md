---
track: agent-run-observability-retention-cli
worktree: .worktrees/agent-run-observability-retention-cli
branch: task/agent-run-observability-retention-cli
phase: pr-open
last_updated: 2026-05-17T04:00:00Z
owner: claude-opus
---

# What I'm doing right now

Phase A leaf #3 of the agent-run-observability wave.

PR open: https://github.com/latentwill/xvision/pull/203

- `xvision-observability::retention` — `resolve` (CLI > env > file >
  default) with per-toggle provenance, `write_config` + sentinel,
  `clear_config`, startup WARN.
- `xvision-observability::janitor` — `expire_old_payload_refs`,
  `truncate_to_max_bytes`, `run_once`, `spawn_periodic`.
- `xvn obs retention {show,set,clear}` + `xvn obs janitor run --once`.

# Blocked on

Nothing. Waiting on conductor merge.

# Contract drift to flag

Contract listed `crates/xvision-cli/src/commands/mod.rs` as the
registration point for new subcommands. In this codebase,
registration actually lives in `crates/xvision-cli/src/lib.rs` (one
new `Command` enum variant + one match arm). PR touches `lib.rs`
instead of `mod.rs` for that wiring. Tiny edits; `team/OWNERSHIP.md`
row 63 should be updated to swap the multi-owner path.

# Next up

Phase A is feature-complete after this PR + #202 merge. Phase B
(`agent-run-observability-ipc-emission`, `otel-bridge`, `export-cli`,
`ui`) is gated on Cline SDK migration reaching step 3
(`xvision-agent-client` crate exists). PR #199 is still draft.

# Notes

- 43/43 tests passing locally: 24 prior unit (redactor + config +
  blobs), 4 migration, 4 retention unit, 7 retention_precedence,
  8 janitor.
- Engine + CLI builds clean (3 unrelated dead-code warnings).
- Round-trip `xvn obs retention show/set/clear` smoke-tested
  manually against a tmp `XVN_HOME`.
- SHA hex tie-break test deliberately equalises file mtimes via
  `File::set_modified` (stable since 1.75) so the deterministic
  tie-break ordering is the only signal.
