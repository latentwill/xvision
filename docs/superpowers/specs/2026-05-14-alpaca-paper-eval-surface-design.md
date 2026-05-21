# Alpaca Paper Eval Surface - Design

> **Status: SUPERSEDED** (2026-05-21) — see `team/intake/2026-05-21-alpaca-live-eval-and-executor-refactor.md`. The Paper-mode executor this spec elaborates on is being **deleted** by the `executor-refactor` track. The Alpaca surface work that's still relevant (`BrokerSurface` widening for live execution) moves into the `live-bar-source-alpaca` and `live-eval-launch-and-freeze` track contracts. Keep this file as historical context for the Alpaca API surface inventory; do not implement against it.
> **Originally drafted:** 2026-05-14.
> **Author:** xvision team.
> **Companion specs:** [Eval Engine Design](./2026-05-08-eval-engine-design.md) | [Custom-Scenario Eval](./2026-05-11-custom-scenario-eval-design.md) | [TradingView Lightweight Eval Surface](./2026-05-14-tradingview-lightweight-eval-surface-design.md)
> **Tracking:** Follow-up to the current BTC-only Alpaca executor and narrow `BrokerSurface`. This spec was originally written to extend the "paper mode is just submit_order/position/balance" cut in `crates/xvision-engine/src/eval/executor/paper.rs`, `crates/xvision-execution/src/broker_surface.rs`, and `crates/xvision-execution/src/alpaca.rs` — but `PaperExecutor` is now scheduled for deletion. See the superseded note above.

---

## 1. Purpose

Xvision's eval engine has a useful paper-mode skeleton, but the surface is much narrower than Alpaca's paper Trading API:

- `BrokerSurface` exposes only `submit_order`, `position`, and `balance`.
- The Alpaca implementation supports only a market-style notional submit path, plus optional bracket legs.
- `xvision-execution/src/alpaca.rs` is still BTC-only (`BTC/USD`) and maps every order through a hardcoded symbol path.
- `PaperExecutor` uses `BTC_REFERENCE_PRICE_USD` to size base units instead of quoting the active asset.
- There is no eval access to order list/get/replace/cancel, order status streams, account activities, portfolio history, assets, market clock/calendar, watchlists, or close-position workflows.
- Scenario granularity is artificially constrained by Xvision (`Hour1 | Day1` in the custom-scenario spec) rather than by Alpaca's available data/order surfaces.

The goal is not to expose Alpaca wholesale. The goal is to expose the subset that can improve a trader agent's decisions, make execution more faithful, or help a user audit why a paper evaluation behaved the way it did. Endpoints that are useful only as product chrome stay out of the agent path and are deferred unless they directly support scenario setup or run evidence.

---

## 2. Source Inventory

Official docs reviewed while drafting:

- Alpaca Trading API overview and Paper Trading: `https://docs.alpaca.markets/docs/trading-api`
- Create/list/get/replace/cancel orders: `https://docs.alpaca.markets/us/reference/postorder`, `https://docs.alpaca.markets/reference/getallorders-1`, `https://docs.alpaca.markets/reference/getorderbyorderid-1`, `https://docs.alpaca.markets/reference/patchorderbyorderid-1`, `https://docs.alpaca.markets/reference/deleteorderbyorderid-1`
- Order behavior, extended hours, advanced order classes, lifecycle, TIFs: `https://docs.alpaca.markets/docs/trading/orders/`
- Account and portfolio history: `https://docs.alpaca.markets/reference/getaccount-1`, `https://docs.alpaca.markets/reference/getaccountportfoliohistory-1`
- Activities: `https://docs.alpaca.markets/reference/getaccountactivities-2`
- Positions: `https://docs.alpaca.markets/us/reference/getallopenpositions`, `https://docs.alpaca.markets/reference/getopenposition`, `https://docs.alpaca.markets/reference/deleteopenposition-1`, `https://docs.alpaca.markets/us/reference/deleteallopenpositions-1`
- Assets and calendar/clock: `https://docs.alpaca.markets/v1.4.2/reference/get-v2-assets-1`, `https://docs.alpaca.markets/reference/get-v2-assets-symbol_or_asset_id`, `https://docs.alpaca.markets/v1.3/reference/getclock`, `https://docs.alpaca.markets/v1.1/reference/getcalendar-1`
- Watchlists: `https://docs.alpaca.markets/reference/getwatchlists-1`, `https://docs.alpaca.markets/reference/postwatchlist-1`, `https://docs.alpaca.markets/reference/getwatchlistbyid-1`, `https://docs.alpaca.markets/reference/getwatchlistbyname-1`, `https://docs.alpaca.markets/reference/updatewatchlistbyid-1`, `https://docs.alpaca.markets/reference/updatewatchlistbyname-1`, `https://docs.alpaca.markets/reference/deletewatchlistbyid-1`, `https://docs.alpaca.markets/reference/deletewatchlistbyname-1`, `https://docs.alpaca.markets/reference/addassettowatchlist-1`, `https://docs.alpaca.markets/reference/removeassetfromwatchlist-1`
- Trading stream and market-data stream: `https://docs.alpaca.markets/us/docs/websocket-streaming`, `https://docs.alpaca.markets/us/docs/streaming-market-data`, `https://docs.alpaca.markets/docs/real-time-crypto-pricing-data`

