# Freqtrade graphs and metrics evaluation for Xvision

Date: 2026-05-11
Source inspected: `freqtrade/freqtrade` open-source repository, shallow clone at commit `c587a1c`
Xvision repo: `latentwill/xvision`, local path `/var/lib/hermes/xvision`

## Executive summary

Freqtrade is strongest as a strategy backtesting/reporting product, not as an execution-microstructure reference. Compared to Hummingbot, the parts most relevant to Xvision are:

- a rich, stable backtest result schema
- per-pair, per-tag, per-exit-reason, and per-period metric tables
- explicit separation between closed-trade metrics and wallet/equity-curve metrics
- Plotly graph generation for trade debugging
- web/API endpoints that expose the same data to FreqUI
- entry/exit analysis that joins strategy indicator values to actual trade outcomes

Xvision should not copy Freqtrade's crypto-bot surface wholesale. The useful lift is narrower: treat every eval run as a reproducible artifact with enough structured timeseries, trade lifecycle records, and metric breakdowns that the dashboard can answer: *what happened, where, when, and why did this strategy behave that way?*

For Xvision scenario generation, this reinforces the previous design decision:

```text
Scenario       = world definition: asset, time window, granularity, venue, fees, slippage, latency, data source, replay mode
StrategyBundle = agent definition: capital, risk caps, intern/trader/risk config, prompts, position sizing
EvalRun        = immutable produced artifact: orders, fills, positions, equity curve, metrics, plots, logs, decisions
```

Freqtrade's graphs and metrics mainly belong on `EvalRun`, not on `Scenario`.

## Source map inspected

Core files:

- `freqtrade/data/metrics.py`
  - market change, combined close dataframe, cumulative profit, underwater/drawdown series, CAGR, expectancy, Sharpe, Sortino, Calmar, SQN
- `freqtrade/optimize/optimize_reports/optimize_reports.py`
  - converts raw backtest trades into strategy stats, pair metrics, tag metrics, daily stats, periodic breakdowns, wallet stats
- `freqtrade/optimize/optimize_reports/bt_output.py`
  - renders CLI result tables and summary metrics
- `freqtrade/plot/plotting.py`
  - Plotly candlestick, indicator, trade marker, profit, underwater, drawdown, and parallelism graphs
- `freqtrade/data/btanalysis/trade_parallelism.py`
  - open-trade parallelism and balance distribution over time
- `freqtrade/data/entryexitanalysis.py`
  - joins entry/exit signal candles and indicator values back to trade results
- `freqtrade/rpc/rpc.py`
  - live/dry-run statistics for profit, performance, stats, balance, count
- `freqtrade/rpc/api_server/api_trading.py`
  - REST endpoints for trading metrics
- `freqtrade/rpc/api_server/api_backtest.py`
  - REST endpoints for backtest execution, history, market-change, and wallet-equity data
- `freqtrade/rpc/api_server/api_schemas.py`
  - response models for `Profit`, `Stats`, `DailyWeeklyMonthly`, and backtest responses
- Docs inspected indirectly via grep:
  - `docs/backtesting.md`
  - `docs/plotting.md`
  - `docs/advanced-backtesting.md`
  - `docs/strategy_analysis_example.md`
  - `docs/rest-api.md`
  - `docs/telegram-usage.md`

## 1. Graph surfaces in Freqtrade

Freqtrade has two main Plotly graph commands plus a FreqUI/webserver surface.

### 1.1 `plot-dataframe`: per-pair strategy-debugging chart

Source: `freqtrade/plot/plotting.py`, especially `generate_candlestick_graph()` and helpers.

What it renders:

- Row 1: OHLC candlestick chart
- Row 1 overlays:
  - configured main indicators
  - Bollinger Band filled region when `bb_lowerband` and `bb_upperband` are present
  - long entry signals: `enter_long`, green triangle-up markers
  - long exit signals: `exit_long`, red triangle-down markers
  - short entry signals: `enter_short`, blue triangle-down markers
  - short exit signals: `exit_short`, violet triangle-up markers
  - actual trade entries: cyan circle-open markers
  - profitable exits: green square-open markers
  - losing exits: red square-open markers
