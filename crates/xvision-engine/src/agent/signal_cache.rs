//! Phase C — per-eval-run in-memory signal cache.
//!
//! Owned by the executor (one per `eval/executor::run()` invocation) and
//! threaded into the per-cycle dispatch loop. Drops when the run
//! completes — there is no SQLite persistence (operator Q5 resolution
//! 2026-05-22). Live trading scenarios rebuild the cache from cycle 1.
//!
//! Cache key is the tuple `(strategy_id, agent_ref_role, scope)`. All
//! components are normalised by the caller (`canonical_role` is the
//! single source of truth for role keys across the engine, so callers
//! should hand in the canonicalised role string). The `scope` field
//! prevents per-asset signals from colliding with global signals or with
//! signals for a different asset when the executor fans out per-asset.
//!
//! Lookup semantics — see the contract acceptance section:
//!
//! * `Bar` granularity: callers do not consult the cache. Every new bar
//!   re-evaluates the Filter.
//! * `Minute` granularity: callers consult the cache and re-fire the
//!   cached signal when the current bar's minute-truncated timestamp is
//!   `<=` the cached signal's minute-truncated timestamp.
//! * `Decision` granularity: callers consult the cache; re-evaluation is
//!   driven by graph topology (the dispatcher walks forward to see if a
//!   Trader is reachable downstream of the Filter — if so, re-evaluate;
//!   otherwise re-fire the cached signal).
//!
//! The cache itself is intentionally tiny — it holds the last
//! `FilterSignal` per `(strategy_id, role, scope)` key plus the
//! `last_evaluated_ts`. The decision of *when* to re-evaluate vs re-fire
//! lives at the call site in `filter_dispatch.rs` and `pipeline.rs`
//! (graph reachability is a pipeline concern, not a cache concern).

use std::collections::HashMap;

use chrono::{DateTime, Utc};

use crate::agent::dispatch_capability::{FilterSignal, SignalScope};

/// Tuple key — see module-level doc. All three components are owned so
/// the cache can outlive the strategy borrow (the executor owns the
/// cache for the duration of the run; the strategy reference is
/// per-cycle).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SignalCacheKey {
    pub strategy_id: String,
    pub role: String,
    pub scope: SignalScope,
}

impl SignalCacheKey {
    pub fn new(strategy_id: impl Into<String>, role: impl Into<String>, scope: SignalScope) -> Self {
        Self {
            strategy_id: strategy_id.into(),
            role: role.into(),
            scope,
        }
    }
}

/// One cached entry — the most recent `FilterSignal` we computed for
/// this `(strategy, role, scope)`, plus the bar timestamp it was computed on.
/// `last_evaluated_ts` mirrors `FilterSignal.ts`; we duplicate it here
/// so cache consumers don't have to clone the signal just to peek at
/// the freshness.
#[derive(Debug, Clone)]
pub struct CachedSignal {
    pub signal: FilterSignal,
    pub last_evaluated_ts: DateTime<Utc>,
}

/// Per-eval-run in-memory cache. `HashMap` (not `BTreeMap`) because
/// ordering is not load-bearing for cache lookups and the average run
/// has a handful of entries.
#[derive(Debug, Default)]
pub struct SignalCache {
    entries: HashMap<SignalCacheKey, CachedSignal>,
}

impl SignalCache {
    /// Fresh cache, owned by the executor for the duration of one run.
    pub fn new() -> Self {
        Self::default()
    }

    /// Read the cached signal for this key, or `None` if no Filter has
    /// produced a signal for this `(strategy, role, scope)` yet.
    pub fn get(&self, key: &SignalCacheKey) -> Option<&CachedSignal> {
        self.entries.get(key)
    }

    /// Insert or overwrite the cached signal for this key. `ts` comes
    /// from the signal's own `ts` field so producer and cache always
    /// agree on freshness.
    pub fn insert(&mut self, key: SignalCacheKey, signal: FilterSignal) {
        let ts = signal.ts;
        self.entries.insert(
            key,
            CachedSignal {
                signal,
                last_evaluated_ts: ts,
            },
        );
    }

