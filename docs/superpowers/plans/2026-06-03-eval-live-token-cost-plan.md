# Implementation Plan: live token count + cost for running evals

**Spec:** `docs/superpowers/specs/2026-06-03-eval-live-token-cost-design.md`
**Branch:** `feat/eval-live-token-cost`
**Build host:** local macOS dev box — `cargo` is permitted here (CLAUDE.md
forbids cargo only on remote/deploy hosts).
**Method:** TDD (RED → GREEN → REFACTOR). Single work unit; the change is
contained to `crates/xvision-cli/src/commands/eval/mod.rs` + its test crate.

## Files in scope

- `crates/xvision-cli/src/commands/eval/mod.rs` (edit: imports, `run_watch`,
  `print_run_status_line`, `print_run_health_card`)
- `crates/xvision-cli/tests/eval_watch_tokens.rs` (new integration test) and/or
  a unit-test module inside `mod.rs` for the pure formatter.
- Possibly `crates/xvision-cli/tests/json_stdout_contract.rs` — only if a new
  positive case belongs there (existing cases stay unchanged; verified no
  break).

## Reused engine API (no engine change)

`xvision_engine::eval::report::{aggregate_run_token_totals, RunTokenTotals}` —
`aggregate_run_token_totals(&pool, run_id) -> RunTokenTotals`, infallible
(returns `RunTokenTotals::default()` on any miss). `RunTokenTotals` already
derives `Serialize`/`Deserialize` and is `pub`.

## Work unit: CLI live token/cost surfacing

### Step 1 — RED: pure formatter unit test (no DB)

Refactor the status-line rendering into a pure, testable helper so the watch
loop's I/O is separable from formatting.

- Introduce `fn render_run_status_line(run: &Run, tokens: &RunTokenTotals) -> String`
  (returns the line; `print_run_status_line` becomes a thin `println!` wrapper,
  or is replaced at the one call site).
- Add `#[cfg(test)] mod status_line_tests` in `mod.rs`:
  - `default_totals_render_na`: `RunTokenTotals::default()` → line contains
    `tokens_in=n/a`, `tokens_out=n/a`, `cost=n/a`.
  - `populated_totals_render_values_with_asterisk`: a manually constructed
    `RunTokenTotals { input_tokens: Some(12400), output_tokens: Some(3100),
    cost_usd_estimate: Some(0.0421), cost_estimate_complete: false,
    model_call_count: 18 }` → line contains `tokens_in=12400`,
    `tokens_out=3100`, and `cost=$0.0421*` (asterisk present).
  - `complete_cost_no_asterisk`: same but `cost_estimate_complete: true` →
    `cost=$0.0421` (no trailing `*`).
  - Fields are tab-delimited (`\t`), consistent with existing line.
  - Tests construct `RunTokenTotals` directly — **no** call to
    `aggregate_run_token_totals`, no DB.

Run `cargo test -p xvision-cli status_line` → expect RED (helper absent).

### Step 2 — GREEN: implement the formatter + wire the watch loop

- Implement `render_run_status_line`: build the existing id/status/mode/scenario
  + metrics + error segments, then append the tokens segment:
  - if `tokens.model_call_count == 0`:
    `\ttokens_in=n/a\ttokens_out=n/a\tcost=n/a`
  - else: `\ttokens_in={in}\ttokens_out={out}\tcost={cost}` where each numeric
    field falls back to `n/a` if its `Option` is `None`, and `cost` is
    `$<.4>` with a trailing `*` iff `!cost_estimate_complete`, or `n/a` if
    `cost_usd_estimate` is `None`.
- In `run_watch`, after `eval::get`, call
  `let tokens = aggregate_run_token_totals(&ctx.db, &args.run_id).await;` each
  poll.
- Human branch: `println!("{}", render_run_status_line(&run, &tokens));`
- JSON branch: replace `print_json(&run)` with
  `print_json(&serde_json::json!({ "run": run, "tokens": tokens }))`.
- Add import: extend the `report::` use to
  `report::{aggregate_run_token_totals, compute_run_report, RunTokenTotals}`.

