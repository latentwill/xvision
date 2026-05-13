# Hackathon sample strategies

Small, regime-aware sample strategies based on the recurring action steps from the strategy transcripts:

1. detect regime first
2. confirm with a higher-timeframe filter
3. require multiple confirmations before entry

These files are intentionally simple, readable, and demo-friendly.

## Index

- [`regime_filter_4h_ema_stack`](regime_filter_4h_ema_stack.md) — trend filter first, then 1h pullback entries.
- [`volume_confirmed_breakout`](volume_confirmed_breakout.md) — breakout only when higher-timeframe bias and volume agree.
- [`range_reversion_rsi_bollinger`](range_reversion_rsi_bollinger.md) — range-mode mean reversion with explicit risk-off gating.
