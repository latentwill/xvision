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

## Pack-level message

This pack is deliberately balanced across regimes:

- one trend continuation strategy
- one breakout strategy
- one mean-reversion strategy

That makes the demo easy to explain:

- when trend is present, use trend rules
- when breakout conditions exist, require confirmation
- when the market is choppy, revert instead of chasing

## What is not covered

- bearish trend continuation
- event shock / flash crash handling
- funding / perp-specific edges

Those can be added later, but they are intentionally left out of the first judge-facing pack so the story stays clean.
