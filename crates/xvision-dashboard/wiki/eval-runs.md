# Eval Runs

An **eval run** is one strategy executed against one scenario in one mode
(backtest, paper, or live). Each run is a row in the dashboard's eval list
with a stable ULID that never changes after creation.

---

## Lifecycle

```
queued → running → completed | failed | cancelled
```

- **queued** — the run is registered and waiting for the engine to pick it
  up.
- **running** — the engine is iterating through the scenario's bars and
  emitting decisions. The dashboard streams progress live.
- **completed** — all bars processed; metrics are finalised.
- **failed** — the run stopped with an error; a failure class is recorded
  and the dashboard filters on it. See [Failure classes](#failure-classes)
  below.
- **cancelled** — the operator hit Stop; partial decisions are retained.

---

## Decisions surface

For each bar the engine produces one decision row containing:

- bar timestamp and OHLCV snapshot
- trader action, quantity, rationale, and references
- risk verdict: Approved, Modified, or Vetoed, plus reason
- executor outcome: filled, rejected, or no-op
- realised and unrealised PnL after this bar

The decisions list paginates lazily; scrolling to the bottom fetches the
next page. The trace dock (flame graph + span inspector) is anchored on the
selected decision — use the per-row link to jump to a span.

Dashboard route: `/eval-runs/<id>`

---

## Comparing runs

Open a side-by-side equity chart in the dashboard:

```
/eval-runs/compare?ids=<id1>,<id2>,...
```

Or use the CLI for a Markdown table suitable for pasting into a PR or chat:

```bash
# Two or more run ids as positional arguments
xvn eval compare <id-1> <id-2> <id-3>

# Flag form; comma-separated list is also accepted
xvn eval compare --runs <id1>,<id2>,<id3> --markdown

# Machine-readable JSON
xvn eval compare --runs <id1>,<id2> --json

# All runs from a batch
xvn eval compare --batch <batch-id> --markdown

# Sort by a specific metric: return | sharpe | drawdown (default: return)
xvn eval compare --batch <batch-id> --sort sharpe --markdown
```

The `--markdown` table columns are: **Scenario | Return | Baseline
(buy_hold) | Sharpe | Max DD | Decisions | Trades | Flips | Avg hold
(bars) | Flat rate | Reentries | Failure mode**. The **Baseline
(buy_hold)** column shows the strategy's return delta versus the
buy-and-hold baseline, so you can tell at a glance whether the strategy
beat the trivial passive alternative.

---

## Baselines

Every completed backtest automatically runs four baseline strategies over
the same bar slice the strategy saw: buy-and-hold, always-flat,
simple-trend, and simple-mean-reversion. The compare table shows the
strategy's return delta against each. Paper-mode runs do not produce
baseline arms; the baseline columns show `-` for those rows.

---

## Behavior summary

After a run completes the engine can derive a short summary of **how** the
strategy traded — not just whether it made money. The summary describes
what fraction of bars the strategy was flat, how many trades it opened, how
often it flipped direction without going flat in between, the average number
of bars held per trade, how often it re-entered after a losing exit, and how
often it exited on an invalidation. A heuristic failure-mode label
(`late_entries`, `churn`, `no_edge`, `over_flat`, or `none_obvious`) is
also derived and surfaced in the compare table's **Failure mode** column.

The behavior summary is computed on demand from the existing decision rows —
there is no extra database write and no migration needed.

```bash
# Human-readable behavior block appended to the normal show output
xvn eval show <run-id> --behavior

# JSON: {"run": ..., "behavior_summary": {...}}
xvn eval show <run-id> --behavior --json
```

Dashboard equivalent: the **Behavior** tab on the run detail page.

---

## Batch runs

A batch launches one run per scenario for a single strategy and waits for
all runs to reach a terminal state.

```bash
# Launch, block until all terminal, and print a summary table
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

# Check the status of a persisted batch
xvn eval batch status <batch-id>

# Compare all runs in a batch
xvn eval compare --batch <batch-id> --markdown
```

The `--wait` flag blocks until every run reaches `completed`, `failed`, or
`cancelled`. The JSON output is a single batch object with:

| Field | Description |
|---|---|
| `batch_id` | Stable ULID assigned at batch creation |
| `strategy_id` | Strategy id |
| `runs[]` | One entry per scenario |

Each run entry carries: `scenario_id`, `scenario_name`, `run_id`, `status`,
`return_pct`, `sharpe`, `drawdown_pct`, `decisions`, `actions` (action-kind
counts), `error`, and — when `--review-with` was set — a `review` object
with `review_id`, `status`, `summary`, `verdict`, and `error`.

When `--review-with <profile>` is set, each completed run is reviewed in
sequence after all runs finish. A failed review does not abort the batch.

`xvn eval batch status <batch-id>` shows the persisted batch row and the
list of attached run ids. Combine with `xvn eval compare --batch <batch-id>`
to get a full metrics breakdown without retyping run ids.

---

## Review

A review pass sends the strategy's decisions, metrics, and scenario context
to an LLM agent for analytical commentary.

```bash
# Run a review after the run completes
xvn eval review <run-id> --agent reasoning-agent

# Full output as JSON (review + findings)
xvn eval review <run-id> --agent reasoning-agent --format json

# Write the JSON to a file
xvn eval review <run-id> --agent reasoning-agent --output review.json

# Force a fresh review even if one already exists for this (run, profile)
xvn eval review <run-id> --agent reasoning-agent --force
```

The `--agent` flag accepts any agent profile id configured in the workspace
(`fast-trader-agent`, `reasoning-agent`, `risk-agent`, `research-agent`, or
an operator-defined profile). Provider and model are resolved from the named
profile.

A prior `failed` review for the same (run, profile) pair is retry-eligible;
the CLI dispatches a fresh attempt rather than returning the stale failure.

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

Dashboard equivalent: `/eval-runs/<id>` → **Review** panel. The panel shows
the same fields plus a findings list with severity badges.

---

## Retry, Rerun, and Delete

- **Retry** — available on `failed` and `cancelled` runs. Spawns a new run
  with the same `(strategy, scenario, mode, params)` fingerprint under a
  fresh id.
- **Rerun** — available on `completed` runs. Same action as Retry; use it
  to repeat a known-good run (for example, against a refreshed bar cache)
  without re-entering parameters.
- **Idempotency** — both Retry and Rerun are coalesced: if a sibling with
  the same fingerprint is already `queued` or `running`, the existing run is
  returned instead of starting a duplicate. Safe to double-click the button.
- **Delete** — available in the eval inspector. Removes the run row,
  decisions, and trace artifacts.

`queued` and `running` runs cannot be retried or rerun — wait for them to
reach a terminal state first, or cancel them.

---

## Failure classes

`failed` runs carry a `[<class>]` prefix on the `error` field. The
dashboard's eval-runs list can filter by class.

**Trader output** (the model returned something the engine could not use):

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
