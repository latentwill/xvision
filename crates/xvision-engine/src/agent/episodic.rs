//! Episodic memory layer for the xvision trading engine.
//!
//! Provides two units:
//!
//! * **U3** — [`IndicatorSnapshot`], [`EpisodicObservation`], and
//!   [`cosine_similarity`]: the data types and feature-extraction math.
//! * **U4** — [`EpisodicStore`]: a bounded ring-buffer with cosine-similarity
//!   top-k retrieval and JSON seeding.
//!
//! This module is pure, self-contained, and performs no I/O.

use serde::{Deserialize, Serialize};

// ── U3: types and feature math ───────────────────────────────────────────────

/// A snapshot of indicator values at the moment an episodic observation was
/// recorded. All fields are optional — indicators that could not be computed
/// (e.g. insufficient price history) are stored as `None` and map to `0.0`
/// in the feature vector.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct IndicatorSnapshot {
    /// Relative Strength Index (0 – 100).
    pub rsi: Option<f64>,
    /// MACD histogram value (fast EMA − slow EMA − signal).
    pub macd_hist: Option<f64>,
    /// EMA cross = EMA-12 minus EMA-26; sign encodes trend direction.
    pub ema_cross: Option<f64>,
    /// Volume z-score relative to a rolling baseline.
    pub volume_zscore: Option<f64>,
}

impl IndicatorSnapshot {
    /// Produce a length-4 feature vector normalized to roughly \[-1, 1\].
    ///
    /// Each slot:
    /// * `rsi`          → `(clamp(rsi, 0, 100) − 50) / 50`
    /// * `macd_hist`    → `clamp(macd_hist, −500, 500) / 500`
    /// * `ema_cross`    → `clamp(ema_cross, −500, 500) / 500`
    /// * `volume_zscore`→ `clamp(zscore, −3, 3) / 3`
    ///
    /// `None` → `0.0` for every slot.
    pub fn feature_vector(&self) -> [f64; 4] {
        let rsi = self
            .rsi
            .map(|v| (v.clamp(0.0, 100.0) - 50.0) / 50.0)
            .unwrap_or(0.0);
        let macd = self
            .macd_hist
            .map(|v| v.clamp(-500.0, 500.0) / 500.0)
            .unwrap_or(0.0);
        let ema = self
            .ema_cross
            .map(|v| v.clamp(-500.0, 500.0) / 500.0)
            .unwrap_or(0.0);
        let vol = self
            .volume_zscore
            .map(|v| v.clamp(-3.0, 3.0) / 3.0)
            .unwrap_or(0.0);
        [rsi, macd, ema, vol]
    }
}

/// A single episodic observation recorded at a trading decision point.
///
/// Stored in [`EpisodicStore`] and retrieved by cosine similarity against a
/// query feature vector.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct EpisodicObservation {
    /// ISO-8601 timestamp of the price bar that triggered this decision.
    pub bar_timestamp: String,
    /// Monotonically increasing decision index within a run.
    pub decision_idx: u32,
    /// Action taken (`"buy"`, `"sell"`, `"hold"`, etc.).
    pub action: String,
    /// Model-reported conviction in \[0, 1\].
    pub conviction: f64,
    /// Entry price, if the action opened or modified a position.
    pub entry_price: Option<f64>,
    /// Human-readable exit reason, if the action closed a position.
    pub exit_reason: Option<String>,
    /// Short excerpt from the LLM rationale (≤ 120 Unicode scalar values).
    /// Guaranteed by the [`EpisodicObservation::new`] constructor.
    pub rationale_excerpt: String,
    /// Indicator values at the time of the decision.
    pub indicators: IndicatorSnapshot,
}