---

## 3. Relevance Model

Every Alpaca capability is classified by the consumer it helps:

| Tier | Consumer | Include when it helps | Examples |
|---|---|---|---|
| **A. Agent decision context** | Trader/risk agent at a decision point | The value can change position sizing, entry/exit timing, order type, risk posture, or abstention. | account buying power, open positions, open orders, latest quote/bar, asset tradability, order status. |
| **B. Agent action surface** | Trader/risk agent issuing or managing a trade | The action is part of a realistic strategy loop and can be policy-gated to the active run. | create order, replace own open order, cancel own open order, close own position. |
| **C. User audit evidence** | User reviewing a completed or running eval | The data explains what happened at a decision point or reconciles Xvision vs broker state. | balance snapshots, portfolio history, fills, activities, order lifecycle stream, positions over time. |
| **D. Scenario/setup support** | User or system before a run | The data helps choose an asset/window or validate that a scenario is tradable. | asset metadata, tradability flags, market calendar for equities. |
| **E. Deferred/window dressing** | Dashboard convenience only | The data does not change the agent's decision and is not required to audit a run. | watchlist mutation, general calendar browsing, arbitrary account maintenance. |

This means the agent does not get a generic Alpaca console. It gets a run-scoped trading workstation: enough state to decide well, enough actions to manage its own orders, and enough broker evidence for the user to inspect the result.

---

## 4. Locked Decisions

| # | Decision |
|---|---|
| 1 | **Scope to paper-eval effectiveness.** Include only functions that improve agent decision quality, execution realism, or user auditability. |
| 2 | **Separate agent, audit, and setup APIs.** A function being useful to the dashboard does not make it agent-callable. |
| 3 | **Agent context is decision-point state.** Give the agent account constraints, positions, open orders, asset capability, current/near-current market data, and relevant order/fill history. |
| 4 | **Agent actions are run-scoped.** The agent can create, replace, cancel, or close only orders/positions attributable to the active paper eval run, unless an operator grants broader permission. |
| 5 | **User audit captures broker truth.** Persist account snapshots, orders, order events, positions, activities, and portfolio history so users can replay any decision point. |
| 6 | **No more BTC reference price.** Order sizing uses quote/latest bar/latest quote/position price depending on asset class and available feed. Failure to quote is a validation error, not a fallback constant. |
| 7 | **Use streams for paper order state.** Polling remains a fallback, but `trade_updates` is the source for fills, partial fills, cancels, replacements, and rejects in long-running paper evals. |
| 8 | **Timeframe unlock serves audit and strategy cadence.** Accept Alpaca-compatible `period`, `timeframe`, `start`, `end`, `intraday_reporting`, and `pnl_reset` where they affect portfolio/equity reconstruction or scenario replay. |
| 9 | **Watchlists are setup inputs, not agent tools.** Read-only watchlist import can help define a scenario universe; mutation is deferred. |
| 10 | **Calendar is validation/audit for equities, not agent alpha.** The agent receives session status only when relevant; general calendar views are user-facing setup/audit. |

---

## 5. Scoped Alpaca Surface

