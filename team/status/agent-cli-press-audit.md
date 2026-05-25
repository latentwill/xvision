---
track: agent-cli-press-audit
phase: in-progress
updated: 2026-05-25
---

# Status

Implementing the 2026-05-25 Agent CLI Press Audit Amendment (6 batches).

## Progress

- [x] Setup: worktree off origin/main (e38f615), contract + status, board-lint.
- [x] Batch 1 — Freeze CLI surface. `tests/cli_surface.rs` (3 tests): snapshot drift, verb→wiki coverage, allowlist→clap existence; `allowlist::referenced_command_paths()` accessor. Green.
- [x] Batch 2 — Agent workbench: `xvn agent ls --format table|json|json-compact` (+ `list` alias) over `agents::list`; `xvn agent lint [--json]` over `agents::validate`, exit 2 on Error-severity. `tests/agent_workbench.rs` (7 tests). Green.
- [x] Batch 5 — Remote-agent docs: new `wiki/remote-cli.md` (6 endpoints grounded in `server.rs`/`routes/cli.rs`), README cancellation+allowlist notes, `xvn-remote.py` uses DELETE cancel. py_compile clean.
- [x] Batch 6 — MCP/engine-API parity matrix (146 API fns vs 31 MCP tools; 16 bespoke) + `tests/parity.rs` tool-set guard + `XvisionTools::tool_names()`. Green.
- [ ] Batch 3 — Output/error contract (Wave 2, cross-cutting). PENDING checkpoint.
- [ ] Batch 4 — Dry-run mutation safety (Wave 2, cross-cutting). PENDING checkpoint.

## Findings surfaced (for Wave 2 / triage)
- Pre-existing red on main (NOT this track): `commands::eval::review::tests::end_to_end_review_persists_inconclusive_with_local_candle` → `no such table: eval_runs_old_live_migration`. Reproduces on clean origin/main. Owned by an eval/migration track.
- Allowlist gap: `xvn agent create` (mutation) is remotely-allowed — `agent` is a SUPPORTED head and there is no `DENIED_NESTED {agent, create}`. Candidate fix for Batch 4 (remote mutation policy).

## Coordination

- `cli-strategy-clone-model-override` owns `commands/strategy.rs` +
  `api/strategy.rs` (live leaf). Wave-1 fan-out avoids these. Batches 3/4
  rebase on it if it lands first.
