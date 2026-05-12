# Hummingbot evaluation notes for Xvision

Date: 2026-05-11
Source inspected: `hummingbot/hummingbot` open-source repository, shallow clone at commit `91ff6bf`
Xvision repo: `latentwill/xvision`, local path `/var/lib/hermes/xvision`

## Executive summary

Hummingbot is a mature, exchange-connector-heavy live trading bot framework. Xvision is currently an agentic strategy/eval harness with `StrategyBundle`s, deterministic risk, Alpaca/Orderly execution surfaces, scenario/eval design, and dashboard/API scaffolding.

The main thing Hummingbot has that Xvision does not yet have is a deep execution microstructure layer:

- order lifecycle state machines
- connector abstractions
- budget/collateral checks
- evented fills
- API throttling
- market-data streams
- executor orchestration
- managed trade lifecycles such as position, grid, DCA, TWAP, and arbitrage executors

Xvision should **not** try to become “Rust Hummingbot.” The useful move is to borrow Hummingbot’s execution architecture selectively, while preserving Xvision’s strengths: agentic strategy generation/eval, reproducible scenarios, strategy lineage, dashboard UX, and marketplace/on-chain direction.

## Repository shape observed

Hummingbot high-level structure:

- `hummingbot/connector/` — exchange, derivative, gateway, order tracking, budget checking
- `hummingbot/core/` — order books, events, throttling, network utilities, kill switch
- `hummingbot/data_feed/` — candles, market data provider, oracle-like rate sources
- `hummingbot/strategy/` — legacy strategies
- `hummingbot/strategy_v2/` — controllers, executors, backtesting engine
- `controllers/` — example/controller implementations

Counts from the inspected checkout:

- spot exchange connector directories: 27
- derivative connector directories: 18
- candle-feed directories: 26
- Strategy V2 executor types: `position_executor`, `arbitrage_executor`, `twap_executor`, `order_executor`, `lp_executor`, `xemm_executor`, `grid_executor`, `dca_executor`
- legacy strategy types include perpetual market making, pure market making, hedge, cross-exchange market making, AMM arb, liquidity mining, Avellaneda market making, and spot/perpetual arbitrage

Important Hummingbot files inspected:

- `hummingbot/connector/client_order_tracker.py`
- `hummingbot/core/data_type/in_flight_order.py`
- `hummingbot/connector/budget_checker.py`
- `hummingbot/core/api_throttler/async_throttler.py`
- `hummingbot/data_feed/market_data_provider.py`
- `hummingbot/strategy_v2/controllers/controller_base.py`
- `hummingbot/strategy_v2/executors/executor_base.py`
- `hummingbot/strategy_v2/executors/executor_orchestrator.py`
- `hummingbot/strategy_v2/executors/position_executor/data_types.py`
- `hummingbot/strategy_v2/executors/grid_executor/data_types.py`
- `hummingbot/strategy_v2/backtesting/backtesting_engine_base.py`
- `hummingbot/core/utils/kill_switch.py`

## Key differences from Xvision

### 1. Connector abstraction is central in Hummingbot

Hummingbot’s center of gravity is the connector system. Connectors standardize REST/WebSocket APIs across CEX spot, CEX perpetual, CLOB DEX, and AMM DEX venues.

Xvision currently has narrower venue implementations:

- `crates/xvision-execution/src/alpaca.rs`
- `crates/xvision-execution/src/orderly.rs`
- `crates/xvision-execution/src/broker_surface.rs`

Xvision has venue implementations, but not yet a full connector framework.

Recommendation for Xvision: add a formal `VenueConnector` layer, but avoid a connector zoo.

Sketch:

