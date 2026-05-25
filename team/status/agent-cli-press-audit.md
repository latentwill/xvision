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
- [x] Batch 3 — Output/error contract: `--format json|json-compact` (legacy `--json` kept as alias) on `agent ls`, `provider list`, `scenario select`, `eval list`, `experiment ls`, `model status`, `strategy ls`/`strategies import`. JSON-stdout contract extended; new `typed_exit_matrix.rs` (Success/Usage/NotFound/Conflict). Green.
- [x] Batch 4 — Dry-run mutation safety: one `--dry-run` convention (validate+preview, write nothing, exit 0) on `scenario create/clone/archive/rm/classify/set-regime`, `provider add/remove/refresh-models`, `strategy new/create/clone`, `agent create`. Closed remote allowlist gap (`DENIED_NESTED {agent,create}`). Green.

## Verification (2026-05-25, integration branch)
MY deliverables — all green when run per-binary:
cli_surface 3/3 · agent_workbench 7/7 · scenario_dryrun_format 9/9 · provider_dryrun_format 7/7 · list_output_format 9/9 · strategy_dryrun_format 9/9 · typed_exit_matrix 5/5 · json_stdout_contract 3/3 · dashboard allowlist 17/17 · mcp parity 1/1. Every pre-existing CLI test binary touching my edited files (scenario_cli, strategy_cli, strategy_clone_cli, strategies_cli, scenario_inspect, strategy_add_filter, run_inspect, provider_list_effective, exit_codes_strategy) passes in isolation.

PRE-EXISTING failures (reproduce IDENTICALLY on clean origin/main 3d23243 — NOT this track):
- `eval_runs_old_live_migration` missing-table → lib `eval::review`, `eval_results_report` (2), `experiment_run` (1), `model_bakeoff_cli` (1), `eval_model_override_cli` (1). Eval-runs migration debt.
- `strategy_validate_warnings` (2: no-Filter warning) — agent-firing-filter wave.
These are out-of-scope (eval/migration is in this contract's forbidden_paths) and were not touched.

NOTE: a full `cargo test -p xvision-cli --no-fail-fast` launches ~38 binaries concurrently and produces spurious `0 passed/N failed in 0.00s` results for tempdir+sqlite binaries from cross-binary contention; the same binaries pass run individually. Run per-binary (or bounded) for a clean signal.

## Findings surfaced (for Wave 2 / triage)
- Pre-existing red on main (NOT this track): `commands::eval::review::tests::end_to_end_review_persists_inconclusive_with_local_candle` → `no such table: eval_runs_old_live_migration`. Reproduces on clean origin/main. Owned by an eval/migration track.
- Allowlist gap: `xvn agent create` (mutation) is remotely-allowed — `agent` is a SUPPORTED head and there is no `DENIED_NESTED {agent, create}`. Candidate fix for Batch 4 (remote mutation policy).

## Coordination

- `cli-strategy-clone-model-override` owns `commands/strategy.rs` +
  `api/strategy.rs` (live leaf). Wave-1 fan-out avoids these. Batches 3/4
  rebase on it if it lands first.
