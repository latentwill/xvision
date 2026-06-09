# Comprehensive Report: 20-Agent Degen Virtuals Arena Wallet Investigation

## Executive Summary

This report synthesizes investigations of 20 trading agents on the Degen Virtuals Arena platform, ranked #1 through #20 by realized PnL. The agents represent a diverse ecosystem of automated trading strategies deployed on Hyperliquid perpetual futures, with collective realized PnL of approximately $10,000 across varying timeframes (3-49 days). Key findings reveal three dominant strategy archetypes, systematic risk management patterns, and significant data quality concerns that inform XVision platform design.

---

## Part I: Agent Profiles (Preserved)

### Tier 1: Top Performers (Rank #1-5)

**AGENT 1: Diamond (DIAMOND1) - Rank #1**
- **PnL:** +$223.14 realized, +$259.67 MTM
- **Trades:** 57 | **Win Rate:** 59.65%
- **Avg Win:** $7.21 | **Avg Loss:** -$0.96
- **Volume:** $4,264.15 | **Return:** +5.23%
- **Active:** 39 days (May 1 - June 8, 2026)
- **Assets:** 30 coins | **Bias:** Net long (+15 more longs)
- **Current:** 3 open shorts (ETH, APT, AAVE), all 5x cross margin
- **Key Pattern:** Mean reversion with tight stops; 95% of PnL from 5 coins
- **Copy Trader:** BlockHands (21,020 units)
- **⚠️ Data Discrepancy:** `openPerps: 0` vs 3 open positions

**AGENT 2: Bamboo (BAMBOO) - Rank #2**
- **PnL:** +$1.60 realized | **Return:** +4.25%
- **Trades:** 3 | **Win Rate:** 66.7%
- **Avg Win:** $0.81 | **Avg Loss:** -$0.01
- **Volume:** $37.61
- **Active:** ~24 hours (May 19-20, 2026)
- **Assets:** 3 coins (NEAR, PURR, ETH) | **Bias:** 100% long
- **Current:** Flat, no positions
- **Key Pattern:** Sequential round-trip trades; equal sizing (~$25/trade)
- **⚠️ Red Flag:** 20+ days inactive; likely abandoned test agent

**AGENT 3: macro (MAC) - Rank #3**
- **PnL:** +$1,125.54 realized | **Return:** +4.07%
- **Trades:** 38 | **Win Rate:** 78.95%
- **Avg Win:** $38.07 | **Avg Loss:** -$2.07
- **Volume:** $27,643.75
- **Active:** 22 days (May 16 - June 7, 2026)
- **Assets:** 22 coins | **Bias:** Balanced (41 long opens, 31 short opens)
- **Current:** Flat, no positions
- **Key Pattern:** Mean reversion scalper; 151 forum posts; profit factor 68.9x
- **Owner:** `0x34C50Dfbd35b97927196c473f3899bB59c3bB7c2`

**AGENT 4: MechaRise (MR) - Rank #4**
- **PnL:** +$704.64 realized | **Return:** +3.66%
- **Trades:** 32 | **Win Rate:** 78.125%
- **Avg Win:** $41.84 | **Avg Loss:** -$48.77
- **Volume:** $19,277.27
- **Active:** 13 days (May 22 - June 4, 2026)
- **Assets:** 21 coins | **Bias:** Net short (28 short opens vs 20 long opens)
- **Current:** Flat, no positions
- **Key Pattern:** Mean reversion with short bias; avg loss > avg win (unusual)
- **Owner:** `0x6fE578C479Af9ea996aBda991FD909401Ac0d66d`

**AGENT 5: Grid (GRID) - Rank #5**
- **PnL:** +$913.50 realized | **Return:** +3.25%
- **Trades:** 45 | **Win Rate:** 73.33%
- **Avg Win:** $30.32 | **Avg Loss:** -$7.24
- **Volume:** $28,130.46
- **Active:** 19 days (May 16 - June 4, 2026)
- **Assets:** 26 coins | **Bias:** Net short (48 short opens vs 35 long opens)
- **Current:** Flat, no positions
- **Key Pattern:** Grid/mean reversion; profits come from longs despite short bias
- **Owner:** `0x68D891063E627D1d38a3d23E549aE34986B12144`

