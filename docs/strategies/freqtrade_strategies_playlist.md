# Freqtrade Strategies Playlist — Strategy Catalog

Source: *Freqtrade Strategies* YouTube playlist by martinopiaggi.

This document turns the playlist transcripts/captions into a practical catalog of strategy mechanics for trading bots.

## How to read this doc

Each strategy entry includes:
- *Core idea* — what the strategy is trying to exploit
- *Indicators / filters* — the main signals used
- *Entry* — the trigger conditions
- *Exit / risk* — stop-loss, trailing stop, or profit target behavior
- *Bot note* — how to implement it cleanly in automation

## Important pattern across the playlist

The playlist repeatedly reinforces these rules:
- Trade *with the regime*, not against it.
- Use a *trend filter* before any momentum or breakout entry.
- Prefer *ATR-based stops* and *trailing exits* for trend systems.
- Use *fixed profit targets* for mean-reversion systems.
- Use *walk-forward / out-of-sample validation* to avoid overfitting.
- Treat DCA / martingale as *position management*, not standalone alpha.

---

## 1) Moving Average Crossover / Golden Cross

**Video:** *I Tested a Moving Average Crossover Strategy for 6 Years… It Made 6200%*

- *Core idea:* simple long-term trend following.
- *Indicators / filters:* 50/200 moving averages.
- *Entry:* go long when 50 MA crosses above 200 MA.
- *Exit / risk:* exit when 50 MA crosses back below 200 MA; use a fixed stop-loss.
- *Bot note:* strongest on higher timeframes; transcript says 4H reduced noise and improved trend capture.

## 2) Breakout + Trend Filter Momentum System

**Video:** *This Crypto Strategy Turned $1K Into $52K (6-Year Backtest)*

- *Core idea:* breakout system wrapped inside a trend filter.
- *Indicators / filters:* EMA stack, ROC > 0, breakout of recent high, extra confirmation layers.
- *Entry:* wait for all conditions to align; breakout only after multiple filters confirm the move.
- *Exit / risk:* trailing breakdown exit plus hard percentage stop-loss.
- *Bot note:* avoid fakeouts; the transcript explicitly says it uses three layers of confirmation and a trailing exit to keep winners.

## 3) Triple KAMA + MACD

**Video:** *I Tested KAMA + MACD Strategy for 1 Year — Here's What I Found*

- *Core idea:* adaptive trend detection plus momentum confirmation.
- *Indicators / filters:* 8/13/21 KAMA stack, MACD, ATR.
- *Entry:* long when 8 > 13 > 21 and MACD confirms strength; short for the inverse.
- *Exit / risk:* stop-loss just beyond the signal candle; target is 2:1 reward-to-risk.
- *Bot note:* use the KAMA stack for trend direction and MACD for timing.

## 4) ADX + OBV

**Video:** *I Tested ADX + OBV Indicator Strategy... 102% Profit!*

- *Core idea:* trade only when trend strength and volume flow both agree.
- *Indicators / filters:* ADX > 25, +DI / -DI direction, OBV above/below its SMA.
- *Entry:* long when ADX is strong, +DI crosses above -DI, and OBV is above its 100-period SMA.
- *Exit / risk:* stop-loss around 2 ATR beyond signal candle; take-profit at about 1.5x stop distance.
- *Bot note:* this is a cleaner trend-following filter than raw MA cross systems.

## 5) Supertrend + MACD + RSI

**Video:** *I Tested Supertrend + MACD + RSI... 153% Profit!*

- *Core idea:* trend-following with momentum and oversold/overbought context.
- *Indicators / filters:* Supertrend, MACD, RSI.
- *Entry:* long when price is above Supertrend, MACD is bullish, and RSI confirms strength.
- *Exit / risk:* stop below most recent swing low; take-profit at 1.5x risk.
- *Bot note:* strong multi-confirmation template for a bot that wants fewer but cleaner trades.

## 6) Donchian Channel + EMA + ADX + CHOP

**Video:** *I Tested Donchian channel + EMA + ADX + CHOP 163% Profit!*

- *Core idea:* breakout only when trend strength is real and chop is low.
- *Indicators / filters:* Donchian channel, EMA trend filter, ADX, Choppiness Index.
- *Entry:* breakout above recent range high only when EMA and ADX confirm and CHOP says the market is trending.
- *Exit / risk:* ATR-based trailing stop.
- *Bot note:* ideal for avoiding false breakouts in sideways markets.

