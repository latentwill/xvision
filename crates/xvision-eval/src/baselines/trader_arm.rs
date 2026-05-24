//! TraderArm — the `Algorithm` adapter that runs the real Stage 1 Intern +
//! Stage 2 Trader pipeline against a `MarketSnapshot`.
//!
//! Post-CV-extraction (ADR 0011) there is no longer a four-arm steering
//! split. TraderArm is a single LLM-without-steering variant that calls a
//! Trader HTTP backend mirroring the Intern HTTP backend.
//!
//! ## Briefing pairing via trajectory replay (Stage 3, Tasks 6 + 9)
//!
//! The in-memory `BriefingCache` (keyed by `(cycle_id, provider, model)`) is
//! retired. Determinism now comes from a trajectory-keyed *briefing replay*:
//! the first arm to run a given intern slot for a cycle RECORDS its briefing;
//! every later arm that resolves to the SAME intern identity REPLAYS it. The
//! pairing key is `TrajectoryKey.fingerprint()` with:
//!
//! * `arm_scope = None` when the intern slot identity (provider + model)
//!   is shared across arms → one shared briefing recording (this is the
//!   exact shared-intern-briefing behavior the old cache provided);
//! * `arm_scope = Some(arm)` when an arm pins a distinct intern model →
//!   that arm records/replays its own briefing.
//!
//! Because backtests run in-process with no SQLite handle threaded into the
//! harness, the replay store here is an in-memory map keyed by the same
//! `TrajectoryKey.fingerprint()` the persistent trajectory store uses. The
//! determinism contract (and the A/B pairing semantics) are identical to the
//! sqlite-backed path; only the storage backend differs.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use uuid::Uuid;

use xvision_core::market::MarketSnapshot;
use xvision_core::trading::{InternBriefing, PortfolioState, TraderDecision};
use xvision_intern::InternBackend;
use xvision_observability::trajectory::key::{TrajectoryKey, TRAJECTORY_SCHEMA_VERSION};
use xvision_trader::{TraderBackend, TraderParams};

use crate::algorithm::Algorithm;

/// Closure that builds the portfolio snapshot the Trader sees on each
/// decision. The harness owns the executor's portfolio; v1 hands the
/// adapter a closure rather than coupling to a specific executor surface.
pub type PortfolioProvider = Arc<dyn Fn() -> PortfolioState + Send + Sync>;

/// Fingerprint-keyed briefing replay store (replaces `BriefingCache`).
///
/// Record-on-first-pass / replay-on-rerun: the first arm to brief a given
/// `(cycle, intern identity, arm_scope)` writes its `InternBriefing`; any
/// later arm that hashes to the SAME `TrajectoryKey.fingerprint()` replays
/// it byte-for-byte. The fingerprint is the dedup key — identical to the
/// persistent trajectory store's keying — so the A/B pairing semantics fall
/// out of the key, not bespoke cache logic (Task 6).
#[derive(Debug, Default)]
pub struct BriefingReplay {
    inner: Mutex<HashMap<String, InternBriefing>>,
}

impl BriefingReplay {
    pub fn new() -> Self {
        Self::default()
    }

    /// Build the trajectory key for an intern-slot briefing.
    ///
    /// `arm_scope = None` pairs arms that share the intern identity;
    /// `Some(arm)` isolates an arm that pins a distinct intern model. Prompt
    /// hashes are intentionally empty here: the briefing pairing key is
    /// `(cycle_id, slot_role, provider, model, arm_scope)`, mirroring the
    /// old `CacheKey` plus the arm-scope dimension.
    pub fn key(
        cycle_id: Uuid,
        provider: &str,
        model: &str,
        arm_scope: Option<&str>,
    ) -> TrajectoryKey {
        TrajectoryKey {
            cycle_id,
            slot_role: "intern".to_string(),
            arm_scope: arm_scope.map(str::to_string),
            simulation_id: None,
            provider: provider.to_string(),
            model: model.to_string(),
            model_version: String::new(),
            schema_version: TRAJECTORY_SCHEMA_VERSION,
            system_prompt_hash: String::new(),
            user_prompt_hash: String::new(),
        }
    }

    /// Replay a recorded briefing for `key`, if one exists.
    pub fn replay(&self, key: &TrajectoryKey) -> Option<InternBriefing> {
        self.inner
            .lock()
            .expect("BriefingReplay mutex poisoned")
            .get(&key.fingerprint())
            .cloned()
    }

    /// Record a briefing as the canonical one for `key` (first pass).
    pub fn record(&self, key: &TrajectoryKey, briefing: InternBriefing) {
        self.inner
            .lock()
            .expect("BriefingReplay mutex poisoned")
            .insert(key.fingerprint(), briefing);
    }

