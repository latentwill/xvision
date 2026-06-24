# Experiments

An **experiment** is a structured research record that pairs a question with
the run that answered it. It captures: the hypothesis you pinned on the
strategy, the comparator scenario set you chose, the decision budget you
designed around, the result summary after the batch finished, and — crucially
— the conclusion and next recommendation you write in afterwards. Later passes
(human or agent) can read the record and replay the reasoning, not just the
numbers.

Experiments are additive — they wrap batches, they do not replace or modify
eval runs. The same batch can be bound to at most one experiment.

Experiments can also run in **forward-test mode**: instead of replaying
historical bars through a scenario, the experiment subscribes to a live
market feed and executes real-time decisions through the strategy's filter
pipeline. The filter timeline — every filter evaluation event and its
outcome (pass / skip / delay) — is recorded as an observable signal
alongside the decision trace. Delayed decisions (those deferred by a
time-window or count-based cooldown) appear as distinct events in the
result summary, giving the operator a full picture of when and why the
strategy acted or held back.

For the hypothesis manifest that lives on the strategy, see
[Strategies](/docs?slug=strategies). For eval batches and individual runs, see
[Eval Runs](/docs?slug=eval-runs).

---

## The loop

Five steps, repeatable indefinitely:

1. **Pin a hypothesis on the strategy** — write a question you want to answer
   about one strategy's behaviour. See `xvn strategy create --hypothesis` in
   [Strategies](/docs?slug=strategies).
2. **Pick a comparator scenario set** — use `xvn scenario select` to filter
   the library by asset, timeframe, regime, and decision count, or supply
   explicit scenario ids. See [Scenarios](/docs?slug=scenarios).
3. **`xvn experiment run`** — the one-shot orchestrator. It creates the
   experiment record, runs the batch, binds the batch to the record, and writes
   the result summary, all in a single CLI call.
4. **Read the compare table and reviewer output** — inspect the metrics table
   that `--compare` prints, or the reviewer's findings when `--review-with` is
   set. Open the dashboard's eval-runs compare view alongside if you want to
   drill into individual decisions.
5. **Conclude the experiment** — write your interpretation into the record with
   `xvn experiment update --conclusion "..." --next-recommendation "..."`. Now
   the record is complete and the next pass has something to build on.

Repeat from step 1 with the next hypothesis.

---

## `xvn experiment run` — the one-shot orchestrator

`xvn experiment run` bundles four steps into a single CLI call:

1. Creates the experiment record before the batch starts, so the experiment id
   exists even if the batch partially fails.
2. Runs the batch using the same production path as `xvn eval batch run` —
   dispatch is resolved per strategy slot, identical to standalone batch runs.
3. Binds the completed batch id to the experiment record.
4. Computes the result summary from the batch output and writes it to the
   experiment record.

### Explicit scenario ids

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

### Selector mode

When you don't have ids in hand, delegate scenario selection to the library
filter. This is equivalent to calling `xvn scenario select` and feeding the
result directly into the run:

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

Either `--scenarios` or `--assets` must be provided; they are mutually
exclusive.

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
| `--decision-budget <N>` | Operator intent cap recorded on the experiment record |
| `--wait` | Block until all runs complete (required for `--compare` and the result summary) |
| `--review-with <profile>` | Agent profile id for post-run analytical reviews (requires `--wait`) |
| `--compare` | Render a compare-style table after the run (requires `--wait`) |
| `--markdown` | Emit compare output as GitHub-flavoured Markdown (requires `--compare`) |
| `--output <path>` | Write `--compare --markdown` output to a file instead of stdout |
| `--json` | Emit the final experiment output as JSON |

---

## Reading an experiment

```bash
xvn experiment show <id>
xvn experiment show <id> --json
```

The `--json` flag emits the full experiment record. Example output with
user-visible fields:

```json
{
  "id": "exp_01K...",
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
  "next_recommendation": null
}
```

`conclusion` and `next_recommendation` are `null` until the operator fills
them in via `xvn experiment update`.

---

## Concluding an experiment

```bash
xvn experiment update <id> \
  --conclusion "Compression-breakout holds in low-vol regimes with a mean return of +3.1%, but breaks down in high-vol ranging periods." \
  --next-recommendation "Retest with a tighter entry filter (ATR < 0.8%) and add a vol-regime pre-check."
```

This is how the loop completes. Writing the conclusion and next recommendation
into the record closes the current iteration and gives the next pass — whether
a human operator or an agent reviewing past experiments — a concrete starting
point rather than raw numbers.

`--bind-batch <batch_id>` is also available on `update` for the manual flow
where you ran the batch separately and want to attach it to an existing
experiment record.

---

## CLI verbs at a glance

See [CLI Reference](/docs?slug=cli-reference) for full flag documentation.

| Verb | Effect |
|---|---|
| `xvn experiment new` (`create`) | Create an experiment record manually without running a batch. |
| `xvn experiment run` | Orchestrator: create record → run batch → bind batch → write result summary. |
| `xvn experiment ls` | List all experiments, most-recent first. |
| `xvn experiment show <id>` (`get`) | Show a single experiment; add `--json` to see the full record including result summary. |
| `xvn experiment update <id>` | Apply partial mutations: `--conclusion`, `--next-recommendation`, `--bind-batch <id>`. |

---

## What's out of scope today

- **Named scenario sets** — `--scenario-set <name>` is not implemented. Named,
  saved scenario sets require additional persistence work that has been
  deferred. The flag is reserved but not wired.
- **Auto-recommended next hypothesis** — `next_recommendation` is an
  operator-written field. No automated step generates or fills it today.
