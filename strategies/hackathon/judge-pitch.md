# Hackathon judge pitch

## One-slide summary

**xvision is a regime-aware trading platform that evaluates strategies instead of blindly running one bot everywhere.**

It does three things well:

1. **Detects the market regime first**
   - trend, breakout, chop, bear, event shock, risk-off
2. **Chooses the right strategy for that regime**
   - trend continuation
   - breakout confirmation
   - mean reversion in chop
   - bearish continuation
   - liquidation-shock reversal
   - safety-first no-trade gating
3. **Explains why a trade is taken or skipped**
   - higher-timeframe filter
   - multi-confirmation entry
   - explicit failure regime

## Why this matters

Most trading demos look like one strategy forcing its way through every market. xvision is different: it switches behavior by regime and shows the rule for every decision.

## The pack we are showing

- `regime_filter_4h_ema_stack` — bull trend pullback continuation
- `volume_confirmed_breakout` — breakout only with participation
- `range_reversion_rsi_bollinger` — chop / mean reversion
- `bearish_trend_filter_4h_ema_stack` — bearish mirror
- `liquidation_event_shock_reversal` — panic / capitulation reversal
- `risk_off_failed_breakout_fade` — hostile-market filter and fakeout fade

## Judge takeaway

**xvision is not just a strategy runner. It is a strategy evaluator that can explain market context, enforce regime fit, and avoid bad trades.**

## Suggested demo order

1. Bull trend pullback
2. Breakout confirmation
3. Chop mean reversion
4. Bear trend mirror
5. Crash / liquidation reversal
6. Risk-off no-trade gating
