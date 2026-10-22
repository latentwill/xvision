# Free-only strategy expansion run

**Started:** 2026-09-04 UTC

## Scope

Evaluate existing xvision strategy families and expand the strategy corpus with
session-aware opening-range breakout variants. The target is exploratory paper
forward testing toward the month-end `$1`/`$2` objective; profitability is not
assumed from backtest results.

## Free-only guardrail

All optimizer mutator, judge, and candidate-evaluation paths must be explicitly
pinned to:

- Provider: `opencode-zen`
- Model: `muse-spark-1.3-contributor-free`

Do not use the default optimizer configs: `/data/autooptimizer.toml` and
`/data/autooptimizer-15m.toml` contain paid DeepSeek settings, and
`/data/autooptimizer-long.toml` contains paid OpenRouter settings. The checked-in
free-only config is `config/autooptimizer-free.toml`; live commands must still
pass `--provider`/`--model` as well as the mutator/judge overrides because the
shared pair controls candidate evaluation.

## Corpus additions

- `strategies/orb/README.md`
- `strategies/orb/orb_opening_range_breakout.md`
- `strategies/orb/orb_pdh_confirmation.md`
- `strategies/orb/orb_failed_breakout_reversal.md`

These use existing deterministic filter indicators and operators, including
opening-range levels, prior-day levels, RVOL, ADX/DI, ATR, and RSI.

## Evaluation protocol

1. Use historical backtests for screening, with fixed day and untouched holdout
   windows.
2. Require sufficient gate trades before accepting an optimizer cycle.
3. Inspect PnL, Sharpe, drawdown, win rate, trade/decision counts, and baseline
   comparison; do not select on a single headline metric.
4. Forward testing means Alpaca paper/simulated money only, with explicit stop
   limits. No live-money execution is in scope.
5. Promote only candidates that survive the holdout and a forward-test period.