## 7) Chaikin Money Flow + EMA

**Video:** *I Tested Chaikin Money Flow + EMA … 325% Profit!*

- *Core idea:* volume-weighted trend confirmation.
- *Indicators / filters:* CMF, 200 EMA.
- *Entry:* long when price is above the 200 EMA and CMF confirms buying pressure; short when below the 200 EMA and CMF confirms distribution.
- *Exit / risk:* ATR stop-loss; 2:1 risk-reward target.
- *Bot note:* the transcript highlights using CMF thresholds to avoid sideways / low-volume false positives.

## 8) Supertrend + ATR + ADX

**Video:** *I Tested Supertrend + ATR + ADX… 184% Profit!*

- *Core idea:* trend-following with volatility regime control.
- *Indicators / filters:* Supertrend, higher-timeframe ATR, ADX, EMA filter.
- *Entry:* long when price is above Supertrend, ATR is elevated, and ADX is above the dynamic threshold.
- *Exit / risk:* Supertrend line acts as trailing stop.
- *Bot note:* only trade when volatility and trend strength both support participation.

## 9) 5-8-13 EMA + PSAR

**Video:** *I Tested 5-8-13 EMA + PSAR… 96% Profit!*

- *Core idea:* short-term trend stack with reversal/trailing support.
- *Indicators / filters:* 5/8/13 EMAs, PSAR, higher-timeframe confirmation.
- *Entry:* long when 5 > 8 > 13 and the higher timeframe confirms the same structure.
- *Exit / risk:* stop below recent swing low; target around 3:1 R:R.
- *Bot note:* a clean template for fast trend continuation bots.

## 10) MACD + RSI + Stochastic

**Video:** *I Tested MACD + RSI + Stochastic… 121% Profit?!*

- *Core idea:* oscillator-combination timing system.
- *Indicators / filters:* MACD, RSI, Stochastic.
- *Entry:* wait until all three oscillators align; use a rolling window so signals do not need to fire on the same candle.
- *Exit / risk:* ATR stop; target around 2R.
- *Bot note:* good candidate for a range or early reversal bot, not a strong trend bot.

## 11) VWAP + MACD

**Video:** *Master VWAP & MACD Strategy – 191% Profit Backtest!*

- *Core idea:* fair-value anchor plus trend confirmation.
- *Indicators / filters:* VWAP, MACD.
- *Entry:* long when price closes above VWAP and MACD confirms bullish momentum.
- *Exit / risk:* stop about 2.5 ATR below the signal candle.
- *Bot note:* transcript says this was run on the 1H timeframe and validated with walk-forward / out-of-sample testing.

## 12) DCA Martingale Trading Bot

**Video:** *DCA Martingale Trading Bot Explained (Risk Controls + Out-of-Sample Backtest)*

- *Core idea:* controlled averaging with explicit risk management.
- *Indicators / filters:* position sizing and safety-order logic rather than pure indicator alpha.
- *Entry:* initial trade enters normally; if price moves against the bot, predefined safety orders are placed.
- *Exit / risk:* hard limits on safety order count and scale; evaluated with out-of-sample backtests.
- *Bot note:* use only with strict caps and regime awareness.

## 13) Supertrend + Chop Filter

**Video:** *Why This Supertrend Strategy Works When Others Fail*

- *Core idea:* only trade strong, clean trends.
- *Indicators / filters:* Supertrend, Choppiness Index.
- *Entry:* long above Supertrend, short below; only trade when CHOP < 50.
- *Exit / risk:* Supertrend trailing stop.
- *Bot note:* the transcript explicitly says this system avoids sideways markets.

## 14) RSI DCA Bot

**Video:** *The Most Powerful Freqtrade DCA Bot Strategy*

- *Core idea:* buy dips with RSI-triggered entries and DCA position sizing.
- *Indicators / filters:* RSI, DCA logic.
- *Entry:* initial buy on RSI pullback condition; add more as price drops.
- *Exit / risk:* fixed profit exits; risk controlled by position sizing and DCA schedule.
- *Bot note:* run on 30m timeframe with data split into training / test segments.

## 15) ALMA + MACD

**Video:** *ALMA + MACD Strategy: Why This Works Better (Real Backtest Results)*

- *Core idea:* smoother trend line with momentum confirmation.
- *Indicators / filters:* ALMA, MACD.
- *Entry:* use ALMA for trend direction and MACD for confirmation.
- *Exit / risk:* treat as a trend strategy with stop below signal structure.
- *Bot note:* ALMA is emphasized as a balance between smoothness and responsiveness.

