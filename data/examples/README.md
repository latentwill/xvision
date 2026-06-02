# xvision example workspace

This directory is populated by `xvn example seed`. Everything in here is
labelled `source=example` and is safe to delete. Re-running
`xvn example seed --reset` rebuilds any example scenarios that no eval
run has referenced yet — see
[Reset behavior](#reset-behavior) below.

## What got seeded

**Scenarios** (rows in `$XVN_HOME/xvn.db`):

| Id | Window | Why it's here |
|---|---|---|
| `example-quickstart-btc-bull-jan-2025` | 7 days, Jan 2025 | Fastest path to a finished backtest. |
| `example-quickstart-btc-flash-aug-2024` | 7 days, Aug 2024 | Same strategy under a flash crash. |

No strategies are seeded. Strategies depend on the operator's agent,
provider, model, tools, and risk preferences, so hardcoded example
strategies would be incomplete for many workspaces.

## Try it

```bash
# Run one of your strategies on the bull-week scenario.
xvn eval run \
  --strategy <your-strategy-id> \
  --scenario example-quickstart-btc-bull-jan-2025 \
  --mode backtest

# Run the same strategy against the flash-crash week.
xvn eval run \
  --strategy <your-strategy-id> \
  --scenario example-quickstart-btc-flash-aug-2024 \
  --mode backtest

# Compare two strategies on the bull-week scenario: launch one run for
# each, then feed the resulting run ids to `xvn eval compare`.
xvn eval run --strategy <strategy-id-A> \
  --scenario example-quickstart-btc-bull-jan-2025 --mode backtest
xvn eval run --strategy <strategy-id-B> \
  --scenario example-quickstart-btc-bull-jan-2025 --mode backtest
xvn eval ls --scenario example-quickstart-btc-bull-jan-2025
xvn eval compare <run-id-A> <run-id-B>
```

The dashboard surfaces the same scenarios under **Scenarios**.

## Reset behavior

```bash
xvn example seed --reset
```

Refreshes the example workspace:

* **Strategies** — every legacy seed-owned strategy (id prefix
  `example-` and creator `@xvision-examples`) is deleted and is not
  re-created. Operator-authored strategies are left untouched.
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
