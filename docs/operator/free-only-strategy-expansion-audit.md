# Strategy corpus audit and free-only expansion

**Audit date:** 2026-09-04 UTC

## Safety constraint

All new optimizer work in this track is pinned to
`opencode-zen / muse-spark-1.3-contributor-free`. The default optimizer
configurations contain paid DeepSeek or OpenRouter models and are not used.
Backtests do not place orders. Forward tests use Alpaca paper/simulated capital
only.

## Existing corpus snapshot

The live xvn strategy registry contains a large corpus of existing variants,
including EMA, Bollinger, RSI/volume, MACD/ADX, VWAP, trend-rider,
mean-reversion, Donchian, and ORB families. Existing ORB records include:

- `wiki-orb-breakout-15m-tp`
- `wiki-orb-pdh-breakout-15m`
- `orb-breakout-15m-ollama-fino1`
- `orb-breakout-15m-deepseek-trader`
- `orb-breakout-15m-deepseek-16k`
- `wiki-orb-pdh-breakout-15m`

The registry also contains many model/provider clones. These should be grouped
by strategy thesis before promotion; a model clone is not a new market edge.

## Stored backtest screening

The stored completed backtests show the following high-return groups, but many
have too few trades to be credible and are not promotion decisions:

- `wiki-macd-adx-trend-1d`: 1 run, 2 trades, +22.036% average return,
  Sharpe 3.560.
- `trend-rider-daily-ollama-qwen3`: 2 runs, 4 trades, +10.119% average return,
  Sharpe 0.785.
- `variant-diguide-wide-8h`: 5 runs, 16 trades, +9.507% average return,
  Sharpe 2.464.
- `wiki-orb-pdh-breakout-15m`: 2 runs, 2 trades, +9.134% average return,
  Sharpe 3.708.
- `orb-breakout-15m-ollama-fino1`: 128 runs, 1,224 trades, +6.334% average
  return, Sharpe 0.402.
- `wiki-orb-breakout-15m-tp`: 2 runs, 4 trades, +5.878% average return,
  Sharpe 0.874.

These figures are historical aggregate summaries, not a guarantee of future
profit. The ORB group with the most observations is more informative than the
single-run high-return rows, but its average Sharpe is modest and requires
holdout/forward confirmation.

## AutoOptimizer results

Two free-only optimizer cycles were attempted against the Muse-pinned
mean-reversion parent:

1. One-month-window retry: **$0.00**, 0 input/output model tokens. The parent
   filter produced only 4 trades; the optimizer requires at least 10 before it
   will mutate candidates.
2. A shorter-window attempt also failed the same gate at **$0.00**.

The optimizer therefore did not generate or keep candidates yet. The next
experiment should use a more active parent or a looser deterministic gate, not
claim that the strategy is unprofitable.

## Forward-test run

A bounded Alpaca paper forward test was launched:

- Run: `01M1P1MMBD80GGN52VW3QXXB1K`
- Strategy: `wiki-orb-breakout-15m-tp`
- Asset: BTC/USD
- Capital: $100 simulated
- Granularity: 15m
- Stops: 12 bars, 4 decisions, or 2 hours
- Provider/model override: `opencode-zen / muse-spark-1.3-contributor-free`

Its terminal metrics must be read from the run record after the stop condition;
being queued/running is not evidence of profitability.

## New strategy specifications

- `strategies/orb/orb_opening_range_breakout.md`
- `strategies/orb/orb_pdh_confirmation.md`
- `strategies/orb/orb_failed_breakout_reversal.md`

These vary one thesis dimension at a time: volume/DMI-confirmed continuation,
prior-day-level confirmation, and failed-breakout reversal.

## Promotion rule toward the month-end target

The `$1`/`$2` goal is an objective, not an expected return. Promote only a
candidate that has positive results across multiple fixed historical windows,
beats or meaningfully complements a simple baseline after costs, has enough
trades to avoid a small-sample result, and remains viable in forward paper
trading. No model/provider cost should be introduced without an explicit change
to the free-only guardrail.
