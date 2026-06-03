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
  appends a **tab-delimited** segment (real `\t`, matching the existing
  delimiters in this function — not the spaces shown below for readability),
  consistent with the existing `key=value` line:

  ```
  …\ttokens_in=12400\ttokens_out=3100\tcost=$0.0421*
  ```

  - The trailing `*` marks `cost_estimate_complete = false` (cost is a lower
    bound), matching the existing asterisk convention in
    `eval/compare_format.rs:173`.
  - When `model_call_count == 0` (nothing landed yet, **or** a
    pre-observability run that never recorded model_calls), emit
    `tokens_in=n/a tokens_out=n/a cost=n/a` so an operator can distinguish
    "no signal" from "0 tokens".

- **JSON** — the emitted value changes from the bare `Run` to a sibling block.
  The `tokens` value is the existing `RunTokenTotals` struct serialized
  **directly** (it already derives `Serialize`), so the wire keys are its
  real field names — no bespoke wrapper type:

  ```json
  {
    "run": { "…existing Run…": "…" },
    "tokens": {
      "input_tokens": 12400,
      "output_tokens": 3100,
      "cost_usd_estimate": 0.0421,
      "cost_estimate_complete": false,
      "model_call_count": 18
    }
  }
  ```

  `input_tokens` / `output_tokens` / `cost_usd_estimate` are `null` when
  unknown. Note `cost_estimate_complete: false` paired with
  `cost_usd_estimate: null` means **"no signal, no claim"** (the zero-rows
  default) — not "incomplete estimate"; consumers distinguish the two via
  `model_call_count == 0`. Both the `--once` and streaming (per-poll) paths
  switch to this shape simultaneously. This is the one wire-shape change in
  this work (see Contract below).

### Surface 2 — `eval show` health card

`print_run_health_card` (`mod.rs:~1233`) already prints `tokens in=… out=…`
from the `RunReport` when both are present, and — **inside the
`if let Some(m) = run.metrics` block** — already prints a finalized
`cost $…` line from `m.inference_cost_quote_total`. To avoid a duplicate cost
line on completed runs, the new aggregate cost line is **only** rendered when
that finalized metrics-cost line is absent (i.e. running runs, where
`run.metrics` is `None` or `inference_cost_quote_total` is `None`):

```
tokens  in=12400 out=3100
cost    $0.0421*          # ← new line, running runs only; finalized runs keep their existing cost line
```

- Source: the `RunReport`'s `cost_usd_estimate` + `cost_estimate_complete`,
  same lower-bound asterisk convention.
- **Suppress** the new line entirely when `cost_usd_estimate` is `None`
  (no model_calls yet / pre-observability run) — do **not** print `cost n/a`.
  This matches the existing card idiom: the tokens line is already suppressed
  (the `_ => {}` arm) when both counts are `None`, rather than printing `n/a`.

No change to `--verbose` (already prints input/output/cost) or `--json`
(already carries the full `report`).

### Contract

`eval watch --json` currently emits the bare `Run` object. Changing it to
`{ run, tokens }` is a shape change governed by the cli-json-stdout-contract.
**Verified during design review:** no existing CLI test asserts the bare-`Run`
top-level shape — `json_stdout_contract.rs:106` only checks the watch output is
parseable JSON with no banner markers, and `exit_codes_eval.rs` only checks the
exit code. So no test is *broken* by the change. Implementation must instead:

1. **Add** a positive test: a run that exists, `--json --once`, asserting the
   top level has a `run` key and a `tokens` object carrying `input_tokens` and
   `cost_estimate_complete`. (The existing contract/exit tests still pass
   unchanged.)
2. Grep `scripts/` and any operator tooling for shell consumers that pipe
   `eval watch --json` (e.g. `jq` over a top-level `Run` field); update or note
   them. The UI consumes live data over its own HTTP/SSE path, not the CLI, so
   it is unaffected.
3. Keep `--once` semantics: a single JSON value to stdout when `--once` or the
   run is already terminal (still one value — now an object with two keys).

## Error / none handling

`aggregate_run_token_totals` is infallible by contract (returns the default on
any miss). Mid-run-before-first-call and pre-observability runs both render as
`n/a` (human) / `null` (JSON), never a panic or error exit.

## Testing (TDD)

- **Watch, human (pure unit test, no DB):** `print_run_status_line` takes a
  `&RunTokenTotals` value. Construct one directly: `RunTokenTotals::default()`
  → asserts the line renders `tokens_in=n/a tokens_out=n/a cost=n/a`; a
  populated value (with `cost_estimate_complete=false`) → asserts the line
  contains `tokens_in=12400`, `tokens_out=3100`, and `cost=$…*` (asterisk
  present). The test must NOT call `aggregate_run_token_totals` (no DB
  dependency).
- **Watch, JSON:** `--json --once` emits an object with a `run` key and a
  `tokens` object carrying `input_tokens` (number or null) and
  `cost_estimate_complete` (bool). Assert on the real `RunTokenTotals` field
  names, not the placeholder names.
- **Health card:** for a running run with cost, the new `cost $…*` line appears
  (asterisk when incomplete); for a run with no model_calls the cost line is
  **absent** (suppressed, not `n/a`); for a completed run with finalized
  metrics cost, exactly **one** cost line appears (the existing metrics line,
  not a duplicate).
- Add the positive `{run, tokens}` watch JSON test described in Contract above.

## Out of scope

- `eval list` (not selected — would widen blast radius to table format + tests).
- Any engine, observability, or schema change.
- Backfilling `Run.actual_*_tokens` mid-run (the aggregate is the source of
  truth; the row columns stay finalize-only).