Run `cargo test -p xvision-cli status_line` → expect GREEN.

### Step 3 — RED→GREEN: watch JSON integration test

Add `crates/xvision-cli/tests/eval_watch_tokens.rs`:
- Mirror the harness used by existing eval CLI tests (e.g. `eval_cancel_cli.rs`
  / `exit_codes_eval.rs`) to create a run, then invoke
  `xvn eval watch <id> --json --once`.
- Assert the parsed JSON has a top-level `run` object and a `tokens` object,
  and that `tokens` contains the key `cost_estimate_complete` (bool) and an
  `input_tokens` key (number or null).
- If creating a real run in-test is heavy, fall back to asserting the shape on
  a known-seeded run id via the same DB fixture the sibling tests use. Match
  the existing test's setup exactly — do not invent a new fixture pattern.

### Step 4 — RED→GREEN: health card cost line

- Add to `print_run_health_card`, in the `if let Some(rpt) = report` block,
  immediately after the tokens-line `match`:
  ```rust
  let finalized_cost_shown = run
      .metrics
      .as_ref()
      .and_then(|m| m.inference_cost_quote_total)
      .is_some();
  if !finalized_cost_shown {
      if let Some(c) = rpt.cost_usd_estimate {
          let star = if rpt.cost_estimate_complete { "" } else { "*" };
          println!("cost    ${c:.4}{star}");
      }
  }
  ```
- This renders the aggregate cost line only when the finalized metrics-cost
  line did not fire (running runs), and suppresses entirely when there is no
  cost signal. No duplicate line on completed runs.
- Test: a unit/integration test asserting:
  - running run (metrics None) with `rpt.cost_usd_estimate = Some(..)`,
    incomplete → one `cost    $…*` line.
  - run with `rpt.cost_usd_estimate = None` → no `cost` line emitted by the
    report block.
  - completed run with finalized `inference_cost_quote_total = Some` → exactly
    one `cost` line (the metrics one), aggregate line suppressed.
  - If `print_run_health_card`'s `println!` I/O makes assertion awkward,
    extract a pure `fn render_health_card(run, report) -> String` mirroring the
    status-line refactor and test that.

### Step 5 — Contract sweep

- `grep -rn "eval watch" scripts/` and the repo for shell consumers that `jq`
  over a top-level `Run` field of `eval watch --json`; update or note any.
- Confirm `cargo test -p xvision-cli` passes whole (no pre-existing watch/
  contract test broke).

## Definition of Done

1. `xvn eval watch <id>` human output shows `tokens_in=`/`tokens_out=`/`cost=`
   each poll, with `n/a` when no model_calls and `*` on incomplete cost.
2. `xvn eval watch <id> --json --once` emits `{ run, tokens }` with the
   `RunTokenTotals` fields.
3. `xvn eval show <id>` (non-verbose health card) shows a live `cost` line for
   running runs, suppressed when no cost signal, and never duplicated on
   completed runs.
4. New tests cover: formatter n/a + populated + asterisk cases; watch JSON
   shape; health-card cost line (running / none / completed).
5. `cargo test -p xvision-cli` green; `cargo clippy -p xvision-cli` clean;
   `cargo fmt` applied.
6. No engine/observability/schema change; no unrelated files committed.

## Validation commands (local build host)

```bash
cargo fmt -p xvision-cli
cargo test -p xvision-cli
cargo clippy -p xvision-cli --all-targets -- -D warnings
```

## Risks / mitigations

- **Watch `--json` shape change** — verified no existing test asserts the
  bare-`Run` shape; mitigation is additive (new positive test) + scripts sweep.
- **Health-card double cost line** — gated on `finalized_cost_shown`; covered
  by the completed-run test.
- **Test harness for a live run** — reuse the exact fixture/util the sibling
  eval CLI tests use rather than inventing one; if a real run is impractical in
  a unit test, the pure-formatter tests (Steps 1/4) carry the behavioral
  coverage and the integration test asserts only wire shape.