### 5.1 Agent Decision Context

These functions can plausibly improve strategy effectiveness at a decision point. They are safe as read-only agent tools inside paper eval.

| Xvision function | Alpaca source | Why it matters to the agent |
|---|---|---|
| `alpaca.account.snapshot` | `GET /v2/account` | Buying power, equity, margin flags, trading-blocked flags, and account status directly affect sizing and whether to trade. |
| `alpaca.positions.list` | `GET /v2/positions` | Open exposure, side, qty, cost basis, market value, and unrealized PnL are core risk inputs. |
| `alpaca.positions.get` | `GET /v2/positions/{symbol_or_asset_id}` | Per-asset state prevents accidental doubling, bad exits, and wrong position assumptions. |
| `alpaca.orders.open` | `GET /v2/orders?status=open` | Avoid duplicate orders, conflicting closing orders, and stale pending exposure. |
| `alpaca.orders.get_by_client_id` | `GET /v2/orders:by_client_order_id` | Idempotency recovery after retry or agent/tool interruption. |
| `alpaca.assets.get` | `GET /v2/assets/{symbol_or_asset_id}` | Tradable, fractionable, shortable, marginable, extended-hours, IPO, options/overnight flags. |
| `alpaca.market.session_status` | `GET /v2/clock` plus scenario calendar policy | Only session status, next open/close, and whether the current order would queue/reject. |
| `alpaca.market.latest_context` | market data stream/rest wrapper | Quote/latest bar/current price for sizing, limit placement, and stop/target calculation. |
| `alpaca.recent_fills` | `GET /v2/account/activities` filtered to trade activity for this run | Recent fills can alter the next action, especially after partial fills or delayed fills. |

Agent context should be packaged as one normalized `BrokerDecisionContext` per decision tick, rather than forcing the agent to call 8 tools every time:

```rust
pub struct BrokerDecisionContext {
    pub account: AccountConstraintSnapshot,
    pub positions: Vec<PositionSnapshot>,
    pub open_orders: Vec<OrderSnapshot>,
    pub asset: AssetCapability,
    pub session: SessionStatus,
    pub latest_price: Option<PriceSnapshot>,
    pub recent_fills: Vec<FillSnapshot>,
    pub broker_warnings: Vec<String>,
}
```

### 5.2 Agent Action Surface

These functions can improve strategy effectiveness because they let the agent express realistic execution intent. They are mutation tools and must be run-scoped.

| Xvision function | Alpaca endpoint/function | Agent value | Guard |
|---|---|---|---|
| `alpaca.orders.create` | `POST /v2/orders` | Use market, limit, stop, stop-limit, trailing-stop, bracket, OCO, OTO where valid. | Must use active run `client_order_id` prefix and pass policy validation. |
| `alpaca.orders.replace_own` | `PATCH /v2/orders/{order_id}` | Adjust limit/stop/trail/qty on the agent's own open order instead of cancel/recreate. | Order must belong to active run and be replaceable. |
| `alpaca.orders.cancel_own` | `DELETE /v2/orders/{order_id}` | Remove stale/unwanted exposure when thesis changes. | Order must belong to active run. |
| `alpaca.positions.close_own_symbol` | `DELETE /v2/positions/{symbol_or_asset_id}` | Exit or reduce the active strategy's own position. | Symbol must be in active scenario/run policy; qty/percentage scoped. |

`cancel_all_orders` and `close_all_positions` are not agent tools. They are operator kill switches and run-teardown helpers.

Create-order model should include all fields that change execution behavior:

```rust
pub struct AgentOrderIntent {
    pub symbol: String,
    pub qty: Option<DecimalString>,
    pub notional: Option<DecimalString>,
    pub side: OrderSide,
    pub order_type: AlpacaOrderType,          // market | limit | stop | stop_limit | trailing_stop
    pub time_in_force: AlpacaTimeInForce,     // asset-class validated
    pub limit_price: Option<DecimalString>,
    pub stop_price: Option<DecimalString>,
    pub trail_price: Option<DecimalString>,
    pub trail_percent: Option<DecimalString>,
    pub extended_hours: Option<bool>,
    pub order_class: Option<OrderClass>,      // simple | bracket | oco | oto
    pub take_profit: Option<TakeProfitLeg>,
    pub stop_loss: Option<StopLossLeg>,
    pub client_order_id: String,
}
```