- Row 2: volume bar chart
- Rows 3..N: configured subplot indicators
- Optional filled areas between arbitrary indicators via `fill_to`, `fill_label`, `fill_color`
- Output: one HTML file per pair named like `freqtrade-plot-<PAIR>-<TIMEFRAME>.html`

Important implementation detail: graph config is driven by the strategy's `plot_config` plus CLI-provided `--indicators1` and `--indicators2`. The plotting layer does not need hardcoded knowledge of RSI/MACD/etc.; it reads dataframe columns.

Xvision lesson:

- Add an eval-run pair/asset debug chart with:
  - OHLC bars
  - agent decisions
  - submitted orders
  - fills
  - position size
  - risk-state overlays
  - strategy-specific diagnostic series
- Keep strategy diagnostics generic: if an agent/strategy emits a numeric timeseries, the graph layer should be able to plot it without a code change.

### 1.2 `plot-profit`: portfolio/run-level chart

Source: `freqtrade/plot/plotting.py`, especially `generate_profit_graph()`.

Freqtrade builds a six-row Plotly figure:

1. **AVG Close Price**
   - average close price across selected pairs
   - used as a crude market benchmark/context line
2. **Combined Profit**
   - cumulative realized profit across all selected pairs
   - includes markers for max drawdown start/end
3. **Profit per pair**
   - cumulative profit line per pair
4. **Parallelism**
   - open trade count over time
5. **Underwater**
   - absolute drawdown area chart
6. **Relative Drawdown**
   - relative drawdown area chart

Output: `user_data/plot/freqtrade-profit-plot.html`.

Caveat in source: the profit calculation is described as not fully realistic but useful for algorithm comparison. That caveat matters for Xvision: graph semantics must be named precisely so nobody confuses a diagnostic series with production-grade accounting.

Xvision lesson:

- Add a run-level graph after M2/M3 with:
  - benchmark asset return or scenario-market return
  - realized PnL
  - unrealized PnL / mark-to-market equity
  - drawdown absolute and percent
  - open position count / exposure
  - per-asset contribution once multi-asset scenarios ship
- Name series explicitly: `realized_pnl`, `unrealized_pnl`, `equity_curve`, `cash`, `gross_exposure`, `net_exposure`.

### 1.3 FreqUI/webserver data surfaces

Source: `freqtrade/rpc/api_server/api_backtest.py`, `api_trading.py`, `api_pair_history.py`, `api_v1.py`, `api_schemas.py`.

Relevant endpoints:

- Backtest lifecycle/history:
  - `POST /backtest`
  - `GET /backtest`
  - `DELETE /backtest`
  - `GET /backtest/abort`
  - `GET /backtest/history`
  - `GET /backtest/history/result`
  - `DELETE /backtest/history/{file}`
  - `PATCH /backtest/history/{file}`
- Backtest graph support:
  - `GET /backtest/history/{file}/market_change`
  - `GET /backtest/history/{file}/{strategy}/wallet`
- Candle/indicator support:
  - `GET /pair_history`
  - `POST /pair_history`
  - `GET /pair_candles`
  - `POST /pair_candles`
  - `GET /plot_config`
- Live/dry-run metrics:
  - `GET /profit`
  - `GET /profit_all`
  - `GET /performance`
  - `GET /stats`
  - `GET /daily`
  - `GET /weekly`
  - `GET /monthly`
  - `GET /balance`
  - `GET /count`

Xvision lesson:

- Do not make the dashboard recompute metrics from logs.
- Persist structured eval artifacts and expose typed endpoints such as:
  - `GET /api/eval-runs/:id/summary`
  - `GET /api/eval-runs/:id/trades`
  - `GET /api/eval-runs/:id/orders`
  - `GET /api/eval-runs/:id/fills`
  - `GET /api/eval-runs/:id/equity-curve`
  - `GET /api/eval-runs/:id/drawdown`
  - `GET /api/eval-runs/:id/market-context`
  - `GET /api/eval-runs/:id/decision-events`
  - `GET /api/eval-runs/:id/plot-config`