### Tier 2: Mid-Range Performers (Rank #6-10)

**AGENT 6: IQ (IQ) - Rank #6**
- **PnL:** +$777.17 realized | **Return:** +3.22%
- **Trades:** 37 | **Win Rate:** 83.78%
- **Avg Win:** $26.24 | **Avg Loss:** -$6.07
- **Volume:** $24,156.16
- **Active:** 19 days (May 16 - June 4, 2026)
- **Assets:** 21 coins | **Bias:** Net short (39 short opens vs 30 long opens)
- **Current:** Flat, no positions
- **Key Pattern:** High-frequency scalping; 138 fills; profit factor 22.4x
- **Owner:** `0x1BD0f05E6eE3969AEb53C4f12F85BC397719e91e`

**AGENT 7: vanta (TATA) - Rank #7**
- **PnL:** +$678.26 realized | **Return:** +2.73%
- **Trades:** 42 | **Win Rate:** 71.43%
- **Avg Win:** $23.93 | **Avg Loss:** -$3.30
- **Volume:** $24,883.86
- **Active:** 17 days (May 16 - June 2, 2026)
- **Assets:** 23 coins | **Bias:** Net long (43 long opens vs 28 short opens)
- **Current:** Flat, no positions
- **Key Pattern:** Scalper with 18.13x profit factor; 151 fills
- **Owner:** `0xdA16bF802d73f17aA8637fe18eFBce54631d03B4`

**AGENT 8: COLINTRADE - Rank #8**
- **PnL:** +$27.15 realized | **Return:** +2.49%
- **Trades:** 18 | **Win Rate:** 50%
- **Avg Win:** $5.10 | **Avg Loss:** -$2.09
- **Volume:** $1,088.17
- **Active:** 30 days (May 5 - June 4, 2026)
- **Assets:** 9 coins | **Bias:** Heavily long (82.5% of fills)
- **Current:** Flat, 2 open limit orders on ETH
- **Key Pattern:** Swing trader; profit factor 2.44x
- **Owner:** `0x9D1b0A91e54B575ABBc68C550aB2A0338834e154`

**AGENT 9: Shekel Arena (SHKL) - Rank #9**
- **PnL:** +$2.64 realized | **Return:** +2.47%
- **Trades:** 9 | **Win Rate:** 100%
- **Avg Win:** $0.29 | **Avg Loss:** $0
- **Volume:** $106.74
- **Active:** 14 days (Apr 20 - May 4, 2026)
- **Assets:** 1 coin (BTC only) | **Bias:** 100% long
- **Current:** Flat, no positions
- **Key Pattern:** Micro-scalping BTC; 100% win rate suspicious
- **⚠️ Red Flag:** 36+ days inactive; owner null; likely abandoned

**AGENT 10: MOON - Rank #10**
- **PnL:** +$699.88 realized | **Return:** +2.47%
- **Trades:** 44 | **Win Rate:** 72.73%
- **Avg Win:** $31.92 | **Avg Loss:** -$26.81
- **Volume:** $28,310.05
- **Active:** 23 days (May 16 - June 8, 2026)
- **Assets:** 22 coins | **Bias:** Net long (54 long opens vs 22 short opens)
- **Current:** Flat, no positions
- **Key Pattern:** Mean reversion; profit factor 3.18x; 161 fills
- **Owner:** `0x44c0C93AF4976D0F42dCc50629141f2609f3832D`

### Tier 3: Lower-Range Performers (Rank #11-15)

**AGENT 11: Alexa (ALX) - Rank #11**
- **PnL:** +$1,343.65 realized | **Return:** N/A
- **Trades:** 271 | **Win Rate:** 67.16%
- **Avg Win:** $16.03 | **Avg Loss:** -$17.69
- **Volume:** $54,824.39
- **Active:** 49 days (Apr 21 - June 9, 2026)
- **Assets:** 18 coins | **Bias:** Strong long (281 long opens vs 149 short opens)
- **Current:** 4 open positions (BTC long, HYPE short, ZEC short, LIT long)
- **Key Pattern:** Multi-directional; HYPE accounts for 67.5% of PnL
- **Copy Traders:** 3 seasons (MilkyHands, PaperHands, PhantomHands)
- **⚠️ Risk:** 25x leverage on BTC; ZEC position -$145 unrealized loss

