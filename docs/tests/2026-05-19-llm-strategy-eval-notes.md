# 2026-05-19 LLM strategy eval notes

This note records the live xvision strategy/eval experiments run through the dashboard API against `https://xvn.tail2bb69.ts.net`.

## Purpose

- Preserve the existing LLM strategy test results before the next bake-off.
- Separate causal strategy work from the profitable-but-overfit timestamp proof-of-control.
- Keep a shortlist of model candidates and causal long/short families for follow-up testing.

## Operational constraints observed

- A strategy draft is not runnable until it has an attached agent with explicit provider/model wiring.
- Eval launch payloads use the strategy id in the `agent_id` field.
- `total_return_pct` and `max_drawdown_pct` are already percentage-point values; do not multiply by 100 when reporting.
- Large concurrent LLM eval grids can trigger OpenRouter `429 Too Many Requests`. Use sequential launches with persisted progress, cooldowns, and terminal-state polling.
- For model comparison, use one shared scenario/timeframe and the same decision count so model behavior is the tested variable.

## Research inputs

Moltbook/forum-derived themes used for strategy design:

- Regime detection matters more than stacking indicators.
- Signal half-life matters: stale breakouts or pullbacks should be ignored quickly.
- Stay flat in chop; only trade when structure, momentum, and volatility agree.
- Translate discussion-board ideas into filters and lifecycle rules, not narrative discretion.
- For shorts: prefer breakdown/retest, failed-breakout traps, ATR-based stops, and squeeze avoidance.

## Existing strategy family tests

### First pass: four idea families on ETH/USD 4h 7-day sprint

Scenario: `sc_01KRZ185GK2HXTCNG3VEFK7GF6` (`ETH 4h 7d sprint`)  
Decision count: 49 each

- `ETH 4h compression sniper` (`01KS06R5940P7J7XDGXYST3ENN`)
  - Run: `01KS06R7PH27H3FG34HRQ1W1GX`
  - Return: `-1.0483900787017776%`
  - Sharpe: `-17.21282815706288`
  - Max drawdown: `1.3319980267435312%`
  - Actions: `45 flat / 2 short_open / 2 hold`
  - Note: best of first pass because it lost least and stayed mostly flat.
- `ETH 4h regime switch breaker` (`01KS06T6HKXPGAT4M3CVAZS0Y2`)
  - Run: `01KS06T8ZKG68AAZJNGFM82AY3`
  - Return: `-9.474240922632451%`
  - Sharpe: `-33.840867703400434`
  - Max drawdown: `10.349641420184272%`
  - Actions: `21 flat / 13 short_open / 14 hold / 1 long_open`
- `ETH 4h time decay scalper` (`01KS06W4YWYJDHYEBAX7A7ADKJ`)
  - Run: `01KS06W7BMM7H10TNX1TR34W9Q`
  - Return: `-32.976376113506556%`
  - Sharpe: `-50.44541919928591`
  - Max drawdown: `32.976376113506556%`
  - Actions: `8 flat / 19 short_open / 6 long_open / 16 hold`
- `ETH 4h mean reversion veto trend` (`01KS06YSMEGYY476RBF85G5PS2`)
  - Run: `01KS06YW0T5REY9883R3SYKEJ2`
  - Return: `-12.435746498660594%`
  - Sharpe: `-24.903890410966525`
  - Max drawdown: `13.69349149209768%`
  - Actions: `19 flat / 15 short_open / 15 hold`

### Second pass: compression sniper v2 across four regimes

Strategy: `ETH 4h compression sniper v2` (`01KS07AZ5R9V7HQ3EWQE1AQAH0`)  
Agent: `01KS07AZN25XB43GEXC98HBY28`  
Decision count: 49 each

- ETH 7-day sprint (`sc_01KRZ185GK2HXTCNG3VEFK7GF6`)
  - Run: `01KS07BE80D255RDBCXF59PEFM`
  - Return: `-8.854583483055467%`
  - Sharpe: `-33.53728465605228`