## 2. Metrics in Freqtrade

Freqtrade has two related but distinct metric paths:

1. Backtest result metrics from exported run artifacts.
2. Live/dry-run metrics from persisted `Trade`/`Order` rows through RPC/API.

This distinction is valuable for Xvision. Eval metrics and paper/live metrics should share definitions where possible, but their data sources and caveats are different.

### 2.1 Per-pair metrics

Source: `generate_pair_metrics()` and `_generate_result_line()` in `optimize_reports.py`.

Per pair plus a final `TOTAL` row:

- trades
- average profit ratio / percent
- total profit absolute
- total profit percent relative to starting balance
- average duration
- wins / draws / losses
- winrate
- CAGR
- expectancy and expectancy ratio
- Sortino
- Sharpe
- Calmar
- SQN
- profit factor
- max drawdown account-relative
- max drawdown absolute

Xvision lesson:

- Even if v1 scenarios enforce `asset.len() == 1`, design the result schema with `results_per_asset` now.
- Multi-asset will need this immediately, and a single-asset run can just have one row plus total.

### 2.2 Tag and reason metrics

Source: `generate_tag_metrics()` in `optimize_reports.py`, rendered by `text_table_tags()` in `bt_output.py`.

Freqtrade computes breakdowns by:

- `enter_tag`
- `exit_reason`
- mixed tuple of `[enter_tag, exit_reason]`

For each group it uses the same result-line metrics:

- count
- average profit
- total profit absolute/percent
- average duration
- wins/draws/losses/winrate
- risk/performance fields where applicable

Xvision lesson:

- Add structured labels to agent decisions and exits.
- For Xvision, analogous group-by keys could be:
  - `decision_kind`: enter / exit / resize / hold / reject
  - `agent_role`: intern / trader / risk
  - `strategy_tag`: strategy-emitted explanation label
  - `exit_reason`: stop_loss / take_profit / risk_exit / time_exit / strategy_exit / forced_end
  - `risk_rule_id`: the rule that blocked or modified an action
- The dashboard should be able to answer: "Which decision tags make or lose money?" without opening logs.

### 2.3 Summary metrics

Source: `generate_strategy_stats()` in `optimize_reports.py`, rendered by `text_table_add_metrics()` in `bt_output.py`.

Freqtrade summary metrics include:

- backtesting start and end
- max open trades
- total trades and daily average trades
- starting balance
- final balance
- absolute profit
- total profit percent
- CAGR
- Sharpe on closed trades
- Sortino on closed trades
- Calmar on closed trades
- SQN
- profit factor
- expectancy and expectancy ratio
- average daily profit
- average stake amount
- market change
- total trade volume
- long/short trade counts and long/short profit when shorts are used
- best pair and worst pair
- best trade and worst trade
- best day and worst day
- winning/draw/losing days
- min/max/avg duration for winners
- min/max/avg duration for losers
- max consecutive wins/losses
- rejected entry signals
- entry/exit timeouts
- canceled trade entries
- canceled entry orders
- replaced entry orders
- min/max balance from closed trades
- absolute drawdown
- drawdown duration
- profit at drawdown start/end
- drawdown start/end timestamps

Xvision lesson:

- Xvision's current eval summary should expand beyond headline PnL.
- The most important additions for an agentic eval system are not just Sharpe/Sortino; they are the behavioral counters:
  - rejected decisions
  - risk-overrides
  - invalid orders
  - order replacements
  - timeouts
  - forced exits at scenario end
  - max consecutive bad decisions

### 2.4 Wallet/equity-curve metrics

Source: `generate_wallet_stats()` in `optimize_reports.py` and balance helpers in `data/metrics.py`.

Freqtrade separately computes wallet-based metrics from historical balance snapshots:

- start balance
- end balance
- high balance
- low balance
- high/low dates
- max drawdown account-relative from wallet balance
- max relative drawdown / underwater
- max drawdown absolute
- drawdown start/end/duration
- Sharpe from daily wallet balance
- Sortino from daily wallet balance
- Calmar from daily wallet balance

This is one of the most important parts to copy conceptually.