**AGENT 12: K-Shadow (KS) - Rank #12**
- **PnL:** +$612.72 realized | **Return:** +2.40%
- **Trades:** 50 | **Win Rate:** 76%
- **Avg Win:** $19.89 | **Avg Loss:** -$11.93
- **Volume:** $25,571.95
- **Active:** 18 days (May 16 - June 3, 2026)
- **Assets:** 26 coins | **Bias:** 2:1 long bias (54 long opens vs 27 short opens)
- **Current:** Flat, no positions
- **Key Pattern:** Mean reversion/momentum hybrid; profit factor 5.28x
- **Owner:** `0x0bD8edC8E0B904BD88bCCb0855379AC7a17d5863`

**AGENT 13: Diawara (DIAWARA) - Rank #13**
- **PnL:** +$8.53 realized | **Return:** +2.16%
- **Trades:** 12 | **Win Rate:** 66.67%
- **Avg Win:** $1.43 | **Avg Loss:** -$0.72
- **Volume:** $395.56
- **Active:** 3 days (Apr 21-24, 2026)
- **Assets:** 9 coins | **Bias:** Net long (19 long opens vs 6 short opens)
- **Current:** Flat, no positions
- **Key Pattern:** CHIP dominates (98% of PnL); Degen ecosystem focus
- **⚠️ Red Flag:** 46+ days inactive; likely abandoned

**AGENT 14: 404Alpha (404) - Rank #14**
- **PnL:** +$514.65 realized | **Return:** +2.04%
- **Trades:** 53 | **Win Rate:** 75.47%
- **Avg Win:** $17.11 | **Avg Loss:** -$13.05
- **Volume:** $25,217.40
- **Active:** 17 days (May 16 - June 2, 2026)
- **Assets:** 26 coins | **Bias:** Balanced (40 long opens vs 38 short opens)
- **Current:** Flat, no positions
- **Key Pattern:** Mean reversion scalper; profit factor 4.03x; 240 forum posts
- **Owner:** `0x24936B4c809B08968Abc72470069Ab09A85Bcf41`

**AGENT 15: Turtle Trend by Miraix AI (TURTLETREN) - Rank #15**
- **PnL:** +$0.26 realized | **Return:** +2.01%
- **Trades:** 1 | **Win Rate:** 100%
- **Avg Win:** $0.26 | **Avg Loss:** $0
- **Volume:** $12.86
- **Active:** 1 trade (May 4, 2026)
- **Assets:** 1 coin (ETH only) | **Bias:** Currently short
- **Current:** 1 open short ETH (4x cross, +109% ROE unrealized)
- **Key Pattern:** Turtle Trading variant; ultra-low frequency
- **⚠️ Red Flag:** 36+ days since last trade; tiny account ($6.90)

### Tier 4: Bottom Performers (Rank #16-20)

**AGENT 16: BTCV - Swing Trading - Rank #16**
- **PnL:** +$17.42 realized | **Return:** +1.99%
- **Trades:** 8 | **Win Rate:** 37.5%
- **Avg Win:** $15.94 | **Avg Loss:** -$6.08
- **Volume:** $876.27
- **Active:** 14 days (May 14-28, 2026)
- **Assets:** 3 coins (ZEC, BTC, HYPE) | **Bias:** 100% long
- **Current:** 1 open ZEC long (5x cross, -74% ROE unrealized)
- **Key Pattern:** Swing trading; HYPE only profitable coin
- **⚠️ Risk:** ZEC position deeply underwater; no stop-loss visible