impl EpisodicObservation {
    /// Construct a new observation, trimming and truncating `rationale_excerpt`
    /// to at most 120 Unicode scalar values (chars) so the stored excerpt is
    /// always within the character budget. Multibyte characters are handled
    /// safely via `.chars().take(120).collect()`.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        bar_timestamp: impl Into<String>,
        decision_idx: u32,
        action: impl Into<String>,
        conviction: f64,
        entry_price: Option<f64>,
        exit_reason: Option<impl Into<String>>,
        rationale_excerpt: impl Into<String>,
        indicators: IndicatorSnapshot,
    ) -> Self {
        let raw: String = rationale_excerpt.into();
        let trimmed = raw.trim();
        let excerpt: String = trimmed.chars().take(120).collect();
        Self {
            bar_timestamp: bar_timestamp.into(),
            decision_idx,
            action: action.into(),
            conviction,
            entry_price,
            exit_reason: exit_reason.map(Into::into),
            rationale_excerpt: excerpt,
            indicators,
        }
    }

    /// Delegate feature-vector extraction to [`IndicatorSnapshot::feature_vector`].
    pub fn feature_vector(&self) -> [f64; 4] {
        self.indicators.feature_vector()
    }
}

/// Cosine similarity between two length-4 feature vectors.
///
/// Returns `0.0` (not NaN) when either vector has zero magnitude.
pub fn cosine_similarity(a: &[f64; 4], b: &[f64; 4]) -> f64 {
    let dot: f64 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
    let mag_a: f64 = a.iter().map(|x| x * x).sum::<f64>().sqrt();
    let mag_b: f64 = b.iter().map(|x| x * x).sum::<f64>().sqrt();
    if mag_a == 0.0 || mag_b == 0.0 {
        return 0.0;
    }
    dot / (mag_a * mag_b)
}

// ── U4: EpisodicStore ────────────────────────────────────────────────────────

/// Default cap used when `EpisodicStore::new(0)` is called.
const DEFAULT_MAX_OBSERVATIONS: usize = 500;

/// A bounded ring-buffer of [`EpisodicObservation`]s with cosine-similarity
/// top-k retrieval.
///
/// When the store is full, the oldest observation (index 0) is evicted before
/// each new push so the length never exceeds `max_observations`.
#[derive(Debug, Clone, Default)]
pub struct EpisodicStore {
    /// Ordered oldest-to-newest.
    pub observations: Vec<EpisodicObservation>,
    /// Hard cap on the number of retained observations.
    pub max_observations: usize,
}

impl EpisodicStore {
    /// Create a new store with the given capacity.
    ///
    /// Passing `0` is treated as a sensible default of
    /// [`DEFAULT_MAX_OBSERVATIONS`] (500) so a misconfigured zero cap does not
    /// silently drop every pushed observation.
    pub fn new(max_observations: usize) -> Self {
        let cap = if max_observations == 0 {
            DEFAULT_MAX_OBSERVATIONS
        } else {
            max_observations
        };
        Self {
            observations: Vec::new(),
            max_observations: cap,
        }
    }

    /// Append an observation. When the store is already at capacity the oldest
    /// entry is removed first so the length stays at or below `max_observations`.
    pub fn push(&mut self, obs: EpisodicObservation) {
        if self.observations.len() >= self.max_observations {
            self.observations.remove(0);
        }
        self.observations.push(obs);
    }

    /// Number of stored observations.
    pub fn len(&self) -> usize {
        self.observations.len()
    }

    /// Returns `true` when no observations are stored.
    pub fn is_empty(&self) -> bool {
        self.observations.is_empty()
    }

    /// Retrieve the top-`k` observations by cosine similarity to `query_vec`.
    ///
    /// Ties are broken by `decision_idx` **descending** (more recent first) so
    /// the ordering is deterministic. Returns an empty vec when the store is
    /// empty or `k == 0`.
    pub fn query(&self, query_vec: [f64; 4], k: usize) -> Vec<&EpisodicObservation> {
        if k == 0 || self.is_empty() {
            return Vec::new();
        }

        let mut scored: Vec<(f64, u32, &EpisodicObservation)> = self
            .observations
            .iter()
            .map(|obs| {
                let fv = obs.feature_vector();
                let sim = cosine_similarity(&query_vec, &fv);
                (sim, obs.decision_idx, obs)
            })
            .collect();

        // Sort descending by similarity, then descending by decision_idx for ties.
        scored.sort_by(|a, b| {
            b.0.partial_cmp(&a.0)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| b.1.cmp(&a.1))
        });

