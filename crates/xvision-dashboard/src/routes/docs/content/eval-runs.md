# Eval Runs

An **eval run** is one strategy executed against one scenario in one
mode (backtest, paper, or live). Each run is a row in
`eval_runs` keyed by an immutable ULID.

## Lifecycle

```
queued → running → completed | failed | cancelled
```

- **queued** — the run is in `RunStore` and the scheduler has not
  picked it up yet.
- **running** — the engine is iterating through the scenario's bars,
  emitting decisions. The dashboard streams progress via SSE.
- **completed** — all bars processed; metrics finalised.
- **failed** — a typed `RunFailure` is recorded (e.g.
  `broker_rejected`, `provider_unavailable`, `unclassified`). The
  detail page surfaces the failure class and the underlying error.
- **cancelled** — operator hit Stop; partial decisions are retained.

## Decisions surface

For each bar, the engine produces one decision row containing:

- the bar timestamp, OHLCV snapshot;
- the `TraderDecision` (action, qty, rationale, references);
- the `RiskDecision` (Approved / Modified / Vetoed + reason);
- the executor outcome (filled / rejected / no-op);
- realised + unrealised PnL after this bar.

The decisions list paginates lazily; scroll-bottom triggers a fetch.
The flame graph + span inspector in the trace dock are anchored on
the selected decision (use the per-row link to jump to a span).

## Comparing runs

`/eval-runs/compare?ids=...` opens a side-by-side equity chart for
two or more runs. Use it to spot regressions when iterating on a
strategy: arms of the same `ab-compare` invocation land here.

## Retry and Delete

- **Retry** — supported on `failed` and `cancelled` runs. Spawns a
  new run with the same `(agent_id, scenario_id, mode)` fingerprint
  under a fresh id.
- **Delete** — available in the eval inspector. Deletes the
  `eval_runs` row, decisions, and trace artifacts.

## Failure classes

`failed` runs are tagged with one of:

`provider_unavailable`, `broker_rejected`, `broker_auth`,
`broker_unsupported`, `broker_insufficient_funds`, `broker_timeout`,
`scenario_invalid`, `tool_loop_exceeded`, `unclassified`.

The dashboard filters by class on the eval-runs list.
