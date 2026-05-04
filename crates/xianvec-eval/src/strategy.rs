//! Strategy trait — async surface so an LLM-backed arm can `.await` an HTTP
//! Intern call inside `decide`. v1.0 was sync (pure function); v1.1 lifts to
//! async to plug the Stage 1 + Stage 2 pipeline in (F3, FOLLOWUPS.md).
//!
//! v1 contract: a strategy maps `MarketSnapshot` → `Option<TraderDecision>`.
//! `None` means "no setup at this bar"; the harness advances time without
//! submitting.
//!
//! `TraderDecision::setup_id` MUST be copied from `snapshot.setup_id` so the
//! harness can pair (setup, decision, fill) records across arms.

use async_trait::async_trait;
use xianvec_core::market::MarketSnapshot;
use xianvec_core::trading::TraderDecision;

#[async_trait]
pub trait Strategy: Send + Sync {
    fn name(&self) -> &'static str;
    async fn decide(&self, snapshot: &MarketSnapshot) -> Option<TraderDecision>;
}
