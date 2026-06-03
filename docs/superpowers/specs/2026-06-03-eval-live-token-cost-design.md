# Live token count + cost for running evals (CLI)

**Date:** 2026-06-03
**Status:** Approved (brainstorming) — pending Design Review Gate
**Surfaces:** `xvn eval watch`, `xvn eval show` (health card)

## Problem

When an eval run is in progress, the dashboard UI shows a live token count and
cost. The CLI cannot surface the same numbers:

- `xvn eval watch <run_id>` (`crates/xvision-cli/src/commands/eval/mod.rs:1266`)
  is the live-monitoring loop. Each poll it calls `eval::get()` → a `Run`, then
  `print_run_status_line()`, which prints status + trading metrics
  (return/sharpe/dd/trades/decisions) but **no tokens and no cost**.
- The `Run` row's `actual_input_tokens` / `actual_output_tokens` are only
  populated at *finalize*, so `eval watch --json` emits `null` for those
  mid-run — the observed "CLI can't pull that info".

## Why the UI can and the CLI doesn't

The live total is **not** read from the `Run` row. It is aggregated from
`model_calls` via `aggregate_run_token_totals(&pool, run_id)` in
`xvision-engine::eval::report`, which sums
`model_calls.{input_token_count, output_token_count, cost_usd}` through the
join `eval_runs → agent_runs → spans → model_calls`.

`SqliteRecorder::handle_event` inserts a `model_calls` row **per model call as
the run streams** (`crates/xvision-observability/src/sqlite.rs:257`), so the
aggregate grows live. `eval show --verbose` already uses this (via
`compute_run_report`); the UI reads the same data. The only gaps are:

1. `eval watch` never calls the aggregation.
2. `eval show`'s non-verbose health card prints tokens but drops cost.

No schema change, no engine change, no new query is required — the function and
data already exist.

## Design

### Data source (reused)

Both surfaces call the existing
`xvision_engine::eval::report::aggregate_run_token_totals(&ctx.db, &run.id)`,
which returns `RunTokenTotals { input_tokens, output_tokens, cost_usd_estimate,
cost_estimate_complete, model_call_count }` (already `pub`). It never errors:
on any join/DB miss it returns `RunTokenTotals::default()` (all `None`,
`model_call_count = 0`).

### Surface 1 — `eval watch`

`run_watch` (`mod.rs:1266`) fetches token totals each poll, after `eval::get`.

- **Human** — `print_run_status_line` gains a `&RunTokenTotals` parameter and
  appends a tab-delimited segment consistent with the existing `key=value`
  line:

  ```
  …  tokens_in=12400  tokens_out=3100  cost=$0.0421*
  ```

  - The trailing `*` marks `cost_estimate_complete = false` (cost is a lower
    bound), matching the existing asterisk convention in
    `eval/compare_format.rs`.
  - When `model_call_count == 0` (nothing landed yet), emit
    `tokens_in=n/a tokens_out=n/a cost=n/a` so an operator can distinguish
    "zero so far / not yet wired" from "0 tokens".

- **JSON** — the emitted value changes from the bare `Run` to a sibling block:

  ```json
  {
    "run":    { "…existing Run…": "…" },
    "tokens": {
      "input": 12400,
      "output": 3100,
      "cost_usd": 0.0421,
      "cost_estimate_complete": false,
      "model_call_count": 18
    }
  }
  ```

  `input` / `output` / `cost_usd` are `null` when unknown. This is the one
  backward-incompatible change in this work (see Contract below).

### Surface 2 — `eval show` health card

`print_run_health_card` (`mod.rs:~1233`) already prints `tokens in=… out=…`
from the `RunReport` when both are present. Add one cost line directly under
it, reusing the report's `cost_usd_estimate` + `cost_estimate_complete` with
the same lower-bound asterisk:

```
tokens  in=12400 out=3100
cost    $0.0421*
```

No change to `--verbose` (already prints input/output/cost) or `--json`
(already carries the full `report`).

### Contract

`eval watch --json` currently emits the bare `Run` object. Changing it to
`{ run, tokens }` is a shape change governed by the cli-json-stdout-contract.
Implementation must:

1. Grep the CLI test suite (`crates/xvision-cli/tests/`, notably
   `json_stdout_contract.rs`, `eval_cancel_cli.rs`, and any watch test) for
   assertions that parse watch's top-level JSON as a `Run`, and update them to
   the `{ run, tokens }` shape.
2. Keep `--once` semantics: a single JSON value to stdout when `--once` or the
   run is already terminal (still one value — now an object with two keys).

## Error / none handling

`aggregate_run_token_totals` is infallible by contract (returns the default on
any miss). Mid-run-before-first-call and pre-observability runs both render as
`n/a` (human) / `null` (JSON), never a panic or error exit.

## Testing (TDD)

- **Watch, human:** status line for a run with model_calls contains
  `tokens_in=` / `tokens_out=` / `cost=`; a run with no model_calls renders
  `n/a` for all three.
- **Watch, JSON:** `--json --once` emits an object with a `run` key and a
  `tokens` object carrying an `input` field (number or null) and
  `cost_estimate_complete` (bool).
- **Health card:** the cost line appears (with `*` when incomplete) for a run
  with cost; shows `n/a` (or is suppressed consistently with the tokens line)
  for a run without.
- **Pure formatting:** the line-rendering helper is unit-testable without a DB
  (token totals passed in as a value).
- Update any existing watch/contract test that asserts the old bare-`Run` JSON
  shape.

## Out of scope

- `eval list` (not selected — would widen blast radius to table format + tests).
- Any engine, observability, or schema change.
- Backfilling `Run.actual_*_tokens` mid-run (the aggregate is the source of
  truth; the row columns stay finalize-only).