- BTC 8-day bull clone (`sc_01KS07B12MHTVHJNAA8MVM6TT1`)
  - Run: `01KS07D9ARJ21VWGNGXKYD570P`
  - Return: `-6.1666666723642266%`
  - Sharpe: `-23.949909539133376`
- BTC 8-day crash clone (`sc_01KS07B1HQ6ZTVM26NH8JK0XAA`)
  - Run: `01KS07F2SJMY4988VPTNZSXF7W`
  - Return: `-9.564611140896158%`
  - Sharpe: `-8.754001035336476`
- SOL 8-day extended clone (`sc_01KS07B20X5BV3RM6CH3YG69ZQ`)
  - Run: `01KS07GZG1JFHW4GDE3FCPTQFW`
  - Return: `-2.3186442989332135%`
  - Sharpe: `-2.6418343137192943`

Takeaway: selectivity helped relative to overactive families, but v2 still lost in every tested regime.

## Profitable proof-of-control, not deployable

Strategy: `BTC 1h timestamp swing oracle v3 140022` (`01KS08KW0Z8S8VXFMWY13X70Z4`)  
Agent: `01KS08KWZJ1ZNEYXTKM98NMXHZ`  
Run: `01KS08KYDMB9VSZVQNV14NZQBQ`  
Scenario: `BTC Jan 13-17 1h momentum window 135413` (`sc_01KS0880VW6854ZQVBXQBVMDHG`)  
Decision count: 100

- Return: `+0.5847%`
- Sharpe: `16.69`
- Max drawdown: `0.0300%`
- Actions: `81 flat / 8 long_open / 1 short_open / 10 hold`

This proves the eval surface can show profitable behavior under constraints, but the strategy used timestamp/index knowledge and should not be treated as a causal strategy.

## Causal v4 retry snapshot

A safe sequential retry batch was started after rate-limit failures in a larger grid. It was stopped before completion to avoid interfering with the requested 30-decision bake-off.

Completed results from `/tmp/xvn_v4_retry_safe_results.json`:

- `v4 causal impulse breakout` (`01KS09W7P8MHT85VTP3XYXEG2P`), Gemini
  - ETH 4h 7d sprint: `-0.3736541946965735%`, 49 decisions, run `01KS0A12FST8JFT6KNP1Z1EVTG`
  - SOL 4h 8d extended: `-0.13377601057349237%`, 49 decisions, run `01KS0A36C1AQESWT15CPB74P5N`
  - BTC 4h bull clone: `-0.2817275854128093%`, 49 decisions, run `01KS0A5DP8KZVQJ03TCKGKYJVN`
  - BTC 4h crash clone: `-0.2632733401411679%`, 49 decisions, run `01KS0A7MZ7N225GY8C7AZTABMZ`
  - BTC 1h Jan13-17 momentum: `-0.8745164970198239%`, 100 decisions, run `01KS0A9WJQT7MP42W8SADT9PHF`
  - BTC 1h bull early Jan causal: `-0.6262908863468474%`, 100 decisions, run `01KS0AVS5AFQ7TBA313TZ88F60`
  - BTC 1h bear early Jul causal: `-0.5455767465065874%`, 100 decisions, run `01KS0B3ADJ41JAD7D72BHQ7NWN`
  - BTC 1h range early Apr causal: `-0.5427260533792433%`, 100 decisions, run `01KS0BB705HWAHFBA2MJZ2EX0R`
  - BTC 1h flash early Aug causal: `-0.8206604426851555%`, 100 decisions, run `01KS0BJRQ8CAZR0KWQKCD5JAEE`
- `v4 causal trend pullback` (`01KS09WA2CRGR2HWTQYTQ6VDWN`), Gemini
  - ETH 4h 7d sprint: `-0.2980741706411791%`, 49 decisions, run `01KS0BT0BADK5GAB8Y55JH3GXD`

Takeaway: causal v4 improved loss size versus the first active variants, but the completed runs were still negative. The 4h variants looked less churn-prone than 1h variants.

## 30-decision model bake-off results