Closed-trade metrics can hide risk because they ignore unrealized drawdown. Wallet/equity metrics capture open-position pain.

Xvision lesson:

- Store `equity_snapshots` during every eval run, not just final trade receipts.
- Compute two metric families:
  - **closed-trade metrics**: realized-only, useful for trade-quality summaries
  - **equity-curve metrics**: mark-to-market, useful for risk and operator trust

### 2.5 Periodic breakdown metrics

Source: `generate_periodic_breakdown_stats()` and `generate_all_periodic_breakdown_stats()` in `optimize_reports.py`.

Freqtrade supports breakdowns by:

- day
- week
- month
- year
- weekday

Each period contains:

- date/period key
- trade count
- absolute profit
- wins/draws/losses
- profit factor

Xvision lesson:

- Add periodic breakdowns to eval result artifacts.
- For v1 with `Hour1 | Day1` bars, start with:
  - by day for Hour1 scenarios
  - by month/year for Day1 scenarios when ranges are long enough
- Add weekday/hour-of-day later if intraday behavior matters.

### 2.6 Drawdown and underwater calculations

Source: `calculate_max_drawdown()`, `calculate_underwater()`, `calculate_max_drawdown_from_balance()` in `data/metrics.py`.

Freqtrade's drawdown machinery tracks:

- cumulative profit
- rolling high-water mark
- drawdown absolute
- drawdown relative to account value
- high date
- low date
- high value
- low value
- current drawdown, not only max historical drawdown

Xvision lesson:

- Persist enough data to render underwater charts and report both:
  - max drawdown over the run
  - current drawdown at the end of the run
- This matters for comparing strategies that end in an open-risk state.

### 2.7 Expectancy, profit factor, SQN, Sharpe, Sortino, Calmar, CAGR

Source: `data/metrics.py`.

Freqtrade formulas at a high level:

- **Profit factor**: gross winning profit / absolute gross losing profit
- **Expectancy**: `(winrate * average_win) - (loss_rate * average_loss)`
- **Expectancy ratio**: `((1 + risk_reward_ratio) * winrate) - 1`
- **CAGR**: `(final_balance / starting_balance) ** (1 / (days / 365)) - 1`
- **Sharpe**: annualized mean return / standard deviation
- **Sortino**: annualized mean return / downside standard deviation
- **Calmar**: annualized return relative to max drawdown
- **SQN**: `sqrt(number_of_trades) * mean_trade_return / std_trade_return`

Implementation caveat: Freqtrade has both trade-based and wallet-balance-based versions for some metrics. Xvision should do the same rather than force one definition.

## 3. Entry/exit analysis: the hidden gem for agent debugging

Source: `freqtrade/data/entryexitanalysis.py` and `docs/advanced-backtesting.md`.

Freqtrade can export extra backtest artifacts:

- signal candles
- exit signal candles
- rejected signal candles

Then `backtesting-analysis` joins those candles back to trade outcomes and can group by:

- group 0: win/loss and expectancy summary by enter tag
- group 1: profit summaries by enter tag
- group 2: profit summaries by enter tag + exit reason
- group 3: profit summaries by pair + enter tag
- group 4: profit summaries by pair + enter tag + exit reason
- group 5: profit summaries by exit reason

It can also print selected indicator values at entry and/or exit, and export tables to CSV.

Xvision lesson:

This maps directly onto agentic observability. Xvision should persist an eval-time `decision_context` record for each strategy decision:

```rust
DecisionContextSnapshot {
    run_id,
    bar_ts,
    asset,
    decision_id,
    agent_role,
    action,
    rationale_tag,
    features: JsonValue,       // indicators, model scores, risk values, prompt-derived fields
    risk_state: JsonValue,
    accepted: bool,
    rejection_reason: Option<String>,
}
```

Then reports can answer:

- Which rationale tags are profitable?
- Which risk blocks saved money?
- Which prompt mode causes drawdowns?
- What did the agent see before the worst trade?
- Which feature values correlate with rejected or losing trades?

This is more important for Xvision than simply copying a Plotly candlestick chart.

## 4. Trade parallelism and exposure metrics