## 16) DCA + Martingale Crypto Bot

**Video:** *DCA + Martingale Crypto Bot Strategy (Full Tutorial)*

- *Core idea:* more explicit DCA/martingale implementation.
- *Indicators / filters:* usually paired with an entry trigger plus scaling rules.
- *Entry:* open initial position, then scale in on adverse move according to rules.
- *Exit / risk:* fixed exit target and controlled risk scaling.
- *Bot note:* treat as a position-management layer, not alpha by itself.

## 17) VWAP + DMI

**Video:** *This VWAP + DMI Algo Made 444% With Freqtrade*

- *Core idea:* price fair-value plus directional movement.
- *Indicators / filters:* VWAP, DMI / DI crossover.
- *Entry:* trade in the direction of VWAP bias and DMI direction.
- *Exit / risk:* likely ATR-based protective stop and trend exit.
- *Bot note:* volume-weighted context improves raw directional signals.

## 18) Williams %R

**Video:** *Williams %R Indicator made 305% Profit!*

- *Core idea:* oscillator reversal timing.
- *Indicators / filters:* Williams %R.
- *Entry:* buy when oversold / reversal setup appears.
- *Exit / risk:* exit on normalization or target hit.
- *Bot note:* best in ranges or pullback environments.

## 19) 13-21-34 WMA + MACD

**Video:** *13-21-34 WMA & MACD Strategy made 293% Profit!*

- *Core idea:* weighted moving average trend system with momentum confirmation.
- *Indicators / filters:* 13/21/34 WMA, MACD.
- *Entry:* take longs when the WMA stack is aligned and MACD confirms.
- *Exit / risk:* use a trend-based exit or ATR stop.
- *Bot note:* a cleaner, faster-reacting version of basic MA trend following.

## 20) VWAP Indicator

**Video:** *VWAP Indicator made 271% Profit!*

- *Core idea:* trade relative to fair value.
- *Indicators / filters:* VWAP.
- *Entry:* use VWAP reclaim / rejection as the trigger.
- *Exit / risk:* use pullback exits or a risk-reward target.
- *Bot note:* often works best when paired with trend or momentum filters.

## 21) Money Flow Index

**Video:** *Money Flow Index indicator made 300% Profit!*

- *Core idea:* volume-aware overbought / oversold timing.
- *Indicators / filters:* MFI.
- *Entry:* buy oversold conditions, sell / short overbought exhaustion.
- *Exit / risk:* normalize at mean / target; keep stops tight.
- *Bot note:* MFI adds volume context that pure RSI lacks.

## 22) RSI Strategy

**Video:** *RSI Trading Strategy made 314% Profit!*

- *Core idea:* classic oscillator-based timing.
- *Indicators / filters:* RSI.
- *Entry:* buy on oversold / sell on overbought, with confirmation logic.
- *Exit / risk:* fixed target or reversal-based exit.
- *Bot note:* best used only in range conditions or with a trend filter.

## 23) Keltner Channel

**Video:** *Keltner Channel Strategy made 126% Profit!*

- *Core idea:* volatility envelope strategy.
- *Indicators / filters:* Keltner Channel.
- *Entry:* either breakout beyond channel or mean-reversion from channel extremes.
- *Exit / risk:* mean return or volatility stop.
- *Bot note:* the exact edge depends on whether the implementation is breakout or reversion.

## 24) Aroon Indicator

**Video:** *Aroon Indicator Trading Strategy 291% Profit!*

- *Core idea:* detect trend age and trend strength.
- *Indicators / filters:* Aroon up/down.
- *Entry:* buy when Aroon up dominates; sell/short when Aroon down dominates.
- *Exit / risk:* exit when the opposite Aroon signal strengthens.
- *Bot note:* useful as a regime detector, not just a trigger.

## 25) Alligator Strategy

**Video:** *Alligator Strategy made 227% Profit!*

- *Core idea:* trend alignment through smoothed moving averages.
- *Indicators / filters:* Alligator lines (jaw, teeth, lips).
- *Entry:* trade when the lines are fanned out and aligned in trend direction.
- *Exit / risk:* exit when the lines converge / lose structure.
- *Bot note:* a classic trend-following template with built-in structure.

## 26) Donchian Channel

**Video:** *Donchian Channel Strategy made 140% Profit!*

