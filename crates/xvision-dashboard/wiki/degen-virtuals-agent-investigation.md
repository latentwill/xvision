# Degen Virtuals Arena top-20 agent wallet investigation

> Snapshot date: 2026-06-09 UTC. Source bundle: `research/degen-virtuals-2026-06-09/`. This page is written for the xvision research wiki and should be read as an empirical snapshot, not investment advice.

## Research method

- Pulled the current top 20 agents from `https://degen.virtuals.io/api/leaderboard`.
- For each agent, fetched `https://degen.virtuals.io/api/agents/<id>` and `https://degen.virtuals.io/api/forums/<id>`.
- For each Hyperliquid wallet, fetched `userFills`, `clearinghouseState`, `portfolio`, and `openOrders` from `https://api.hyperliquid.xyz/info`.
- For token/creator metadata, fetched `https://api2.virtuals.io/api/virtuals/<virtualId>?populate=genesis,vibesInfo` where a Virtuals id was available.
- Ran a 20-task 100x DeepSeek pass, one task per agent wallet, then synthesis. Output: `research/degen-virtuals-2026-06-09/100x-agent-investigations/phase_1_synthesis.md`.
- Forum posts in gated `Alphas` threads were not fetched because unauthenticated `api/forums/<forumId>/posts` returned `401 Unauthorized`; the report uses visible post counts only.

## Run artifacts

- Raw leaderboard/API/Hyperliquid summaries: `research/degen-virtuals-2026-06-09/top20_summary_enriched.json`.
- Per-agent raw JSON: `research/degen-virtuals-2026-06-09/agent_XX_<id>.json`.
- Virtuals metadata JSON: `research/degen-virtuals-2026-06-09/virtuals_<virtualId>.json`.
- 100x tasks: `research/degen-virtuals-2026-06-09/100x_tasks_phased.json`.
- 100x pipeline cost: `$0.087018`; task status: `{'success': 20}`.

## Executive findings

- The top-20 snapshot contains 20/20 agents with all realized PnL coming from perps, not spot.
- Combined realized PnL across the fetched top 20 was $10,506.00 on reported trade volume of $468,648.57.
- 5/20 wallets had open Hyperliquid positions at fetch time; most agents were flat despite many having active trade histories.
- The strongest recurring strategy shape is diversified short-horizon mean reversion/scalping: high win rate, many coins, frequent flattening, and tight realized losses.
- Concentrated momentum/swing agents exist, but they carry more visible open-position risk and more inactivity/sample-size warnings.
- Creator metadata is thin: Virtuals creator wallets/user ids are visible, but most agents expose no public bio/socials. Exceptions include BambooAgentAI and bro/bluenami links.
- Data reconciliation matters: at least one agent showed `openPerps: 0` in Degen performance while Hyperliquid returned live open positions.

## Strategy archetype distribution

- diversified mean-reversion/scalping candidate: 10 agents
- hybrid tactical perp trader: 3 agents
- concentrated long-only momentum/scalp candidate: 3 agents
- trend-following / swing candidate: 2 agents
- grid / mean-reversion candidate: 1 agents
- concentrated swing / momentum candidate: 1 agents

## Top-20 ranking overview

- **#1 Diamond** (`DIAMOND1`): return 5.23%; realized $223.14; MTM $259.67; trades 57; win rate 59.65%; volume $4,264.15; unique coins 30; inferred `hybrid tactical perp trader` (medium).
- **#2 Bamboo** (`BAMBOO`): return 4.25%; realized $1.60; MTM $1.60; trades 3; win rate 66.67%; volume $37.61; unique coins 3; inferred `concentrated long-only momentum/scalp candidate` (low sample).
- **#3 macro** (`MAC`): return 4.07%; realized $1,125.54; MTM $1,125.54; trades 38; win rate 78.95%; volume $27,643.75; unique coins 22; inferred `diversified mean-reversion/scalping candidate` (medium).
- **#4 MechaRise** (`MR`): return 3.66%; realized $704.64; MTM $704.64; trades 32; win rate 78.12%; volume $19,277.27; unique coins 21; inferred `diversified mean-reversion/scalping candidate` (medium).
- **#5 Grid** (`GRID`): return 3.25%; realized $913.50; MTM $913.50; trades 45; win rate 73.33%; volume $28,130.46; unique coins 26; inferred `grid / mean-reversion candidate` (medium).
- **#6 IQ** (`IQ`): return 3.22%; realized $777.17; MTM $777.17; trades 37; win rate 83.78%; volume $24,156.16; unique coins 21; inferred `diversified mean-reversion/scalping candidate` (medium).
- **#7 vanta** (`TATA`): return 2.73%; realized $678.26; MTM $678.26; trades 42; win rate 71.43%; volume $24,883.86; unique coins 23; inferred `diversified mean-reversion/scalping candidate` (medium).
- **#8 colintrade** (`COLINTRADE`): return 2.49%; realized $27.15; MTM $27.15; trades 18; win rate 50.00%; volume $1,088.17; unique coins 9; inferred `hybrid tactical perp trader` (medium-low).
- **#9 Shekel Arena** (`SHKL`): return 2.47%; realized $2.64; MTM $2.64; trades 9; win rate 100.00%; volume $106.74; unique coins 1; inferred `concentrated long-only momentum/scalp candidate` (low sample).
- **#10 moon** (`MOON`): return 2.47%; realized $699.88; MTM $699.88; trades 44; win rate 72.73%; volume $28,310.05; unique coins 22; inferred `diversified mean-reversion/scalping candidate` (medium).
- **#11 Alexa** (`ALX`): return 2.45%; realized $1,343.65; MTM $1,321.22; trades 271; win rate 67.16%; volume $54,824.39; unique coins 18; inferred `diversified mean-reversion/scalping candidate` (medium).
- **#12 K-Shadow** (`KS`): return 2.40%; realized $612.72; MTM $612.72; trades 50; win rate 76.00%; volume $25,571.95; unique coins 26; inferred `diversified mean-reversion/scalping candidate` (medium).
- **#13 Diawara** (`DIAWARA`): return 2.16%; realized $8.53; MTM $8.53; trades 12; win rate 66.67%; volume $395.56; unique coins 9; inferred `hybrid tactical perp trader` (medium-low).
- **#14 404Alpha** (`404`): return 2.04%; realized $514.65; MTM $514.65; trades 53; win rate 75.47%; volume $25,217.40; unique coins 26; inferred `diversified mean-reversion/scalping candidate` (medium).
- **#15 Turtle Trend by Miraix AI** (`TURTLETREN`): return 2.01%; realized $0.26; MTM $3.92; trades 1; win rate 100.00%; volume $12.86; unique coins 1; inferred `trend-following / swing candidate` (low sample).
- **#16 BTCV - Swing Trading** (`no symbol`): return 1.99%; realized $17.42; MTM $2.48; trades 8; win rate 37.50%; volume $876.27; unique coins 3; inferred `trend-following / swing candidate` (low sample).
- **#17 DEX** (`DEX`): return 1.94%; realized $544.18; MTM $544.18; trades 56; win rate 78.57%; volume $28,004.20; unique coins 25; inferred `diversified mean-reversion/scalping candidate` (medium).
- **#18 Rapidote** (`RAP`): return 1.60%; realized $130.36; MTM $-41.62; trades 69; win rate 50.72%; volume $8,124.51; unique coins 10; inferred `concentrated swing / momentum candidate` (medium).
- **#19 bro** (`BRO`): return 1.58%; realized $2.07; MTM $2.07; trades 4; win rate 50.00%; volume $130.63; unique coins 1; inferred `concentrated long-only momentum/scalp candidate` (low sample).
- **#20 ORACLE** (`ORACLE`): return 1.30%; realized $2,178.65; MTM $2,178.65; trades 57; win rate 87.72%; volume $167,592.58; unique coins 23; inferred `diversified mean-reversion/scalping candidate` (medium).

## Per-agent investigations

### 1. Diamond (`DIAMOND1`)

**Official links and identifiers**
- Degen page: https://degen.virtuals.io/agents/958
- Virtuals page: https://app.virtuals.io/virtuals/74345
- Degen agent id: `958`; Virtuals id: `74345`
- Hyperliquid / agent wallet: `0xc40d6898b69449df91b4f8e04def4fcd1101406a`
- Token address: `0x4dBb9F7a72C62A16D8F60a370Ea4193a2E3093B5`; Virtuals preToken: `0x4dBb9F7a72C62A16D8F60a370Ea4193a2E3093B5`; pair: `0xFEf253C54b8e7438202EcD26DC6b2E67d5c827d6`

**Observed performance**
- Realized PnL: $223.14; perp realized PnL: $223.14; spot realized PnL: $0.00.
- Holdings value: $551.34; MTM PnL: $259.67; return: 5.23%.
- Trades: 57; wins/losses: 34/23; win rate: 59.65%; avg win/loss: $7.21/$-0.96.
- Reported trade volume: $4,264.15; Sharpe: n/a; last trade: 2026-06-08T21:09:33.051Z; performance calculated: 2026-06-09T06:06:14.519Z.

**Wallet / trading behavior**
- Hyperliquid fills fetched: 144; first/last fill: 2026-05-01T04:44:13.968000+00:00 → 2026-06-08T21:09:33.051000+00:00.
- Unique coins: 30; top coins by fills: SOL (13), ETH (12), SUI (9), LINK (9), PENDLE (9), BCH (7), AR (7), TIA (6).
- Top coins by notional volume: ETH ($1,389.78), SOL ($1,266.28), BCH ($917.23), PENDLE ($830.69), TIA ($739.51), LINK ($437.05), LDO ($397.24), SUI ($282.20).
- PnL by coin, largest absolute contributors: ETH ($57.18), SOL ($56.89), ONDO ($35.52), BCH ($32.35), PENDLE ($30.52), HYPE ($-7.77), OP ($7.31), LINK ($5.71), ICP ($5.26), LDO ($2.65).
- Direction counts: [['Open Long', 48], ['Close Long', 40], ['Open Short', 33], ['Close Short', 23]]; open-long/open-short fills: 48/33; close-long/close-short fills: 40/23.
- Closed fill wins/losses: 36/27; gross win/loss from fills: $245.14/$-21.99.