        scored.into_iter().take(k).map(|(_, _, obs)| obs).collect()
    }

    /// Return the top-`k` observations serialised as a JSON array, or `None`
    /// when the store is empty or the query yields no results.
    ///
    /// Each element is the full [`EpisodicObservation`] serialised via
    /// [`serde_json::to_value`].
    pub fn to_seed_json(&self, query_vec: [f64; 4], k: usize) -> Option<serde_json::Value> {
        if self.is_empty() {
            return None;
        }
        let top = self.query(query_vec, k);
        if top.is_empty() {
            return None;
        }
        let arr: Vec<serde_json::Value> = top
            .iter()
            .map(|obs| serde_json::to_value(obs).expect("EpisodicObservation must serialize"))
            .collect();
        Some(serde_json::Value::Array(arr))
    }
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── helpers ──────────────────────────────────────────────────────────────

    fn make_obs(decision_idx: u32, indicators: IndicatorSnapshot) -> EpisodicObservation {
        EpisodicObservation::new(
            "2026-01-01T00:00:00Z",
            decision_idx,
            "hold",
            0.5,
            None,
            None::<String>,
            "some rationale",
            indicators,
        )
    }

    // ── U3: IndicatorSnapshot::feature_vector ────────────────────────────────

    #[test]
    fn feature_vector_all_none_is_zero() {
        let snap = IndicatorSnapshot::default();
        assert_eq!(snap.feature_vector(), [0.0; 4]);
    }

    #[test]
    fn rsi_70_normalizes_to_0_4() {
        let snap = IndicatorSnapshot {
            rsi: Some(70.0),
            ..Default::default()
        };
        let fv = snap.feature_vector();
        let expected = (70.0_f64 - 50.0) / 50.0; // 0.4
        assert!((fv[0] - expected).abs() < 1e-10, "rsi slot = {}", fv[0]);
        assert_eq!(fv[1], 0.0);
        assert_eq!(fv[2], 0.0);
        assert_eq!(fv[3], 0.0);
    }

    #[test]
    fn macd_hist_clamps_positive() {
        let snap = IndicatorSnapshot {
            macd_hist: Some(1000.0),
            ..Default::default()
        };
        let fv = snap.feature_vector();
        assert!((fv[1] - 1.0).abs() < 1e-10, "macd slot = {}", fv[1]);
    }

    #[test]
    fn macd_hist_clamps_negative() {
        let snap = IndicatorSnapshot {
            macd_hist: Some(-1000.0),
            ..Default::default()
        };
        let fv = snap.feature_vector();
        assert!((fv[1] - (-1.0)).abs() < 1e-10, "macd slot = {}", fv[1]);
    }

    #[test]
    fn rsi_out_of_range_clamps_to_plus_one() {
        let snap = IndicatorSnapshot {
            rsi: Some(110.0),
            ..Default::default()
        };
        let fv = snap.feature_vector();
        // clamp(110, 0, 100) = 100 → (100-50)/50 = 1.0
        assert!((fv[0] - 1.0).abs() < 1e-10, "rsi slot with 110 = {}", fv[0]);
    }

    // ── U3: cosine_similarity ────────────────────────────────────────────────

    #[test]
    fn cosine_identical_vectors_is_one() {
        let v = [0.4_f64, 0.2, -0.3, 0.1];
        let sim = cosine_similarity(&v, &v);
        assert!((sim - 1.0).abs() < 1e-10, "sim = {sim}");
    }

    #[test]
    fn cosine_orthogonal_vectors_is_zero() {
        let a = [1.0_f64, 0.0, 0.0, 0.0];
        let b = [0.0_f64, 1.0, 0.0, 0.0];
        let sim = cosine_similarity(&a, &b);
        assert!(sim.abs() < 1e-10, "sim = {sim}");
    }

    #[test]
    fn cosine_opposite_vectors_is_minus_one() {
        let a = [1.0_f64, 0.5, 0.0, -0.2];
        let b = [-1.0_f64, -0.5, 0.0, 0.2];
        let sim = cosine_similarity(&a, &b);
        assert!((sim - (-1.0)).abs() < 1e-10, "sim = {sim}");
    }

    #[test]
    fn cosine_zero_vector_returns_zero_not_nan() {
        let zero = [0.0_f64; 4];
        let other = [0.5_f64, 0.0, -0.3, 0.1];
        let sim = cosine_similarity(&zero, &other);
        assert!(!sim.is_nan(), "must not be NaN");
        assert_eq!(sim, 0.0);
    }

    // ── U3: EpisodicObservation rationale_excerpt truncation ─────────────────

    #[test]
    fn rationale_long_string_truncates_to_120() {
        let long = "a".repeat(200);
        let obs = EpisodicObservation::new(
            "ts",
            0,
            "buy",
            0.9,
            None,
            None::<String>,
            long,
            IndicatorSnapshot::default(),
        );
        assert_eq!(obs.rationale_excerpt.chars().count(), 120);
    }

    #[test]
    fn rationale_short_string_unchanged() {
        let short = "short rationale";
        let obs = EpisodicObservation::new(
            "ts",
            0,
            "buy",
            0.9,
            None,
            None::<String>,
            short,
            IndicatorSnapshot::default(),
        );
        assert_eq!(obs.rationale_excerpt, short);
    }

    #[test]
    fn rationale_multibyte_string_does_not_panic() {
        // Each '€' is 3 bytes. 50 of them = 150 chars of content, should be
        // truncated to 120 chars (120 '€' codepoints) without a byte-split panic.
        let multibyte = "€".repeat(150);
        let obs = EpisodicObservation::new(
            "ts",
            0,
            "buy",
            0.9,
            None,
            None::<String>,
            multibyte,
            IndicatorSnapshot::default(),
        );
        assert_eq!(obs.rationale_excerpt.chars().count(), 120);
        // Verify it's valid UTF-8 (no panic is the real test, but this is explicit).
        let _ = obs.rationale_excerpt.as_str();
    }

    #[test]
    fn rationale_whitespace_trimmed_before_truncation() {
        let padded = format!("   {}   ", "x".repeat(5));
        let obs = EpisodicObservation::new(
            "ts",
            0,
            "hold",
            0.5,
            None,
            None::<String>,
            padded,
            IndicatorSnapshot::default(),
        );
        assert_eq!(obs.rationale_excerpt, "xxxxx");
    }

    // ── U4: EpisodicStore — empty ─────────────────────────────────────────────

    #[test]
    fn empty_store_query_returns_empty() {
        let store = EpisodicStore::new(10);
        let result = store.query([0.0; 4], 5);
        assert!(result.is_empty());
    }

    #[test]
    fn empty_store_to_seed_json_returns_none() {
        let store = EpisodicStore::new(10);
        assert!(store.to_seed_json([0.0; 4], 3).is_none());
    }

    // ── U4: single observation ────────────────────────────────────────────────

    #[test]
    fn single_observation_returned_by_query() {
        let mut store = EpisodicStore::new(10);
        let obs = make_obs(1, IndicatorSnapshot { rsi: Some(60.0), ..Default::default() });
        store.push(obs.clone());
        let result = store.query([0.2, 0.0, 0.0, 0.0], 1);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].decision_idx, 1);
    }

    // ── U4: highest similarity first ─────────────────────────────────────────

    #[test]
    fn highest_similarity_observation_is_first() {
        let mut store = EpisodicStore::new(10);
        // obs A: rsi=70 → fv[0] ≈ +0.4; strongly aligned with query [1,0,0,0]
        let obs_a = make_obs(1, IndicatorSnapshot { rsi: Some(70.0), ..Default::default() });
        // obs B: rsi=30 → fv[0] ≈ -0.4; anti-aligned with query [1,0,0,0]
        let obs_b = make_obs(2, IndicatorSnapshot { rsi: Some(30.0), ..Default::default() });
        store.push(obs_a);
        store.push(obs_b);

        let query = [1.0_f64, 0.0, 0.0, 0.0];
        let result = store.query(query, 2);
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].decision_idx, 1, "obs_a (rsi=70) should rank first");
    }

    // ── U4: push beyond cap evicts oldest ────────────────────────────────────

    #[test]
    fn push_beyond_cap_evicts_oldest() {
        let cap = 3;
        let mut store = EpisodicStore::new(cap);
        for i in 0..5u32 {
            store.push(make_obs(i, IndicatorSnapshot::default()));
        }
        assert_eq!(store.len(), cap);
        // Oldest surviving decision_idx should be 2 (0 and 1 were evicted).
        assert_eq!(store.observations[0].decision_idx, 2);
    }

    // ── U4: to_seed_json returns k entries ───────────────────────────────────

    #[test]
    fn to_seed_json_returns_k_entries() {
        let mut store = EpisodicStore::new(20);
        for i in 0..10u32 {
            let snap = IndicatorSnapshot {
                rsi: Some(50.0 + i as f64),
                ..Default::default()
            };
            store.push(make_obs(i, snap));
        }
        let json = store.to_seed_json([1.0, 0.0, 0.0, 0.0], 3);
        assert!(json.is_some());
        let arr = json.unwrap();
        let entries = arr.as_array().expect("must be array");
        assert_eq!(entries.len(), 3);
        // Each entry must round-trip to a valid EpisodicObservation.
        for entry in entries {
            let _obs: EpisodicObservation =
                serde_json::from_value(entry.clone()).expect("must deserialize");
        }
    }

    // ── U4: tie-break by decision_idx descending ──────────────────────────────

    #[test]
    fn tie_break_by_decision_idx_descending() {
        let mut store = EpisodicStore::new(10);
        // Both observations have identical all-None indicators → identical zero
        // feature vectors → identical cosine similarity (0.0 vs any query).
        let obs_low = make_obs(1, IndicatorSnapshot::default());
        let obs_high = make_obs(5, IndicatorSnapshot::default());
        store.push(obs_low);
        store.push(obs_high);

        // Any query works since both vectors are zero → similarity is 0.0 for both.
        let result = store.query([1.0, 0.0, 0.0, 0.0], 2);
        assert_eq!(result.len(), 2);
        // Higher decision_idx (5) should be first on tie-break.
        assert_eq!(result[0].decision_idx, 5);
        assert_eq!(result[1].decision_idx, 1);
    }

    // ── U4: new(0) defaults to 500, not zero-cap ─────────────────────────────

    #[test]
    fn new_zero_cap_defaults_to_500() {
        let store = EpisodicStore::new(0);
        assert_eq!(store.max_observations, 500);
    }

    #[test]
    fn new_zero_cap_can_store_observations() {
        let mut store = EpisodicStore::new(0);
        store.push(make_obs(1, IndicatorSnapshot::default()));
        assert_eq!(store.len(), 1);
    }

    // ── U4: k=0 query returns empty ──────────────────────────────────────────

    #[test]
    fn query_k_zero_returns_empty() {
        let mut store = EpisodicStore::new(10);
        store.push(make_obs(1, IndicatorSnapshot::default()));
        let result = store.query([0.0; 4], 0);
        assert!(result.is_empty());
    }
}
