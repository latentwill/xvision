# xvision example workspace

This directory is populated by `xvn example seed`. Everything in here is
labelled `source=example` and is safe to delete. Re-running
`xvn example seed --reset` rebuilds the seed-owned strategies and any
example scenarios that no eval run has referenced yet — see
[Reset behavior](#reset-behavior) below.

## What got seeded

**Strategies** (live at `$XVN_HOME/strategies/*.json`):

| Id | Template | What to look at |
|---|---|---|
| `example-trend-follower` | `trend_follower` | Single-slot trader; the 80% case. |
| `example-mean-reversion` | `mean_reversion` | Two-stage regime → trader pipeline. |
| `example-breakout` | `breakout` | Donchian breakout, conservative risk. |

**Scenarios** (rows in `$XVN_HOME/xvn.db`):

| Id | Window | Why it's here |
|---|---|---|
| `example-quickstart-btc-bull-jan-2025` | 7 days, Jan 2025 | Fastest path to a finished backtest. |
| `example-quickstart-btc-flash-aug-2024` | 7 days, Aug 2024 | Same strategy under a flash crash. |

## Try it

```bash
# Single backtest on the bull-week scenario.
xvn eval run \
  --strategy example-trend-follower \
  --scenario example-quickstart-btc-bull-jan-2025 \
  --mode backtest

# Same strategy against the flash-crash week.
xvn eval run \
  --strategy example-trend-follower \
  --scenario example-quickstart-btc-flash-aug-2024 \
  --mode backtest

# Compare two strategies on the bull-week scenario: launch one run for
# each, then feed the resulting run ids to `xvn eval compare`.
xvn eval run --strategy example-trend-follower \
  --scenario example-quickstart-btc-bull-jan-2025 --mode backtest
xvn eval run --strategy example-breakout \
  --scenario example-quickstart-btc-bull-jan-2025 --mode backtest
xvn eval ls --scenario example-quickstart-btc-bull-jan-2025
xvn eval compare <run-id-A> <run-id-B>
```

The dashboard surfaces the same scenarios under **Scenarios** and the
same strategies under **Strategies**.

## Reset behavior

```bash
xvn example seed --reset
```

Refreshes the example workspace:

* **Strategies** — every seed-owned strategy (id prefix `example-` and
  creator `@xvision-examples`) is deleted, then re-created from the
  current curated set. The latest prompts, params, and risk config land
  on disk.
* **Scenarios** — example scenarios that no eval run has referenced yet
  are deleted and re-inserted, picking up any updated body. Example
  scenarios already referenced by an eval run are preserved as-is: the
  `scenarios` table is immutable post-insert and deleting a referenced
  row would orphan the audit trail. The reset output lists those rows
  under `scenarios_preserved_referenced` so it is clear why they kept
  their old body.
* **Tutorial** — this `README.md` is rewritten on every run.

Operator-authored strategies and scenarios are never read or replaced.
A name collision on a row that is not labelled as a seed-owned example
halts the seed with exit code `7` (Conflict).