**Current Hyperliquid snapshot**
- Margin summary: `{'accountValue': '44.05333', 'totalNtlPos': '194.631908', 'totalRawUsd': '238.685238', 'totalMarginUsed': '38.926381'}`.
- Open positions: `[{'coin': 'ETH', 'szi': '-0.043', 'entryPx': '1858.34', 'positionValue': '72.5969', 'unrealizedPnl': '7.31196', 'returnOnEquity': '0.4575187282', 'liquidationPx': '14005.5649794802', 'marginUsed': '14.51938', 'maxLeverage': 25, 'leverage': {'type': 'cross', 'value': 5}}, {'coin': 'APT', 'szi': '-87.93', 'entryPx': '0.8301', 'positionValue': '58.526208', 'unrealizedPnl': '14.46989', 'returnOnEquity': '0.9911413347', 'liquidationPx': '6.5169562368', 'marginUsed': '11.705241', 'maxLeverage': 10, 'leverage': {'type': 'cross', 'value': 5}}, {'coin': 'AAVE', 'szi': '-1.01', 'entryPx': '73.9682', 'positionValue': '63.5088', 'unrealizedPnl': '11.19914', 'returnOnEquity': '0.7495280957', 'liquidationPx': '572.2955983027', 'marginUsed': '12.70176', 'maxLeverage': 10, 'leverage': {'type': 'cross', 'value': 5}}]`.
- Open orders: `[{'coin': 'AAVE', 'side': 'B', 'limitPx': '70.5', 'sz': '1.01', 'oid': 459027655771, 'timestamp': 1780657369649, 'origSz': '1.01', 'reduceOnly': True}, {'coin': 'ETH', 'side': 'B', 'limitPx': '1762.0', 'sz': '0.043', 'oid': 459027624745, 'timestamp': 1780657367602, 'origSz': '0.043', 'reduceOnly': True}, {'coin': 'APT', 'side': 'B', 'limitPx': '0.7322', 'sz': '87.93', 'oid': 458807259653, 'timestamp': 1780646659875, 'origSz': '87.93', 'reduceOnly': True}]`.

**Forum / signals surface**
- Degen `forumPostCount`: 0; visible threads: Discussion / DISCUSSION / gated=False / posts=0 / thread `959`; Alphas / SIGNALS / gated=True / posts=0 / thread `960`

**Creator / token metadata**
- Virtuals creator wallet `0xBD4917b8684Ff0C14EB4B0b1dCfBD3a32ADaBc27`
- Project members: Owner user 586086 (no verified links)
- Virtuals category/status/chain/factory: `IP MIRROR` / `UNDERGRAD` / `BASE` / `BONDING_V5`.
- Virtuals holder/liquidity/dev holding: holders 2; top10 holders 100%; liquidity $7,200.87; dev holding 0.1.

**Inference for xvision**
- Strategy inference: **hybrid tactical perp trader**; confidence: **medium**.
- Risk flags: large open-position ROE on APT: 0.99, large open-position ROE on AAVE: 0.75, high margin use 88%, Degen openPerps=0 disagrees with Hyperliquid open positions.
- Tooling inference: programmatic Hyperliquid execution is observed. Specific indicator stack, model stack, and creator runtime are not visible from public APIs; any RSI/Bollinger/grid/momentum language is inferred from fills, naming, and position patterns, not confirmed source code.

### 2. Bamboo (`BAMBOO`)

**Official links and identifiers**
- Degen page: https://degen.virtuals.io/agents/1168
- Virtuals page: https://app.virtuals.io/virtuals/76891
- Degen agent id: `1168`; Virtuals id: `76891`
- Hyperliquid / agent wallet: `0x8649f14009964411fc230e8f29032d00b35fb850`
- Token address: `0x673F6476D9679E7Ed6B25301A56BD5f678576eDf`; Virtuals preToken: `0x673F6476D9679E7Ed6B25301A56BD5f678576eDf`; pair: `0xa941ef32c14a30B69cAFb5D2763A94fF151aE7f3`

**Observed performance**
- Realized PnL: $1.60; perp realized PnL: $1.60; spot realized PnL: $0.00.
- Holdings value: $0.00; MTM PnL: $1.60; return: 4.25%.
- Trades: 3; wins/losses: 2/1; win rate: 66.67%; avg win/loss: $0.81/$-0.01.
- Reported trade volume: $37.61; Sharpe: n/a; last trade: 2026-05-20T12:48:25.813Z; performance calculated: 2026-06-09T04:31:06.332Z.

**Wallet / trading behavior**
- Hyperliquid fills fetched: 6; first/last fill: 2026-05-19T12:24:59.676000+00:00 → 2026-05-20T12:48:25.813000+00:00.
- Unique coins: 3; top coins by fills: NEAR (2), PURR (2), ETH (2).
- Top coins by notional volume: PURR ($25.46), ETH ($24.10), NEAR ($24.06).
- PnL by coin, largest absolute contributors: PURR ($1.49), NEAR ($0.12), ETH ($-0.01).
- Direction counts: [['Close Long', 3], ['Open Long', 3]]; open-long/open-short fills: 3/0; close-long/close-short fills: 3/0.
- Closed fill wins/losses: 2/1; gross win/loss from fills: $1.61/$-0.01.

**Current Hyperliquid snapshot**
- Margin summary: `{'accountValue': '0.0', 'totalNtlPos': '0.0', 'totalRawUsd': '0.0', 'totalMarginUsed': '0.0'}`.
- Open positions: `[]`.
- Open orders: `[]`.

**Forum / signals surface**
- Degen `forumPostCount`: 4; visible threads: Discussion / DISCUSSION / gated=False / posts=0 / thread `1249`; Alphas / SIGNALS / gated=True / posts=4 / thread `1250`

**Creator / token metadata**
- Virtuals creator wallet `0x8649F14009964411Fc230E8f29032d00b35fb850`
- Project members: Owner user 588324 (no verified links)
- Agent socials `{'VERIFIED_LINKS': {'TWITTER': 'https://x.com/BambooAgentAI'}, 'VERIFIED_USERNAMES': {'TWITTER': 'BambooAgentAI'}}`
- Virtuals category/status/chain/factory: `IP MIRROR` / `UNDERGRAD` / `BASE` / `BONDING_V5`.
- Virtuals holder/liquidity/dev holding: holders 3; top10 holders 100%; liquidity $7,192.12; dev holding 0.

**Inference for xvision**
- Strategy inference: **concentrated long-only momentum/scalp candidate**; confidence: **low sample**.
- Risk flags: small sample, inactive since before June 2026 snapshot.
- Tooling inference: programmatic Hyperliquid execution is observed. Specific indicator stack, model stack, and creator runtime are not visible from public APIs; any RSI/Bollinger/grid/momentum language is inferred from fills, naming, and position patterns, not confirmed source code.

### 3. macro (`MAC`)

**Official links and identifiers**
- Degen page: https://degen.virtuals.io/agents/1138
- Virtuals page: https://app.virtuals.io/virtuals/76673
- Degen agent id: `1138`; Virtuals id: `76673`
- Hyperliquid / agent wallet: `0x1e7ad2101fd8dfdfecf82e914064c7d14b7c4392`
- Token address: `0x219d4fb7D17020C6aCBC788731EDe79B9c68b255`; Virtuals preToken: `0x219d4fb7D17020C6aCBC788731EDe79B9c68b255`; pair: `0x454A339B754AB32389E492861E9eefF39F0E1A2d`

**Observed performance**
- Realized PnL: $1,125.54; perp realized PnL: $1,125.54; spot realized PnL: $0.00.
- Holdings value: $2,666.70; MTM PnL: $1,125.54; return: 4.07%.
- Trades: 38; wins/losses: 30/8; win rate: 78.95%; avg win/loss: $38.07/$-2.07.
- Reported trade volume: $27,643.75; Sharpe: n/a; last trade: 2026-06-07T15:46:16.547Z; performance calculated: 2026-06-09T06:06:14.519Z.

**Wallet / trading behavior**
- Hyperliquid fills fetched: 152; first/last fill: 2026-05-16T18:00:52.492000+00:00 → 2026-06-07T15:46:16.547000+00:00.
- Unique coins: 22; top coins by fills: ARB (16), CRV (14), UNI (11), VIRTUAL (10), APT (9), LDO (9), NEAR (8), DOT (8).
- Top coins by notional volume: CRV ($5,684.97), NEAR ($4,337.27), BNB ($3,808.52), ENA ($3,490.09), ARB ($3,402.21), AAVE ($3,313.81), HYPE ($3,101.55), TRUMP ($2,676.52).
- PnL by coin, largest absolute contributors: CRV ($145.67), NEAR ($123.91), WLD ($116.22), HYPE ($94.63), DOGE ($82.70), ENA ($70.93), AAVE ($66.20), TRUMP ($64.76), LTC ($54.05), ONDO ($50.92).
- Direction counts: [['Close Short', 43], ['Open Long', 41], ['Close Long', 37], ['Open Short', 31]]; open-long/open-short fills: 41/31; close-long/close-short fills: 37/43.
- Closed fill wins/losses: 71/9; gross win/loss from fills: $1,142.11/$-16.57.

**Current Hyperliquid snapshot**
- Margin summary: `{'accountValue': '0.0', 'totalNtlPos': '0.0', 'totalRawUsd': '0.0', 'totalMarginUsed': '0.0'}`.
- Open positions: `[]`.
- Open orders: `[]`.

**Forum / signals surface**
- Degen `forumPostCount`: 151; visible threads: Discussion / DISCUSSION / gated=False / posts=151 / thread `1195`; Alphas / SIGNALS / gated=True / posts=0 / thread `1196`

**Creator / token metadata**
- Virtuals creator wallet `0x34C50Dfbd35b97927196c473f3899bB59c3bB7c2`
- Degen owner `0x34C50Dfbd35b97927196c473f3899bB59c3bB7c2` (user id 1509)
- Project members: Owner user 437147 (no verified links)
- Virtuals category/status/chain/factory: `IP MIRROR` / `UNDERGRAD` / `BASE` / `BONDING_V5`.
- Virtuals holder/liquidity/dev holding: holders 1; top10 holders 100%; liquidity $7,192.08; dev holding 0.

**Inference for xvision**
- Strategy inference: **diversified mean-reversion/scalping candidate**; confidence: **medium**.
- Risk flags: no high-severity flag from fetched snapshot.
- Tooling inference: programmatic Hyperliquid execution is observed. Specific indicator stack, model stack, and creator runtime are not visible from public APIs; any RSI/Bollinger/grid/momentum language is inferred from fills, naming, and position patterns, not confirmed source code.

### 4. MechaRise (`MR`)

**Official links and identifiers**
- Degen page: https://degen.virtuals.io/agents/1147
- Virtuals page: https://app.virtuals.io/virtuals/76682
- Degen agent id: `1147`; Virtuals id: `76682`
- Hyperliquid / agent wallet: `0xc054cd8819ae25ecd0e4a2c02437d7ab69f8ac7c`
- Token address: `0x44060ba890cb73578a45a39DCe92dbC37fb86A7c`; Virtuals preToken: `0x44060ba890cb73578a45a39DCe92dbC37fb86A7c`; pair: `0x1c28c121834041c6599891371Ffd51890173d532`

**Observed performance**
- Realized PnL: $704.64; perp realized PnL: $704.64; spot realized PnL: $0.00.
- Holdings value: $2,107.62; MTM PnL: $704.64; return: 3.66%.
- Trades: 32; wins/losses: 25/7; win rate: 78.12%; avg win/loss: $41.84/$-48.77.
- Reported trade volume: $19,277.27; Sharpe: n/a; last trade: 2026-06-04T02:02:51.229Z; performance calculated: 2026-06-09T06:06:14.519Z.

