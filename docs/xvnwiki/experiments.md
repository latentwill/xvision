# Experiments

An **experiment** is a ledger row that persists one complete research cycle:
a question, the strategies and scenarios under test, the batch run that answered
it, the computed metrics summary, and the operator's conclusion and next step.
It is the durable record of "we ran this hypothesis against these conditions
and learned X."

Experiments are additive — they do not replace or modify eval runs or batches.
They wrap them. The same batch can be bound to at most one experiment.

For the hypothesis manifest that lives on the strategy side, see
[Strategies](/docs?slug=strategies).

## The loop

```
hypothesis on Strategy
  → xvn scenario ls / select (choose scenario ids)
  → xvn experiment run (orchestrator)
      ├─ creates ledger row
      ├─ runs batch (same path as xvn eval batch run)
      ├─ binds batch_id to row
      └─ computes + persists result_json
  → operator reads output / --compare table
  → xvn experiment update --conclusion "..." --next-recommendation "..."
  → repeat with next iteration
```

`xvn experiment run` is the fast path. For finer control — run the batch
first, then create the ledger row and bind manually:

```bash
xvn eval batch run --strategy <id> --scenarios sc_01K...,sc_01K... --wait
xvn experiment new --name "compression-q3" --strategy <id> \
  --scenarios sc_01K...,sc_01K... --question "Does compression-breakout work in low-vol?"
xvn experiment update <exp_id> --bind-batch <batch_id>
```

## Ledger schema

Migration `023_hypothesis_and_experiments.sql` creates the `experiments` table.

| Column | Type | Notes |
|---|---|---|
| `experiment_id` | TEXT PK | `exp_<ULID>` — immutable |
| `name` | TEXT NOT NULL | Short slug, e.g. `compression-q3-btc` |
| `question` | TEXT | 1–2 sentence research question; nullable |
| `strategy_ids` | TEXT NOT NULL | JSON array of strategy ids |
| `scenario_ids` | TEXT NOT NULL | JSON array of scenario ids |
| `batch_id` | TEXT | FK → `eval_batches.batch_id`; nullable until run |
| `decision_budget` | INTEGER | Operator intent cap; nullable |
| `result_json` | TEXT | JSON result summary; populated when batch finishes |
| `conclusion` | TEXT | Operator-written outcome summary; nullable |
| `next_recommendation` | TEXT | Operator-written next step; nullable |
| `created_at` | TEXT NOT NULL | ISO-8601 timestamp |
| `updated_at` | TEXT NOT NULL | ISO-8601 timestamp |

`decision_budget` records operator intent (decisions per scenario this
experiment was designed around) and is stored on the row for cross-experiment
comparability. It does not cap actual eval execution today — that is a
follow-on change to the eval pipeline.

## The orchestrator: `xvn experiment run`

`xvn experiment run` bundles four steps into one verb:

1. **Create** — inserts the experiment ledger row before the batch starts,
   so the `experiment_id` exists even if the batch partially fails.
2. **Run batch** — delegates to the same production path as
   `xvn eval batch run`. Dispatch is resolved per-strategy-slot, identical
   to standalone batch runs.
3. **Bind** — calls `update_experiment` to write the `batch_id` onto the row.
4. **Compute result** — derives `result_json` from the `BatchResult` via
   `ExperimentStore::set_result` and persists it.

```bash
xvn experiment run \
  --name "compression-btc-q3" \
  --question "Does compression-breakout hold in low-volatility BTC regimes?" \
  --strategy strat_01K... \
  --scenarios sc_01K...,sc_01K...,sc_01K... \
  --decision-budget 50 \
  --wait \
  --compare \
  --markdown
```

Selector mode (delegates to `xvn scenario select` logic):

```bash
xvn experiment run \
  --name "eth-daily-sweep" \
  --strategy strat_01K... \
  --assets ETH/USD \
  --timeframe 1440 \
  --count 6 \
  --regimes bull,trending \
  --wait \
  --json
```

### Flags

| Flag | Description |
|---|---|
| `--name <slug>` | Short name for the experiment (required) |
| `--question <text>` | Research question (optional) |
| `--strategy <id>` | Strategy id to run (required) |
| `--scenarios <id1,id2,...>` | Explicit comma-separated scenario ids |
| `--assets <a1,a2,...>` | Asset filter for selector mode |
| `--timeframe <minutes>` | Bar granularity for selector mode |
| `--target-decisions <N>` | Select scenarios closest to N decisions |
| `--same-decisions` | Select scenarios sharing the same decision count (requires `--max-decisions`) |
| `--max-decisions <N>` | Upper bound when `--same-decisions` is set |
| `--count <N>` | Number of scenarios to select in selector mode (default: 4) |
| `--regimes <r1,r2,...>` | Regime labels for selector mode |
| `--decision-budget <N>` | Operator intent cap recorded on the ledger row |
| `--wait` | Block until all runs complete (required for `--compare` and `result_json`) |
| `--review-with <profile>` | Agent profile id for post-run analytical reviews (requires `--wait`) |
| `--compare` | Render a compare-style table after the run (requires `--wait`) |
| `--markdown` | Emit compare output as GitHub-flavoured Markdown (requires `--compare`) |
| `--output <path>` | Write `--compare --markdown` output to a file instead of stdout |
| `--json` | Emit the final `ExperimentRunOutput` as JSON |

Either `--scenarios` or `--assets` must be provided; they are mutually exclusive.

## Output shape

`ExperimentRunOutput` — the JSON shape emitted by `--json`:

```json
{
  "experiment_id": "exp_01K...",
  "name": "compression-btc-q3",
  "question": "Does compression-breakout hold in low-volatility BTC regimes?",
  "strategy_ids": ["strat_01K..."],
  "scenario_ids": ["sc_01K...", "sc_01K...", "sc_01K..."],
  "batch_id": "batch_01K...",
  "decision_budget": 50,
  "result": {
    "profitable_count": 2,
    "best_scenario": "sc_01K...",
    "worst_scenario": "sc_01K...",
    "runs": [
      {
        "scenario_id": "sc_01K...",
        "scenario_name": "BTC/USD 4h low-vol 2024-Q1",
        "run_id": "run_01K...",
        "status": "completed",
        "return_pct": 3.14,
        "sharpe": 0.821,
        "drawdown_pct": -4.2,
        "decisions": 48
      }
    ]
  },
  "conclusion": null,
  "next_recommendation": null,
  "compare_markdown": "..."
}
```

`compare_markdown` is omitted unless `--compare --markdown` is requested.
`conclusion` and `next_recommendation` are `null` until the operator fills
them in via `xvn experiment update`.

## CLI parity

See [CLI Reference](/docs?slug=cli-reference) for full flag documentation.

- `xvn experiment new` (`create`) — create a ledger row manually.
- `xvn experiment run` — orchestrator: create + batch + bind + result in one shot.
- `xvn experiment ls` — list all experiments, most-recent first.
- `xvn experiment show <id>` (`get`) — show a single experiment; use `--json`
  to view `result_json`.
- `xvn experiment update <id>` — apply partial mutations: `--conclusion`,
  `--next-recommendation`, `--bind-batch <batch_id>`.

## What it is not

The following are explicitly out of scope today:

- **`--scenario-set <name>`** — named, saved scenario sets. Would require a
  persisted `scenario_sets` table punted in wave B. The flag is reserved but
  not wired.
- **Auto-recommendation** — automatic generation of the next hypothesis from
  the current result. `next_recommendation` is an operator-written text field;
  no LLM step writes to it automatically today.