    /// Number of cached entries — used in tests to assert the cache
    /// stays bounded to the number of Filter slots in the strategy.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

/// Truncate a UTC timestamp to the start of its minute. Used by the
/// Minute-granularity stale-check: the cached signal is fresh iff the
/// current bar's minute matches the cached signal's minute.
pub fn truncate_to_minute(ts: DateTime<Utc>) -> DateTime<Utc> {
    use chrono::Timelike;
    ts.with_second(0).and_then(|t| t.with_nanosecond(0)).unwrap_or(ts)
}

/// Returns `true` when the Minute-granularity cached signal is still
/// fresh for `now`: i.e. truncating both to their minute yields the
/// same instant. Used by `filter_dispatch::should_reevaluate_minute`.
pub fn minute_cache_is_fresh(cached_ts: DateTime<Utc>, now: DateTime<Utc>) -> bool {
    truncate_to_minute(now) <= truncate_to_minute(cached_ts)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::dispatch_capability::{FilterGranularity, SignalScope};
    use chrono::TimeZone;
    use serde_json::json;

    fn signal_at(name: &str, ts: DateTime<Utc>) -> FilterSignal {
        FilterSignal {
            name: name.to_string(),
            payload: json!({"active": true}),
            granularity: FilterGranularity::Minute,
            ts,
            scope: crate::agent::dispatch_capability::SignalScope::Global,
        }
    }

    #[test]
    fn cache_starts_empty() {
        let c = SignalCache::new();
        assert!(c.is_empty());
        assert_eq!(c.len(), 0);
    }

    #[test]
    fn cache_insert_then_get_round_trips() {
        let mut c = SignalCache::new();
        let ts = Utc.with_ymd_and_hms(2026, 5, 22, 9, 30, 0).unwrap();
        let key = SignalCacheKey::new("sid", "regime_filter", SignalScope::Global);
        c.insert(key.clone(), signal_at("regime_filter", ts));
        let got = c.get(&key).expect("signal present");
        assert_eq!(got.signal.name, "regime_filter");
        assert_eq!(got.last_evaluated_ts, ts);
    }

    #[test]
    fn cache_overwrite_replaces_prior_signal() {
        let mut c = SignalCache::new();
        let t1 = Utc.with_ymd_and_hms(2026, 5, 22, 9, 30, 0).unwrap();
        let t2 = Utc.with_ymd_and_hms(2026, 5, 22, 9, 31, 0).unwrap();
        let key = SignalCacheKey::new("sid", "f", SignalScope::Global);
        c.insert(key.clone(), signal_at("f", t1));
        c.insert(key.clone(), signal_at("f", t2));
        assert_eq!(c.len(), 1);
        assert_eq!(c.get(&key).unwrap().last_evaluated_ts, t2);
    }

    #[test]
    fn keys_differ_by_scope() {
        use xvision_core::trading::AssetSymbol;
        let mut c = SignalCache::new();
        let ts = Utc.with_ymd_and_hms(2026, 5, 22, 9, 30, 0).unwrap();
        let btc = SignalCacheKey::new("sid", "regime", SignalScope::Asset(AssetSymbol::Btc));
        let eth = SignalCacheKey::new("sid", "regime", SignalScope::Asset(AssetSymbol::Eth));
        c.insert(btc.clone(), signal_at("regime", ts));
        c.insert(eth.clone(), signal_at("regime", ts));
        assert_eq!(c.len(), 2, "same role, different asset scope must not collide");
        assert!(c.get(&btc).is_some());
        assert!(c.get(&eth).is_some());
    }

    #[test]
    fn truncate_to_minute_zeroes_subminute_components() {
        let ts = Utc.with_ymd_and_hms(2026, 5, 22, 9, 30, 45).unwrap();
        let expected = Utc.with_ymd_and_hms(2026, 5, 22, 9, 30, 0).unwrap();
        assert_eq!(truncate_to_minute(ts), expected);
    }

    #[test]
    fn minute_cache_is_fresh_within_same_minute() {
        let cached = Utc.with_ymd_and_hms(2026, 5, 22, 9, 30, 0).unwrap();
        let now = Utc.with_ymd_and_hms(2026, 5, 22, 9, 30, 45).unwrap();
        assert!(minute_cache_is_fresh(cached, now));
    }

    #[test]
    fn minute_cache_is_stale_at_next_minute() {
        let cached = Utc.with_ymd_and_hms(2026, 5, 22, 9, 30, 0).unwrap();
        let now = Utc.with_ymd_and_hms(2026, 5, 22, 9, 31, 0).unwrap();
        assert!(!minute_cache_is_fresh(cached, now));
    }
}