**Wallet / trading behavior**
- Hyperliquid fills fetched: 104; first/last fill: 2026-05-22T17:01:20.066000+00:00 → 2026-06-04T02:02:51.229000+00:00.
- Unique coins: 21; top coins by fills: TON (10), WLD (10), DOT (9), AAVE (8), APT (8), UNI (7), DOGE (7), LINK (5).
- Top coins by notional volume: TON ($4,249.87), DOT ($3,397.15), DOGE ($3,390.65), ETH ($3,111.71), UNI ($2,656.76), ENA ($2,388.42), WLD ($2,095.95), SOL ($1,942.72).
- PnL by coin, largest absolute contributors: TON ($148.63), UNI ($-143.87), ENA ($107.96), WLD ($89.63), APT ($73.34), LINK ($66.26), CRV ($60.37), ARB ($58.15), ETH ($-57.74), SOL ($56.56).
- Direction counts: [['Close Short', 34], ['Open Short', 28], ['Close Long', 22], ['Open Long', 20]]; open-long/open-short fills: 20/28; close-long/close-short fills: 22/34.
- Closed fill wins/losses: 45/11; gross win/loss from fills: $1,046.03/$-341.39.

**Current Hyperliquid snapshot**
- Margin summary: `{'accountValue': '0.0', 'totalNtlPos': '0.0', 'totalRawUsd': '0.0', 'totalMarginUsed': '0.0'}`.
- Open positions: `[]`.
- Open orders: `[]`.

**Forum / signals surface**
- Degen `forumPostCount`: 129; visible threads: Discussion / DISCUSSION / gated=False / posts=129 / thread `1213`; Alphas / SIGNALS / gated=True / posts=0 / thread `1214`

**Creator / token metadata**
- Virtuals creator wallet `0x6fE578C479Af9ea996aBda991FD909401Ac0d66d`
- Degen owner `0x6fE578C479Af9ea996aBda991FD909401Ac0d66d` (user id 1501)
- Project members: Owner user 437171 (no verified links)
- Virtuals category/status/chain/factory: `IP MIRROR` / `UNDERGRAD` / `BASE` / `BONDING_V5`.
- Virtuals holder/liquidity/dev holding: holders 1; top10 holders 100%; liquidity $7,192.08; dev holding 0.

**Inference for xvision**
- Strategy inference: **diversified mean-reversion/scalping candidate**; confidence: **medium**.
- Risk flags: no high-severity flag from fetched snapshot.
- Tooling inference: programmatic Hyperliquid execution is observed. Specific indicator stack, model stack, and creator runtime are not visible from public APIs; any RSI/Bollinger/grid/momentum language is inferred from fills, naming, and position patterns, not confirmed source code.

### 5. Grid (`GRID`)

**Official links and identifiers**
- Degen page: https://degen.virtuals.io/agents/1129
- Virtuals page: https://app.virtuals.io/virtuals/76665
- Degen agent id: `1129`; Virtuals id: `76665`
- Hyperliquid / agent wallet: `0xc34c460fea6d74f26688b7f255047e862051d4c3`
- Token address: `0xcB579b46B186dCC2d20f52d5E283EAA254b57E7F`; Virtuals preToken: `0xcB579b46B186dCC2d20f52d5E283EAA254b57E7F`; pair: `0x0130cc7381b463c91e98d472Cd4BC5c01FDe4E7C`

**Observed performance**
- Realized PnL: $913.50; perp realized PnL: $913.50; spot realized PnL: $0.00.
- Holdings value: $2,575.12; MTM PnL: $913.50; return: 3.25%.
- Trades: 45; wins/losses: 33/12; win rate: 73.33%; avg win/loss: $30.32/$-7.24.
- Reported trade volume: $28,130.46; Sharpe: n/a; last trade: 2026-06-04T13:56:57.397Z; performance calculated: 2026-06-09T06:06:14.519Z.

**Wallet / trading behavior**
- Hyperliquid fills fetched: 155; first/last fill: 2026-05-16T17:44:55.329000+00:00 → 2026-06-04T13:56:57.397000+00:00.
- Unique coins: 26; top coins by fills: ARB (22), AAVE (16), CRV (13), LTC (11), DOGE (9), BNB (7), DOT (6), ADA (6).
- Top coins by notional volume: AAVE ($5,665.44), DOGE ($5,262.43), CRV ($4,551.90), ARB ($4,265.69), LTC ($3,653.67), ADA ($2,983.93), SOL ($2,821.51), BNB ($2,766.21).
- PnL by coin, largest absolute contributors: WLD ($133.31), CRV ($121.24), DOGE ($99.07), LTC ($71.23), ADA ($70.17), ARB ($61.61), AAVE ($56.69), VIRTUAL ($52.67), HYPE ($52.60), ZEC ($-51.88).
- Direction counts: [['Open Short', 48], ['Close Short', 39], ['Open Long', 35], ['Close Long', 33]]; open-long/open-short fills: 35/48; close-long/close-short fills: 33/39.
- Closed fill wins/losses: 56/16; gross win/loss from fills: $1,000.40/$-86.90.

**Current Hyperliquid snapshot**
- Margin summary: `{'accountValue': '0.0', 'totalNtlPos': '0.0', 'totalRawUsd': '0.0', 'totalMarginUsed': '0.0'}`.
- Open positions: `[]`.
- Open orders: `[]`.

**Forum / signals surface**
- Degen `forumPostCount`: 182; visible threads: Discussion / DISCUSSION / gated=False / posts=182 / thread `1179`; Alphas / SIGNALS / gated=True / posts=0 / thread `1180`

**Creator / token metadata**
- Virtuals creator wallet `0x68D891063E627D1d38a3d23E549aE34986B12144`
- Degen owner `0x68D891063E627D1d38a3d23E549aE34986B12144` (user id 1516)
- Project members: Owner user 434518 (no verified links)
- Virtuals category/status/chain/factory: `IP MIRROR` / `UNDERGRAD` / `BASE` / `BONDING_V5`.
- Virtuals holder/liquidity/dev holding: holders 4; top10 holders 100%; liquidity $7,192.52; dev holding 0.

**Inference for xvision**
- Strategy inference: **grid / mean-reversion candidate**; confidence: **medium**.
- Risk flags: no high-severity flag from fetched snapshot.
- Tooling inference: programmatic Hyperliquid execution is observed. Specific indicator stack, model stack, and creator runtime are not visible from public APIs; any RSI/Bollinger/grid/momentum language is inferred from fills, naming, and position patterns, not confirmed source code.

### 6. IQ (`IQ`)

**Official links and identifiers**
- Degen page: https://degen.virtuals.io/agents/1119
- Virtuals page: https://app.virtuals.io/virtuals/76622
- Degen agent id: `1119`; Virtuals id: `76622`
- Hyperliquid / agent wallet: `0x5cb20cf7b8288fa50484b05b8cb339ccf8448d30`
- Token address: `0x5CB5698B27ed0e41EED10Fe2851298a28e89caC4`; Virtuals preToken: `0x5CB5698B27ed0e41EED10Fe2851298a28e89caC4`; pair: `0x404EfE3243B268080f526Da71555bB062c9840A2`

**Observed performance**
- Realized PnL: $777.17; perp realized PnL: $777.17; spot realized PnL: $0.00.
- Holdings value: $2,018.51; MTM PnL: $777.17; return: 3.22%.
- Trades: 37; wins/losses: 31/6; win rate: 83.78%; avg win/loss: $26.24/$-6.07.
- Reported trade volume: $24,156.16; Sharpe: n/a; last trade: 2026-06-04T13:59:32.404Z; performance calculated: 2026-06-09T06:06:14.519Z.

**Wallet / trading behavior**
- Hyperliquid fills fetched: 138; first/last fill: 2026-05-16T07:35:30.465000+00:00 → 2026-06-04T13:59:32.404000+00:00.
- Unique coins: 21; top coins by fills: ARB (20), APT (12), UNI (12), ENA (11), TON (9), DOGE (9), AAVE (6), BNB (6).
- Top coins by notional volume: ENA ($3,993.10), SOL ($3,918.11), APT ($3,807.23), BNB ($3,644.49), TON ($3,614.15), ARB ($3,544.65), UNI ($3,318.73), ONDO ($3,175.90).
- PnL by coin, largest absolute contributors: ONDO ($134.85), WLD ($116.20), LTC ($84.92), ENA ($73.43), APT ($61.45), HYPE ($58.18), AAVE ($51.28), ARB ($41.48), CRV ($40.42), SOL ($37.01).
- Direction counts: [['Open Short', 39], ['Close Short', 38], ['Close Long', 31], ['Open Long', 30]]; open-long/open-short fills: 30/39; close-long/close-short fills: 31/38.
- Closed fill wins/losses: 60/9; gross win/loss from fills: $813.57/$-36.40.

**Current Hyperliquid snapshot**
- Margin summary: `{'accountValue': '0.0', 'totalNtlPos': '0.0', 'totalRawUsd': '0.0', 'totalMarginUsed': '0.0'}`.
- Open positions: `[]`.
- Open orders: `[]`.

**Forum / signals surface**
- Degen `forumPostCount`: 110; visible threads: Discussion / DISCUSSION / gated=False / posts=110 / thread `1161`; Alphas / SIGNALS / gated=True / posts=0 / thread `1162`

**Creator / token metadata**
- Virtuals creator wallet `0x1BD0f05E6eE3969AEb53C4f12F85BC397719e91e`
- Degen owner `0x1BD0f05E6eE3969AEb53C4f12F85BC397719e91e` (user id 1493)
- Project members: Owner user 434471 (no verified links)
- Virtuals category/status/chain/factory: `IP MIRROR` / `UNDERGRAD` / `BASE` / `BONDING_V5`.
- Virtuals holder/liquidity/dev holding: holders 1; top10 holders 100%; liquidity $7,192.08; dev holding 0.

**Inference for xvision**
- Strategy inference: **diversified mean-reversion/scalping candidate**; confidence: **medium**.
- Risk flags: no high-severity flag from fetched snapshot.
- Tooling inference: programmatic Hyperliquid execution is observed. Specific indicator stack, model stack, and creator runtime are not visible from public APIs; any RSI/Bollinger/grid/momentum language is inferred from fills, naming, and position patterns, not confirmed source code.

### 7. vanta (`TATA`)

**Official links and identifiers**
- Degen page: https://degen.virtuals.io/agents/1085
- Virtuals page: https://app.virtuals.io/virtuals/75857
- Degen agent id: `1085`; Virtuals id: `75857`
- Hyperliquid / agent wallet: `0x268b502ddadd9c8e3e64ae4dc6a395ea9b8eb86e`
- Token address: `0x2878FcB5fb49047b302A6f855FDf215422c2c7e8`; Virtuals preToken: `0x2878FcB5fb49047b302A6f855FDf215422c2c7e8`; pair: `0xa99Bde19ce20f06CD5668dd4D89cc57601f05021`

**Observed performance**
- Realized PnL: $678.26; perp realized PnL: $678.26; spot realized PnL: $0.00.
- Holdings value: $1,888.20; MTM PnL: $678.26; return: 2.73%.
- Trades: 42; wins/losses: 30/12; win rate: 71.43%; avg win/loss: $23.93/$-3.30.
- Reported trade volume: $24,883.86; Sharpe: n/a; last trade: 2026-06-02T15:51:38.687Z; performance calculated: 2026-06-09T06:06:14.519Z.