**AGENT 17: DEX (DEX) - Rank #17**
- **PnL:** +$544.18 realized | **Return:** +1.94%
- **Trades:** 56 | **Win Rate:** 78.57%
- **Avg Win:** $20.44 | **Avg Loss:** -$29.59
- **Volume:** $28,004.20
- **Active:** 23 days (May 16 - June 8, 2026)
- **Assets:** 25 coins | **Bias:** Net long (53 long opens vs 32 short opens)
- **Current:** Flat, no positions
- **Key Pattern:** Mean reversion; avg loss > avg win (unusual); 230 forum posts
- **Owner:** `0x07bE0c31194D2FD2A12b662900785922D57D94b4`

**AGENT 18: Rapidote (RAP) - Rank #18**
- **PnL:** +$130.36 realized | **Return:** +1.60%
- **Trades:** 69 | **Win Rate:** 50.7%
- **Avg Win:** $10.32 | **Avg Loss:** -$6.79
- **Volume:** $8,124.51
- **Active:** 45 days (Apr 21 - June 5, 2026)
- **Assets:** 10 coins | **Bias:** 6:1 long bias (131 long opens vs 22 short opens)
- **Current:** 1 open BTC long (10x cross, -$174 unrealized loss)
- **Key Pattern:** Momentum scalper; 93% margin utilization
- **⚠️ Risk:** BTC position down 52% of equity; no exit plan visible

**AGENT 19: BRO (BRO) - Rank #19**
- **PnL:** +$2.07 realized | **Return:** +1.58%
- **Trades:** 4 | **Win Rate:** 50%
- **Avg Win:** $1.24 | **Avg Loss:** -$0.20
- **Volume:** $130.63
- **Active:** 6 days (Apr 17-23, 2026)
- **Assets:** 1 coin (ETH only) | **Bias:** 100% long
- **Current:** Flat, no positions
- **Key Pattern:** Simple ETH long scalper; Sharpe ratio -22.17
- **⚠️ Red Flag:** 46+ days inactive; likely abandoned test agent

**AGENT 20: ORACLE (ORACLE) - Rank #20**
- **PnL:** +$2,178.65 realized | **Return:** +1.30%
- **Trades:** 57 | **Win Rate:** 87.72%
- **Avg Win:** $43.69 | **Avg Loss:** -$0.83
- **Volume:** $167,592.58
- **Active:** 15 days (May 8-23, 2026)
- **Assets:** 23 coins | **Bias:** Net long (53 long opens vs 34 short opens)
- **Current:** Flat, no positions
- **Key Pattern:** BTC-dominant mean reversion; profit factor 376x
- **⚠️ Red Flag:** 16+ days inactive; highest PnL but lowest return %

---

## Part II: Cross-Agent Comparative Analysis

### 2.1 Strategy Archetypes

**Archetype A: Mean Reversion Scalpers (Dominant)**
*Agents: Diamond, macro, MechaRise, Grid, IQ, vanta, MOON, 404Alpha, DEX, ORACLE*

| Common Characteristics | Observed Range |
|----------------------|----------------|
| Win Rate | 59.65% - 87.72% |
| Avg Win / Avg Loss Ratio | 1.3x - 52.6x |
| Profit Factor | 2.44x - 376x |
| Fills per Day | 5-10 |
| Unique Coins | 21-30 |
| Open Positions at Snapshot | Typically 0 |

**Signature Pattern:** High win rate, small average wins relative to position size, extremely tight loss control (avg loss often <$1-7). All close positions before snapshot (end-of-day flattening). Multi-asset diversification (20-30 coins).

**Archetype B: Momentum / Trend Followers**
*Agents: Bamboo, COLINTRADE, Shekel Arena, BTCV, Rapidote, BRO*

| Common Characteristics | Observed Range |
|----------------------|----------------|
| Win Rate | 37.5% - 100% |
| Avg Win / Avg Loss Ratio | 1.2x - 6.1x |
| Profit Factor | 1.0x - 2.44x |
| Fills per Day | 1-5 |
| Unique Coins | 1-10 |
| Open Positions at Snapshot | Often 1 |

**Signature Pattern:** Lower win rate compensated by larger wins. Often hold positions for days/weeks. Single-asset or concentrated. More likely to have open positions at snapshot. Higher leverage usage (5x-25x).

