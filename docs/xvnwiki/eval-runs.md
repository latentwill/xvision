# Eval Runs

An **eval run** is one strategy executed against one scenario in one
mode (backtest or paper). Each run is a row in `eval_runs` keyed by an
immutable ULID.

## Lifecycle

```
queued → running → completed | failed | cancelled
```

- **queued** — the run is in `RunStore` and the scheduler has not
  picked it up yet.
- **running** — the engine is iterating through the scenario's bars,
  emitting decisions. The dashboard streams progress via SSE.
- **completed** — all bars processed; metrics finalised.
- **failed** — a typed failure class is recorded (see Failure classes
  below). The detail page surfaces the class and the underlying error.
- **cancelled** — operator hit Stop; partial decisions are retained.

## Decisions surface

For each bar, the engine produces one decision row containing:

- the bar timestamp, OHLCV snapshot;
- the `TraderDecision` (action, qty, rationale, references);
- the `RiskDecision` (Approved / Modified / Vetoed + reason);
- the executor outcome (filled / rejected / no-op);
- realised + unrealised PnL after this bar.

The decisions list paginates lazily; scroll-bottom triggers a fetch.
The flame graph + span inspector in the trace dock are anchored on the
selected decision (use the per-row link to jump to a span).

## Batch runs

A batch launches one run per scenario for a single strategy and waits
for all runs to reach a terminal state.

```bash
# Launch, wait, and print a side-by-side summary table
xvn eval batch run \
  --strategy <strategy-id> \
  --scenarios sc_01K...,sc_01K...,sc_01K... \
  --wait \
  --json

# Trigger an auto-review after each completed run
xvn eval batch run \
  --strategy <strategy-id> \
  --scenarios sc_01K...,sc_01K... \
  --wait \
  --review-with reasoning-agent

# Poll a persisted batch by id
xvn eval batch status <batch-id>

# Side-by-side compare all runs in a batch
xvn eval compare --batch <batch-id> --markdown
```

The `--wait` flag is required; the command blocks until every run
reaches `completed`, `failed`, or `cancelled`. The response is a single
`BatchResult` object containing:

| Field | Description |
|---|---|
| `batch_id` | Stable ULID assigned at batch creation |
| `strategy_id` | Strategy agent id |
| `runs[]` | One `RunEntry` per scenario |

Each `RunEntry` carries: `scenario_id`, `scenario_name`, `run_id`,
`status`, `return_pct`, `sharpe`, `drawdown_pct`, `decisions`,
`actions` (action-kind counts), `error`, and optionally `review`.

When `--review-with <profile>` is set, each completed run is reviewed
in sequence after all runs finish. The `review` field in the
`RunEntry` holds `review_id`, `status`, `summary`, `verdict`, and
`error`. Failed reviews do not abort the batch.

`xvn eval batch status <batch-id>` shows the persisted batch row and
the list of attached run ids. Combine with `xvn eval compare --batch
<batch-id>` for a full metrics breakdown without re-typing run ids.

## Comparing runs

```bash
# Positional run ids
xvn eval compare <run-id-1> <run-id-2> <run-id-3>

# Flag form; comma-separated list is accepted
xvn eval compare --runs <id1>,<id2>,<id3> --markdown

# Machine-readable JSON
xvn eval compare --runs <id1>,<id2> --json

# All runs from a batch
xvn eval compare --batch <batch-id> --markdown

# Sort by a specific metric (return | sharpe | drawdown)
xvn eval compare --batch <batch-id> --sort sharpe --markdown
```

`--markdown` emits a GitHub-flavoured Markdown table. The table
includes a **Baseline (buy_hold)** column — the strategy return minus
the buy-and-hold return for that bar slice — alongside Return, Sharpe,
Max DD, Decisions, Trades, Flips, Avg hold (bars), Flat rate,
Reentries, and Failure mode.

`/eval-runs/compare?ids=...` in the dashboard opens the same
side-by-side equity chart view.

## Baseline auto-comparison

Every completed backtest automatically runs four baseline arms over the
same bar slice the strategy saw: buy-and-hold, always-flat,
simple-trend, and simple-mean-reversion (via `xvision-eval`). Results
are stored in `MetricsSummary.baselines` (inside the existing
`metrics_json` column; no migration required — old rows deserialise
with `baselines: null`).

The `BaselinesReport` shape:

| Path | Type | Description |
|---|---|---|
| `baselines.buy_hold.return_pct` | f64 | Buy-and-hold total return % |
| `baselines.buy_hold.sharpe` | f64 | Buy-and-hold Sharpe |
| `baselines.always_flat.return_pct` | f64 | Always-flat return % |
| `baselines.always_flat.sharpe` | f64 | Always-flat Sharpe |
| `baselines.simple_trend.return_pct` | f64 | Simple-trend return % |
| `baselines.simple_trend.sharpe` | f64 | Simple-trend Sharpe |
| `baselines.simple_mean_reversion.return_pct` | f64 | Simple mean-reversion return % |
| `baselines.simple_mean_reversion.sharpe` | f64 | Simple mean-reversion Sharpe |
| `baselines.relative_to.buy_hold` | f64 | strategy − buy_hold (return_pct delta) |
| `baselines.relative_to.always_flat` | f64 | strategy − always_flat delta |
| `baselines.relative_to.simple_trend` | f64 | strategy − simple_trend delta |
| `baselines.relative_to.simple_mean_reversion` | f64 | strategy − simple_mean_reversion delta |