Model but do not agent-enable in this spec:

- `mleg` options.
- `position_intent`.
- `advanced_instructions`.
- IPO/fixed-income-specific flows.

### 5.3 User Audit Evidence

These functions may not help the agent choose a better trade, but they help the user understand and trust the eval.

| Xvision function | Alpaca source | Audit value |
|---|---|---|
| `alpaca.account.snapshots` | `GET /v2/account` sampled per decision and on broker events | Show balance, equity, buying power, and restrictions at the exact decision point. |
| `alpaca.orders.timeline` | `GET /v2/orders` plus `trade_updates` | Reconstruct order lifecycle from submit to terminal state. |
| `alpaca.activities.reconcile` | `GET /v2/account/activities` | Confirm fills, fees, cash movements, dividends, option events, and other broker-side effects. |
| `alpaca.portfolio_history.import` | `GET /v2/account/portfolio/history` | Compare official paper-account equity/PnL to Xvision's stored equity curve. |
| `alpaca.positions.snapshots` | `GET /v2/positions` sampled per decision and on fills | Explain exposure and PnL changes over time. |
| `alpaca.stream.trade_updates` | `wss://paper-api.alpaca.markets/stream` | Capture partial fills, cancels, rejects, replaces, and terminal state changes. |

Required portfolio-history query support for audit and charts:

- `period`: `number + unit`, units `D`, `W`, `M`, `A`.
- `timeframe`: `1Min`, `5Min`, `15Min`, `1H`, `1D`.
- `start`, `end`: RFC3339; only two of `start`, `end`, `period` allowed.
- `intraday_reporting`: `market_hours`, `extended_hours`, `continuous`.
- `pnl_reset`: `per_day`, `no_reset`.
- `cashflow_types`: `ALL`, `NONE`, or comma-separated activity types.
- Keep deprecated `extended_hours` as read-compatible only; prefer `intraday_reporting`.

Order enums and validation retained because they affect execution quality:

- Order types: `market`, `limit`, `stop`, `stop_limit`, `trailing_stop`.
- Equity TIFs: `day`, `gtc`, `opg`, `cls`, `ioc`, `fok`.
- Crypto TIFs: `gtc`, `ioc`.
- Options TIF: `day`.
- Equity order classes: simple/empty, `bracket`, `oco`, `oto`.
- Crypto: simple/empty.
- Fractional orders: support both `qty` and `notional` rules; reject invalid combinations before HTTP.
- Extended hours: expose `extended_hours`; validate that it only works for documented order type/TIF combinations.

List-order filters relevant to audit/user views:

- `status`: `open`, `closed`, `all`.
- `limit`: default/max validated by API capability.
- `after`, `until`, `direction`.
- `nested`.
- `symbols`.
- `side`.
- `asset_class`.
- `before_order_id`, `after_order_id` with mutual exclusion against `after`/`until`.

Close-position controls relevant to agent/user:

- `qty` and `percentage`, mutually exclusive.
- `cancel_orders` on close-all.
- Preserve Alpaca multi-status results so partial liquidation failures are visible in run findings.

### 5.4 Scenario and Setup Support

These are user/system setup tools, not normally agent tools.

| Xvision function | Alpaca source | Include because |
|---|---|---|
| `alpaca.assets.search` | `GET /v2/assets` | User needs valid tradable assets for scenario creation and strategy universe selection. |
| `alpaca.assets.get` | `GET /v2/assets/{symbol_or_asset_id}` | Scenario validator needs tradability/capability flags. |
| `alpaca.market.calendar.validate_window` | `GET /v2/calendar` | Equity scenarios need valid trading dates, early-close awareness, and settlement/trading-date distinction. |
| `alpaca.market.clock` | `GET /v2/clock` | Paper mirror needs live session state; dashboard can show whether orders will route or queue. |
| `alpaca.watchlists.import` | `GET /v2/watchlists`, `GET /v2/watchlists/{id}`, `GET /v2/watchlists:by_name` | Optional source of a user-defined universe; useful for setup, not per-tick agent reasoning. |

Asset list filters:

- `status`.
- `asset_class`.
- `exchange`.
- `attributes`: `ptp_no_exception`, `ptp_with_exception`, `ipo`, `has_options`, `options_late_close`, `fractional_eh_enabled`, `overnight_tradable`, `overnight_halted`.

Calendar query support:

- `start`, `end`.
- `date_type`: `TRADING` or `SETTLEMENT`.

Watchlist policy:

- **Include read-only import** for scenario/universe creation.
- **Defer mutation** (`create`, `update`, `delete`, `add_asset`, `remove_asset`) because it does not improve a paper eval decision and creates product/account side effects.
- Do not put watchlist tools in agent context unless the active strategy explicitly defines "trade this watchlist" as its universe and the scenario is multi-asset.

### 5.5 Explicitly Deferred

| Alpaca function family | Why deferred |
|---|---|
| Watchlist mutation | Helpful product feature, not agent effectiveness or run audit. |
| General account maintenance/reset flows | Dangerous and unrelated to evaluating a strategy. |
| Calendar browser UI beyond scenario validation/session status | Useful to humans, but not decision-changing for agents. |
| Options/multileg trading | Potentially high value later, but needs a dedicated options eval model and risk controls. |
| Advanced router instructions | Execution-quality surface, but only after core order semantics are proven. |
| Broker API account-management endpoints | Xvision is evaluating a user's own paper Trading API account, not building a brokerage app. |

### 5.6 Streams

Streams are relevant when they improve execution realism or audit precision.

| Xvision function | Alpaca stream | Include because |
|---|---|---|
| `alpaca.stream.trade_updates` | `wss://paper-api.alpaca.markets/stream` | Essential for realistic order state, fills, partial fills, cancellations, replacements, and rejects. |
| `alpaca.stream.market_data` | `wss://stream.data.alpaca.markets/{version}/{feed}` | Relevant for live paper mirror decisions and latest-price sizing; chart streaming consumes the normalized Xvision stream. |

Trade-update events to persist:

- Common: `new`, `fill`, `partial_fill`, `canceled`, `expired`, `done_for_day`, `replaced`.
- Less common: `accepted`, `rejected`, `pending_new`, `stopped`, `pending_cancel`, `pending_replace`, `calculated`, `suspended`, `order_replace_rejected`, `order_cancel_rejected`.

Market-data stream channels to support by feed capability:

- Stocks: trades, quotes, bars, updated bars, daily bars, statuses, LULD, corrections, cancel errors.
- Crypto: trades, quotes, orderbooks, minute bars, daily bars.
- Options/news are deferred until options/news eval surfaces exist.

---

## 6. Xvision Architecture

### 6.1 New module boundary

```text
crates/xvision-execution/src/alpaca/
  mod.rs
  client.rs              # HTTP client + auth + base URL selection
  eval_surface.rs        # scoped paper-eval facade
  orders.rs              # request/response models + validation
  account.rs
  positions.rs
  assets.rs
  watchlists.rs          # read-only import only
  calendar.rs
  streams.rs
  capability.rs
```

`BrokerSurface` stays intentionally narrow for mode-agnostic strategy execution. The new `AlpacaPaperEvalSurface` is the scoped adapter used by:

- eval paper mirror,
- run-detail audit panels,
- CLI audit/operator commands,
- MCP/tool registry functions,
- reconciliation jobs,
- findings extractor inputs.

### 6.2 Trait split

