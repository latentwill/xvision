# Perpetuals eval simulator — leverage, funding, liquidation in backtest

> **For agentic workers:** REQUIRED SUB-SKILL: Use `superpowers:subagent-driven-development` (recommended) or `superpowers:executing-plans` to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.
> **Status:** Follow-up / deferred plan. Not on the v1 critical path. Picked up when an operator wants to backtest a perp-with-leverage strategy without waiting for the Orderly live path to ship.
> **Source:** Conversation 2026-05-11 — "Can Alpaca do perps? Test with leverage? Or build it in eval?" Conclusion: build it in eval, decouple from broker support.

---

## Goal

Add a perpetuals simulator to `xvision-eval` so backtests can include funding payments, leverage, and liquidation. The simulator is a new executor backend; the `Strategy` / `TraderDecision` / `RiskDecision` pipeline above it is unchanged. Strategies that emit a leverage parameter route to the perp sim; strategies that don't keep going through the existing spot simulator.

## Architecture

**Three-tier paper/test story** (locked in by this plan):

| Tier | Surface | What's tested | Status |
|---|---|---|---|
| Backtest perps | **In-eval perp simulator** (this plan) | Strategy logic + funding/leverage/liquidation math | NEW |
| Forward-paper spot/equities | Alpaca paper | Strategy logic + real fills (no leverage, no perps) | v1 (exists) |
| Forward-paper perps | Orderly testnet (`OrderlyExecutor` stub) | Strategy + signed-order roundtrip on a real DEX | v1.5 (stubbed) |
| Live spot/equities | Alpaca live | Real money on spot | deferred |
| Live perps | Orderly mainnet | Real money on perps | deferred (wallet plan) |

**Why eval is the right place for perp simulation, not a broker adapter:**

- The math is deterministic and only needs historical data (funding rates + price). No broker dependency.
- Decouples strategy development from execution-path readiness — operators can iterate on a perp strategy *today* and forward-paper it on Orderly *whenever* that lights up.
- Same code path as spot eval; we extend, not replace.

**The shape of the change:**

```
TraderDecision { side, qty, leverage: Option<f64> }
        │
        ▼
RiskDecision (now includes a leverage check + maint-margin headroom check)
        │
        ▼
Executor backend (chosen per eval-run config):
  ├─ SpotSimulator       — existing
  ├─ PerpSimulator       — NEW (this plan)
  ├─ AlpacaPaperExecutor — existing
  └─ OrderlyExecutor     — existing stub
```

The simulator consumes `OrderRequest`s the same way the spot sim does; the difference is internal accounting (collateral, leverage, funding accrual, liquidation engine).

## Tech stack

- **Rust** (extending `xvision-eval`)
- **`sqlx` + SQLite** for funding-rate cache (already in `xvision-data`)
- **`parquet` via `arrow`** — optional, for bulk funding-rate ingestion if SQLite becomes a bottleneck. Default to SQLite; switch only if benchmarks demand it.
- **`reqwest`** for one-off historical data fetches (Binance, Bybit public REST endpoints — both expose free funding-rate history without auth)

No new top-level dependencies. All transitive crates are already in the workspace.

## Out of scope (intentionally not in this plan)

- **Live Orderly mainnet integration** — separate wallet plan / blockchain track
- **Multi-venue arbitrage strategies** — single-venue perp logic only
- **Options pricing / Greeks** — perp-specific
- **Insurance fund modeling** — use simple liquidation haircut (configurable bps); insurance-fund dynamics are venue-internal and not material for strategy-level backtests
- **Cross-exchange basis trades** — would need synchronized data across venues
- **Slippage models beyond constant bps** — order-book impact modeling deferred; default is taker-fee + bps haircut

## File structure

