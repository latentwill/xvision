# Strategies and models to test next

This backlog is for short, causal xvision evals. It intentionally excludes timestamp/index oracle behavior.

## Model bake-off setup

Use one shared scenario/timeframe with 30 decisions for every model. Prefer a BTC/USD 1h window with 200 warmup bars and 30 post-warmup bars, because it gives enough decisions for behavior comparison without hundreds of provider calls.

Run sequentially, persist progress after every terminal run, and use cooldowns on provider errors.

## Frozen prompt family for the bake-off

Use one compact causal long/short strategy prompt for all models:

- Trade only when flat unless managing an already-open position.
- Long setup: recent swing high/retest break, close above short trend, expanding range/volume proxy, no immediate exhaustion bar.
- Short setup: recent swing low/retest break, close below short trend, expanding range/volume proxy, no immediate squeeze risk.
- Stay flat in chop, inside bars, or after stale moves.
- Hold only while invalidation has not fired.
- Exit on failed breakout/reclaim, opposite momentum, or stop-risk invalidation.
- Return strict JSON matching the eval schema with concise rationale.

## Causal strategy families

### 1. Breakdown retest long/short

- Enter after support/resistance breaks and retests from the other side.
- Stop/invalidation: reclaim of broken level or ATR-scale adverse move.
- Reason to test: explicit structure reduces chasing.

### 2. Failed breakout trap reversal

- Fade a breakout that closes back inside the prior range with expansion exhaustion.
- Stop/invalidation: second clean breakout close outside the range.
- Reason to test: good for range/chop regimes if the model can avoid premature entries.

### 3. Trend pullback strict

- Enter only in established trend after a shallow pullback that resumes with momentum.
- Stop/invalidation: trend slope loss or break of pullback low/high.
- Reason to test: less sensitive to exact breakout timing.

### 4. Volatility compression expansion

- Wait for compressed candles/ranges, then trade first decisive expansion only.
- Stop/invalidation: expansion fails to hold direction within 1-2 bars.
- Reason to test: previous compression ideas were the least bad, but prompts must prevent repeated re-entry.

### 5. Range fade with hard veto

- Fade extremes only when price fails to close outside the range and momentum is weakening.
- Veto if range expands into a trend or repeated tests weaken the level.
- Reason to test: captures chop, but requires strong anti-breakout discipline.

## Candidate OpenRouter models

- `google/gemini-3.1-flash-lite` or enabled Gemini Flash Lite baseline
- `deepseek/deepseek-v4-flash`
- `qwen/qwen3.5-flash-02-23`
- `mistralai/mistral-small-3.2-24b-instruct`
- `xiaomi/mimo-v2-flash`
- `qwen/qwen3-30b-a3b-instruct-2507` if enabled and affordable

## Comparison report checklist

For each model, record:

- Strategy id and attached agent id.
- Scenario id, asset, timeframe, decision count.
- Run id and terminal status.
- Return percentage, Sharpe, max drawdown.
- Action distribution.
- JSON/provider failures, if any.
- Qualitative notes: churn, flat discipline, late entries, or holding through invalidation.