```rust
#[async_trait]
pub trait BrokerSurface {
    async fn submit_order(&self, req: OrderRequest) -> anyhow::Result<OrderConfirmation>;
    async fn position(&self, asset: &str) -> anyhow::Result<f64>;
    async fn balance(&self) -> anyhow::Result<f64>;
}

#[async_trait]
pub trait AlpacaPaperEvalSurface {
    // One call for agent context at a decision point.
    async fn decision_context(&self, req: DecisionContextRequest) -> Result<BrokerDecisionContext>;

    // Run-scoped agent actions.
    async fn create_order(&self, req: AgentOrderIntent) -> Result<AlpacaOrder>;
    async fn replace_run_order(&self, run_id: RunId, id: OrderId, req: ReplaceOrderRequest) -> Result<AlpacaOrder>;
    async fn cancel_run_order(&self, run_id: RunId, id: OrderId) -> Result<CancelResult>;
    async fn close_run_position(&self, run_id: RunId, symbol_or_asset_id: &str, req: ClosePositionRequest) -> Result<AlpacaOrder>;

    // Audit/reconciliation.
    async fn get_account_snapshot(&self) -> Result<AlpacaAccount>;
    async fn get_portfolio_history(&self, req: PortfolioHistoryRequest) -> Result<PortfolioHistory>;
    async fn list_activities(&self, req: ActivitiesRequest) -> Result<Vec<AccountActivity>>;
    async fn list_orders(&self, req: ListOrdersRequest) -> Result<Vec<AlpacaOrder>>;
    async fn get_order(&self, id: OrderId, nested: bool) -> Result<AlpacaOrder>;
    async fn get_order_by_client_id(&self, client_order_id: &str) -> Result<AlpacaOrder>;
    async fn list_positions(&self) -> Result<Vec<AlpacaPosition>>;
    async fn get_position(&self, symbol_or_asset_id: &str) -> Result<Option<AlpacaPosition>>;

    // Scenario/setup support.
    async fn list_assets(&self, req: ListAssetsRequest) -> Result<Vec<AlpacaAsset>>;
    async fn get_asset(&self, symbol_or_asset_id: &str) -> Result<AlpacaAsset>;
    async fn get_clock(&self) -> Result<MarketClock>;
    async fn get_calendar(&self, req: CalendarRequest) -> Result<Vec<MarketCalendarDay>>;
    async fn import_watchlist(&self, req: WatchlistImportRequest) -> Result<ScenarioUniverse>;
}
```

Operator-only kill switches (`cancel_all_orders`, `close_all_positions`) live outside the agent trait in `AlpacaPaperOperatorSurface`.

### 6.3 Eval run integration

Paper mirror run flow:

1. Resolve strategy and scenario.
2. Validate scenario asset(s) through `assets.get/list`.
3. Validate session and reporting policy through `clock/calendar` only when the asset class needs it.
4. Open `trade_updates` stream when run starts.
5. On each decision tick:
   - build one `BrokerDecisionContext`,
   - persist the account/position/order snapshots for audit,
   - feed the context into the agent,
   - validate requested order against account/asset/session policy,
   - submit order with deterministic `client_order_id`,
   - record order immediately,
   - consume stream events until terminal or policy timeout.
6. Reconcile at run end:
   - `orders.list(status=all, after=run.started_at)`,
   - `activities` for fills/non-trade activity,
   - `portfolio_history` for official equity curve,
   - positions snapshot.
7. Persist raw broker artifacts under the run directory and normalized rows in SQLite.

### 6.4 Storage additions

New SQLite tables:

- `alpaca_order_events`: raw `trade_updates` payloads by run/order/client id.
- `alpaca_orders`: latest normalized order state.
- `alpaca_positions`: periodic position snapshots.
- `alpaca_account_snapshots`: equity/buying power/status samples.
- `alpaca_portfolio_history`: official history imports keyed by request hash.
- `alpaca_activities`: reconciled account activities.
- `alpaca_assets_cache`: assets list cache with `fetched_at`.
- `alpaca_universe_imports`: read-only watchlist/asset-list imports used to create scenarios.

Run directory additions:

```text
~/.xvn/runs/<run_id>/
  broker/
    account.jsonl
    orders.jsonl
    order_events.jsonl
    positions.jsonl
    activities.jsonl
    portfolio_history.json
    stream_status.jsonl
```

---

## 7. API, CLI, MCP, Dashboard Surfaces

### 7.1 Engine API

Add `crate::api::broker::alpaca` functions:

- `decision_context`
- `agent_order_create`
- `agent_order_replace`
- `agent_order_cancel`
- `agent_position_close`
- `audit_account_snapshot`
- `audit_portfolio_history`
- `audit_activities_list`
- `audit_orders_list`
- `audit_orders_get`
- `audit_order_get_by_client_id`
- `audit_positions_list`
- `audit_positions_get`
- `setup_assets_list`
- `setup_assets_get`
- `setup_clock_get`
- `setup_calendar_validate`
- `setup_watchlist_import`
- `operator_orders_cancel_all`
- `operator_positions_close_all`
- stream status/control functions for paper runs

