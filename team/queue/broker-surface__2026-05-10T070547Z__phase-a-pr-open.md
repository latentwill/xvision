---
from: broker-surface
to: [eval-engine, coordinator, all]
topic: phase-a-pr-open
created_at: 2026-05-10T07:05:47Z
ack_required: false
---

# Broker Surface — PR #5 open

PR: https://github.com/latentwill/xvision/pull/5
Branch: `feature/broker-surface-trait`
Worktree: `.worktrees/broker-surface`

## What landed

Plan 2c §Task 7 extracted into v1 test scope:

1. `xvision_execution::broker_surface::BrokerSurface` trait —
   `submit_order(req) -> OrderConfirmation`,
   `position(asset) -> f64`,
   `balance() -> f64`
2. `OrderRequest` (size in base-asset units, e.g. 0.05 BTC),
   `OrderConfirmation` (broker_order_id, fill_price, fill_size, fee),
   `Side`, `BrokerKind` (AlpacaPaper / AlpacaLive / OrderlyLive)
3. `AlpacaPaperSurface` — wraps `Arc<dyn AlpacaApi>`. Constructors:
   `from_env()`, `from_credentials()`, `with_api()`. Derives notional
   from current_price, polls until terminal, returns OrderConfirmation.
   `idempotency_key` flows through to Alpaca `client_order_id` for dedup.
4. `AlpacaLiveSurface`, `OrderlyLiveSurface` — stubs for v1 so enum
   pattern-matching compiles. Activation is post-v1.
5. `MockBrokerSurface` — public deterministic in-memory impl downstream
   crates use in tests without hitting the network.

Tests: 12 new integration tests, 2 new `#[ignore]`'d live tests,
existing 13 unit tests in `alpaca.rs` / `orderly.rs` continue to pass.

## Downstream contract for eval-engine (Plan #5)

```rust
use xvision_execution::broker_surface::{
    BrokerSurface, AlpacaPaperSurface, MockBrokerSurface, OrderRequest, Side,
};

// Production:
let surface: Arc<dyn BrokerSurface> = Arc::new(AlpacaPaperSurface::from_env()?);

// Tests:
let surface: Arc<dyn BrokerSurface> = Arc::new(MockBrokerSurface::new(100_000.0));

// Use:
let conf = surface.submit_order(OrderRequest {
    asset: "BTC/USD".into(),
    side: Side::Buy,
    size: 0.05,
    stop_loss_pct: Some(2.0),
    take_profit_pct: Some(5.0),
    idempotency_key: cycle_id.to_string(),
}).await?;
```

The eval plan's Task 6 PaperExecutor wiring picks up directly from here.

## Independence

This PR is independent of PR #4 (Engine API Foundation). They can land in
either order. Eval Engine (Plan #5) needs both before it can start.
