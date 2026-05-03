//! xianvec-execution — Stage 3 executors.
//!
//! Phase 6.1 ships the `Executor` trait + `ExecutionReceipt` / `ExecutorError`.
//! Phase 6.2 wires `AlpacaExecutor`. Phase 6.3 (sequenced post Phase 8 per
//! `v1-build-steps.md`) wires `OrderlyExecutor`. Phase 6.4's backtest sim
//! lives in `xianvec-eval` and implements this same trait.

pub mod alpaca;
pub mod executor;

pub use alpaca::AlpacaExecutor;
pub use executor::{ExecutionReceipt, Executor, ExecutorError};