**Wallet / trading behavior**
- Hyperliquid fills fetched: 151; first/last fill: 2026-05-16T07:17:48.245000+00:00 → 2026-06-02T15:51:38.687000+00:00.
- Unique coins: 23; top coins by fills: BTC (16), SOL (14), ONDO (14), UNI (12), AAVE (11), WLD (9), ARB (7), TRUMP (7).
- Top coins by notional volume: AAVE ($5,044.62), TRUMP ($4,157.14), ONDO ($3,539.77), UNI ($3,219.60), WLD ($3,081.08), SOL ($2,993.32), BTC ($2,604.86), DOGE ($2,594.91).
- PnL by coin, largest absolute contributors: ONDO ($155.69), WLD ($98.17), HYPE ($77.91), AAVE ($66.70), TRUMP ($52.63), VIRTUAL ($46.18), CRV ($40.97), ZEC ($40.97), DOT ($26.37), ARB ($24.80).
- Direction counts: [['Open Long', 43], ['Close Short', 41], ['Close Long', 39], ['Open Short', 28]]; open-long/open-short fills: 43/28; close-long/close-short fills: 39/41.
- Closed fill wins/losses: 62/18; gross win/loss from fills: $717.86/$-39.60.

**Current Hyperliquid snapshot**
- Margin summary: `{'accountValue': '0.0', 'totalNtlPos': '0.0', 'totalRawUsd': '0.0', 'totalMarginUsed': '0.0'}`.
- Open positions: `[]`.
- Open orders: `[]`.

**Forum / signals surface**
- Degen `forumPostCount`: 72; visible threads: Discussion / DISCUSSION / gated=False / posts=72 / thread `1109`; Alphas / SIGNALS / gated=True / posts=0 / thread `1110`

**Creator / token metadata**
- Virtuals creator wallet `0xdA16bF802d73f17aA8637fe18eFBce54631d03B4`
- Degen owner `0xdA16bF802d73f17aA8637fe18eFBce54631d03B4` (user id 1488)
- Project members: Owner user 434462 (no verified links)
- Virtuals category/status/chain/factory: `IP MIRROR` / `UNDERGRAD` / `BASE` / `BONDING_V5`.
- Virtuals holder/liquidity/dev holding: holders 1; top10 holders 100%; liquidity $7,193.34; dev holding 0.

**Inference for xvision**
- Strategy inference: **diversified mean-reversion/scalping candidate**; confidence: **medium**.
- Risk flags: no high-severity flag from fetched snapshot.
- Tooling inference: programmatic Hyperliquid execution is observed. Specific indicator stack, model stack, and creator runtime are not visible from public APIs; any RSI/Bollinger/grid/momentum language is inferred from fills, naming, and position patterns, not confirmed source code.

### 8. colintrade (`COLINTRADE`)

**Official links and identifiers**
- Degen page: https://degen.virtuals.io/agents/998
- Virtuals page: https://app.virtuals.io/virtuals/74208
- Degen agent id: `998`; Virtuals id: `74208`
- Hyperliquid / agent wallet: `0x108e360db90398c2a0e47b4fb5d9a33c7545a5a7`
- Token address: `0xa59db3bd9e1781cb07b55A3dCfA6E22e959a58Fe`; Virtuals preToken: `0xa59db3bd9e1781cb07b55A3dCfA6E22e959a58Fe`; pair: `0xCf884892826c707cf045c6f4f87d6AEd1f64aA3F`

**Observed performance**
- Realized PnL: $27.15; perp realized PnL: $27.15; spot realized PnL: $0.00.
- Holdings value: $173.97; MTM PnL: $27.15; return: 2.49%.
- Trades: 18; wins/losses: 9/9; win rate: 50.00%; avg win/loss: $5.10/$-2.09.
- Reported trade volume: $1,088.17; Sharpe: n/a; last trade: 2026-06-04T05:29:19.516Z; performance calculated: 2026-06-09T06:06:14.519Z.

**Wallet / trading behavior**
- Hyperliquid fills fetched: 40; first/last fill: 2026-05-05T18:11:10.499000+00:00 → 2026-06-04T05:29:19.516000+00:00.
- Unique coins: 9; top coins by fills: SOL (9), HYPE (8), BTC (6), SUI (4), ETH (4), TAO (3), ZEC (2), TON (2).
- Top coins by notional volume: SOL ($526.81), ETH ($438.12), TON ($318.25), BTC ($254.35), HYPE ($254.30), SUI ($133.42), TAO ($115.93), ZEC ($101.75).
- PnL by coin, largest absolute contributors: TON ($18.47), BTC ($-13.43), ETH ($12.48), HYPE ($5.03), SUI ($3.47), WIF ($1.80), ZEC ($-1.45), SOL ($1.13), TAO ($-0.34).
- Direction counts: [['Close Long', 17], ['Open Long', 16], ['Close Short', 4], ['Open Short', 3]]; open-long/open-short fills: 16/3; close-long/close-short fills: 17/4.
- Closed fill wins/losses: 11/9; gross win/loss from fills: $45.94/$-18.80.

**Current Hyperliquid snapshot**
- Margin summary: `{'accountValue': '52.9038', 'totalNtlPos': '0.0', 'totalRawUsd': '52.9038', 'totalMarginUsed': '0.0'}`.
- Open positions: `[]`.
- Open orders: `[{'coin': 'ETH', 'side': 'B', 'limitPx': '1860.0', 'sz': '0.0582', 'oid': 456640280510, 'timestamp': 1780556629659, 'origSz': '0.0582', 'reduceOnly': True}, {'coin': 'ETH', 'side': 'A', 'limitPx': '1800.0', 'sz': '0.0582', 'oid': 456640247413, 'timestamp': 1780556627981, 'origSz': '0.0582'}]`.

**Forum / signals surface**
- Degen `forumPostCount`: 0; visible threads: Discussion / DISCUSSION / gated=False / posts=0 / thread `999`; Alphas / SIGNALS / gated=True / posts=0 / thread `1000`

**Creator / token metadata**
- Virtuals creator wallet `0x9D1b0A91e54B575ABBc68C550aB2A0338834e154`
- Degen owner `0x9D1b0A91e54B575ABBc68C550aB2A0338834e154` (user id 1606)
- Project members: Owner user 585957 (no verified links)
- Virtuals category/status/chain/factory: `IP MIRROR` / `UNDERGRAD` / `BASE` / `BONDING_V5`.
- Virtuals holder/liquidity/dev holding: holders 2; top10 holders 100%; liquidity $7,193.34; dev holding 0.

**Inference for xvision**
- Strategy inference: **hybrid tactical perp trader**; confidence: **medium-low**.
- Risk flags: no high-severity flag from fetched snapshot.
- Tooling inference: programmatic Hyperliquid execution is observed. Specific indicator stack, model stack, and creator runtime are not visible from public APIs; any RSI/Bollinger/grid/momentum language is inferred from fills, naming, and position patterns, not confirmed source code.

### 9. Shekel Arena (`SHKL`)

**Official links and identifiers**
- Degen page: https://degen.virtuals.io/agents/862
- Virtuals page: https://app.virtuals.io/virtuals/72858
- Degen agent id: `862`; Virtuals id: `72858`
- Hyperliquid / agent wallet: `0x47d854e1cca7f332113453cc4484ec6b04d035b7`
- Token address: `0x5f882caFdfA095Cb85D03DA388d162C80aECc5f4`; Virtuals preToken: `0x5f882caFdfA095Cb85D03DA388d162C80aECc5f4`; pair: `0xd31b8E83CF2130396dcD5088Cd358df33d283DD7`

**Observed performance**
- Realized PnL: $2.64; perp realized PnL: $2.64; spot realized PnL: $0.00.
- Holdings value: $11.51; MTM PnL: $2.64; return: 2.47%.
- Trades: 9; wins/losses: 9/0; win rate: 100.00%; avg win/loss: $0.29/$0.00.
- Reported trade volume: $106.74; Sharpe: n/a; last trade: 2026-05-04T01:56:00.149Z; performance calculated: 2026-06-09T04:13:00.985Z.

**Wallet / trading behavior**
- Hyperliquid fills fetched: 31; first/last fill: 2026-04-20T00:04:41.114000+00:00 → 2026-05-04T01:56:00.149000+00:00.
- Unique coins: 1; top coins by fills: BTC (31).
- Top coins by notional volume: BTC ($210.83).
- PnL by coin, largest absolute contributors: BTC ($2.64).
- Direction counts: [['Open Long', 22], ['Close Long', 9]]; open-long/open-short fills: 22/0; close-long/close-short fills: 9/0.
- Closed fill wins/losses: 9/0; gross win/loss from fills: $2.64/$0.00.

**Current Hyperliquid snapshot**
- Margin summary: `{'accountValue': '0.0', 'totalNtlPos': '0.0', 'totalRawUsd': '0.0', 'totalMarginUsed': '0.0'}`.
- Open positions: `[]`.
- Open orders: `[]`.

**Forum / signals surface**
- Degen `forumPostCount`: 0; visible threads: Discussion / DISCUSSION / gated=False / posts=0 / thread `885`; Alphas / SIGNALS / gated=True / posts=0 / thread `886`

**Creator / token metadata**
- Virtuals creator wallet `0x7a953f8db4036076dd0f538981c36461ca2471ae`
- Project members: Owner user 584949 (no verified links)
- description: Shekel Hyperliquid mirror agent for Degenerate Claw Arena
- Virtuals category/status/chain/factory: `ACP_LAUNCH` / `UNDERGRAD` / `BASE` / `BONDING_V5`.
- Virtuals holder/liquidity/dev holding: holders 9; top10 holders 100%; liquidity $7,226.90; dev holding 0.

**Inference for xvision**
- Strategy inference: **concentrated long-only momentum/scalp candidate**; confidence: **low sample**.
- Risk flags: small sample, inactive since before June 2026 snapshot.
- Tooling inference: programmatic Hyperliquid execution is observed. Specific indicator stack, model stack, and creator runtime are not visible from public APIs; any RSI/Bollinger/grid/momentum language is inferred from fills, naming, and position patterns, not confirmed source code.

### 10. moon (`MOON`)

**Official links and identifiers**
- Degen page: https://degen.virtuals.io/agents/1142
- Virtuals page: https://app.virtuals.io/virtuals/76677
- Degen agent id: `1142`; Virtuals id: `76677`
- Hyperliquid / agent wallet: `0x23009ece97ba15129cfe21a35954744fe0567c13`
- Token address: `0xF0B1d14D936185A37f7BcBeAD3DcbB3f1d0253A5`; Virtuals preToken: `0xF0B1d14D936185A37f7BcBeAD3DcbB3f1d0253A5`; pair: `0xcFde624b41ff370BeEF0D83152D40168CDBA83EB`

**Observed performance**
- Realized PnL: $699.88; perp realized PnL: $699.88; spot realized PnL: $0.00.
- Holdings value: $2,141.67; MTM PnL: $699.88; return: 2.47%.
- Trades: 44; wins/losses: 32/12; win rate: 72.73%; avg win/loss: $31.92/$-26.81.
- Reported trade volume: $28,310.05; Sharpe: n/a; last trade: 2026-06-08T11:53:52.789Z; performance calculated: 2026-06-09T06:06:14.519Z.

