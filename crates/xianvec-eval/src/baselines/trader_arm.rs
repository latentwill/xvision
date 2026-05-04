//! TraderArm — the `Strategy` adapter that runs the real Stage 1 Intern +
//! Stage 2 Trader pipeline against a `MarketSnapshot`. Phase 9.2's four
//! experimental arms (off, on, random, orthogonal) all share this adapter,
//! differing only in `VectorConfig`.
//!
//! ## v1 status
//! - Only `VectorConfig::Off` is end-to-end tested.
//! - `On / Random / Orthogonal` compile + load but degrade with a
//!   `tracing::warn` to vectors-off behaviour until F1 (spike directional
//!   match through candle) and F2 (production vector extraction) land.
//!
//! ## Briefing pairing (Tier 1 fix #1)
//! All arms share a `BriefingCache`; the cache key is
//! `(setup_id, intern_provider, intern_model)`. Vector arms differ only in
//! Stage 2 (which vector is loaded into Qwen3Engine), never in Stage 1, so
//! decision divergence reflects vector influence rather than Intern
//! non-determinism.

use std::path::PathBuf;
use std::sync::Arc;

use async_trait::async_trait;
use tokio::sync::Mutex;

use xianvec_core::market::MarketSnapshot;
use xianvec_core::trading::{DispositionAxis, PortfolioState, TraderDecision};
use xianvec_core::Manifest;
use xianvec_inference::engine::Qwen3Engine;
use xianvec_intern::cache::CacheKey;
use xianvec_intern::{BriefingCache, InternBackend};
use xianvec_trader::TraderParams;

use crate::strategy::Strategy;

/// Which vector (if any) the arm loads into `Qwen3Engine` before running the
/// Trader. `Off` is the no-vector control; the other three are experimental
/// arms whose validity rides on F1 / F2.
#[derive(Debug, Clone)]
pub enum VectorConfig {
    Off,
    On {
        manifest: Manifest,
        npz_path: PathBuf,
        alpha: f32,
    },
    Random {
        seed: u64,
        layer: u16,
        hidden_dim: usize,
        alpha: f32,
    },
    Orthogonal {
        axis: DispositionAxis,
        seed: u64,
        npz_path: PathBuf,
        alpha: f32,
    },
}

impl VectorConfig {
    pub fn label(&self) -> &'static str {
        match self {
            VectorConfig::Off => "off",
            VectorConfig::On { .. } => "on",
            VectorConfig::Random { .. } => "random",
            VectorConfig::Orthogonal { .. } => "orthogonal",
        }
    }
}

/// Closure that builds the portfolio snapshot the Trader sees on each
/// decision. The harness owns the executor's portfolio; v1 hands the
/// adapter a closure rather than coupling to a specific executor surface.
pub type PortfolioProvider = Arc<dyn Fn() -> PortfolioState + Send + Sync>;

/// The `Strategy`-implementing adapter.
pub struct TraderArm {
    arm_name: &'static str,
    intern: Arc<dyn InternBackend>,
    intern_provider: String,
    intern_model: String,
    cache: Arc<BriefingCache>,
    /// Trader engine guarded by a Mutex — `Qwen3Engine::generate` is
    /// `&mut self`. The harness already serialises arms inside its
    /// decision loop, so contention is per-snapshot and bounded.
    engine: Arc<Mutex<Qwen3Engine>>,
    trader_params: TraderParams,
    vector_config: VectorConfig,
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
        engine: Arc<Mutex<Qwen3Engine>>,
        trader_params: TraderParams,
        vector_config: VectorConfig,
        portfolio_provider: PortfolioProvider,
    ) -> Self {
        Self {
            arm_name,
            intern,
            intern_provider: intern_provider.into(),
            intern_model: intern_model.into(),
            cache,
            engine,
            trader_params,
            vector_config,
            portfolio_provider,
        }
    }

    pub fn vector_config(&self) -> &VectorConfig {
        &self.vector_config
    }
}

#[async_trait]
impl Strategy for TraderArm {
    fn name(&self) -> &'static str {
        self.arm_name
    }

    async fn decide(&self, snapshot: &MarketSnapshot) -> Option<TraderDecision> {
        use xianvec_intern::prompt::{build_intern_prompt, PromptOpts};
        use xianvec_trader::run_trader;

        // 1. Stage 1: cached or fresh briefing (Tier 1 fix #1).
        let key = CacheKey {
            setup_id: snapshot.setup_id,
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
                    snapshot.setup_id,
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

        // 2. Vector load — v1 only `Off` is fully wired. The other three
        //    arms compile + accept their config but degrade-with-warn to
        //    vectors-off behaviour until F1 + F2 land. The harness shape is
        //    exercised end-to-end either way; only the directional claim of
        //    the experimental arms is invalid pre-F1/F2.
        if !matches!(self.vector_config, VectorConfig::Off) {
            tracing::warn!(
                target: "trader_arm",
                arm = self.arm_name,
                config = self.vector_config.label(),
                "non-Off VectorConfig requested; F1/F2 not yet landed — \
                 running the Trader without an installed vector. \
                 This arm's directional claim is invalid until F2."
            );
        }

        // 3. Stage 2: Trader call (Mutex-guarded engine).
        let portfolio = (self.portfolio_provider)();
        let mut engine = self.engine.lock().await;
        match run_trader(&mut *engine, &briefing, &portfolio, &self.trader_params) {
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

    #[test]
    fn vector_config_labels_are_distinct() {
        let off = VectorConfig::Off.label();
        let rand = VectorConfig::Random {
            seed: 0,
            layer: 20,
            hidden_dim: 5120,
            alpha: 1.0,
        }
        .label();
        assert_eq!(off, "off");
        assert_eq!(rand, "random");
        assert_ne!(off, rand);
    }

    /// Mock Intern that returns a fixed briefing — exercises the cache key
    /// shape + the async InternBackend trait without any HTTP traffic.
    struct MockIntern;

    #[async_trait]
    impl InternBackend for MockIntern {
        async fn brief(
            &self,
            _prompt: &str,
            setup_id: Uuid,
            asset: AssetSymbol,
            regime: Regime,
            horizon_hours: u32,
        ) -> Result<InternBriefing, InternError> {
            Ok(InternBriefing {
                setup_id,
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
            setup_id: Uuid::new_v4(),
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
    /// once a real Qwen3Engine is loaded; the pairing-cache contract itself
    /// only depends on the cache key + the Intern's async surface.
    #[tokio::test]
    async fn cache_key_pairs_arms_for_same_setup_id() {
        let cache = Arc::new(BriefingCache::new());
        let snap = mk_snapshot();

        let key = CacheKey {
            setup_id: snap.setup_id,
            provider: "mock".into(),
            model: "mock".into(),
        };
        assert!(cache.get(&key).is_none());

        let intern = MockIntern;
        let briefing = intern
            .brief(
                "p",
                snap.setup_id,
                snap.asset,
                snap.regime,
                snap.horizon_hours,
            )
            .await
            .expect("mock intern always succeeds");
        cache.insert(key.clone(), briefing.clone());

        // Same setup_id → same briefing on lookup (paired arms read it).
        let again = cache.get(&key).expect("cache hit");
        assert_eq!(again.setup_id, briefing.setup_id);
        assert_eq!(again.bull_case, briefing.bull_case);
    }
}
