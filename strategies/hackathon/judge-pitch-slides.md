# xvision — regime-aware strategy pack

## Judge takeaway

xvision is a trading platform that *matches strategy to market regime* instead of forcing one bot to trade every market.

---

## What it does

1. **Detects the market regime first**
   - bull trend
   - breakout
   - chop / range
   - bear trend
   - event shock
   - risk-off

2. **Selects the right behavior for that regime**
   - trend continuation
   - breakout confirmation
   - mean reversion
   - bearish continuation
   - shock reversal
   - no-trade gating

3. **Explains every decision**
   - why a trade was taken
   - why a trade was skipped
   - why risk was reduced

---

## Why judges should care

Most trading demos show one strategy pushed through every market.

xvision is different:
- it is **regime-aware**
- it is **explainable**
- it is **safer by design**
- it demonstrates both **opportunity regimes** and **danger regimes**

---

## The 6-strategy hackathon pack

- `regime_filter_4h_ema_stack`
  - bull trend pullback continuation

- `volume_confirmed_breakout`
  - breakout only with participation

- `range_reversion_rsi_bollinger`
  - chop / mean reversion

- `bearish_trend_filter_4h_ema_stack`
  - bearish mirror of trend continuation

- `liquidation_event_shock_reversal`
  - panic / capitulation reversal

- `risk_off_failed_breakout_fade`
  - hostile-market filter and fakeout fade

---

## What the demo proves

This is not just a strategy runner.

It is a **strategy evaluator** that:
- adapts to market context
- enforces regime fit
- avoids low-quality trades
- explains the logic in plain language

---

## Suggested demo order

1. Bull trend pullback
2. Breakout confirmation
3. Chop mean reversion
4. Bear trend mirror
5. Crash / liquidation reversal
6. Risk-off no-trade gating
