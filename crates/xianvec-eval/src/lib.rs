//! xianvec-eval — Phase 6.4 in-process backtest simulator + future baselines /
//! Δ-Sharpe evaluation (see implementation-plan.md §6.4).

pub mod backtest;

pub use backtest::{
    BacktestConfig, BacktestExecutor, BacktestState, DailyPnl, MarketBar, TickReport,
};