```rust
#[async_trait]
pub trait VenueConnector: Send + Sync {
    async fn trading_rules(&self, asset: &AssetRef) -> anyhow::Result<TradingRules>;
    async fn balances(&self) -> anyhow::Result<AccountBalances>;
    async fn submit_order(&self, order: VenueOrder) -> anyhow::Result<OrderAccepted>;
    async fn cancel_order(&self, client_order_id: &str) -> anyhow::Result<CancelResult>;
    async fn order_status(&self, client_order_id: &str) -> anyhow::Result<OrderStatus>;
    async fn stream_events(&self) -> anyhow::Result<OrderEventStream>;
}
```

Near-term connectors should remain narrow:

1. Alpaca
2. Orderly
3. Simulated/backtest venue
4. maybe Hyperliquid later

### 2. Hummingbot has a serious order lifecycle model

Hummingbot models order state explicitly in `InFlightOrder` and tracks orders through `ClientOrderTracker`.

Observed Hummingbot order states:

- `PENDING_CREATE`
- `OPEN`
- `PENDING_CANCEL`
- `CANCELED`
- `PARTIALLY_FILLED`
- `FILLED`
- `FAILED`
- `PENDING_APPROVAL`
- `APPROVED`
- `CREATED`
- `COMPLETED`

It tracks:

- client order id
- exchange order id
- fills by trade id
- executed base amount
- executed quote amount
- average executed price
- cumulative fees
- stale/lost orders
- late fill updates
- order-not-found counters
- cached completed orders for late events

Xvision’s current `BrokerSurface` returns a simpler `OrderConfirmation`:

```rust
pub struct OrderConfirmation {
    pub broker_order_id: String,
    pub fill_price: Option<f64>,
    pub fill_size: f64,
    pub fee: Option<f64>,
}
```

That is acceptable for early eval, but not enough for live/paper parity.

Recommendation for Xvision: add an `OrderLifecycle` model.

```rust
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum OrderState {
    PendingCreate,
    Open,
    PartiallyFilled,
    Filled,
    PendingCancel,
    Canceled,
    Rejected,
    Failed,
    Lost,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InFlightOrder {
    pub client_order_id: String,
    pub venue_order_id: Option<String>,
    pub asset: AssetRef,
    pub side: Side,
    pub order_type: OrderType,
    pub requested_qty: Decimal,
    pub limit_price: Option<Decimal>,
    pub state: OrderState,
    pub fills: Vec<FillEvent>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}
```

This is one of the highest-value ideas to borrow. It enables:

- better paper/live parity
- replayable execution traces
- dashboard debugging
- safer idempotent retries
- partial-fill readiness
- late-fill handling
- an eventual `orders` and `fills` table

### 3. Hummingbot separates controllers from executors

Hummingbot Strategy V2 splits:

- **Controller:** decides what should happen
- **Executor:** manages a concrete trade/order lifecycle

Relevant files:

- `hummingbot/strategy_v2/controllers/controller_base.py`
- `hummingbot/strategy_v2/executors/executor_base.py`
- `hummingbot/strategy_v2/executors/executor_orchestrator.py`

Xvision’s current flow is more like:

```text
Intern → TraderDecision → RiskDecision → Executor
```

Xvision’s current executor is closer to “submit this approved decision.” Hummingbot’s executor is closer to “own this trade until it closes.”

Recommendation for Xvision: add an `ExecutionPlan` / `ExecutionIntent` / `ManagedExecutor` layer.

```rust
pub enum ExecutionIntent {
    EnterPosition(PositionPlan),
    ExitPosition(ExitPlan),
    Rebalance(RebalancePlan),
    Cancel(CancelPlan),
}

#[async_trait]
pub trait ManagedExecutor {
    fn id(&self) -> ExecutorId;
    fn status(&self) -> ExecutorStatus;
    async fn on_market_event(&mut self, event: MarketEvent) -> Vec<VenueOrderAction>;
    async fn on_order_event(&mut self, event: OrderEvent) -> Vec<VenueOrderAction>;
    fn report(&self) -> ExecutorReport;
}
```

This would let a strategy emit “open long ETH with 2% stop, 5% take profit, 24h time limit,” and let a managed executor own the lifecycle.