**Wallet / trading behavior**
- Hyperliquid fills fetched: 161; first/last fill: 2026-05-16T18:05:16.813000+00:00 → 2026-06-08T11:53:52.789000+00:00.
- Unique coins: 22; top coins by fills: WLD (14), TON (13), TRUMP (12), CRV (11), NEAR (10), VIRTUAL (10), LDO (10), ARB (9).
- Top coins by notional volume: CRV ($5,760.87), TRUMP ($5,565.36), NEAR ($3,915.52), TON ($3,880.60), WLD ($3,710.31), ARB ($3,395.91), DOT ($3,313.09), SOL ($2,401.88).
- PnL by coin, largest absolute contributors: WLD ($177.04), ARB ($-132.88), ONDO ($129.12), CRV ($112.36), TRUMP ($109.69), DOT ($-107.32), HYPE ($78.31), VIRTUAL ($77.82), TON ($77.71), APT ($44.34).
- Direction counts: [['Close Long', 55], ['Open Long', 54], ['Close Short', 30], ['Open Short', 22]]; open-long/open-short fills: 54/22; close-long/close-short fills: 55/30.
- Closed fill wins/losses: 68/16; gross win/loss from fills: $1,021.58/$-321.70.

**Current Hyperliquid snapshot**
- Margin summary: `{'accountValue': '0.0', 'totalNtlPos': '0.0', 'totalRawUsd': '0.0', 'totalMarginUsed': '0.0'}`.
- Open positions: `[]`.
- Open orders: `[]`.

**Forum / signals surface**
- Degen `forumPostCount`: 155; visible threads: Discussion / DISCUSSION / gated=False / posts=155 / thread `1203`; Alphas / SIGNALS / gated=True / posts=0 / thread `1204`

**Creator / token metadata**
- Virtuals creator wallet `0x44c0C93AF4976D0F42dCc50629141f2609f3832D`
- Degen owner `0x44c0C93AF4976D0F42dCc50629141f2609f3832D` (user id 1505)
- Project members: Owner user 437157 (no verified links)
- Virtuals category/status/chain/factory: `IP MIRROR` / `UNDERGRAD` / `BASE` / `BONDING_V5`.
- Virtuals holder/liquidity/dev holding: holders 4; top10 holders 100%; liquidity $7,192.55; dev holding 0.

**Inference for xvision**
- Strategy inference: **diversified mean-reversion/scalping candidate**; confidence: **medium**.
- Risk flags: no high-severity flag from fetched snapshot.
- Tooling inference: programmatic Hyperliquid execution is observed. Specific indicator stack, model stack, and creator runtime are not visible from public APIs; any RSI/Bollinger/grid/momentum language is inferred from fills, naming, and position patterns, not confirmed source code.

### 11. Alexa (`ALX`)

**Official links and identifiers**
- Degen page: https://degen.virtuals.io/agents/873
- Virtuals page: https://app.virtuals.io/virtuals/73338
- Degen agent id: `873`; Virtuals id: `73338`
- Hyperliquid / agent wallet: `0x26e2349e0dbdc5136f11cb3d76644e85fb0ad4d5`
- Token address: `0x4363B533aA4d2b7aF3c88ac0f82871bf3da1958f`; Virtuals preToken: `0x4363B533aA4d2b7aF3c88ac0f82871bf3da1958f`; pair: `0x9B80426785f2A75d3671A4da8e875ae9E08A0f37`

**Observed performance**
- Realized PnL: $1,343.65; perp realized PnL: $1,343.65; spot realized PnL: $0.00.
- Holdings value: $7,026.31; MTM PnL: $1,321.22; return: 2.45%.
- Trades: 271; wins/losses: 182/89; win rate: 67.16%; avg win/loss: $16.03/$-17.69.
- Reported trade volume: $54,824.39; Sharpe: n/a; last trade: 2026-06-09T05:35:35.003Z; performance calculated: 2026-06-09T06:06:14.519Z.

**Wallet / trading behavior**
- Hyperliquid fills fetched: 745; first/last fill: 2026-04-21T08:01:44.058000+00:00 → 2026-06-09T06:06:36.962000+00:00.
- Unique coins: 18; top coins by fills: BTC (156), HYPE (146), ETH (81), LIT (77), ZEC (67), XRP (45), SOL (44), xyz:SPCX (42).
- Top coins by notional volume: BTC ($39,308.35), HYPE ($20,235.15), ETH ($16,676.95), ZEC ($7,819.45), NEAR ($5,797.13), LIT ($4,801.53), xyz:SPCX ($3,670.86), XRP ($3,349.41).
- PnL by coin, largest absolute contributors: HYPE ($1,402.68), ZEC ($-916.44), BTC ($284.13), XLM ($277.73), xyz:SPCX ($-200.46), LIT ($194.80), NEAR ($188.59), ETH ($94.35), XRP ($54.78), xyz:CL ($-31.35).
- Direction counts: [['Open Long', 281], ['Close Long', 180], ['Open Short', 149], ['Close Short', 134], ['Short > Long', 1]]; open-long/open-short fills: 281/149; close-long/close-short fills: 180/134.
- Closed fill wins/losses: 214/101; gross win/loss from fills: $2,918.26/$-1,574.76.

**Current Hyperliquid snapshot**
- Margin summary: `{'accountValue': '480.705469', 'totalNtlPos': '3339.81976', 'totalRawUsd': '-101.573171', 'totalMarginUsed': '453.393712'}`.
- Open positions: `[{'coin': 'BTC', 'szi': '0.00757', 'entryPx': '63302.2', 'positionValue': '479.3324', 'unrealizedPnl': '0.13417', 'returnOnEquity': '0.0069997128', 'liquidationPx': None, 'marginUsed': '19.173296', 'maxLeverage': 40, 'leverage': {'type': 'cross', 'value': 25}}, {'coin': 'HYPE', 'szi': '-7.99', 'entryPx': '62.7475', 'positionValue': '496.05116', 'unrealizedPnl': '5.30144', 'returnOnEquity': '0.1057427447', 'liquidationPx': '872.0148167352', 'marginUsed': '49.605116', 'maxLeverage': 10, 'leverage': {'type': 'cross', 'value': 10}}, {'coin': 'ZEC', 'szi': '-1.94', 'entryPx': '379.9512', 'positionValue': '882.7194', 'unrealizedPnl': '-145.614', 'returnOnEquity': '-1.9754841031', 'liquidationPx': '3790.7559926362', 'marginUsed': '88.27194', 'maxLeverage': 10, 'leverage': {'type': 'cross', 'value': 10}}, {'coin': 'LIT', 'szi': '927.0', 'entryPx': '1.480321', 'positionValue': '1481.7168', 'unrealizedPnl': '109.4584', 'returnOnEquity': '0.3988257605', 'liquidationPx': None, 'marginUsed': '296.34336', 'maxLeverage': 5, 'leverage': {'type': 'cross', 'value': 5}}]`.
- Open orders: `[]`.

**Forum / signals surface**
- Degen `forumPostCount`: 94; visible threads: Discussion / DISCUSSION / gated=False / posts=0 / thread `895`; Alphas / SIGNALS / gated=True / posts=94 / thread `896`

**Creator / token metadata**
- Virtuals creator wallet `0x93b0886f44be1e61b8a3dae6f1d1491e5a82b349`
- Degen owner `0x93b0886f44bE1e61b8A3Dae6F1d1491E5a82b349` (user id 1166)
- Project members: Owner user 585192 (no verified links)
- description: sharp, no-nonsense crypto trader with 7 years of experience riding bull runs and surviving brutal bear markets
- Virtuals category/status/chain/factory: `ACP_LAUNCH` / `UNDERGRAD` / `BASE` / `BONDING_V5`.
- Virtuals holder/liquidity/dev holding: holders 7; top10 holders 100%; liquidity $7,380.56; dev holding 0.

**Inference for xvision**
- Strategy inference: **diversified mean-reversion/scalping candidate**; confidence: **medium**.
- Risk flags: high leverage 25x on BTC, high leverage 10x on HYPE, large open-position ROE on ZEC: -1.98, high leverage 10x on ZEC, high margin use 94%, Degen openPerps=0 disagrees with Hyperliquid open positions.
- Tooling inference: programmatic Hyperliquid execution is observed. Specific indicator stack, model stack, and creator runtime are not visible from public APIs; any RSI/Bollinger/grid/momentum language is inferred from fills, naming, and position patterns, not confirmed source code.

### 12. K-Shadow (`KS`)

**Official links and identifiers**
- Degen page: https://degen.virtuals.io/agents/1074
- Virtuals page: https://app.virtuals.io/virtuals/75846
- Degen agent id: `1074`; Virtuals id: `75846`
- Hyperliquid / agent wallet: `0x4f8cfde2e5ec87824383868246d6ed8d467ff092`
- Token address: `0x696A5036fD152fAaF609A1bE159e761632c6Feed`; Virtuals preToken: `0x696A5036fD152fAaF609A1bE159e761632c6Feed`; pair: `0x6Eabfb7B11dAd39bD23f307D01c3f17dAEAC3b58`

**Observed performance**
- Realized PnL: $612.72; perp realized PnL: $612.72; spot realized PnL: $0.00.
- Holdings value: $1,829.59; MTM PnL: $612.72; return: 2.40%.
- Trades: 50; wins/losses: 38/12; win rate: 76.00%; avg win/loss: $19.89/$-11.93.
- Reported trade volume: $25,571.95; Sharpe: n/a; last trade: 2026-06-03T05:45:29.521Z; performance calculated: 2026-06-09T06:06:14.519Z.

**Wallet / trading behavior**
- Hyperliquid fills fetched: 159; first/last fill: 2026-05-16T06:53:57.056000+00:00 → 2026-06-03T05:45:29.521000+00:00.
- Unique coins: 26; top coins by fills: BTC (11), TON (11), ENA (10), LTC (10), ARB (10), APT (10), DOT (9), AAVE (7).
- Top coins by notional volume: TON ($3,992.63), ENA ($3,903.28), AAVE ($3,237.32), APT ($3,236.14), LTC ($3,091.03), DOT ($3,064.81), HYPE ($3,032.29), ADA ($2,743.84).
- PnL by coin, largest absolute contributors: HYPE ($126.42), NEAR ($105.96), ENA ($91.57), SUI ($-83.42), ONDO ($71.19), DOT ($51.04), VIRTUAL ($39.42), TRUMP ($33.55), BNB ($33.04), APT ($32.20).
- Direction counts: [['Open Long', 54], ['Close Long', 48], ['Close Short', 30], ['Open Short', 27]]; open-long/open-short fills: 54/27; close-long/close-short fills: 48/30.
- Closed fill wins/losses: 63/15; gross win/loss from fills: $755.91/$-143.19.

**Current Hyperliquid snapshot**
- Margin summary: `{'accountValue': '0.0', 'totalNtlPos': '0.0', 'totalRawUsd': '0.0', 'totalMarginUsed': '0.0'}`.
- Open positions: `[]`.
- Open orders: `[]`.

**Forum / signals surface**
- Degen `forumPostCount`: 107; visible threads: Discussion / DISCUSSION / gated=False / posts=107 / thread `1087`; Alphas / SIGNALS / gated=True / posts=0 / thread `1088`

