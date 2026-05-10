---
from: eval-3d-run
to: all
topic: rebased-mergeable
created_at: 2026-05-10T12:53:00Z
ack_required: false
---

# PR #26 rebased onto current `main` — now MERGEABLE

PR: https://github.com/latentwill/xvision/pull/26
Branch: `feature/eval-3d-run`
Worktree: `.worktrees/eval-3d-run`

## State

- Was `CONFLICTING` against the post-#24 main; rebase had been started but
  abandoned mid-conflict in `crates/xvision-engine/src/api/eval.rs`.
- Conflict resolved by interleaving the new `run` / `run_with_deps` /
  `EvalRunRequest` / `run_inner` block alongside the `get_run` /
  `get_run_inner` / `RunDetail` block that landed in `de0577c` (PR #24). All
  five public surfaces (`list`, `list_summaries`, `get`, `get_run`,
  `scenarios`, `run`, `run_with_deps`) coexist in one module — module-level
  doc comment updated to reflect the full surface and which PR each came
  from.
- Imports unioned (`Arc`, `Scenario`, `api_strategy`, `Executor`,
  `PaperExecutor`, `AnthropicDispatch`, `LlmDispatch`, `ToolRegistry`,
  `AlpacaPaperSurface`, `BrokerSurface`).
- Force-pushed with `--force-with-lease`. `gh pr view 26` reports
  `mergeable: MERGEABLE`, `mergeStateStatus: UNSTABLE` (docker `build-default`
  is in progress at workflow run 25629223131).

## Verification

- `cargo build --workspace` — green
- `cargo test --workspace` — 423 passed / 0 failed
- `cargo test -p xvision-engine --test api_eval_run` — 6/6 pass
  (`run_returns_not_found_for_unknown_strategy`,
  `run_rejects_backtest_mode_until_executor_lands`,
  `run_returns_not_found_for_unknown_scenario`,
  `run_with_deps_completes_paper_run_with_mocks`,
  `run_persists_run_to_runstore_so_get_finds_it`,
  `run_writes_audit_row_on_completion`)

## Ready for operator merge

Once docker `build-default` lands green, this is the v1 demo command —
`xvn eval run --strategy <agent_id> --scenario crypto-bull-q1-2025` drives
the full pipeline end-to-end with a real Alpaca paper account + Anthropic
key. Last open PR on the board.

## Follow-ups (deferred from this PR — same as PR #26 body)

- BacktestExecutor (Phase 3.B-backtest) — separate PR with parquet
  fixtures + indicator panel + simulate_fill + Sharpe-from-equity-samples
- MCP `eval/run` verb (Phase 3.D Task 12)
- SSE progress endpoint (Phase 3.D Task 13)
- `xvn eval compare` (Task 10)
- `xvn eval extract-findings <run>` / `xvn eval attest <run>` —
  thin wrappers over PR #19 + PR #17
