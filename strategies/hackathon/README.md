# Hackathon strategy pack

Judge-facing sample strategies for xvision.

The pack is intentionally small and easy to explain:

1. detect regime first
2. confirm with a higher-timeframe filter
3. require multiple confirmations before entry

The goal is not to maximize strategy count. The goal is to show that xvision can:
- separate trend from chop
- explain why a trade is allowed
- explain why a trade is blocked
- keep the rules simple enough for a judge to follow

## Recommended demo order

1. `regime_filter_4h_ema_stack` — the cleanest “trend first” story
2. `volume_confirmed_breakout` — shows confirmation + participation
3. `range_reversion_rsi_bollinger` — shows the non-trend / chop mode

## Canonical eval mapping

- `regime:bull` → `regime_filter_4h_ema_stack`
- `regime:bull` or `regime:breakout` → `volume_confirmed_breakout`
- `regime:chop` → `range_reversion_rsi_bollinger`
- `regime:bear` / `regime:event` → intentionally *not* covered by this pack

## What each strategy is for

- `regime_filter_4h_ema_stack` — trend continuation after a pullback.
  - Should lose in chop and fast reversal regimes.
- `volume_confirmed_breakout` — directional expansion after compression.
  - Should lose when breakouts lack volume or when the market is range-bound.
- `range_reversion_rsi_bollinger` — mean reversion in non-trending markets.
  - Should lose when a strong trend is already underway.

## Judge rubric

When presenting the pack, emphasize:
- clarity: can a non-trader understand the rule?
- regime fit: does the strategy only act in the right market state?
- simplicity: are the entry rules mechanical and compact?
- honesty: does it clearly name the failure regime?

## Index

- [`scenario-map.md`](scenario-map.md) — one-page map from strategy to regime and expected failure mode.
- [`regime_filter_4h_ema_stack`](regime_filter_4h_ema_stack.md) — trend filter first, then 1h pullback entries.
- [`volume_confirmed_breakout`](volume_confirmed_breakout.md) — breakout only when higher-timeframe bias and volume agree.
- [`range_reversion_rsi_bollinger`](range_reversion_rsi_bollinger.md) — range-mode mean reversion with explicit risk-off gating.