```
crates/xvision-eval/src/
├── perp_sim/                                   # NEW
│   ├── mod.rs                                  # re-exports + venue enum
│   ├── venue.rs                                # PerpVenueConfig
│   ├── funding.rs                              # FundingProvider trait + SQLite impl
│   ├── account.rs                              # PerpPositionAccount, equity, margin headroom
│   ├── liquidation.rs                          # maint-margin schedule, liquidation engine
│   └── simulator.rs                            # main loop — consumes OrderRequests, emits Fills
│
├── executor.rs                                 # MODIFY — add PerpSim arm to Executor enum
└── config.rs                                   # MODIFY — add venue: Venue field to EvalRunConfig

crates/xvision-data/src/migrations/
└── 20260511000010_funding_rates.sql            # NEW — funding_rates table

crates/xvision-engine/src/
├── api/trader.rs                               # MODIFY — TraderDecision.leverage: Option<f64>
└── api/risk.rs                                 # MODIFY — risk gate consumes leverage, returns maint-margin verdict

data/funding/                                    # NEW (gitignored) — local CSV/parquet cache for bulk pulls

scripts/
└── fetch_funding_history.py                     # NEW — one-off Binance/Bybit pull, writes to data/funding/
```

## Decision: which exchange's funding semantics do we model?

Default to **Binance perps** as the reference venue:

- Funding cadence: 8h (00:00, 08:00, 16:00 UTC)
- Funding formula: `(premium index + clamp(interest rate − premium index, ±0.05%))`, capped per-symbol
- Maintenance margin: tier-based (free at small notional, scales up to ~50% at multi-million USD positions)
- Taker fee: ~5 bps (post-VIP discount); maker −2 bps rebate

Orderly's semantics differ slightly (different funding cap, different maint-margin tiers). When the Orderly live path lights up, add `PerpVenueConfig::orderly_v2()` alongside `binance()`; same simulator code consumes both. **One simulator, N venue configs.**

---

## Task 1 — `funding_rates` table + data ingest

**Files:**
- Create: `crates/xvision-data/src/migrations/20260511000010_funding_rates.sql`
- Create: `scripts/fetch_funding_history.py`

### Step 1: Failing test for the SQL schema

Create `crates/xvision-data/tests/funding_rates_schema.rs`:

```rust
#[sqlx::test]
async fn funding_rates_round_trip(pool: SqlitePool) {
    sqlx::query("INSERT INTO funding_rates (venue, symbol, ts_ms, rate_bps) VALUES (?, ?, ?, ?)")
        .bind("binance").bind("BTCUSDT").bind(1_700_000_000_000i64).bind(1.5_f64)
        .execute(&pool).await.unwrap();

    let r: (String, String, i64, f64) = sqlx::query_as(
        "SELECT venue, symbol, ts_ms, rate_bps FROM funding_rates WHERE symbol = 'BTCUSDT'"
    ).fetch_one(&pool).await.unwrap();
    assert_eq!(r.0, "binance");
    assert!((r.3 - 1.5).abs() < f64::EPSILON);
}
```

### Step 2: Migration

```sql
CREATE TABLE funding_rates (
    venue       TEXT NOT NULL,
    symbol      TEXT NOT NULL,
    ts_ms       INTEGER NOT NULL,
    rate_bps    REAL NOT NULL,
    PRIMARY KEY (venue, symbol, ts_ms)
) WITHOUT ROWID;

CREATE INDEX idx_funding_rates_symbol_ts ON funding_rates(symbol, ts_ms);
```

### Step 3: Bulk-pull script

`scripts/fetch_funding_history.py` (Binance public REST, no auth):

```python
# GET https://fapi.binance.com/fapi/v1/fundingRate?symbol=BTCUSDT&limit=1000
# Paginated by startTime; writes funding_rates rows to data/funding/binance_<symbol>.csv
# CLI: python scripts/fetch_funding_history.py --venue binance --symbol BTCUSDT --since 2023-01-01
```

Operator then loads into the DB:

```sh
sqlite3 ~/.xvn/xvn.db <<EOF
.mode csv
.import data/funding/binance_BTCUSDT.csv funding_rates
EOF
```

### Step 4: Commit

```sh
git add crates/xvision-data/src/migrations/20260511000010_funding_rates.sql \
        scripts/fetch_funding_history.py
git commit -m "feat(eval): funding_rates table + Binance bulk-pull script"
```

---

## Task 2 — `PerpVenueConfig` and `FundingProvider` trait

**Files:**
- Create: `crates/xvision-eval/src/perp_sim/venue.rs`
- Create: `crates/xvision-eval/src/perp_sim/funding.rs`

### Step 1: Failing test