Run one frozen causal strategy prompt across one shared scenario/timeframe with exactly 30 decisions per model.

Shared scenario:

- Scenario: `BTC 1h 30-decision model bakeoff 20260519151152`
- Scenario id: `sc_01KS0CP7P6MH8AQE87R1EP79EM`
- Asset/timeframe: `BTC/USD`, `1h`
- Window: `2025-01-13T16:00:00Z` → `2025-01-14T21:00:00Z`
- Warmup bars: `200`
- Decisions: `30` for every completed run
- Results file: `/tmp/xvn_model_bakeoff_30_results_20260519151152.json`

Completed runs:

- `google/gemini-3.1-flash-lite`
  - Run: `01KS0CPAK0CA4H50Q7XMHAG2HN`
  - Return: `-0.02441499335868866%`
  - Sharpe: `-19.210362382652555`
  - Max drawdown: `0.02441499335868866%`
  - Actions: `20 flat / 5 hold / 3 long_open / 2 short_open`
  - Runtime: about `40s`
  - Tokens: `701436` input / `962` output
- `deepseek/deepseek-v4-flash`
  - Run: `01KS0CS30E1X1N21BVFA8VPNTJ`
  - Return: `-0.04205031631085148%`
  - Sharpe: `-49.418862003908494`
  - Max drawdown: `0.04205031631085148%`
  - Actions: `19 flat / 3 hold / 8 short_open`
  - Runtime: about `664s`
  - Tokens: `466327` input / `54669` output
- `qwen/qwen3.5-flash-02-23`
  - Run: `01KS0DEY93SVJ95X39SRAF9QXN`
  - Return: `-0.02161184249151847%`
  - Sharpe: `-16.901919818658723`
  - Max drawdown: `0.02161184249151847%`
  - Actions: `23 flat / 3 hold / 2 long_open / 2 short_open`
  - Runtime: about `835s`
  - Tokens: `698976` input / `120458` output
- `mistralai/mistral-small-3.2-24b-instruct`
  - Run: `01KS0EA0SH536PBKPV28G4B71X`
  - Return: `0.0%`
  - Sharpe: `0.0`
  - Max drawdown: `0.0%`
  - Actions: `30 flat`
  - Runtime: about `83s`
  - Tokens: `655403` input / `775` output
- `xiaomi/mimo-v2-flash`
  - Run: `01KS0EE204H6C08ER3BKM114VX`
  - Return: `-0.02670993748528417%`
  - Sharpe: `-25.399743563414084`
  - Max drawdown: `0.02670993748528417%`
  - Actions: `25 flat / 2 hold / 3 short_open`
  - Runtime: about `61s`
  - Tokens: `655748` input / `885` output
- `qwen/qwen3-30b-a3b-instruct-2507`
  - Run: `01KS0EHEQVQAGMPKQ768Y0DC0D`
  - Return: `0.0%`
  - Sharpe: `0.0`
  - Max drawdown: `0.0%`
  - Actions: `29 flat / 1 hold`
  - Runtime: about `126s`
  - Tokens: `655463` input / `1161` output

Takeaways:

- Best capital preservation: `mistralai/mistral-small-3.2-24b-instruct` and `qwen/qwen3-30b-a3b-instruct-2507` both avoided trades and finished flat at `0.0%`; this is useful as a risk baseline but not evidence of signal discovery.
- Best active trader: `qwen/qwen3.5-flash-02-23` had the least negative active result (`-0.0216%`) with a selective action mix, but it was extremely slow and verbose.
- Best speed/behavior compromise: `google/gemini-3.1-flash-lite` finished quickly and traded selectively, with only slightly worse active performance than Qwen 3.5 Flash.
- Worst active result: `deepseek/deepseek-v4-flash`; it over-shorted relative to the other models and had the worst return/drawdown in this window.
- Operationally, this run supports keeping Gemini Flash Lite as the default fast iteration model, using Qwen 3.5 Flash only for slower quality checks, and treating all-flat models as safety baselines rather than discovery engines.
