---
track: qa10-eval-trader-empty-output-resilience
worktree: .worktrees/qa10-eval-trader-empty-output-resilience
branch: qa10-eval-trader-empty-output-resilience
claimed_at: 2026-05-16T02:54:07Z
last_updated: 2026-05-16T03:15:50Z
status: implemented-pending-pr
---

# What I'm doing right now

Implementation complete and locally green. Drafting the PR description next.

# Scope landed

- Typed trader-output failure classification: new `TraderFailureKind` enum
  (`empty`, `tool_use_only`, `truncated`, `invalid_json`, `missing_field`,
  `invalid_field`, `missing_response`) and a `TraderOutputError` struct that
  carries raw provider diagnostics (`stop_reason`, input/output tokens, a
  `<=240`-char raw text excerpt) and a stable Display:
  `run <id> decision <n>: trader_output[<tag>]: <detail> (stop_reason=...,
   input_tokens=..., output_tokens=..., raw_excerpt=...)`.
- Parser refactor in `crates/xvision-engine/src/eval/executor/trader_output.rs`
  now classifies:
  - empty text → `EmptyText`
  - empty text + tool_use blocks → `ToolUseOnly`
  - empty text + `StopReason::MaxTokens` → `Truncated`
  - parser failure under `MaxTokens` → `Truncated` (operator-priority signal)
  - parser failure with "missing field" serde error → `MissingField`
  - other JSON parse failures → `InvalidJson`
  - validation failure (action, conviction, justification) → `InvalidField`
- Missing trader pipeline slot now produces a typed `MissingResponse`
  error instead of a bare `anyhow!()` string, so downstream review reads the
  same `trader_output[missing_response]` prefix as the other classes.
- Executor failure handler (paper + backtest) routes the run-level error
  through a new `classify_run_failure(&anyhow::Error) -> &'static str`
  helper in `eval::executor` and prefixes the persisted `eval_runs.error`,
  `ProgressEvent::RunFailed.error`, and chart-status message with
  `[<class>] `. Provider-transport classes covered: `provider_timeout`,
  `provider_connect`, `provider_http_error`, plus `unclassified`.
- Order-safety property: the parser short-circuits with `?` before
  `submit_order` (paper) and `simulate_fill` (backtest), so an empty /
  truncated / invalid trader output cannot produce an order or a decision
  row. New tests assert this explicitly.

No new migration was needed; the `eval_runs.error` column stores the
self-classifying prefix.

# Tests

New parser tests in `crates/xvision-engine/src/eval/executor/trader_output.rs`:

- `tool_use_only_response_classifies_as_tool_use_only`
- `max_tokens_empty_response_classifies_as_truncated`
- `raw_excerpt_is_truncated_at_limit`
- `failure_kind_round_trips_through_tag`
- `missing_response_helper_classifies_as_missing_response`

New executor integration tests:

- `crates/xvision-engine/tests/eval_executor_paper.rs`:
  - `paper_executor_fails_with_empty_class_on_empty_trader_output`
  - `paper_executor_fails_with_truncated_class_on_max_tokens_no_text`
  - `paper_executor_fails_with_tool_use_only_class_when_no_final_text`
  - `paper_executor_invalid_json_failure_preserves_invalid_json_class`

  Each asserts the `[<class>] ` prefix on `eval_runs.error`, the
  `trader_output[<tag>]:` body, that `mock.submitted()` is empty, and that
  zero `eval_decisions` rows are persisted.

- `crates/xvision-engine/tests/eval_progress_backtest.rs`:
  - `backtest_executor_fails_with_empty_class_on_empty_trader_output`

  Drives the QA10 reproduction (run `01KRMKWZ1KJ2BGRNWGP518ZQ3Q`-style
  empty-text response) end-to-end on the backtest executor and asserts:
  - run.status = Failed
  - `ProgressEvent::RunFailed.error` starts with `[empty]` and contains the
    `trader_output[empty]` tag and provider diagnostics
  - no `FillRecorded`/`DecisionEmitted` events were emitted
  - persisted `eval_runs.error` carries the `[empty]` prefix and raw
    diagnostics

# Verification (board-listed)

- `cargo test -p xvision-engine --test eval_executor_paper` — 11 passed.
- `cargo test -p xvision-engine --test eval_progress_backtest` — 5 passed.
- `cargo test -p xvision-engine --lib eval::executor::trader_output` — 13
  passed.
- `cargo check -p xvision-dashboard -p xvision-cli` — clean.

Pre-existing failures on `main` (unrelated, replicated on the base commit):

- `authoring::tests::validate_draft_reports_missing_agent_for_fresh_template`
- `eval::postprocess::tests::extract_and_record_*` (3 tests)
- `eval_attestation::run_store_*` (2 tests)
- `eval_findings::run_store_record_finding_and_read_findings_round_trip`
- `eval_run_scenario::backtest_missing_cache_and_fixture_returns_actionable_validation`
- `api_eval_run::*` (4 tests)

These are migration / fixture / test-isolation issues on `main` that
existed before this branch was cut; not in scope for this track.

# Out of scope (deliberate)

- Bounded retry. The board says "A bounded retry is acceptable only if it
  is explicitly idempotent and recorded in run events." This PR does not
  add retry; it fails fast on the first invalid trader output to keep the
  order-safety property simple and verify the new diagnostics surface. A
  follow-up can add a structured retry helper that records each attempt's
  class on the run-events stream.
- Stream-abort detection across the `LlmDispatch` trait. Reqwest currently
  raises generic `anyhow::Error` for transport faults; we string-classify
  these into `provider_timeout` / `provider_connect` / `provider_http_error`.
  A typed `LlmDispatchError` would be cleaner but is a bigger surface
  change and overlaps with `qa10-stop-eval-run-control`.
- UI surfacing of the `[<class>]` prefix. The persisted shape is stable
  for future eval-review / run-detail consumers; this PR does not change
  the dashboard render.

# Next up

- Open PR against `main`.
- Append a one-line entry to the execution board's track closeout once
  merged.