### 4. Triple-barrier trade management maps well to Xvision

Hummingbot has `TripleBarrierConfig`:

```python
class TripleBarrierConfig:
    stop_loss
    take_profit
    time_limit
    trailing_stop
    open_order_type
    take_profit_order_type
    stop_loss_order_type
    time_limit_order_type
```

This maps directly to Xvision’s existing stop-loss/take-profit concept, but makes it more explicit and reportable.

Recommendation for Xvision: add an `ExitPolicy` and `CloseReason`.

```rust
pub struct ExitPolicy {
    pub stop_loss_pct: Option<Decimal>,
    pub take_profit_pct: Option<Decimal>,
    pub time_limit: Option<Duration>,
    pub trailing_stop: Option<TrailingStop>,
    pub stop_loss_order_type: OrderType,
    pub take_profit_order_type: OrderType,
    pub time_limit_order_type: OrderType,
}

pub enum CloseReason {
    StopLoss,
    TakeProfit,
    TimeLimit,
    TrailingStop,
    Manual,
    EndOfScenario,
    RiskVeto,
    Failed,
}
```

This improves:

- scenario replay clarity
- run reports
- debugging
- strategy comparison
- dashboard UX

### 5. Budget checking before order placement

Hummingbot’s `BudgetChecker` checks whether a set of candidate orders can be placed with available balances/collateral, and can resize or zero orders.

This is separate from risk.

- Risk asks: “Should we trade?”
- Budget asks: “Can this exact order physically be placed with current balances, fees, minimums, and locked collateral?”

Recommendation for Xvision: add `OrderCandidate` + `BudgetChecker` between `RiskDecision` and `VenueConnector`.

```rust
pub struct OrderCandidate {
    pub asset: AssetRef,
    pub side: Side,
    pub order_type: OrderType,
    pub qty: Decimal,
    pub limit_price: Option<Decimal>,
    pub estimated_fee: Decimal,
    pub required_collateral: Decimal,
}

pub trait BudgetChecker {
    fn adjust_candidates(
        &self,
        candidates: Vec<OrderCandidate>,
        account: &AccountSnapshot,
        rules: &TradingRules,
    ) -> Vec<OrderCandidate>;
}
```

Pipeline placement:

```text
RiskDecision → ExecutionPlan → BudgetChecker → VenueConnector
```

### 6. Trading rules / exchange constraints

Hummingbot connectors expose trading rules, and executors use them to quantize and validate orders.

Useful rules include:

- min order size
- min notional
- tick size
- step size
- leverage limits
- supported order types

Recommendation for Xvision: add `TradingRules` early, even if values are hardcoded in v1.

```rust
pub struct TradingRules {
    pub min_base_qty: Decimal,
    pub min_notional: Decimal,
    pub qty_step: Decimal,
    pub price_tick: Decimal,
    pub max_leverage: Option<Decimal>,
    pub supports_market: bool,
    pub supports_limit: bool,
    pub supports_brackets: bool,
}
```

This matters for both real trading and realistic simulation. A scenario that ignores min-notional/tick-size can produce fake profitable strategies that cannot actually trade.

### 7. API throttling / rate limiting

Hummingbot’s async throttler supports:

- endpoint-specific rate limits
- weighted calls
- related limits
- FIFO waiting
- warnings near capacity

Recommendation for Xvision: add a venue-aware rate limiter around:

- historical bar fetch
- order submit
- order status polling
- account polling
- position polling
- dashboard polling if it hits venue APIs

Sketch:

```rust
pub struct RateLimit {
    pub id: String,
    pub max_weight: u32,
    pub interval: Duration,
}

pub struct EndpointLimit {
    pub endpoint: String,
    pub weight: u32,
    pub related_limits: Vec<String>,
}
```

This becomes important once M1 adds Alpaca historical fetch/cache and later paper/live loops.

