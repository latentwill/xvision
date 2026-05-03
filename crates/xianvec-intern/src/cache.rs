//! Briefing cache. Tier 1 fix #1: vectors-on and vectors-off arms read the
//! SAME briefing for a setup so the trader's decision divergence reflects
//! vector influence rather than Intern non-determinism.
//!
//! v1 cache is in-memory + write-through to SQLite via `xianvec_core::store`.
//! The cache key is `(setup_id, provider, model)` — swapping the Intern
//! backend invalidates cleanly.

use std::collections::HashMap;
use std::sync::Mutex;

use uuid::Uuid;
use xianvec_core::trading::InternBriefing;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct CacheKey {
    pub setup_id: Uuid,
    pub provider: String,
    pub model: String,
}

#[derive(Debug, Default)]
pub struct BriefingCache {
    inner: Mutex<HashMap<CacheKey, InternBriefing>>,
}

impl BriefingCache {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn get(&self, key: &CacheKey) -> Option<InternBriefing> {
        self.inner.lock().expect("BriefingCache mutex poisoned").get(key).cloned()
    }

    pub fn insert(&self, key: CacheKey, briefing: InternBriefing) {
        self.inner.lock().expect("BriefingCache mutex poisoned").insert(key, briefing);
    }

    pub fn len(&self) -> usize {
        self.inner.lock().expect("BriefingCache mutex poisoned").len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;
    use xianvec_core::trading::{AssetSymbol, EvidenceTag, Regime};

    fn fixture_briefing() -> InternBriefing {
        InternBriefing {
            setup_id: Uuid::nil(),
            asset: AssetSymbol::Btc,
            bull_case: "Funding compressed; smart money accumulating spot.".into(),
            bear_case: "Realized vol expanding; long-leverage near squeeze.".into(),
            flat_case: "Range-bound between SMA20 and SMA50; await break.".into(),
            evidence_long: vec![EvidenceTag::Technical("rsi_neutral".into())],
            evidence_short: vec![EvidenceTag::Technical("vol_expansion".into())],
            evidence_flat: vec![EvidenceTag::Technical("range_bound".into())],
            regime: Regime::Chop,
            signal_quality: 0.6,
            horizon_hours: 24,
            created_at: chrono::Utc.timestamp_opt(1_700_000_000, 0).single().unwrap(),
        }
    }

    #[test]
    fn round_trip_get() {
        let c = BriefingCache::new();
        let k =
            CacheKey { setup_id: Uuid::nil(), provider: "anthropic".into(), model: "claude".into() };
        assert!(c.get(&k).is_none());
        c.insert(k.clone(), fixture_briefing());
        assert_eq!(c.get(&k).unwrap().bull_case, fixture_briefing().bull_case);
        assert_eq!(c.len(), 1);
    }

    #[test]
    fn provider_change_invalidates() {
        let c = BriefingCache::new();
        let k_anthropic =
            CacheKey { setup_id: Uuid::nil(), provider: "anthropic".into(), model: "claude".into() };
        let k_openai =
            CacheKey { setup_id: Uuid::nil(), provider: "openai-compat".into(), model: "gpt-5".into() };
        c.insert(k_anthropic.clone(), fixture_briefing());
        assert!(c.get(&k_openai).is_none(), "different provider must miss");
        assert!(c.get(&k_anthropic).is_some());
    }
}