Source: `data/btanalysis/trade_parallelism.py`.

Freqtrade expands each trade across every candle it was open, then resamples to count overlapping trades. It uses this for:

- open-trade parallelism chart
- detecting periods where backtest exceeds configured `max_open_trades`

The same module also has `balance_distribution_over_time()`, which reconstructs stake currency, base-currency holdings, leverage, short/long state, and collateral over time from order fills.

Xvision lesson:

- Add exposure reconstruction from fills, not just from final trade summaries.
- Useful run-level timeseries:
  - open positions count
  - gross exposure
  - net exposure
  - cash
  - locked collateral / reserved capital
  - per-asset position size
  - leverage once relevant

## 5. Backtest result artifact shape

Freqtrade stores a rich JSON-ish backtest result under `strategy[strategy_name]`, with a companion `metadata` object and a `strategy_comparison` list.

Important fields include:

- `trades`
- `locks`
- `best_pair`, `worst_pair`
- `results_per_pair`
- `results_per_enter_tag`
- `exit_reason_summary`
- `mix_tag_stats`
- `left_open_trades`
- `total_trades`
- `trade_count_long`, `trade_count_short`
- `total_volume`
- `avg_stake_amount`
- `profit_mean`, `profit_median`, `profit_total`, `profit_total_abs`
- `profit_total_long`, `profit_total_short`
- `cagr`, `expectancy`, `expectancy_ratio`, `sortino`, `sharpe`, `calmar`, `sqn`
- `wallet_stats`
- `profit_factor`
- start/end timestamps
- `trades_per_day`
- `market_change`
- `pairlist`
- `starting_balance`, `final_balance`
- rejected/timeouts/canceled/replaced counters
- scenario-ish config fields: timeframe, timerange, protections, stoploss, trailing stop, ROI, trading mode
- `periodic_breakdown`
- `daily_profit`
- drawdown fields

Xvision lesson:

- Split Freqtrade's mixed strategy/config/result blob into cleaner Xvision tables, but keep the richness.
- Suggested Xvision artifact tables:
  - `eval_runs`: run identity, scenario_id, strategy_bundle_id, status, created_at, completed_at
  - `eval_run_config_snapshot`: immutable copies of scenario and strategy bundle used for the run
  - `eval_orders`: submitted orders and state transitions
  - `eval_fills`: fills/fees/slippage
  - `eval_positions`: position lifecycle summaries
  - `eval_equity_snapshots`: cash, equity, exposure, drawdown inputs per bar
  - `eval_decision_events`: agent decisions, prompts/trace IDs, risk approvals/rejections
  - `eval_run_metrics`: scalar summary metrics
  - `eval_metric_breakdowns`: per-asset/tag/exit/period metrics
  - `eval_plot_series`: optional materialized timeseries for fast dashboard plots

## 6. Recommended Xvision metrics set

### V1 minimum, aligned with continuous replay

Add these as soon as scenario DB/eval run artifacts land:

- run status and runtime
- asset
- scenario time window
- granularity
- starting equity
- final equity
- realized PnL
- unrealized PnL at end
- total return percent
- market change percent for the scenario asset
- alpha vs market change
- total trades
- trades per day
- win/draw/loss count
- winrate
- average trade return
- median trade return
- profit factor
- expectancy and expectancy ratio
- max drawdown absolute
- max drawdown percent
- drawdown start/end timestamps
- current drawdown at run end
- best trade / worst trade
- best day / worst day when Hour1 data is used
- average holding duration
- max consecutive wins/losses
- rejected decisions
- risk-blocked decisions
- forced exits at scenario end
- order count
- fill count
- fees paid
- estimated slippage paid

### V1 graph set

- **Asset debug chart**
  - OHLC bars
  - buy/sell/hold/resize decisions
  - submitted order markers
  - fill markers
  - position size overlay
  - optional strategy diagnostic series
- **Run equity chart**
  - cash
  - realized PnL
  - unrealized PnL
  - total equity
  - benchmark/market-change line
- **Drawdown chart**
  - absolute underwater
  - percent underwater
  - max drawdown markers
