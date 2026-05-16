# xvision example workspace

This directory is populated by `xvn example seed`. Everything in here is
labelled `source=example` and is safe to delete — re-running
`xvn example seed --reset` rebuilds it from the workspace defaults.

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

# A/B compare two strategies on the bull-week scenario.
xvn ab-compare \
  --scenario example-quickstart-btc-bull-jan-2025 \
  --arms example-trend-follower,example-breakout
```

The dashboard surfaces the same scenarios under **Scenarios** and the
same strategies under **Strategies**.

## Reset

```bash
xvn example seed --reset
```

Deletes every row labelled `source=example` and re-creates the set above.
Operator-authored strategies and scenarios are never touched.
