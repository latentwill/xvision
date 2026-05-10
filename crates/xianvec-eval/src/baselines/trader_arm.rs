//! TraderArm — the `Algorithm` adapter that runs the real Stage 1 Intern +
//! Stage 2 Trader pipeline against a `MarketSnapshot`.
//!
//! Post-CV-extraction (ADR 0011) there is no longer a four-arm steering
//! split. TraderArm is a single LLM-without-steering variant that calls a
//! Trader HTTP backend mirroring the Intern HTTP backend.
//!
//! ## Briefing pairing (Tier 1 fix #1)
//! All arms (this one + the classical baselines) share a `BriefingCache`;
//! the cache key is `(cycle_id, intern_provider, intern_model)`. Decision
//! divergence reflects strategy difference rather than Intern non-determinism.

use std::sync::Arc;

use async_trait::async_trait;

use xianvec_core::market::MarketSnapshot;
use xianvec_core::trading::{PortfolioState, TraderDecision};
use xianvec_intern::cache::CacheKey;
use xianvec_intern::{BriefingCache, InternBackend};
use xianvec_trader::{TraderBackend, TraderParams};

use crate::algorithm::Algorithm;

/// Closure that builds the portfolio snapshot the Trader sees on each
/// decision. The harness owns the executor's portfolio; v1 hands the
/// adapter a closure rather than coupling to a specific executor surface.
pub type PortfolioProvider = Arc<dyn Fn() -> PortfolioState + Send + Sync>;

/// The `Algorithm`-implementing adapter.
pub struct TraderArm {
    arm_name: &'static str,
    intern: Arc<dyn InternBackend>,
    intern_provider: String,
    intern_model: String,
    cache: Arc<BriefingCache>,
    /// Trader HTTP backend. `&dyn TraderBackend` is `Send + Sync` so no
    /// per-request locking is needed.
    trader: Arc<dyn TraderBackend>,
    trader_params: TraderParams,
    portfolio_provider: PortfolioProvider,
}

impl TraderArm {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        arm_name: &'static str,
        intern: Arc<dyn InternBackend>,
        intern_provider: impl Into<String>,
        intern_model: impl Into<String>,
        cache: Arc<BriefingCache>,
        trader: Arc<dyn TraderBackend>,
        trader_params: TraderParams,
        portfolio_provider: PortfolioProvider,
    ) -> Self {
        Self {
            arm_name,
            intern,
            intern_provider: intern_provider.into(),
            intern_model: intern_model.into(),
            cache,
            trader,
            trader_params,
            portfolio_provider,
        }
    }
}

#[async_trait]
impl Algorithm for TraderArm {
    fn name(&self) -> &'static str {
        self.arm_name
    }

    async fn decide(&self, snapshot: &MarketSnapshot) -> Option<TraderDecision> {
        use xianvec_intern::prompt::{build_intern_prompt, PromptOpts};
        use xianvec_trader::run_trader;

        // 1. Stage 1: cached or fresh briefing (Tier 1 fix #1).
        let key = CacheKey {
            cycle_id: snapshot.cycle_id,
            provider: self.intern_provider.clone(),
            model: self.intern_model.clone(),
        };
        let briefing = if let Some(b) = self.cache.get(&key) {
            b
        } else {
            let prompt = build_intern_prompt(snapshot, &[], &PromptOpts::default());
            match self
                .intern
                .brief(
                    &prompt,
                    snapshot.cycle_id,
                    snapshot.asset,
                    snapshot.regime,
                    snapshot.horizon_hours,
                )
                .await
            {
                Ok(b) => {
                    self.cache.insert(key, b.clone());
                    b
                }
                Err(e) => {
                    tracing::warn!(
                        target: "trader_arm",
                        arm = self.arm_name,
                        error = %e,
                        "intern brief failed; arm emitting None"
                    );
                    return None;
                }
            }
        };

        // 2. Stage 2: Trader call.
        let portfolio = (self.portfolio_provider)();
        match run_trader(self.trader.as_ref(), &briefing, &portfolio, &self.trader_params).await {
            Ok(d) => Some(d),
            Err(e) => {
                tracing::warn!(
                    target: "trader_arm",
                    arm = self.arm_name,
                    error = %e,
                    "trader failed; arm emitting None"
                );
                None
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{TimeZone, Utc};
    use uuid::Uuid;

    use xianvec_core::market::{IndicatorPanel, OnchainPanel, Ohlcv};
    use xianvec_core::trading::{
        AssetSymbol, EvidenceTag, InternBriefing, Regime,
    };
    use xianvec_intern::backend::InternError;

    /// Mock Intern that returns a fixed briefing — exercises the cache key
    /// shape + the async InternBackend trait without any HTTP traffic.
    struct MockIntern;

    #[async_trait]
    impl InternBackend for MockIntern {
        async fn brief(
            &self,
            _prompt: &str,
            cycle_id: Uuid,
            asset: AssetSymbol,
            regime: Regime,
            horizon_hours: u32,
        ) -> Result<InternBriefing, InternError> {
            Ok(InternBriefing {
                cycle_id,
                asset,
                bull_case: "Funding rate compressed; smart money accumulating spot.".into(),
                bear_case: "Realized vol expanding; long-leverage approaching prior squeeze.".into(),
                flat_case: "Range-bound between SMA20 and SMA50; await break.".into(),
                evidence_long: vec![EvidenceTag::Onchain("smart_money_inflow".into())],
                evidence_short: vec![EvidenceTag::Technical("rsi_overbought".into())],
                evidence_flat: vec![],
                regime,
                signal_quality: 0.6,
                horizon_hours,
                created_at: Utc::now(),
            })
        }
    }

    fn mk_snapshot() -> MarketSnapshot {
        MarketSnapshot {
            cycle_id: Uuid::new_v4(),
            asset: AssetSymbol::Btc,
            timestamp: Utc.timestamp_opt(1_700_000_000, 0).single().unwrap(),
            price: 50_000.0,
            volume_24h: None,
            recent_bars: vec![Ohlcv {
                timestamp: Utc.timestamp_opt(1_700_000_000, 0).single().unwrap(),
                open: 50_000.0,
                high: 50_500.0,
                low: 49_500.0,
                close: 50_000.0,
                volume: 10_000.0,
            }],
            indicators: IndicatorPanel::default(),
            onchain: OnchainPanel::default(),
            regime: Regime::Bull,
            horizon_hours: 24,
        }
    }

    /// Smoke: drives the cache + Intern directly to verify the contract.
    /// The full TraderArm.decide path is exercised by the workspace test
    /// once a real TraderBackend is wired; the pairing-cache contract itself
    /// only depends on the cache key + the Intern's async surface.
    #[tokio::test]
    async fn cache_key_pairs_arms_for_same_cycle_id() {
        let cache = Arc::new(BriefingCache::new());
        let snap = mk_snapshot();

        let key = CacheKey {
            cycle_id: snap.cycle_id,
            provider: "mock".into(),
            model: "mock".into(),
        };
        assert!(cache.get(&key).is_none());

        let intern = MockIntern;
        let briefing = intern
            .brief(
                "p",
                snap.cycle_id,
                snap.asset,
                snap.regime,
                snap.horizon_hours,
            )
            .await
            .expect("mock intern always succeeds");
        cache.insert(key.clone(), briefing.clone());

        // Same cycle_id → same briefing on lookup (paired arms read it).
        let again = cache.get(&key).expect("cache hit");
        assert_eq!(again.cycle_id, briefing.cycle_id);
        assert_eq!(again.bull_case, briefing.bull_case);
    }
}