```rust
#[tokio::test]
async fn binance_venue_loads_funding_at_8h_boundary() {
    let pool = test_pool_with_funding_fixtures().await;
    let venue = PerpVenueConfig::binance();
    let provider = SqliteFundingProvider::new(pool, venue.clone());

    // 2024-01-01T08:00:00 UTC = ts_ms 1704096000000
    let rate = provider.rate_at("BTCUSDT", 1_704_096_000_000).await.unwrap();
    assert!(rate.is_some());

    // Mid-window query returns the previous boundary's rate
    let rate_mid = provider.rate_at("BTCUSDT", 1_704_096_000_000 + 4 * 3600 * 1000).await.unwrap();
    assert_eq!(rate, rate_mid);
}
```

### Step 2: Implement

```rust
// venue.rs
pub struct PerpVenueConfig {
    pub name: &'static str,
    pub funding_cadence_hours: u32,             // 8 for Binance, 1 for Bybit, …
    pub max_leverage: f64,                      // per-symbol caps live in a separate table
    pub taker_fee_bps: f64,
    pub maker_fee_bps: f64,                     // negative = rebate
    pub liquidation_fee_bps: f64,               // haircut applied to remaining collateral on liq
    pub maint_margin_schedule: MaintMarginSchedule,
}

impl PerpVenueConfig {
    pub fn binance() -> Self { /* … */ }
    // pub fn orderly_v2() -> Self { … }   // add when Orderly testnet lights up
}

// funding.rs
#[async_trait]
pub trait FundingProvider: Send + Sync {
    /// Return the most recent funding rate at or before `ts_ms`, in bps.
    /// `None` if no rate exists for this symbol in the window.
    async fn rate_at(&self, symbol: &str, ts_ms: i64) -> anyhow::Result<Option<f64>>;
}

pub struct SqliteFundingProvider { pool: SqlitePool, venue: PerpVenueConfig }
```

### Step 3: Run + commit

```sh
cargo test -p xvision-eval perp_sim::funding
git add crates/xvision-eval/src/perp_sim/{venue,funding}.rs
git commit -m "feat(eval): PerpVenueConfig + SqliteFundingProvider"
```

---

## Task 3 — `PerpPositionAccount` (collateral, equity, unrealized PnL, accumulated funding)

**Files:**
- Create: `crates/xvision-eval/src/perp_sim/account.rs`

### Step 1: Failing tests

```rust
#[test]
fn opening_position_locks_collateral() {
    let mut acc = PerpPositionAccount::new(10_000.0);  // 10k USDC starting collateral
    acc.open("BTCUSDT", Side::Long, qty=1.0, mark=50_000.0, leverage=10.0).unwrap();
    assert!((acc.locked_collateral() - 5_000.0).abs() < 0.01);  // 1 BTC * 50k / 10x
    assert!((acc.free_collateral() - 5_000.0).abs() < 0.01);
}

#[test]
fn funding_payment_long_pays_when_rate_positive() {
    let mut acc = PerpPositionAccount::new(10_000.0);
    acc.open("BTCUSDT", Side::Long, 1.0, 50_000.0, 10.0).unwrap();
    acc.apply_funding("BTCUSDT", rate_bps=1.0, mark=50_000.0);
    // 1 BTC long × 50k notional × 0.01% = 5 USDC paid by long
    assert!((acc.cumulative_funding_usdc() - (-5.0)).abs() < 0.01);
}

#[test]
fn unrealized_pnl_tracks_mark_move() {
    let mut acc = PerpPositionAccount::new(10_000.0);
    acc.open("BTCUSDT", Side::Long, 1.0, 50_000.0, 10.0).unwrap();
    let pnl = acc.unrealized_pnl("BTCUSDT", mark=51_000.0);
    assert!((pnl - 1_000.0).abs() < 0.01);
}

#[test]
fn equity_includes_unrealized_minus_funding() {
    let mut acc = PerpPositionAccount::new(10_000.0);
    acc.open("BTCUSDT", Side::Long, 1.0, 50_000.0, 10.0).unwrap();
    acc.apply_funding("BTCUSDT", 1.0, 50_000.0);
    let eq = acc.equity(&hashmap!{"BTCUSDT".into() => 51_000.0});
    assert!((eq - (10_000.0 + 1_000.0 - 5.0)).abs() < 0.01);
}
```