**Creator / token metadata**
- Virtuals creator wallet `0x0bD8edC8E0B904BD88bCCb0855379AC7a17d5863`
- Degen owner `0x0bD8edC8E0B904BD88bCCb0855379AC7a17d5863` (user id 1479)
- Project members: Owner user 434376 (no verified links)
- Virtuals category/status/chain/factory: `IP MIRROR` / `UNDERGRAD` / `BASE` / `BONDING_V5`.
- Virtuals holder/liquidity/dev holding: holders 1; top10 holders 100%; liquidity $7,193.34; dev holding 0.

**Inference for xvision**
- Strategy inference: **diversified mean-reversion/scalping candidate**; confidence: **medium**.
- Risk flags: no high-severity flag from fetched snapshot.
- Tooling inference: programmatic Hyperliquid execution is observed. Specific indicator stack, model stack, and creator runtime are not visible from public APIs; any RSI/Bollinger/grid/momentum language is inferred from fills, naming, and position patterns, not confirmed source code.

### 13. Diawara (`DIAWARA`)

**Official links and identifiers**
- Degen page: https://degen.virtuals.io/agents/779
- Virtuals page: https://app.virtuals.io/virtuals/72179
- Degen agent id: `779`; Virtuals id: `72179`
- Hyperliquid / agent wallet: `0x49ddcc3358d88941c7549f277bba4f5eaf4bdc47`
- Token address: `0xFCE8fcf8daC75F2f1793059Ac0B247507Ca25D21`; Virtuals preToken: `0xFCE8fcf8daC75F2f1793059Ac0B247507Ca25D21`; pair: `0x9149C7EA689e4f9F0287D6f4f88dB804b286C0f1`

**Observed performance**
- Realized PnL: $8.53; perp realized PnL: $8.53; spot realized PnL: $0.00.
- Holdings value: $0.04; MTM PnL: $8.53; return: 2.16%.
- Trades: 12; wins/losses: 8/4; win rate: 66.67%; avg win/loss: $1.43/$-0.72.
- Reported trade volume: $395.56; Sharpe: n/a; last trade: 2026-04-24T05:54:15.513Z; performance calculated: 2026-06-09T04:13:00.985Z.

**Wallet / trading behavior**
- Hyperliquid fills fetched: 37; first/last fill: 2026-04-21T19:58:12.554000+00:00 → 2026-04-24T05:54:15.513000+00:00.
- Unique coins: 9; top coins by fills: CHIP (12), BIO (6), SUPER (4), OP (3), TAO (3), SEI (3), APEX (2), ACE (2).
- Top coins by notional volume: CHIP ($285.47), BIO ($189.82), SUPER ($83.60), APEX ($60.13), ACE ($60.13), SEI ($52.36), TAO ($40.53), OP ($14.28).
- PnL by coin, largest absolute contributors: CHIP ($8.33), SEI ($2.33), BIO ($-2.17), SUPER ($-0.42), VIRTUAL ($0.32), TAO ($0.17), APEX ($-0.14), OP ($0.10), ACE ($0.03).
- Direction counts: [['Open Long', 19], ['Close Long', 10], ['Open Short', 6], ['Close Short', 2]]; open-long/open-short fills: 19/6; close-long/close-short fills: 10/2.
- Closed fill wins/losses: 8/4; gross win/loss from fills: $11.41/$-2.88.

**Current Hyperliquid snapshot**
- Margin summary: `{'accountValue': '0.0', 'totalNtlPos': '0.0', 'totalRawUsd': '0.0', 'totalMarginUsed': '0.0'}`.
- Open positions: `[]`.
- Open orders: `[]`.

**Forum / signals surface**
- Degen `forumPostCount`: 0; visible threads: Discussion / DISCUSSION / gated=False / posts=0 / thread `849`; Alphas / SIGNALS / gated=True / posts=0 / thread `850`

**Creator / token metadata**
- Virtuals creator wallet `0xe209a914e26398bcd31f2c369263a879d9d83597`
- Degen owner `0xE209a914e26398Bcd31F2C369263a879d9d83597` (user id 147)
- Project members: Owner user 565816 (no verified links)
- description: Diawara
- Virtuals category/status/chain/factory: `ACP_LAUNCH` / `UNDERGRAD` / `BASE` / `BONDING_V5`.
- Virtuals holder/liquidity/dev holding: holders 2; top10 holders 100%; liquidity $7,196.96; dev holding 0.

**Inference for xvision**
- Strategy inference: **hybrid tactical perp trader**; confidence: **medium-low**.
- Risk flags: inactive since before June 2026 snapshot.
- Tooling inference: programmatic Hyperliquid execution is observed. Specific indicator stack, model stack, and creator runtime are not visible from public APIs; any RSI/Bollinger/grid/momentum language is inferred from fills, naming, and position patterns, not confirmed source code.

### 14. 404Alpha (`404`)

**Official links and identifiers**
- Degen page: https://degen.virtuals.io/agents/1078
- Virtuals page: https://app.virtuals.io/virtuals/75850
- Degen agent id: `1078`; Virtuals id: `75850`
- Hyperliquid / agent wallet: `0x6f2b86e4ebcf9f417243fd3445656a43d213fbbe`
- Token address: `0x5f306828CE91Fd96c3EC251E2CA030a6cbE9fab0`; Virtuals preToken: `0x5f306828CE91Fd96c3EC251E2CA030a6cbE9fab0`; pair: `0x7FdABbD44026338f2e3bB2d6f9FD81C73cf609EC`

**Observed performance**
- Realized PnL: $514.65; perp realized PnL: $514.65; spot realized PnL: $0.00.
- Holdings value: $1,730.03; MTM PnL: $514.65; return: 2.04%.
- Trades: 53; wins/losses: 40/13; win rate: 75.47%; avg win/loss: $17.11/$-13.05.
- Reported trade volume: $25,217.40; Sharpe: n/a; last trade: 2026-06-02T15:50:15.461Z; performance calculated: 2026-06-09T06:06:14.519Z.

**Wallet / trading behavior**
- Hyperliquid fills fetched: 158; first/last fill: 2026-05-16T07:09:07.481000+00:00 → 2026-06-02T15:50:15.461000+00:00.
- Unique coins: 26; top coins by fills: UNI (14), ARB (13), ENA (11), APT (10), BTC (9), TRUMP (9), LINK (8), ONDO (8).
- Top coins by notional volume: UNI ($3,900.82), LINK ($3,275.95), ENA ($3,200.26), ADA ($3,185.08), ONDO ($3,182.85), TRUMP ($3,097.55), AAVE ($3,009.74), SUI ($2,790.05).
- PnL by coin, largest absolute contributors: ONDO ($139.29), NEAR ($97.92), LINK ($58.65), HYPE ($-54.02), VIRTUAL ($52.07), ENA ($33.53), APT ($32.56), DOT ($31.02), TON ($-26.41), ETH ($24.12).
- Direction counts: [['Close Short', 41], ['Open Long', 40], ['Close Long', 39], ['Open Short', 38]]; open-long/open-short fills: 40/38; close-long/close-short fills: 39/41.
- Closed fill wins/losses: 67/13; gross win/loss from fills: $684.35/$-169.70.

**Current Hyperliquid snapshot**
- Margin summary: `{'accountValue': '0.0', 'totalNtlPos': '0.0', 'totalRawUsd': '0.0', 'totalMarginUsed': '0.0'}`.
- Open positions: `[]`.
- Open orders: `[]`.

**Forum / signals surface**
- Degen `forumPostCount`: 240; visible threads: Discussion / DISCUSSION / gated=False / posts=240 / thread `1095`; Alphas / SIGNALS / gated=True / posts=0 / thread `1096`

**Creator / token metadata**
- Virtuals creator wallet `0x24936B4c809B08968Abc72470069Ab09A85Bcf41`
- Degen owner `0x24936B4c809B08968Abc72470069Ab09A85Bcf41` (user id 1476)
- Project members: Owner user 434383 (no verified links)
- Virtuals category/status/chain/factory: `IP MIRROR` / `UNDERGRAD` / `BASE` / `BONDING_V5`.
- Virtuals holder/liquidity/dev holding: holders 1; top10 holders 100%; liquidity $7,193.34; dev holding 0.

**Inference for xvision**
- Strategy inference: **diversified mean-reversion/scalping candidate**; confidence: **medium**.
- Risk flags: no high-severity flag from fetched snapshot.
- Tooling inference: programmatic Hyperliquid execution is observed. Specific indicator stack, model stack, and creator runtime are not visible from public APIs; any RSI/Bollinger/grid/momentum language is inferred from fills, naming, and position patterns, not confirmed source code.

### 15. Turtle Trend by Miraix AI (`TURTLETREN`)

**Official links and identifiers**
- Degen page: https://degen.virtuals.io/agents/992
- Virtuals page: https://app.virtuals.io/virtuals/74860
- Degen agent id: `992`; Virtuals id: `74860`
- Hyperliquid / agent wallet: `0x7530ead023fb0d38f971bd3065ec954d4e4cdc57`
- Token address: `0x925a3fAC4a143aeEaF53D1eC3A83FA4Cf7453556`; Virtuals preToken: `0x925a3fAC4a143aeEaF53D1eC3A83FA4Cf7453556`; pair: `0x80E587Cd490975E7EFEdAD273D07e84E602931F4`

**Observed performance**
- Realized PnL: $0.26; perp realized PnL: $0.26; spot realized PnL: $0.00.
- Holdings value: $12.98; MTM PnL: $3.92; return: 2.01%.
- Trades: 1; wins/losses: 1/0; win rate: 100.00%; avg win/loss: $0.26/$0.00.
- Reported trade volume: $12.86; Sharpe: n/a; last trade: 2026-05-04T10:16:07.122Z; performance calculated: 2026-06-09T04:13:00.985Z.

**Wallet / trading behavior**
- Hyperliquid fills fetched: 3; first/last fill: 2026-05-03T22:25:40.524000+00:00 → 2026-05-04T10:16:07.122000+00:00.
- Unique coins: 1; top coins by fills: ETH (3).
- Top coins by notional volume: ETH ($38.49).
- PnL by coin, largest absolute contributors: ETH ($0.26).
- Direction counts: [['Open Short', 1], ['Close Long', 1], ['Open Long', 1]]; open-long/open-short fills: 1/1; close-long/close-short fills: 1/0.
- Closed fill wins/losses: 1/0; gross win/loss from fills: $0.26/$0.00.

**Current Hyperliquid snapshot**
- Margin summary: `{'accountValue': '6.900632', 'totalNtlPos': '9.4528', 'totalRawUsd': '16.353432', 'totalMarginUsed': '2.3632'}`.
- Open positions: `[{'coin': 'ETH', 'szi': '-0.0056', 'entryPx': '2324.7', 'positionValue': '9.4528', 'unrealizedPnl': '3.56552', 'returnOnEquity': '1.0955392094', 'liquidationPx': '3909.4576330532', 'marginUsed': '2.3632', 'maxLeverage': 25, 'leverage': {'type': 'cross', 'value': 4}}]`.
- Open orders: `[]`.

**Forum / signals surface**
- Degen `forumPostCount`: 25; visible threads: Discussion / DISCUSSION / gated=False / posts=25 / thread `989`; Alphas / SIGNALS / gated=True / posts=0 / thread `990`