### 8. Market data provider abstraction

Hummingbot’s `MarketDataProvider` unifies:

- candles
- order books
- connector fallback
- public-data-only connectors
- rate sources
- price lookups
- caching/reuse of feeds
- dynamic feed initialization

Xvision has `xvision-data`, indicators, fixtures, and planned Alpaca bar cache, but not yet this market data bus.

Recommendation for Xvision: add `MarketDataProvider`.

```rust
#[async_trait]
pub trait MarketDataProvider {
    async fn candles(&self, req: CandlesRequest) -> anyhow::Result<Vec<MarketBar>>;
    async fn latest_price(&self, asset: &AssetRef) -> anyhow::Result<Price>;
    async fn order_book(&self, asset: &AssetRef) -> anyhow::Result<Option<OrderBook>>;
    async fn trading_rules(&self, asset: &AssetRef) -> anyhow::Result<TradingRules>;
}
```

For v1, this can wrap only:

- `AlpacaHistorical`
- local `bars_cache`
- fixtures

Later it can wrap live Alpaca/Orderly streams.

### 9. Backtesting is executor-aware

Hummingbot’s backtesting engine simulates executor types:

- position executors
- DCA executors
- grid executors
- order executors

Xvision’s current backtest is simpler:

- one instrument
- OHLCV bars
- fee bps
- ATR slippage
- stop/target auto-fills
- risk outcomes
- fill logs

Recommendation for Xvision: eventually simulate execution plans, not only final decisions.

```rust
pub trait ExecutorSimulator {
    type Config;

    fn simulate(
        &self,
        config: Self::Config,
        bars: &[MarketBar],
        venue: &VenueSettings,
    ) -> ExecutorSimulation;
}
```

Add simulators incrementally:

1. `PositionExecutorSimulator`
2. `OrderExecutorSimulator`
3. `DcaExecutorSimulator`
4. `GridExecutorSimulator`

For Xvision v1, only `PositionExecutorSimulator` is likely needed.

### 10. Runtime kill switch

Hummingbot has a profitability kill switch that periodically checks profitability and shuts down the bot if threshold is reached.

Xvision has deterministic risk rules, but runtime kill switches are different: they monitor the whole running system.

Recommendation for Xvision: add this later in the live/paper daemon.

```rust
pub enum KillSwitchTrigger {
    MaxDrawdownPct,
    DailyLossUsd,
    ConsecutiveLossDays,
    VenueDisconnected,
    OrderRejectRate,
    Manual,
}
```

## What not to copy

### Do not copy Hummingbot’s connector breadth

Hummingbot’s connector breadth is impressive, but Xvision should not become “Rust Hummingbot.” Xvision’s edge is agentic strategy generation/eval, reproducible scenarios, strategy lineage, dashboard UX, and marketplace/on-chain identity.

Keep venue support narrow and deep.

### Do not copy legacy strategy templates wholesale

Hummingbot’s legacy strategies are useful references, but Xvision’s `StrategyBundle`/agent architecture is different.

Extract concepts instead:

- grid executor
- DCA executor
- TWAP executor
- triple-barrier exits
- inventory skew
- order refresh cadence

### Do not bring in live order-book complexity before scenario replay is solid

Order books, partial fills, queue position, maker/taker modeling, and event streams are valuable but can swamp v1.

For scenario v1, OHLCV + explicit venue assumptions is enough.

## Scenario-generation implications

For the scenario schema work, Hummingbot suggests adding the following concepts.

### Make execution mode explicit

Avoid conflating “Alpaca historical bars” with “Alpaca paper order routing.”

```rust
pub enum ExecutionMode {
    SimulatedHistorical,
    PaperBroker,
    LiveBroker,
}
```

Use it inside `VenueSettings`:

```rust
pub struct VenueSettings {
    pub venue: VenueKind,
    pub execution_mode: ExecutionMode,
    pub fees: FeeModel,
    pub slippage: SlippageModel,
    pub latency: LatencyModel,
    pub fill_model: FillModel,
    pub trading_rules: Option<TradingRules>,
}
```

### Add fill model

```rust
pub struct FillModel {
    pub market_order: MarketFillModel,
    pub limit_order: LimitFillModel,
    pub partial_fills: PartialFillModel,
    pub volume_constraints: VolumeConstraint,
}
```

### Add trading rules

```rust
pub struct TradingRules {
    pub min_notional: Decimal,
    pub price_tick: Decimal,
    pub qty_step: Decimal,
}
```

### Add close reasons to fills/results

```rust
pub struct RunFill {
    pub order_id: String,
    pub asset: AssetRef,
    pub side: Side,
    pub price: Decimal,
    pub qty: Decimal,
    pub fee: Decimal,
    pub reason: FillReason,
}
```

### Add execution plan shape over time

Instead of only:

```rust
pub struct TraderDecision {
    pub action: Action,
    pub direction: Direction,
    pub size_bps: u32,
    pub stop_loss_pct: f32,
    pub take_profit_pct: f32,
}
```

Eventually support:

```rust
pub struct ExecutionPlan {
    pub entry: EntryPlan,
    pub exit_policy: ExitPolicy,
    pub sizing: SizingPolicy,
    pub time_limit: Option<Duration>,
}
```

This is closer to Hummingbot’s executor model and will make scenario replay more credible.

## Recommended Xvision roadmap additions

### P0 — add before/with scenario work

1. Expand `VenueSettings`
   - add `execution_mode`
   - add `fill_model`
   - add optional `trading_rules`

2. Add `CloseReason` to backtest/eval output
   - `TakeProfit`
   - `StopLoss`
   - `TimeLimit`
   - `TrailingStop`
   - `Manual`
   - `EndOfScenario`
   - `RiskVeto`
   - `Failed`

3. Add minimal `TradingRules`
   - `min_notional`
   - `qty_step`
   - `price_tick`

### P1 — add after scenario table/bar cache

4. Add `OrderLifecycle` + `InFlightOrder`

5. Persist `orders` and `fills`

6. Add `BudgetChecker`

7. Add endpoint-aware `VenueRateLimiter`

### P2 — add when live-paper mirroring starts

8. Add managed executors
   - `PositionExecutor`
   - `OrderExecutor`
   - later `DcaExecutor`
   - later `GridExecutor`
   - maybe `TwapExecutor`

9. Add `MarketDataProvider`

10. Add runtime kill switch / live daemon guard

## Proposed near-term design principle

The highest-leverage Hummingbot lesson is:

> Treat execution as a stateful lifecycle with orders, fills, close reasons, budget constraints, and venue rules — not as a single final order confirmation.

That principle fits Xvision without turning it into a connector-heavy trading bot clone.

## Open design questions for Xvision

1. Should `CloseReason` live in `xvision-core` or `xvision-eval`?
   - If live/paper will use it, prefer `xvision-core`.
   - If only backtest reports need it initially, start in `xvision-eval` and promote later.

2. Should `TradingRules` be part of `Scenario` or resolved from `VenueKind`?
   - For reproducibility, runs should snapshot the rules used.
   - For usability, scenarios can default by venue/asset, then persist the resolved values.

3. Should `OrderLifecycle` be introduced before managed executors?
   - Recommendation: yes. It is useful independently for paper/live parity and better eval traces.

4. Should v1 model partial fills?
   - Recommendation: no. Add the enum/schema shape now, but use `PartialFillModel::Disabled` in v1.

5. Should scenario v1 include order books?
   - Recommendation: no. OHLCV + fill assumptions is enough. Add order-book support only for later paper/live mirror parity or high-frequency/maker strategies.

## Naming note

This directory is named `hummingbird-eval` per request, but the upstream project inspected is **Hummingbot** (`hummingbot/hummingbot`).