    pub fn len(&self) -> usize {
        self.inner.lock().expect("BriefingReplay mutex poisoned").len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// The `Algorithm`-implementing adapter.
pub struct TraderArm {
    arm_name: &'static str,
    intern: Arc<dyn InternBackend>,
    intern_provider: String,
    intern_model: String,
    /// `None` when this arm shares the intern slot identity with its peers
    /// (shared briefing); `Some(arm)` when it pins a distinct intern model
    /// and must record/replay its own briefing (Task 6).
    intern_arm_scope: Option<String>,
    replay: Arc<BriefingReplay>,
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
        intern_arm_scope: Option<String>,
        replay: Arc<BriefingReplay>,
        trader: Arc<dyn TraderBackend>,
        trader_params: TraderParams,
        portfolio_provider: PortfolioProvider,
    ) -> Self {
        Self {
            arm_name,
            intern,
            intern_provider: intern_provider.into(),
            intern_model: intern_model.into(),
            intern_arm_scope,
            replay,
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
        use xvision_intern::prompt::{build_intern_prompt, PromptOpts};
        use xvision_trader::run_trader;

        // 1. Stage 1: replay a recorded briefing, or record a fresh one
        //    (Tasks 6 + 9 — determinism via trajectory replay).
        let key = BriefingReplay::key(
            snapshot.cycle_id,
            &self.intern_provider,
            &self.intern_model,
            self.intern_arm_scope.as_deref(),
        );
        let briefing = if let Some(b) = self.replay.replay(&key) {
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
                    self.replay.record(&key, b.clone());
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

    use xvision_core::market::{IndicatorPanel, Ohlcv, OnchainPanel};
    use xvision_core::trading::{AssetSymbol, EvidenceTag, InternBriefing, Regime};
    use xvision_intern::backend::InternError;

    /// Mock Intern that returns a fixed briefing — exercises the replay key
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

    /// Shared-briefing pairing: two arms with the SAME intern identity and
    /// `arm_scope = None` resolve to ONE recording — the second arm replays
    /// the first arm's briefing (the determinism the old cache provided).
    #[tokio::test]
    async fn shared_intern_identity_replays_one_briefing() {
        let replay = Arc::new(BriefingReplay::new());
        let snap = mk_snapshot();

        let key = BriefingReplay::key(snap.cycle_id, "anthropic", "claude-haiku-4-5", None);
        assert!(replay.replay(&key).is_none());

        let intern = MockIntern;
        let briefing = intern
            .brief("p", snap.cycle_id, snap.asset, snap.regime, snap.horizon_hours)
            .await
            .expect("mock intern always succeeds");
        replay.record(&key, briefing.clone());

        // A second arm with the SAME intern identity replays the SAME briefing.
        let key2 = BriefingReplay::key(snap.cycle_id, "anthropic", "claude-haiku-4-5", None);
        let again = replay.replay(&key2).expect("shared identity replays");
        assert_eq!(again.cycle_id, briefing.cycle_id);
        assert_eq!(again.bull_case, briefing.bull_case);
        assert_eq!(replay.len(), 1, "shared identity -> one recording");
    }

    /// Per-arm pairing: an arm pinning a DIFFERENT intern model gets its own
    /// recording — the shared one must not satisfy it (Task 6 arm-specific).
    #[tokio::test]
    async fn distinct_intern_model_records_independently() {
        let replay = Arc::new(BriefingReplay::new());
        let snap = mk_snapshot();

        let key_haiku = BriefingReplay::key(snap.cycle_id, "anthropic", "claude-haiku-4-5", None);
        let key_opus = BriefingReplay::key(snap.cycle_id, "anthropic", "claude-opus-4-7", None);
        assert_ne!(
            key_haiku.fingerprint(),
            key_opus.fingerprint(),
            "different intern model -> different fingerprint"
        );

        let intern = MockIntern;
        let briefing = intern
            .brief("p", snap.cycle_id, snap.asset, snap.regime, snap.horizon_hours)
            .await
            .unwrap();
        replay.record(&key_haiku, briefing);
        assert!(replay.replay(&key_haiku).is_some());
        assert!(
            replay.replay(&key_opus).is_none(),
            "different intern model must miss -> Stage 1 re-runs"
        );
    }

    /// Arm-scope dimension: same intern identity but distinct `arm_scope`
    /// values isolate recordings (the per-arm-slot mode from Task 6).
    #[test]
    fn arm_scope_isolates_recordings() {
        let cycle = Uuid::new_v4();
        let shared = BriefingReplay::key(cycle, "anthropic", "claude-haiku-4-5", None);
        let scoped = BriefingReplay::key(cycle, "anthropic", "claude-haiku-4-5", Some("arm_b"));
        assert_ne!(shared.fingerprint(), scoped.fingerprint());
    }
}