- **Decision breakdown chart/table**
  - PnL and winrate by rationale tag / exit reason / risk rule

### V2 metrics

- Sharpe, Sortino, Calmar, CAGR, SQN
- periodic breakdowns by day/week/month/year/weekday
- per-asset metrics for multi-asset scenarios
- exposure over time
- turnover and volume
- capital utilization
- average/peak open positions
- average/peak gross exposure
- wallet-balance metrics distinct from closed-trade metrics
- reason-code confusion matrix: desired action vs risk-approved action vs executed fill

## 7. Concrete API shape for Xvision

Suggested REST/data contracts after M2/M3:

```text
GET /api/eval-runs/:id/summary
GET /api/eval-runs/:id/metrics
GET /api/eval-runs/:id/metrics/breakdowns?by=asset|decision_tag|exit_reason|period|risk_rule
GET /api/eval-runs/:id/trades
GET /api/eval-runs/:id/orders
GET /api/eval-runs/:id/fills
GET /api/eval-runs/:id/positions
GET /api/eval-runs/:id/equity
GET /api/eval-runs/:id/drawdown
GET /api/eval-runs/:id/market-context
GET /api/eval-runs/:id/decisions
GET /api/eval-runs/:id/plot-config
```

Suggested CLI commands:

```text
xvn eval show <run_id>
xvn eval metrics <run_id>
xvn eval plot <run_id> --asset BTC --open
xvn eval breakdown <run_id> --by decision-tag
xvn eval breakdown <run_id> --by exit-reason
xvn eval export <run_id> --format json|csv
```

## 8. What not to copy from Freqtrade

Do not copy these directly:

- Freqtrade's exchange/pairlist assumptions as-is; Xvision scenarios should stay venue/data-source explicit.
- Its strategy API shape; Xvision's core value is agentic eval, StrategyBundle lineage, and reproducibility.
- Its single large strategy-stat blob; Xvision should use normalized DB tables plus JSON snapshots where appropriate.
- Its crypto-only pair notation as the central abstraction. Xvision should keep `AssetSymbol`/venue-specific asset mapping.
- Plotly HTML-file generation as the only graph story. Xvision dashboard should render from API-returned structured series, while CLI can still export standalone HTML later.

## 9. Priority recommendation

For the current scenario/eval roadmap, fold Freqtrade ideas into the existing three milestones like this:

### M1 — Bar cache + Alpaca fetcher + asset unlock

Do not add the full graph stack here. Add only enough market-context plumbing to calculate `market_change` for an asset/time window.

### M2 — Scenario table + CLI

Add structured eval artifacts and v1 metrics:

- `eval_equity_snapshots`
- `eval_run_metrics`
- `eval_metric_breakdowns`
- `eval_decision_events`
- closed-trade vs equity-curve metric distinction
- CLI `xvn eval metrics <run_id>`

This is where Freqtrade's reporting lessons matter most.

### M3 — Dashboard surface

Add graph surfaces:

- asset debug chart
- equity curve
- underwater/drawdown chart
- decision breakdown panels
- clone/re-run scenario buttons with comparison view

## 10. Bottom line

Freqtrade's best contribution to Xvision is not its Alpaca support or exchange execution. It is its evaluation ergonomics:

- every run becomes a durable artifact
- every artifact has scalar metrics, grouped metrics, and timeseries
- every graph is explainability-oriented
- reports expose not only PnL, but also why PnL happened: pair, tag, exit reason, period, drawdown, signal, and indicator context

For Xvision, adapt that into an agentic eval language:

```text
Freqtrade enter_tag / exit_reason  -> Xvision decision_tag / exit_reason / risk_rule
Freqtrade pair metrics             -> Xvision asset metrics
Freqtrade plot-dataframe           -> Xvision per-asset decision/fill debug chart
Freqtrade plot-profit              -> Xvision run equity + drawdown dashboard
Freqtrade wallet stats             -> Xvision mark-to-market equity snapshots
Freqtrade backtesting-analysis     -> Xvision decision-context analysis
```

That gives Xvision the scenario creation layer you are designing plus the reporting layer needed to make disposable scenario testing useful rather than just numerous.