### Step 2: Implement

The account holds: starting collateral, per-symbol positions `{qty, entry_price, side}`, cumulative funding USDC, cumulative fees USDC. Equity at any mark is `starting_collateral + Σ realized_pnl + Σ unrealized_pnl + cum_funding − cum_fees`.

### Step 3: Run + commit

---

## Task 4 — Liquidation engine

**Files:**
- Create: `crates/xvision-eval/src/perp_sim/liquidation.rs`

### Step 1: Failing tests

```rust
#[test]
fn liquidates_when_equity_below_maint_margin() {
    let mut acc = PerpPositionAccount::new(10_000.0);
    acc.open("BTCUSDT", Side::Long, 1.0, 50_000.0, 10.0).unwrap();

    // Maint margin for BTC at this notional ≈ 0.5%, so liquidation occurs near 45_250
    let liq_price = liquidation_price(&acc, "BTCUSDT", &PerpVenueConfig::binance());
    assert!((liq_price - 45_250.0).abs() < 50.0);
}

#[test]
fn liquidate_zeros_position_and_haircuts_collateral() {
    let mut acc = PerpPositionAccount::new(10_000.0);
    acc.open("BTCUSDT", Side::Long, 1.0, 50_000.0, 10.0).unwrap();

    let result = liquidate(&mut acc, "BTCUSDT", mark=45_000.0,
                           venue=&PerpVenueConfig::binance()).unwrap();
    assert_eq!(acc.position_qty("BTCUSDT"), 0.0);
    assert_eq!(result.kind, LiquidationKind::Forced);
    assert!(acc.equity_at_close() < 5_000.0);  // collateral haircut applied
}
```

### Step 2: Implement

Liquidation engine: at each tick, compute equity at current mark per symbol. If equity < maint_margin_required, force-close at mark with liquidation_fee_bps haircut, zero position, emit `LiquidationEvent`.

### Step 3: Run + commit

---

## Task 5 — `PerpSimulator` main loop

**Files:**
- Create: `crates/xvision-eval/src/perp_sim/simulator.rs`
- Modify: `crates/xvision-eval/src/executor.rs`

### Step 1: Failing integration test

```rust
#[tokio::test]
async fn long_btc_funding_drag_over_30_days() {
    let pool = test_pool_with_funding_2024().await;
    let mut sim = PerpSimulator::new(
        10_000.0,
        PerpVenueConfig::binance(),
        Box::new(SqliteFundingProvider::new(pool, PerpVenueConfig::binance())),
    );

    // Day 0: open 1 BTC long, 10x leverage
    sim.execute(OrderRequest {
        symbol: "BTCUSDT", side: Side::Long, qty: 1.0, leverage: Some(10.0)
    }, ts_ms=DAY_0, mark=50_000.0).await.unwrap();

    // Step through 30 days of marks + funding ticks
    for day in 1..=30 {
        sim.tick(ts_ms=DAY_0 + day * 86400_000, marks=&day_marks[day]).await.unwrap();
    }

    let report = sim.report();
    assert!(report.cumulative_funding_usdc < 0.0);  // long paid during a positive-funding regime
    assert!(report.equity_final != 10_000.0);       // moved either way
}
```

### Step 2: Implement

`PerpSimulator` owns: `PerpPositionAccount`, `PerpVenueConfig`, `FundingProvider`, event log. Public methods:

- `execute(order, ts_ms, mark) -> Result<FillEvent>`
- `tick(ts_ms, marks) -> Result<Vec<TickEvent>>` — applies funding at boundaries, runs liquidation check
- `report() -> PerpRunReport` — funding total, fees total, liquidations count, equity series

### Step 3: Wire into `Executor` enum

```rust
pub enum Executor {
    SpotSim(SpotSimulator),
    PerpSim(PerpSimulator),                     // NEW
    AlpacaPaper(AlpacaPaperExecutor),
    OrderlyTestnet(OrderlyExecutor),            // existing stub
}
```

Dispatch by `EvalRunConfig.venue`.

### Step 4: Run + commit

---

## Task 6 — Plumb `leverage` through `TraderDecision` and risk gate