**Creator / token metadata**
- Virtuals creator wallet `0xf9c1dfafa383270Bb92bC368815e205EAf30E2bC`
- Degen owner `0xf9c1dfafa383270Bb92bC368815e205EAf30E2bC` (user id 1341)
- Project members: Owner user 476855 (no verified links)
- Virtuals category/status/chain/factory: `IP MIRROR` / `UNDERGRAD` / `BASE` / `BONDING_V5`.
- Virtuals holder/liquidity/dev holding: holders 1; top10 holders 100%; liquidity $7,159.32; dev holding 0.

**Inference for xvision**
- Strategy inference: **trend-following / swing candidate**; confidence: **low sample**.
- Risk flags: small sample, inactive since before June 2026 snapshot, large open-position ROE on ETH: 1.10, Degen openPerps=0 disagrees with Hyperliquid open positions.
- Tooling inference: programmatic Hyperliquid execution is observed. Specific indicator stack, model stack, and creator runtime are not visible from public APIs; any RSI/Bollinger/grid/momentum language is inferred from fills, naming, and position patterns, not confirmed source code.

### 16. BTCV - Swing Trading (`no symbol`)

**Official links and identifiers**
- Degen page: https://degen.virtuals.io/agents/1104
- Virtuals page: n/a
- Degen agent id: `1104`; Virtuals id: `n/a`
- Hyperliquid / agent wallet: `0x3db01698f6aa18bcc8399da41bc9ce6aa127ae03`
- Token address: `n/a`; Virtuals preToken: `n/a`; pair: `n/a`

**Observed performance**
- Realized PnL: $17.42; perp realized PnL: $17.42; spot realized PnL: $0.00.
- Holdings value: $118.94; MTM PnL: $2.48; return: 1.99%.
- Trades: 8; wins/losses: 3/5; win rate: 37.50%; avg win/loss: $15.94/$-6.08.
- Reported trade volume: $876.27; Sharpe: n/a; last trade: 2026-05-28T15:53:29.141Z; performance calculated: 2026-06-09T06:06:14.519Z.

**Wallet / trading behavior**
- Hyperliquid fills fetched: 27; first/last fill: 2026-05-14T18:31:36.131000+00:00 → 2026-05-28T15:53:29.141000+00:00.
- Unique coins: 3; top coins by fills: ZEC (12), BTC (9), HYPE (6).
- Top coins by notional volume: ZEC ($696.38), BTC ($613.14), HYPE ($527.17).
- PnL by coin, largest absolute contributors: HYPE ($26.32), BTC ($-8.21), ZEC ($-0.70).
- Direction counts: [['Open Long', 17], ['Close Long', 10]]; open-long/open-short fills: 17/0; close-long/close-short fills: 10/0.
- Closed fill wins/losses: 4/6; gross win/loss from fills: $47.81/$-30.39.

**Current Hyperliquid snapshot**
- Margin summary: `{'accountValue': '47.128605', 'totalNtlPos': '86.4861', 'totalRawUsd': '-39.357495', 'totalMarginUsed': '17.29722'}`.
- Open positions: `[{'coin': 'ZEC', 'szi': '0.19', 'entryPx': '534.58', 'positionValue': '86.4861', 'unrealizedPnl': '-15.0841', 'returnOnEquity': '-0.7425455498', 'liquidationPx': None, 'marginUsed': '17.29722', 'maxLeverage': 10, 'leverage': {'type': 'cross', 'value': 5}}]`.
- Open orders: `[]`.

**Forum / signals surface**
- Degen `forumPostCount`: 21; visible threads: Discussion / DISCUSSION / gated=False / posts=0 / thread `1135`; Alphas / SIGNALS / gated=True / posts=21 / thread `1136`

**Creator / token metadata**
- Degen owner `0xa7c87D700D145E5E16a87beD9d6e51e08D6efE96` (user id 1078)
- Virtuals category/status/chain/factory: `n/a` / `n/a` / `n/a` / `n/a`.
- Virtuals holder/liquidity/dev holding: holders n/a; top10 holders n/a%; liquidity n/a; dev holding n/a.

**Inference for xvision**
- Strategy inference: **trend-following / swing candidate**; confidence: **low sample**.
- Risk flags: small sample, inactive since before June 2026 snapshot, large open-position ROE on ZEC: -0.74, Degen openPerps=0 disagrees with Hyperliquid open positions.
- Tooling inference: programmatic Hyperliquid execution is observed. Specific indicator stack, model stack, and creator runtime are not visible from public APIs; any RSI/Bollinger/grid/momentum language is inferred from fills, naming, and position patterns, not confirmed source code.

### 17. DEX (`DEX`)

**Official links and identifiers**
- Degen page: https://degen.virtuals.io/agents/1134
- Virtuals page: https://app.virtuals.io/virtuals/76669
- Degen agent id: `1134`; Virtuals id: `76669`
- Hyperliquid / agent wallet: `0x2280412badca7096d5b467943a7556a1d498e597`
- Token address: `0x59Ed9849aa3AF1267ff6ebD3fc806a37b393079B`; Virtuals preToken: `0x59Ed9849aa3AF1267ff6ebD3fc806a37b393079B`; pair: `0x0795395C5f3B1F782B4ca3Cf77573F703d84E695`

**Observed performance**
- Realized PnL: $544.18; perp realized PnL: $544.18; spot realized PnL: $0.00.
- Holdings value: $1,985.41; MTM PnL: $544.18; return: 1.94%.
- Trades: 56; wins/losses: 44/12; win rate: 78.57%; avg win/loss: $20.44/$-29.59.
- Reported trade volume: $28,004.20; Sharpe: n/a; last trade: 2026-06-08T11:54:45.242Z; performance calculated: 2026-06-09T06:06:14.519Z.

**Wallet / trading behavior**
- Hyperliquid fills fetched: 164; first/last fill: 2026-05-16T17:51:00.549000+00:00 → 2026-06-08T11:54:45.242000+00:00.
- Unique coins: 25; top coins by fills: ARB (18), ENA (14), APT (13), UNI (11), DOGE (9), DOT (8), ONDO (8), ADA (8).
- Top coins by notional volume: DOGE ($4,252.94), DOT ($3,789.85), NEAR ($3,786.24), APT ($3,321.30), ETH ($3,296.30), WLD ($3,184.28), SUI ($3,062.73), UNI ($2,993.99).
- PnL by coin, largest absolute contributors: WLD ($155.04), NEAR ($135.16), ONDO ($110.30), SUI ($-105.48), UNI ($-95.76), HYPE ($81.58), DOT ($-70.20), APT ($47.59), TON ($46.21), CRV ($38.72).
- Direction counts: [['Open Long', 53], ['Close Long', 52], ['Open Short', 32], ['Close Short', 27]]; open-long/open-short fills: 53/32; close-long/close-short fills: 52/27.
- Closed fill wins/losses: 62/17; gross win/loss from fills: $899.22/$-355.04.

**Current Hyperliquid snapshot**
- Margin summary: `{'accountValue': '0.0', 'totalNtlPos': '0.0', 'totalRawUsd': '0.0', 'totalMarginUsed': '0.0'}`.
- Open positions: `[]`.
- Open orders: `[]`.

**Forum / signals surface**
- Degen `forumPostCount`: 230; visible threads: Discussion / DISCUSSION / gated=False / posts=230 / thread `1187`; Alphas / SIGNALS / gated=True / posts=0 / thread `1188`

**Creator / token metadata**
- Virtuals creator wallet `0x07bE0c31194D2FD2A12b662900785922D57D94b4`
- Degen owner `0x07bE0c31194D2FD2A12b662900785922D57D94b4` (user id 1497)
- Project members: Owner user 437140 (no verified links)
- Virtuals category/status/chain/factory: `IP MIRROR` / `UNDERGRAD` / `BASE` / `BONDING_V5`.
- Virtuals holder/liquidity/dev holding: holders 4; top10 holders 100%; liquidity $7,192.53; dev holding 0.

**Inference for xvision**
- Strategy inference: **diversified mean-reversion/scalping candidate**; confidence: **medium**.
- Risk flags: no high-severity flag from fetched snapshot.
- Tooling inference: programmatic Hyperliquid execution is observed. Specific indicator stack, model stack, and creator runtime are not visible from public APIs; any RSI/Bollinger/grid/momentum language is inferred from fills, naming, and position patterns, not confirmed source code.

### 18. Rapidote (`RAP`)

**Official links and identifiers**
- Degen page: https://degen.virtuals.io/agents/875
- Virtuals page: https://app.virtuals.io/virtuals/73354
- Degen agent id: `875`; Virtuals id: `73354`
- Hyperliquid / agent wallet: `0xa4c720e90bab95c071deae90bc8b10784aba844a`
- Token address: `0xBfF0c490A74F7C06d7D700814F49530a7E274ABe`; Virtuals preToken: `0xBfF0c490A74F7C06d7D700814F49530a7E274ABe`; pair: `0x6Ffb336929ea4eA103C2aC107F5741Dd8CA2AA4a`

**Observed performance**
- Realized PnL: $130.36; perp realized PnL: $130.36; spot realized PnL: $0.00.
- Holdings value: $1,133.84; MTM PnL: $-41.62; return: 1.60%.
- Trades: 69; wins/losses: 35/34; win rate: 50.72%; avg win/loss: $10.32/$-6.79.
- Reported trade volume: $8,124.51; Sharpe: n/a; last trade: 2026-06-05T03:35:25.500Z; performance calculated: 2026-06-09T06:06:14.519Z.

**Wallet / trading behavior**
- Hyperliquid fills fetched: 228; first/last fill: 2026-04-21T09:03:35.931000+00:00 → 2026-06-05T03:35:25.500000+00:00.
- Unique coins: 10; top coins by fills: BTC (78), VVV (32), ZEC (31), HYPE (21), LIT (15), TON (14), SOL (13), XMR (10).
- Top coins by notional volume: BTC ($7,766.99), ZEC ($2,525.13), TON ($1,887.68), HYPE ($1,629.92), VVV ($1,615.87), SOL ($1,265.08), LIT ($1,031.48), ETH ($723.53).
- PnL by coin, largest absolute contributors: VVV ($115.87), TON ($-114.33), ZEC ($80.70), LIT ($76.38), HYPE ($32.68), BTC ($-32.34), SOL ($-31.84), XMR ($5.89), NEAR ($-1.93), ETH ($-0.73).
- Direction counts: [['Open Long', 131], ['Close Long', 66], ['Open Short', 22], ['Close Short', 9]]; open-long/open-short fills: 131/22; close-long/close-short fills: 66/9.
- Closed fill wins/losses: 39/36; gross win/loss from fills: $361.30/$-230.94.

**Current Hyperliquid snapshot**
- Margin summary: `{'accountValue': '337.733885', 'totalNtlPos': '3144.4712', 'totalRawUsd': '-2806.737315', 'totalMarginUsed': '314.44712'}`.
- Open positions: `[{'coin': 'BTC', 'szi': '0.04966', 'entryPx': '66824.1', 'positionValue': '3144.4712', 'unrealizedPnl': '-174.01366', 'returnOnEquity': '-0.5243768387', 'liquidationPx': '41042.0084320213', 'marginUsed': '314.44712', 'maxLeverage': 40, 'leverage': {'type': 'cross', 'value': 10}}]`.
- Open orders: `[]`.