- *Core idea:* pure breakout system.
- *Indicators / filters:* recent highest high / lowest low channel.
- *Entry:* buy when price breaks the upper channel; short on lower-channel break.
- *Exit / risk:* trend exit or trailing stop.
- *Bot note:* works best when paired with trend / momentum filters.

## 27) Mean Reversion Strategy

**Video:** *Mean Reversion Trading Strategy Explained & Backtested – 179% Profit*

- *Core idea:* fade extremes and target return to the mean.
- *Indicators / filters:* reversion bands / oversold-overbought logic.
- *Entry:* buy stretched weakness; short stretched strength.
- *Exit / risk:* exit at midpoint / mean / fixed target.
- *Bot note:* keep this away from strong trending regimes.

## 28) Awesome Oscillator

**Video:** *Awesome Oscillator Strategy Made 189% Profit!*

- *Core idea:* momentum shift detection.
- *Indicators / filters:* Awesome Oscillator.
- *Entry:* enter when oscillator changes direction / crosses from negative to positive.
- *Exit / risk:* exit on weakening momentum or opposite shift.
- *Bot note:* useful as a confirmation layer, especially for reversals.

## 29) ADX Indicator Strategy

**Video:** *Freqtrade: ADX Indicator Strategy in Python makes 329%*

- *Core idea:* trade only when trend strength is sufficient.
- *Indicators / filters:* ADX plus direction filters.
- *Entry:* long when ADX confirms a strong trend and direction agrees.
- *Exit / risk:* trend-loss exit or trailing stop.
- *Bot note:* ADX is better as a regime filter than a sole entry trigger.

## 30) Triple KAMA & MACD

**Video:** *Triple KAMA & MACD Strategy Made 214% Profit!*

- *Core idea:* adaptive trend stack with momentum filter.
- *Indicators / filters:* multiple KAMAs, MACD.
- *Entry:* align the KAMA stack and confirm with MACD.
- *Exit / risk:* trend stop or volatility stop.
- *Bot note:* conceptually similar to the earlier KAMA system, but more layered.

## 31) Chaikin Money Flow

**Video:** *Chaikin Money Flow strategy made 316% Profit!*

- *Core idea:* accumulation/distribution trading.
- *Indicators / filters:* CMF plus trend filter.
- *Entry:* long when CMF indicates money inflow; short when CMF indicates distribution.
- *Exit / risk:* ATR stop and reward target.
- *Bot note:* best when aligned with the prevailing trend.

## 32) VWAP + MACD

**Video:** *VWAP + MACD Strategy Made 256% Profit*

- *Core idea:* fair-value + momentum hybrid.
- *Indicators / filters:* VWAP, MACD.
- *Entry:* price relative to VWAP plus MACD direction.
- *Exit / risk:* trend exit or risk-reward target.
- *Bot note:* essentially a refined version of the VWAP/MACD system.

## 33) ADX + OBV

**Video:** *ADX + OBV Strategy Generated 336% Profit*

- *Core idea:* trend strength plus volume confirmation.
- *Indicators / filters:* ADX, OBV.
- *Entry:* require a strong trend and supportive volume flow.
- *Exit / risk:* stop-loss and trend-failure exit.
- *Bot note:* a strong filter for avoiding weak trend signals.

## 34) Supertrend + MACD + RSI

**Video:** *Supertrend + MACD + RSI Strategy | Backtest 299% Profit Using Python & Freqtrade*

- *Core idea:* trend + momentum + oscillator confirmation.
- *Indicators / filters:* Supertrend, MACD, RSI.
- *Entry:* go long when Supertrend is bullish and both MACD / RSI support the move.
- *Exit / risk:* Supertrend-based or structure-based exit.
- *Bot note:* this is one of the most “bot-ready” mechanical templates in the playlist.

## 35) Stochastic RSI + EMA

**Video:** *Build a Crypto Trading Bot with Stochastic RSI & EMA: 207% Profit in 1 Year*

- *Core idea:* pullback timing inside an EMA trend.
- *Indicators / filters:* Stochastic RSI, EMA.
- *Entry:* trade reversals in the direction of the EMA trend when Stoch RSI resets.
- *Exit / risk:* fixed target or trend exit.
- *Bot note:* good hybrid of mean reversion and trend filtering.

## 36) Supertrend Strategy to Python Bot

**Video:** *Supertrend Strategy to Python Bot: 212% Profit*