Each records `api_audit` with:

- domain: `alpaca`
- action: function name
- target: order id, symbol, watchlist id, or account
- mode: `paper`
- outcome and latency
- run id when action is tied to an eval

### 7.2 CLI

New commands:

```text
xvn alpaca audit account
xvn alpaca audit portfolio-history --period 1M --timeframe 5Min --intraday-reporting extended_hours
xvn alpaca audit activities --after ... --until ... --category trade_activity
xvn alpaca audit orders ls --status all --symbols AAPL,BTC/USD --nested
xvn alpaca audit orders get <order_id>
xvn alpaca audit orders get-client <client_order_id>
xvn alpaca audit positions ls
xvn alpaca audit positions get <symbol>
xvn alpaca setup assets ls --asset-class us_equity --status active --attribute overnight_tradable
xvn alpaca setup assets get AAPL
xvn alpaca setup calendar validate --asset-class us_equity --start 2026-05-01 --end 2026-05-31
xvn alpaca setup watchlist import --name "Large Cap Momentum"
xvn alpaca operator orders cancel-all --confirm cancel-all-paper-orders
xvn alpaca operator positions close-all --cancel-orders --confirm close-all-paper-positions
```

Order create/replace/cancel and close-position are not general CLI commands in the primary flow; they are invoked through paper eval runs or explicit operator/debug surfaces so they retain run attribution.

### 7.3 MCP/tools

Agent tools available by default inside paper eval:

- `broker.decision_context`.
- `broker.create_order`.
- `broker.replace_own_order`.
- `broker.cancel_own_order`.
- `broker.close_own_position`.

Read-only audit/setup tools available to user-facing assistants, not the trader agent by default:

- account snapshot, portfolio history, activities, orders list/get, positions list/get, assets, session/calendar validation, watchlist import.

Mutation tools require explicit strategy/run policy:

- create/replace/cancel order,
- close position,

Operator-only tools are never trader-agent tools:

- cancel all orders,
- close all positions.

Tool outputs use the same normalized DTOs as API/CLI so agent prompts can compare them consistently.

### 7.4 Dashboard

Add broker panels where they improve awareness:

- Run detail: decision-point account snapshot, order timeline, fills, positions, activities, portfolio history reconciliation, stream status.
- Scenario setup: asset browser, asset capability flags, calendar/window validation, optional watchlist import.
- Settings: credential/test-connection status only. Full account maintenance is out of scope.

---

## 8. Timeframe and Calendar Capability Matrix

Replace hardcoded Xvision enum limitations with a capability matrix:

```rust
pub enum AlpacaTimeframe {
    Min1,
    Min5,
    Min15,
    Hour1,
    Day1,
}

pub enum IntradayReporting {
    MarketHours,
    ExtendedHours,
    Continuous,
}

pub struct AlpacaReportingCapability {
    pub portfolio_history_timeframes: Vec<AlpacaTimeframe>,
    pub max_intraday_period_days: u32,
    pub supports_start_end_period_rule: bool,
    pub intraday_reporting: Vec<IntradayReporting>,
    pub pnl_reset_modes: Vec<PnlReset>,
}
```

Rules:

- Portfolio history exposes `1Min`, `5Min`, `15Min`, `1H`, `1D`.
- Intraday history accepts `market_hours`, `extended_hours`, or `continuous`.
- Scenarios can request finer bars where the data fetcher supports them; eval execution cadence is independent of chart/reporting timeframe.
- UI range buttons are convenience presets only; API accepts arbitrary `start`/`end`/`period` requests.
- Market calendar is used for equities; crypto uses continuous calendar and crypto stream capabilities.

---

## 9. Safety Model

Safety is explicit because this surface can liquidate paper positions and will later share concepts with live trading.