**Files:**
- Modify: `crates/xvision-engine/src/api/trader.rs` — `TraderDecision { side, qty, leverage: Option<f64> }`
- Modify: `crates/xvision-engine/src/api/risk.rs` — risk gate gains a `max_leverage` knob; rejects orders that exceed it; gains a maint-margin-headroom check
- Modify: existing baseline strategies in `crates/xvision-eval/src/baselines/` — emit `leverage: None` (no behavior change for spot strategies)

### Step 1: Failing test — risk gate rejects over-leveraged order

```rust
#[test]
fn risk_gate_vetoes_leverage_over_cap() {
    let gate = RiskGate::new(RiskConfig { max_leverage: 5.0, ..Default::default() });
    let decision = TraderDecision { side: Long, qty: 1.0, leverage: Some(10.0) };
    let verdict = gate.evaluate(&decision, &account_snapshot());
    assert!(matches!(verdict, RiskDecision::Vetoed { reason } if reason.contains("leverage")));
}
```

### Step 2: Implement leverage cap + maint-margin check

Risk gate now reads the account's projected maint-margin headroom *after* the proposed fill and vetoes if headroom < safety threshold (configurable, default 2× maint margin).

### Step 3: Backward-compat sweep

All existing strategies use `leverage: None` by default. Spot venues ignore the field. No test regressions.

### Step 4: Run + commit

---

## Task 7 — Eval-run config and reporting

**Files:**
- Modify: `crates/xvision-eval/src/config.rs` — `EvalRunConfig.venue: Venue` enum
- Modify: `crates/xvision-eval/src/result.rs` — `BacktestResult` gains optional `perp_metrics` block

### Step 1: Failing test — perp run produces perp metrics

```rust
#[tokio::test]
async fn perp_run_emits_funding_and_liquidation_metrics() {
    let cfg = EvalRunConfig {
        venue: Venue::PerpSim { config: PerpVenueConfig::binance() },
        strategy: simple_long_only_strategy(),
        ..test_config()
    };
    let result = run_eval(cfg).await.unwrap();
    let perp = result.perp_metrics.expect("perp metrics present");
    assert!(perp.cumulative_funding_usdc != 0.0);
    // Liquidation count may be 0 in a tame window; just check the field exists
    let _ = perp.liquidations_count;
}
```

### Step 2: Implement reporting

```rust
pub struct PerpMetrics {
    pub cumulative_funding_usdc: f64,
    pub cumulative_fees_usdc: f64,
    pub liquidations_count: u32,
    pub max_leverage_observed: f64,
    pub time_at_high_leverage_pct: f64,  // % of session above 5x
    pub min_margin_headroom: f64,
}
```

### Step 3: Update dashboard `/eval/runs/:id` to surface perp metrics

Add a "Perpetuals" card to the run detail page when `perp_metrics` is present. Show: cumulative funding, fees, liquidations, max leverage. Out of scope for this plan to wire visually — file a frontend follow-up in FOLLOWUPS.md once the engine side ships.

### Step 4: Run + commit

---

## Task 8 — Cross-venue comparison smoke test

**Files:**
- Create: `crates/xvision-eval/tests/perp_vs_spot_compare.rs`

The same strategy run on spot-sim vs perp-sim should produce different equity curves (perp adds funding + fees + leverage). Smoke test that the compare endpoint handles mixed-venue runs.

### Step 1: Test

```rust
#[tokio::test]
async fn ab_compare_spot_vs_perp_runs_to_completion() {
    let spot = run_eval(spot_config()).await.unwrap();
    let perp = run_eval(perp_config()).await.unwrap();

    assert_eq!(spot.cycles_evaluated, perp.cycles_evaluated);
    assert!(spot.perp_metrics.is_none());
    assert!(perp.perp_metrics.is_some());

    let compare = compare_runs(&[spot.run_id, perp.run_id]).await.unwrap();
    assert_eq!(compare.runs.len(), 2);
}
```

### Step 2: Commit

---

## Self-review

**Spec coverage** — eight tasks, each with TDD failing test + implementation + commit:

| Task | Touches | What it ships |
|---|---|---|
| 1 | `xvision-data` migration + Python script | `funding_rates` table, Binance bulk-pull |
| 2 | `xvision-eval::perp_sim::{venue,funding}` | `PerpVenueConfig`, `FundingProvider` |
| 3 | `xvision-eval::perp_sim::account` | `PerpPositionAccount` math |
| 4 | `xvision-eval::perp_sim::liquidation` | Liquidation engine |
| 5 | `xvision-eval::perp_sim::simulator` + `executor.rs` | Main loop, wires into `Executor` enum |
| 6 | `xvision-engine::api::{trader,risk}` | `leverage` on `TraderDecision`, risk gate cap |
| 7 | `xvision-eval::{config,result}` | `Venue` enum on run config, `PerpMetrics` on result |
| 8 | `xvision-eval/tests/perp_vs_spot_compare.rs` | End-to-end smoke |

**Backward compatibility** — every existing spot strategy continues to work unchanged:
- `TraderDecision.leverage` defaults to `None`
- `EvalRunConfig.venue` defaults to `Venue::SpotSim`
- `BacktestResult.perp_metrics` is `Option<PerpMetrics>`
- Risk-gate leverage cap is `Option<f64>` — `None` means "spot semantics, no cap"

**Where this connects to Orderly live** (when the wallet plan lights up):
- `PerpVenueConfig::orderly_v2()` is added as a peer of `binance()` — same simulator code consumes it
- `OrderlyExecutor` (existing stub at `crates/xvision-execution/src/orderly.rs`) becomes the live target; the perp sim becomes the backtest target. Same `Strategy`, same `RiskDecision`, two executors.
- An eval run that succeeded against `Venue::PerpSim { orderly_v2 }` can forward-paper to Orderly testnet with no strategy changes.

**Type/name consistency** — uses `cycle_id` (post-rename), `agent_id` (post-rename), `StrategyBundle` (immutable config), `Algorithm`/`TraderArm` for producers. No `setup_id` or `strategy_id` (per `CLAUDE.md` terminology table).

**Dependencies between tasks:**
- 1 → 2 (provider needs the table)
- 2 → 3 (account uses provider for funding)
- 3 → 4 (liquidation reads account state)
- 3, 4 → 5 (simulator composes both)
- 5 → 6 (executor enum needs the type; risk gate consumes the leverage param the simulator respects)
- 6 → 7 (config + result need the trader/risk surface stable)
- 7 → 8 (smoke test uses the new config + result fields)

Recommended execution order: 1 → 2 → 3 → 4 → 5 → 6 → 7 → 8 (linear; small enough to parallelize within tasks but not across).

**Estimated effort:** ~5–7 days for a single engineer. Funding ingest + venue config (1 day), account + liquidation math (2 days), simulator + executor wiring (1 day), risk gate + trader surface (1 day), config/result/smoke (1 day), polish + dashboard handoff (1 day).

**What this plan does NOT solve** (gracefully deferred):

- Order-book impact / slippage beyond constant bps — open question whether to model an L2 book replay or stay with the bps haircut; defer until a strategy actually needs L2 fidelity
- Cross-margin contagion across multiple symbols — the account is single-collateral-pool today, but cross-margin failure modes (one symbol drawdown triggers another's liq) need a dedicated test fixture
- Insurance-fund + auto-deleveraging dynamics — venue-internal, immaterial for strategy-level backtests; revisit if a strategy specifically targets liq-cascade alpha
- Multi-venue strategies (basis trades) — requires synchronized cross-venue data; out of scope for this plan
- Funding-rate prediction models — provider returns historical rates; strategies that want to *forecast* funding are responsible for their own model

**Open questions for the operator** (flag for sign-off before execution, not before this plan lands):

1. Which symbols seed the funding table at task 1? Default proposal: `BTCUSDT`, `ETHUSDT`, `SOLUSDT` on Binance, 2 years of history. Expand on demand.
2. What's the default safety threshold on the risk gate's maint-margin headroom check? Proposal: 2× maint margin (i.e., refuse new orders if projected headroom < 200% of maintenance). Conservative; operator can lower per-strategy.
3. Should perp metrics on the run-detail page show the funding-PnL contribution as a separate equity-curve series, or roll it into total equity? Proposal: separate series (overlay-able), so the funding drag is visually attributable.