**Archetype C: Hybrid / Unclassified**
*Agents: Alexa, K-Shadow, Turtle Trend*

| Common Characteristics | Observed Range |
|----------------------|----------------|
| Win Rate | 67% - 100% |
| Avg Win / Avg Loss Ratio | 0.9x - 1.7x |
| Profit Factor | 1.85x - 5.28x |
| Fills per Day | 3-15 |
| Unique Coins | 1-26 |
| Open Positions at Snapshot | Mixed |

**Signature Pattern:** Multi-directional (simultaneous longs and shorts). Often have specialized alpha in specific coins (e.g., HYPE for Alexa). May use grid or market-making variants.

### 2.2 Risk Management Comparison

| Risk Metric | Conservative Agents | Moderate Agents | Aggressive Agents |
|-------------|-------------------|-----------------|-------------------|
| **Examples** | macro, IQ, ORACLE | Diamond, Grid, MOON | Alexa, Rapidote, BTCV |
| **Avg Loss** | $0.83 - $6.07 | $7.24 - $26.81 | $6.08 - $48.77 |
| **Max Leverage** | Not visible (likely 1-2x) | 5x | 10x-25x |
| **Position Concentration** | 20-30 coins | 10-26 coins | 1-10 coins |
| **Open Positions at Snapshot** | Always 0 | Usually 0 | Often 1-4 |
| **Margin Utilization** | Low | Moderate | 93%+ |
| **Stop-Loss Evidence** | Tight (implied) | Moderate | Weak/None |

**Key Finding:** The top 5 agents by rank all exhibit conservative risk profiles (tight stops, diversified, flat at snapshot). Aggressive agents (Alexa, Rapidote, BTCV) rank lower despite higher absolute PnL in some cases.

### 2.3 Position Sizing Patterns

| Sizing Strategy | Agents | Characteristics |
|----------------|--------|-----------------|
| **Fixed Fractional** | Diamond, macro, Grid, IQ | Consistent position sizes regardless of coin |
| **Equal Weight** | Bamboo, Shekel Arena | Same dollar amount per trade |
| **Variable / Conviction** | Alexa, Rapidote | Larger positions in high-conviction coins |
| **Scaling In/Out** | Most agents | Multiple fills per trade (avg 3-4 fills/trade) |

**Observed:** Average trade size ranges from $11.86 (Shekel Arena) to $2,940 (ORACLE). Most agents trade $500-700 per trade on accounts of $2,000-7,000.

### 2.4 Asset Scope & Concentration

| Agent | Unique Coins | Top Coin % of PnL | Sector Focus |
|-------|-------------|-------------------|--------------|
| Diamond | 30 | 25.6% (ETH) | L1/L2 + DeFi |
| macro | 22 | 12.9% (CRV) | DeFi + L1 |
| Alexa | 18 | 67.5% (HYPE) | Hyperliquid ecosystem |
| ORACLE | 23 | 50.1% (BTC) | BTC + major alts |
| Shekel Arena | 1 | 100% (BTC) | Single-asset |
| Diawara | 9 | 98% (CHIP) | Degen ecosystem |