| Operation | Default access | Extra guard |
|---|---|---|
| Decision context snapshot | Agent inside run; user audit | Run id required for agent. |
| Read account/orders/positions/activities/portfolio history | User audit; optionally assistant audit | Not injected into trader agent except via curated context. |
| Read assets/calendar/watchlist imports | Scenario setup | Watchlists read-only. |
| Create order | Agent only inside active paper run; user via CLI/UI | run policy + idempotency key |
| Replace/cancel single order | Agent if run policy allows | target order must belong to current run unless operator override |
| Cancel all orders | User only | confirmation phrase + audit |
| Close position | User; agent if run policy allows | symbol scoped |
| Close all positions | User only | confirmation phrase + audit |
| Watchlist mutation/delete | Deferred | Not in v1 paper-eval scope. |

No live Alpaca endpoint is enabled by this spec. Base URL must be paper unless a future live-trading spec adds a separate opt-in.

---

## 10. Milestones

### M1 - Decision context and audit snapshots

Ships:

- New Alpaca module layout and HTTP client.
- `BrokerDecisionContext` builder.
- Read-only audit API/CLI: account snapshot, portfolio history, activities, orders list/get, positions list/get.
- Setup API/CLI: assets list/get, session/calendar validation, watchlist import.
- Tests with mocked Alpaca responses.
- Existing `BrokerSurface` continues to compile.

Acceptance:

- `xvn alpaca portfolio-history --timeframe 1Min --intraday-reporting continuous` reaches the typed request layer.
- Assets list can filter by `overnight_tradable` and `fractional_eh_enabled`.
- `cargo test -p xvision-execution alpaca_surface_read_only`.

### M2 - Run-scoped order and position actions

Ships:

- Agent create order.
- Agent replace/cancel own open order.
- Agent close/reduce own position.
- Typed validation for order classes, TIFs, fractional/notional rules, extended hours, nested orders, and replace restrictions.
- `PaperExecutor` stops using `BTC_REFERENCE_PRICE_USD`; it quotes active asset.
- Audit records for every mutation.

Acceptance:

- Mock tests cover market, limit, stop, stop-limit, trailing-stop, bracket, OCO, OTO, fractional notional, extended-hours limit, replace, cancel, and close position.
- Invalid combinations fail before HTTP.

### M3 - Streaming paper mirror

Ships:

- `trade_updates` stream client.
- Event persistence and reconciliation.
- Paper run consumes stream fills instead of polling when stream is available.
- Polling fallback remains.
- Dashboard stream status.

Acceptance:

- Simulated stream events update order state and run fill markers.
- Partial fills and replace/cancel rejects become findings inputs.

### M4 - Scenario/timeframe unlock for eval relevance

Ships:

- Scenario granularity uses capability matrix instead of `{Hour1, Day1}` restriction.
- Portfolio-history controls support every documented timeframe/reporting mode.
- Calendar-aware equity scenario validation and session status.
- Crypto 24/7 continuous reporting.

Acceptance:

- User can create a paper portfolio-history chart for arbitrary `start/end` with `1Min`, `5Min`, `15Min`, `1H`, or `1D`.
- Scenario create validates against asset/calendar/data-source capability instead of a fixed enum.

### M5 - Awareness panels and agent tools

Ships:

- Run-detail broker awareness panels.
- MCP/tool registry functions.
- Run-detail broker artifact panels.
- Watchlist read integration with scenario universe creation.

Acceptance:

- A paper run detail can show the broker order timeline, fills, activities, positions, and official portfolio history side by side with Xvision decisions.

---

## 11. Test Plan

- Unit tests for every included request validator.
- Golden JSON tests for Alpaca DTO de/serialization.
- Mock HTTP tests for each endpoint and failure class.
- Stream parser tests for every trade-update event.
- Eval integration test with `MockBrokerSurface` plus mocked stream events.
- CLI smoke tests for every command.
- Audit tests verifying operator-only actions require confirmation and record target/outcome.
- Backward compatibility test: existing `eval run --mode paper` still works against the minimal broker path while the expanded facade is introduced.

---

## 12. Open Questions

- Should order mutation tools be available to all paper-eval agents by default, or only after a strategy declares `paper_trading_permissions`?
- Do we store raw Alpaca payloads indefinitely, or garbage-collect them after normalized rows and findings are produced?
- Is watchlist import sufficiently useful for v1, or should even read-only watchlist support wait for multi-asset scenarios?
- Which market-data quote endpoint becomes the default quote provider for equities before opening a position?
- Do options/multileg models ship as disabled DTOs in M1, or wait until an options eval spec?
