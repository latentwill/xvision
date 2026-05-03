//! Strategy trait — common surface for Phase 7 baselines and the Phase 9
//! vector-arm trader. The Phase 8 BacktestRunner drives any `Strategy`
//! through historical OHLCV via the `BacktestExecutor`.
//!
//! v1 contract: a strategy is a pure function from `MarketSnapshot` →
//! `Option<TraderDecision>`. `None` means "no setup at this bar"
//! (the harness just advances time without submitting).
//!
//! `TraderDecision::setup_id` MUST be copied from `snapshot.setup_id`
//! so the harness can pair (setup, decision, fill) records across arms.

use xianvec_core::trading::TraderDecision;
use xianvec_core::market::MarketSnapshot;

pub trait Strategy: Send + Sync {
    fn name(&self) -> &'static str;
    fn decide(&self, snapshot: &MarketSnapshot) -> Option<TraderDecision>;
}