Positive `relative_to.*` values mean the strategy beat that baseline
on raw total return. The compare Markdown report surfaces
`relative_to.buy_hold` as the **Baseline (buy_hold)** column.

Paper-mode runs do not produce baseline arms (bars are not available
post-hoc); `baselines` is `null` for those rows.

## Behavior summary

For every completed run the engine can derive a behavior summary
on-demand from the existing decision rows — no DB write, no migration.

```bash
# Human-readable behavior block appended to the normal show output
xvn eval show <run-id> --behavior

# Wrapped JSON: {"run": ..., "behavior_summary": ...}
xvn eval show <run-id> --behavior --json
```

The dashboard surfaces the same fields in the **Behavior** tab of the
run detail page.

| Field | Type | Description |
|---|---|---|
| `flat_rate` | f64 | Fraction of decisions that are `flat` or `hold` (0–1) |
| `trades_opened` | u32 | Count of `long_open` + `short_open` decisions |
| `direct_flips` | u32 | Opposite-direction opens without a `flat` in between |
| `avg_bars_held` | f64? | Mean bars between an open and the next `flat` per asset; `null` when no complete round-trips |
| `reentries_after_loss` | u32 | Opens immediately following a `flat` with `pnl_realized < 0` on the same asset |
| `exits_on_invalidation` | u32 | `flat` decisions with `pnl_realized < 0` |
| `primary_failure_mode` | string | Heuristic label: `late_entries`, `churn`, `no_edge`, `over_flat`, `none_obvious` |

`primary_failure_mode` rules (first match wins):

| Label | Condition |
|---|---|
| `late_entries` | `reentries_after_loss / max(1, trades) > 0.4` |
| `churn` | `direct_flips / max(1, trades) > 0.2` |
| `no_edge` | trades > 0 and `exits_on_invalidation / max(1, trades) > 0.5` |
| `over_flat` | `flat_rate > 0.85` |
| `none_obvious` | fallthrough |

The `compare --markdown` report includes behavior columns (Trades,
Flips, Avg hold, Flat rate, Reentries, Failure mode) derived inline
from each run's decision rows.

## Review agent

A review pass sends the strategy's decisions, metrics, and scenario
context to an LLM agent for analytical commentary.

```bash
# Run a review after the run completes
xvn eval review <run-id> --agent reasoning-agent

# JSON output (full EvalReview + findings)
xvn eval review <run-id> --agent reasoning-agent --format json

# Write JSON to a file
xvn eval review <run-id> --agent reasoning-agent --output review.json

# Force a fresh review even if one already exists for this (run, profile)
xvn eval review <run-id> --agent reasoning-agent --force
```

The `--agent` flag accepts any agent profile id configured in the
workspace (`fast-trader-agent`, `reasoning-agent`, `risk-agent`,
`research-agent`, or an operator-defined profile). The reviewer is
itself an agent — provider and model are resolved from the named
profile's `provider` column, using the same posture as the chat rail.

A prior `failed` review for the same (run, profile) pair is
retry-eligible; the CLI will dispatch a fresh attempt rather than
returning the stale failure.

The review output carries:

| Field | Description |
|---|---|
| `id` | Review ULID |
| `status` | `complete` or `failed` |
| `verdict` | `strong_pass`, `pass`, `inconclusive`, `fail`, `strong_fail` |
| `score` | Numeric score (provider-dependent scale) |
| `confidence` | 0–1 confidence in the verdict |
| `summary` | Free-text analytical summary |
| `findings[]` | Structured finding records with severity, kind, title, description, recommendation |

Dashboard equivalent: `/eval-runs/<id>` → **Review** panel. The panel
shows the same fields plus a findings list with severity badges.

## Retry, Rerun, and Delete

- **Retry** — supported on `failed` and `cancelled` runs. Spawns a
  new run with the same
  `(agent_id, scenario_id, mode, params_override)` fingerprint under a
  fresh id. Tagged internally as `FailureRecovery`.
- **Rerun** — supported on `completed` runs. Same action surface as
  Retry; spawns a new run with the same fingerprint and is tagged
  `ManualRerun`. Use it to repeat a known-good run (e.g. against a
  refreshed bar cache) without re-entering parameters.
- **Idempotency** — both Retry and Rerun are coalesced: if a sibling
  with the same fingerprint is already `queued` or `running`, the
  existing run is returned instead of starting a duplicate. Safe to
  double-click the button.
- **Delete** — available in the eval inspector. Deletes the
  `eval_runs` row, decisions, and trace artifacts.

`queued` and `running` runs cannot be retried or rerun — wait for them
to terminate first, or cancel them.

## Failure classes

`failed` runs carry a `[<class>]` prefix on the `error` field. The
dashboard filters by class on the eval-runs list.

**Trader output** (model returned something the engine could not parse):

`empty`, `tool_use_only`, `truncated`, `invalid_json`, `missing_field`,
`invalid_field`, `missing_response`

**Provider transport** (network or API-level failures):

`provider_timeout`, `provider_connect`, `provider_http_error`,
`provider_decode`, `provider_rate_limited`, `provider_missing_choices`

**Broker** (order execution failures):

`broker_rejected`, `broker_auth`, `broker_unsupported`,
`broker_insufficient_funds`, `broker_timeout`

**Loop control**:

`repeated_broker_error` — circuit breaker tripped after N consecutive
identical recoverable broker rejections.

**Catch-all**:

`unclassified` — anything the classifier did not match.
