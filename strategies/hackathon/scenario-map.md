# Hackathon scenario map

One-page reference for judges and reviewers.

## Mapping

- `regime_filter_4h_ema_stack`
  - Intended regime: bull trend / clean pullback
  - Canonical eval fit: `regime:bull`
  - What it should lose to: chop, fast reversal, event shock
  - Demo value: shows the strongest higher-timeframe filter story

- `volume_confirmed_breakout`
  - Intended regime: bullish expansion after compression
  - Canonical eval fit: `regime:bull`, `regime:breakout`
  - What it should lose to: low-volume fakeouts, range-bound conditions
  - Demo value: shows confirmation from both structure and participation

- `range_reversion_rsi_bollinger`
  - Intended regime: range / chop / non-trend
  - Canonical eval fit: `regime:chop`
  - What it should lose to: strong trends, momentum expansions
  - Demo value: shows that xvision can switch behavior instead of forcing one strategy everywhere

- `bearish_trend_filter_4h_ema_stack`
  - Intended regime: bear trend / clean pullback
  - Canonical eval fit: `regime:bear`
  - What it should lose to: chop, fast reversal, event shock
  - Demo value: shows the bearish mirror of the bull trend story

- `liquidation_event_shock_reversal`
  - Intended regime: event shock / liquidation cascade / capitulation
  - Canonical eval fit: `regime:event`
  - What it should lose to: trend-day continuation, repeated shock waves
  - Demo value: shows crash / panic handling

- `risk_off_failed_breakout_fade`
  - Intended regime: risk-off / hostile market / weak participation
  - Canonical eval fit: `regime:risk_off`
  - What it should lose to: clean trend continuation with real volume
  - Demo value: shows safety-first behavior and no-trade gating

## Pack-level message

This pack is deliberately balanced across regimes:

- one bull trend continuation strategy
- one breakout strategy
- one chop / mean-reversion strategy
- one bear trend continuation strategy
- one event-shock reversal strategy
- one risk-off / no-trade gate strategy

That makes the demo easy to explain:

- when trend is present, use trend rules
- when breakout conditions exist, require confirmation
- when the market is choppy, revert instead of chasing
- when the market is hostile, suppress or downshift risk

## What is covered now

- bull trend continuation
- bullish breakout confirmation
- range / chop mean reversion
- bear trend continuation
- liquidation / crash reversal
- risk-off failure gating

## Why this is judge-friendly

The pack now demonstrates that xvision is not just a “long-only trend bot.” It can:

- switch behavior by regime
- explain why each trade is taken
- explain why each trade is skipped
- cover both opportunity regimes and danger regimes
