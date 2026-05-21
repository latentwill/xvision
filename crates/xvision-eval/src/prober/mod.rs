//! Lookahead-bias prober — detects indicator-based leakage in `Algorithm`
//! implementations (research doc §3.5).
//!
//! ## Two-pass strategy
//!
//! * **Pass 1:** full backtest over all bars, recording each signal-firing bar's
//!   decision.
//! * **Pass 2:** for each signal-firing bar `t`, re-run the algorithm with only
//!   the bars `[..=t-1]` available in `snapshot.recent_bars`, and assert the
//!   decision at `t` is identical.
//!
//! If Pass 1 and Pass 2 disagree on the action taken at bar `t`, the prober
//! emits a `lookahead_suspected` finding.  This catches ~90% of indicator-
//! based leakage without requiring source-level inspection.
//!
//! ## Limitations (out of scope for v1)
//! * Cross-asset leakage (BTC future used to decide ETH now).
//! * Regime-label leakage (regime computed with future bars).
//! * Prompt-encoded leakage in LLM strategies.

pub mod lookahead;

pub use lookahead::{LookaheadFinding, LookaheadProber, ProberConfig};
