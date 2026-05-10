//! xvision-execution — Stage 3 executors.
//!
//! Phase 6.1 ships the `Executor` trait + `ExecutionReceipt` / `ExecutorError`.
//! Phase 6.2 wires `AlpacaExecutor`. Phase 6.3 (sequenced post Phase 8 per
//! `v1-build-steps.md`) wires `OrderlyExecutor`. Phase 6.4's backtest sim
//! lives in `xvision-eval` and implements this same trait.

pub mod alpaca;
pub mod broker_surface;
pub mod executor;
pub mod orderly;

pub use alpaca::AlpacaExecutor;
pub use broker_surface::{
    AlpacaLiveSurface, AlpacaPaperSurface, BrokerKind, BrokerSurface, MockBrokerSurface,
    OrderConfirmation, OrderRequest as BrokerOrderRequest, OrderlyLiveSurface, Side,
};
pub use executor::{ExecutionReceipt, Executor, ExecutorError};
pub use orderly::OrderlyExecutor;