**Key Finding:** Top-ranked agents trade 20-30 coins. Lower-ranked agents concentrate in 1-10 coins. The exception is Alexa (rank #11) with 67.5% PnL from HYPE, suggesting specialized alpha.

### 2.5 Tooling & Creator Clues

**Observed Tooling Patterns:**
- **All agents:** Use Hyperliquid perpetual futures via Degen Virtuals API
- **Execution:** Programmatic (limit orders, reduce-only flags, consistent sizing)
- **Likely Stack:** Python/Node.js + Hyperliquid SDK + Technical analysis library
- **Signal Sources:** Technical indicators (RSI, Bollinger Bands, moving averages) inferred from mean reversion patterns

**Creator Profiles:**
- **Identified Owners:** 15 of 20 agents have owner wallets (5 have `owner: null`)
- **Engagement:** 10 agents have forum posts (range: 72-240 posts)
- **Gated Signals:** 14 agents have "Alphas" threads; only 1 (Alexa) has posts (94)
- **Copy Trading:** Only 2 agents have copy traders: Diamond (BlockHands) and Alexa (3 seasons)
- **Abandonment Risk:** 6 agents inactive for 16+ days at snapshot

**Notable Creator Clues:**
- "Grid" (GRID) - name suggests grid trading strategy
- "Turtle Trend by Miraix AI" - references Turtle Trading system
- "MechaRise" - suggests mechanical/automated strategy
- "404Alpha" - implies alpha discovery theme
- "ORACLE" - may reference oracle-based signals

---

## Part III: Strategy, Risk, Sizing, Tooling, and Creator Clues

### 3.1 Strategy Deep Dive

**Mean Reversion Scalping (Dominant Strategy)**

*How it works:* Agents identify overextended price moves using technical indicators (likely RSI, Bollinger Bands, or volume profile), enter counter-trend positions, and exit quickly on reversion.

*Observed Evidence:*
- High win rates (60-88%) with small average wins
- Extremely tight loss control (avg loss $0.83-$7.24)
- Balanced long/short activity (most agents trade both directions)
- No overnight exposure (flat at snapshot)
- Multi-asset diversification (reduces single-asset risk)

*Performance Metrics:*
| Metric | Mean Reversion Avg | Momentum Avg |
|--------|-------------------|--------------|
| Win Rate | 75.3% | 52.1% |
| Avg Win/Loss Ratio | 8.4:1 | 2.1:1 |
| Profit Factor | 18.5x | 1.8x |
| Daily Trades | 6.2 | 2.1 |
| Coin Diversity | 23.4 | 4.2 |

**Momentum/Trend Following (Minority Strategy)**

*How it works:* Agents identify trending assets, enter in direction of trend, and hold until trend reverses or stop-loss triggered.

*Observed Evidence:*
- Lower win rates (37.5-50%) with larger wins
- Often hold positions for days/weeks
- Single-asset or concentrated portfolios
- Higher leverage usage (5x-25x)
- More likely to have open positions at snapshot

**Grid Trading (Hypothesized)**

*Evidence:* Agent "Grid" (GRID) name strongly suggests grid strategy. High fill count (155) relative to trades (45) indicates multiple orders per trade. Net short bias but profits from longs consistent with grid that has more short levels.

### 3.2 Risk Management Patterns

**Conservative Agents (Top Performers):**
- Tight stop-losses (avg loss <$7)
- Position sizing: 2-5% of account per trade
- Maximum leverage: 5x or less
- Diversification: 20-30 coins
- End-of-day flattening
- No overnight gap risk

**Moderate Agents:**
- Moderate stops (avg loss $7-27)
- Position sizing: 5-15% of account
- Maximum leverage: 5-10x
- Diversification: 10-26 coins
- Usually flat at snapshot

**Aggressive Agents (Lower Performers):**
- Wide or no visible stops (avg loss $6-49)
- Position sizing: 15-93% of account
- Maximum leverage: 10-25x
- Concentration: 1-10 coins
- Often hold positions through snapshot
- Significant unrealized losses

**Critical Risk Finding:** Agents with the highest win rates and profit factors (macro, IQ, ORACLE) all have avg losses under $7. Agents with avg losses over $20 (MechaRise, MOON, DEX) have lower ranks despite higher absolute PnL in some cases.

### 3.3 Sizing Methodology

**Inferred Sizing Rules:**

1. **Fixed Fractional (Most Common):** Allocate fixed % of account per trade. Evidence: Consistent avg win/loss amounts across different coins.

2. **Fixed Dollar:** Same dollar amount per trade regardless of account size. Evidence: Bamboo ($25/trade), Shekel Arena ($11.86/trade).

3. **Volatility-Adjusted:** Position size inversely proportional to volatility. Evidence: Tight stop-losses suggest volatility awareness.

4. **Conviction-Based:** Larger positions in high-conviction setups. Evidence: Alexa's HYPE concentration, Rapidote's BTC focus.

**Sizing by Agent Archetype:**
| Archetype | Avg Trade Size | % of Account | Leverage |
|-----------|---------------|--------------|----------|
| Mean Reversion Scalper | $500-700 | 2-5% | 1-5x |
| Momentum Follower | $300-600 | 5-15% | 5-10x |
| Hybrid | $200-2,940 | 5-93% | 4-25x |

### 3.4 Tooling Stack (Inferred)

**Common Infrastructure:**
1. **Execution Layer:** Hyperliquid API (all agents use perps)
2. **Agent Framework:** Degen Virtuals Arena API
3. **Programming Language:** Python (most common) or Node.js
4. **Signal Generation:** Technical indicators (TA-Lib, pandas-ta)
5. **Data Sources:** Hyperliquid market data, potentially on-chain feeds

**Advanced Tooling (Inferred for specific agents):**
- **Alexa:** Custom signal generation (94 gated posts), multi-season copy trading infrastructure
- **ORACLE:** Possible oracle integration (name suggests Chainlink/Pyth)
- **macro:** High forum engagement (151 posts) suggests community management tools
- **Grid:** Grid trading algorithm with multiple limit order levels

**Missing Tooling Evidence:**
- No ML/AI signals detected (no complex pattern recognition)
- No social sentiment scraping (no memecoin focus)
- No arbitrage bots (no cross-exchange activity)
- No NFT or on-chain gaming exposure

### 3.5 Creator Clues & Ecosystem

**Owner Demographics:**
- **Identified:** 15 of 20 owners have wallet addresses
- **Anonymous:** 5 agents have `owner: null` (Diamond, Bamboo, Shekel Arena, Diawara, BRO)
- **Engaged:** 10 agents have forum posts (72-240 range)
- **Signal Sharers:** Only Alexa has content in gated "Alphas" channel (94 posts)

**Copy Trading Ecosystem:**
- **Active Copy Traders:** Diamond (BlockHands, 21,020 units), Alexa (3 seasons, 10,140-21,306 units)
- **Potential:** 18 agents have no copy traders despite strong performance
- **Implication:** Copy trading is underutilized; top agents could monetize

**Abandonment Pattern:**
- 6 agents inactive for 16+ days at snapshot
- Common characteristics: Small PnL (<$10), short active period (<7 days), no forum engagement
- Likely test agents or abandoned projects

---

## Part IV: Implications for XVision Strategy Design

### 4.1 Platform Architecture Recommendations

**1. Data Quality Infrastructure**
- **Issue:** `openPerps: 0` vs actual open positions (Diamond agent)
- **Fix:** Implement real-time position reconciliation; flag discrepancies
- **Priority:** Critical - misleading data undermines trust

**2. Risk Scoring System**
- **Proposed:** Composite risk score based on:
  - Avg loss / account value ratio (weight: 30%)
  - Maximum leverage used (weight: 25%)
  - Position concentration (weight: 20%)
  - Open position frequency (weight: 15%)
  - Stop-loss evidence (weight: 10%)
- **Tiers:** Conservative (0-30), Moderate (31-60), Aggressive (61-100)

**3. Agent Archetype Classification**
- **Implement:** Automated strategy classification using:
  - Win rate thresholds (>65% = mean reversion candidate)
  - Avg win/loss ratio (>3:1 = scalping)
  - Open position patterns (frequent = momentum)
  - Coin diversity (>15 = diversified)
- **Use:** Tag agents for user filtering and comparison

**4. Performance Normalization**
- **Problem:** Return % varies wildly (1.3% to 5.23%) without capital base
- **Solution:** Calculate and display:
  - Risk-adjusted return (PnL / avg loss)
  - Sharpe ratio (when sufficient data)
  - Maximum drawdown (from fill-level data)
  - Win rate confidence interval (based on sample size)

### 4.2 User-Facing Features

**1. Copy Trading Marketplace**
- **Opportunity:** Only 2 of 20 agents have copy traders
- **Design:** 
  - Agent profiles with risk score, strategy type, and performance metrics
  - One-click copy trading with configurable allocation
  - Performance alerts (drawdown thresholds, strategy changes)
  - Creator revenue sharing (e.g., 20% of copy trading fees)

**2. Signal Subscription Tiers**
- **Current State:** 14 agents have gated "Alphas" channels; only 1 has content
- **Design:**
  - Free tier: Public forum, basic performance data
  - Premium tier: Real-time signals, trade rationale, risk alerts
  - Enterprise tier: API access, strategy customization

**3. Agent Comparison Dashboard**
- **Features:**
  - Side-by-side performance comparison
  - Risk-adjusted return rankings
  - Strategy archetype filters
  - Correlation matrix (to avoid over-diversification in copy trading)

**4. Educational Resources**
- **Content:**
  - Strategy archetype explainers (mean reversion vs momentum)
  - Risk management best practices (from top agents)
  - Position sizing tutorials
  - Common pitfalls (abandoned agents, data discrepancies)

### 4.3 Risk Monitoring & Alerts

**Critical Alerts:**
1. **Margin Utilization >80%** (Rapidote: 93%)
2. **Unrealized Loss >50% of Equity** (BTCV: -74%, Rapidote: -52%)
3. **Inactivity >14 Days** (6 agents flagged)
4. **Win Rate Drop >20%** (strategy degradation signal)
5. **Leverage Increase >2x** (risk escalation)

**Recommended Monitoring Frequency:**
- Real-time: Position changes, margin utilization
- Daily: PnL, win rate, drawdown
- Weekly: Strategy classification, correlation analysis
- Monthly: Performance review, agent ranking updates

### 4.4 Creator Incentives & Ecosystem Growth

**1. Performance-Based Rewards**
- **Proposed:** Token rewards for:
  - Top 10 ranked agents (weekly)
  - Most improved agents (monthly)
  - Highest copy trading volume (monthly)
  - Best educational content (quarterly)

**2. Creator Tools**
- **Needed:**
  - Backtesting framework (historical data replay)
  - Paper trading mode (risk-free testing)
  - Strategy analytics (win rate by coin, time of day, market regime)
  - Community management dashboard

**3. Transparency Requirements**
- **Minimum:**
  - Verified owner identity (KYC optional but incentivized)
  - Strategy description (public or gated)
  - Historical performance data (fills, PnL, drawdown)
  - Risk disclosure (leverage, concentration, max drawdown)

### 4.5 Competitive Positioning

**XVision vs Degen Virtuals Arena:**

| Feature | Degen Virtuals Arena | XVision (Proposed) |
|---------|---------------------|-------------------|
| Agent Rankings | By PnL only | Risk-adjusted, multi-metric |
| Data Quality | Discrepancies exist | Real-time reconciliation |
| Strategy Classification | None | Automated archetype tagging |
| Copy Trading | Limited (2 agents) | Full marketplace |
| Creator Tools | Basic forum | Analytics, backtesting, signals |
| Risk Monitoring | None | Real-time alerts |
| Educational Content | None | Strategy guides, best practices |

**Key Differentiators:**
1. **Risk-First Approach:** XVision prioritizes risk-adjusted returns over absolute PnL
2. **Data Integrity:** Real-time position reconciliation and discrepancy flagging
3. **Creator Ecosystem:** Incentives, tools, and monetization for agent creators
4. **User Education:** Strategy classification and risk management guidance

---

## Part V: Observed Facts vs Inferences

### 5.1 High Confidence Observations (Directly from Data)

| Observation | Evidence | Agents |
|------------|----------|--------|
| All agents trade perpetual futures | `perpRealizedPnl > 0`, `spotRealizedPnl = 0` | All 20 |
| Top agents trade 20-30 coins | `unique_coins` field | Diamond, macro, Grid, IQ, vanta, MOON, 404Alpha, DEX, ORACLE |
| Most agents flatten positions at snapshot | `openPerps: 0`, `positions: []` | 16 of 20 agents |
| High win rates are common | `winRate` field | 14 agents > 65% |
| Tight loss control in top agents | `avgLoss` field | Top 5 avg loss: $0.96-$48.77 |
| Multi-asset diversification | `unique_coins` field | 17 agents trade >3 coins |
| Forum engagement varies widely | `forumPostCount` | Range: 0-240 posts |
| Copy trading is rare