- *Core idea:* turn Supertrend into a fully coded bot.
- *Indicators / filters:* Supertrend.
- *Entry:* bullish Supertrend flip for longs; bearish flip for shorts.
- *Exit / risk:* Supertrend trailing stop.
- *Bot note:* mostly an implementation/tutorial video rather than a new alpha source.

## 37) RSI Trading Bot

**Video:** *Master Python RSI Trading Bot | Build, Backtest & Optimize for Crypto!*

- *Core idea:* classic RSI bot implementation.
- *Indicators / filters:* RSI.
- *Entry:* oversold / overbought logic with backtest optimization.
- *Exit / risk:* fixed target / reversal exit.
- *Bot note:* use only with strong regime rules so the bot does not fade trends blindly.

## 38) ADX + Moving Averages

**Video:** *ADX & Moving Averages: Freqtrade Crypto Strategy That Actually Works!*

- *Core idea:* MA trend direction with ADX trend-strength gating.
- *Indicators / filters:* ADX, moving averages.
- *Entry:* only trade when MA trend and ADX agree.
- *Exit / risk:* trend stop or MA breakdown.
- *Bot note:* this is a very practical trend-filter template.

## 39) 8-13-21 EMA + MACD

**Video:** *Master the 8-13-21 EMA & MACD Strategy – 500% Profit Backtest!*

- *Core idea:* stacked EMA trend + momentum.
- *Indicators / filters:* 8/13/21 EMA, MACD.
- *Entry:* fast EMA above slower EMAs and MACD confirms.
- *Exit / risk:* stop below swing low; risk-reward target.
- *Bot note:* clean mechanical template for directional markets.

## 40) 5-8-13 EMA Optimization

**Video:** *How I Optimized the 5-8-13 EMA Strategy for Higher Crypto Profits*

- *Core idea:* parameter optimization of a fast EMA stack.
- *Indicators / filters:* 5/8/13 EMAs.
- *Entry:* EMA alignment in the direction of the trend.
- *Exit / risk:* optimized stop and target logic.
- *Bot note:* the main lesson is tuning, not just the indicator set itself.

## 41) Ichimoku Cloud

**Video:** *Ultimate Ichimoku Cloud Strategy [2944% Profit Backtest]*

- *Core idea:* full regime-aware trend system.
- *Indicators / filters:* Ichimoku Cloud components.
- *Entry:* trade in the direction of cloud bias and confirmation structure.
- *Exit / risk:* cloud loss / trend failure exit.
- *Bot note:* one of the most complete trend frameworks in the playlist.

## 42) RSI + MACD + Stochastic

**Video:** *Maximize Crypto Trades: Backtest RSI, MACD & Stochastic with Freqtrade*

- *Core idea:* oscillator stack used together for directional timing.
- *Indicators / filters:* RSI, MACD, Stochastic.
- *Entry:* only take trades when the three indicators align.
- *Exit / risk:* version likely uses reversal or fixed target exits.
- *Bot note:* multi-oscillator systems need strict regime filters to avoid chop-trading.

---

## Strategy families by bot behavior

### Trend-following bots
- Moving Average Crossover / Golden Cross
- Triple KAMA + MACD
- Supertrend + MACD + RSI
- Supertrend + ATR + ADX
- 5-8-13 EMA + PSAR
- ALMA + MACD
- Alligator
- ADX + Moving Averages
- 8-13-21 EMA + MACD
- Ichimoku Cloud

### Breakout bots
- Breakout + Trend Filter Momentum System
- Donchian Channel + EMA + ADX + CHOP
- Donchian Channel
- VWAP-based breakout variants
- TS Momentum

### Volume-confirmed bots
- ADX + OBV
- Chaikin Money Flow + EMA
- VWAP + DMI
- VWAP + MACD
- ADX + OBV
- MFI-based setups

### Mean-reversion bots
- RSI Strategy
- Williams %R
- Mean Reversion Strategy
- Stochastic RSI + EMA
- MACD + RSI + Stochastic
- Keltner Channel variants

### Risk / position-management bots
- DCA Martingale Trading Bot
- DCA + Martingale Crypto Bot
- RSI DCA Bot

---

## Practical implementation rule from the playlist

If you turn these into automated trading agents, the safest construction is:
1. *Regime detector*
2. *Strategy selector*
3. *Entry engine*
4. *Exit engine*
5. *Risk manager*

That is the recurring lesson from the whole playlist: *the edge comes from matching the right logic to the right regime*.