**Forum / signals surface**
- Degen `forumPostCount`: 33; visible threads: Discussion / DISCUSSION / gated=False / posts=0 / thread `899`; Alphas / SIGNALS / gated=True / posts=33 / thread `900`

**Creator / token metadata**
- Virtuals creator wallet `0x02247b0c4945027252d2243a826cc10a87e196fe`
- Degen owner `0x02247b0C4945027252D2243a826cC10A87E196Fe` (user id 1169)
- Project members: Owner user 585205 (no verified links)
- description: A one-way beta punter chasing BTC with size, scale-ins, and loud conviction
- Virtuals category/status/chain/factory: `ACP_LAUNCH` / `UNDERGRAD` / `BASE` / `BONDING_V5`.
- Virtuals holder/liquidity/dev holding: holders 1; top10 holders 100%; liquidity $7,193.34; dev holding 0.

**Inference for xvision**
- Strategy inference: **concentrated swing / momentum candidate**; confidence: **medium**.
- Risk flags: large open-position ROE on BTC: -0.52, high leverage 10x on BTC, high margin use 93%, Degen openPerps=0 disagrees with Hyperliquid open positions.
- Tooling inference: programmatic Hyperliquid execution is observed. Specific indicator stack, model stack, and creator runtime are not visible from public APIs; any RSI/Bollinger/grid/momentum language is inferred from fills, naming, and position patterns, not confirmed source code.

### 19. bro (`BRO`)

**Official links and identifiers**
- Degen page: https://degen.virtuals.io/agents/395
- Virtuals page: https://app.virtuals.io/virtuals/68781
- Degen agent id: `395`; Virtuals id: `68781`
- Hyperliquid / agent wallet: `0x7dc0934e35d035df1a081988abcb590733653b79`
- Token address: `0xcb8413d8bEfb1257F2f6B166C3cE69CC32B30f55`; Virtuals preToken: `0xcb8413d8bEfb1257F2f6B166C3cE69CC32B30f55`; pair: `0x99f86AbD7D73c89b14132102bF54707764383AbE`

**Observed performance**
- Realized PnL: $2.07; perp realized PnL: $2.07; spot realized PnL: $0.00.
- Holdings value: $23.90; MTM PnL: $2.07; return: 1.58%.
- Trades: 4; wins/losses: 2/2; win rate: 50.00%; avg win/loss: $1.24/$-0.20.
- Reported trade volume: $130.63; Sharpe: -22.17025601; last trade: 2026-04-23T12:01:21.772Z; performance calculated: 2026-06-09T04:13:00.985Z.

**Wallet / trading behavior**
- Hyperliquid fills fetched: 13; first/last fill: 2026-04-17T04:40:55.719000+00:00 → 2026-04-23T12:01:21.772000+00:00.
- Unique coins: 1; top coins by fills: ETH (13).
- Top coins by notional volume: ETH ($259.19).
- PnL by coin, largest absolute contributors: ETH ($2.07).
- Direction counts: [['Open Long', 9], ['Close Long', 4]]; open-long/open-short fills: 9/0; close-long/close-short fills: 4/0.
- Closed fill wins/losses: 2/2; gross win/loss from fills: $2.47/$-0.40.

**Current Hyperliquid snapshot**
- Margin summary: `{'accountValue': '0.0', 'totalNtlPos': '0.0', 'totalRawUsd': '0.0', 'totalMarginUsed': '0.0'}`.
- Open positions: `[]`.
- Open orders: `[]`.

**Forum / signals surface**
- Degen `forumPostCount`: 0; visible threads: Discussion / DISCUSSION / gated=False / posts=0 / thread `367`; Alphas / SIGNALS / gated=True / posts=0 / thread `368`

**Creator / token metadata**
- Virtuals creator wallet `0x18E00346C8C1c8E23ff526B0d88d3db22501a27f`
- Degen owner `0x18E00346C8C1c8E23ff526B0d88d3db22501a27f` (user id 137)
- Project members: Owner user 15080 (GITHUB: https://github.com/iCx6, TWITTER: https://x.com/bluenami866, TELEGRAM: https://t.me/bluenamieth; TWITTER: bluenami866)
- description: bro
- Virtuals category/status/chain/factory: `ACP_LAUNCH` / `UNDERGRAD` / `BASE` / `BONDING_V5`.
- Virtuals holder/liquidity/dev holding: holders 2; top10 holders 100%; liquidity $7,193.34; dev holding 0.

**Inference for xvision**
- Strategy inference: **concentrated long-only momentum/scalp candidate**; confidence: **low sample**.
- Risk flags: small sample, inactive since before June 2026 snapshot.
- Tooling inference: programmatic Hyperliquid execution is observed. Specific indicator stack, model stack, and creator runtime are not visible from public APIs; any RSI/Bollinger/grid/momentum language is inferred from fills, naming, and position patterns, not confirmed source code.

### 20. ORACLE (`ORACLE`)

**Official links and identifiers**
- Degen page: https://degen.virtuals.io/agents/1044
- Virtuals page: https://app.virtuals.io/virtuals/75501
- Degen agent id: `1044`; Virtuals id: `75501`
- Hyperliquid / agent wallet: `0xc30221cf54cc7ce269557cf70c30e7d9d39ef587`
- Token address: `0x4e3fF4a8dE8CCe5006Cd27B3D3162AB192A9C24f`; Virtuals preToken: `0x4e3fF4a8dE8CCe5006Cd27B3D3162AB192A9C24f`; pair: `0x5Df3B8AcE8a8358118e2350E2bda14e63d711969`

**Observed performance**
- Realized PnL: $2,178.65; perp realized PnL: $2,178.65; spot realized PnL: $0.00.
- Holdings value: $0.00; MTM PnL: $2,178.65; return: 1.30%.
- Trades: 57; wins/losses: 50/7; win rate: 87.72%; avg win/loss: $43.69/$-0.83.
- Reported trade volume: $167,592.58; Sharpe: n/a; last trade: 2026-05-23T02:57:31.843Z; performance calculated: 2026-06-09T04:13:00.985Z.

**Wallet / trading behavior**
- Hyperliquid fills fetched: 184; first/last fill: 2026-05-08T05:45:59.746000+00:00 → 2026-05-23T02:57:31.843000+00:00.
- Unique coins: 23; top coins by fills: BTC (75), NEAR (9), HYPE (9), ARB (8), APT (7), SOL (6), WLD (6), SUI (6).
- Top coins by notional volume: BTC ($285,898.65), SOL ($16,516.92), DOGE ($2,755.01), TRUMP ($2,430.58), LTC ($2,278.31), ONDO ($2,136.46), WLD ($2,132.48), APT ($1,960.63).
- PnL by coin, largest absolute contributors: BTC ($1,091.43), SOL ($463.40), ONDO ($95.33), WLD ($70.82), VIRTUAL ($46.15), CRV ($44.15), LINK ($40.10), SUI ($40.03), DOGE ($39.73), APT ($39.18).
- Direction counts: [['Close Long', 61], ['Open Long', 53], ['Close Short', 36], ['Open Short', 34]]; open-long/open-short fills: 53/34; close-long/close-short fills: 61/36.
- Closed fill wins/losses: 89/8; gross win/loss from fills: $2,184.46/$-5.81.

**Current Hyperliquid snapshot**
- Margin summary: `{'accountValue': '0.0', 'totalNtlPos': '0.0', 'totalRawUsd': '0.0', 'totalMarginUsed': '0.0'}`.
- Open positions: `[]`.
- Open orders: `[]`.

**Forum / signals surface**
- Degen `forumPostCount`: 87; visible threads: Discussion / DISCUSSION / gated=False / posts=87 / thread `1045`; Alphas / SIGNALS / gated=True / posts=0 / thread `1046`

**Creator / token metadata**
- Virtuals creator wallet `0x4B91200101513B1892743B501d7D9c74Ec4bf9Fb`
- Degen owner `0x4B91200101513B1892743B501d7D9c74Ec4bf9Fb` (user id 1415)
- Project members: Owner user 434367 (no verified links)
- Virtuals category/status/chain/factory: `IP MIRROR` / `UNDERGRAD` / `BASE` / `BONDING_V5`.
- Virtuals holder/liquidity/dev holding: holders 19; top10 holders 100%; liquidity $7,378.64; dev holding 0.

**Inference for xvision**
- Strategy inference: **diversified mean-reversion/scalping candidate**; confidence: **medium**.
- Risk flags: inactive since before June 2026 snapshot.
- Tooling inference: programmatic Hyperliquid execution is observed. Specific indicator stack, model stack, and creator runtime are not visible from public APIs; any RSI/Bollinger/grid/momentum language is inferred from fills, naming, and position patterns, not confirmed source code.

## Cross-agent implications for xvision

### Product and data model implications

- Store both venue-native position state and platform-reported summaries, then surface mismatches as first-class reconciliation warnings.
- Treat strategy archetype as an inferred label with confidence, not a creator claim, unless the creator provides source/prompt/tooling metadata.
- Rank by risk-adjusted performance as well as raw return: sample size, open exposure, margin utilization, leverage, concentration, and inactivity materially change interpretation.
- Separate realized PnL, MTM PnL, and open-risk dashboards. Several wallets look strong on realized PnL while carrying underwater open positions.
- Add an `inactive`/`stale strategy` badge for agents with strong historical metrics but no recent fills.

### Strategy-design takeaways

- The repeatable winner shape is not a single asset call; it is diversified, mechanically executed, and risk-controlled across many Hyperliquid perps.
- High win rate alone is not enough. Low sample agents with 100% win rates are mostly abandoned/test-like and should be downweighted.
- Concentrated long-only/swing agents are easier to explain but need stricter live risk controls: stop evidence, liquidation distance, margin use, and exposure caps.
- Copy-trading/social-signal surfaces appear underused relative to the number of agents with gated signal threads; xvision can differentiate with clearer provenance and post-trade rationale capture.

### Open questions for the next research pass

- Authenticate to fetch gated `Alphas` posts where permitted, especially Alexa, 404Alpha, DEX, macro, and BTCV.
- Pull full Hyperliquid historical portfolios and reconstruct drawdowns, holding periods, and per-trade lifecycle rather than fill-level approximations.
- Query Base token-holder and transaction graphs for creator clustering across agents; several top agents have similarly sparse metadata and may share creator tooling.
- Monitor the leaderboard over multiple days to distinguish stable edge from season-start or stale-wallet artifacts.
- Search creator socials more deeply for BambooAgentAI and bro/bluenami, the two public-social cases found in metadata.

## Source index

- Degen Virtuals Arena: https://degen.virtuals.io/
- Leaderboard API: https://degen.virtuals.io/api/leaderboard
- Hero dashboard API: https://degen.virtuals.io/api/hero-dashboard
- Hyperliquid info API: https://api.hyperliquid.xyz/info
- Virtuals app/API: https://app.virtuals.io/ and https://api2.virtuals.io/api/virtuals/<virtualId>
- Public app metadata includes the Arena description: 'A live arena where AI agents trade Hyperliquid perps for real USDC and compete on-chain.'
