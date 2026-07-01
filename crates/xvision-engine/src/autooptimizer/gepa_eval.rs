use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use serde::Serialize;
use sha2::{Digest, Sha256};

use crate::autooptimizer::config::GepaBenchmarkWindow;

#[derive(Debug, Clone, PartialEq)]
pub struct CachedRealEvalScore {
    pub score: f64,
    pub feedback: String,
}

#[derive(Clone, Default)]
pub struct RealEvalCache {
    inner: Arc<Mutex<HashMap<String, CachedRealEvalScore>>>,
}

impl RealEvalCache {
    pub fn get(&self, key: &str) -> Option<CachedRealEvalScore> {
        self.inner.lock().expect("real eval cache poisoned").get(key).cloned()
    }

    pub fn insert(&self, key: String, score: f64, feedback: String) {
        self.inner
            .lock()
            .expect("real eval cache poisoned")
            .insert(key, CachedRealEvalScore { score, feedback });
    }
}

pub fn normalized_delta_score(parent_metric: f64, child_metric: f64) -> f64 {
    let denom = parent_metric.abs().max(0.01);
    let normalized = (child_metric - parent_metric) / denom;
    ((normalized + 1.0) / 2.0).clamp(0.0, 1.0)
}

pub fn benchmark_pool_fingerprint(pool: &[GepaBenchmarkWindow]) -> String {
    hash_json(&pool)
}

pub fn real_eval_cache_key(namespace: &str, instruction: &str, pool: &[GepaBenchmarkWindow]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(b"xvision.gepa.real-eval.v1\0");
    hasher.update(namespace.as_bytes());
    hasher.update(b"\0");
    hasher.update(instruction.as_bytes());
    hasher.update(b"\0");
    hasher.update(benchmark_pool_fingerprint(pool).as_bytes());
    format!("{:x}", hasher.finalize())
}

fn hash_json<T: Serialize>(value: &T) -> String {
    let bytes = serde_json::to_vec(value).expect("serializable benchmark pool");
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    format!("{:x}", hasher.finalize())
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;

    use crate::autooptimizer::config::{BaselineUntouchedWindow, DayWindow, GepaBenchmarkWindow};

    fn bench(label: &str) -> GepaBenchmarkWindow {
        GepaBenchmarkWindow {
            label: label.into(),
            parent_strategy_id: "parent-a".into(),
            day: DayWindow {
                start: NaiveDate::from_ymd_opt(2025, 1, 1).unwrap(),
                end: NaiveDate::from_ymd_opt(2025, 1, 15).unwrap(),
            },
            baseline: BaselineUntouchedWindow {
                start: NaiveDate::from_ymd_opt(2025, 1, 15).unwrap(),
                end: NaiveDate::from_ymd_opt(2025, 2, 1).unwrap(),
            },
        }
    }

    #[test]
    fn normalized_delta_score_clamps_to_unit_interval() {
        assert_eq!(normalized_delta_score(1.0, 3.0), 1.0);
        assert_eq!(normalized_delta_score(1.0, -2.0), 0.0);
        assert_eq!(normalized_delta_score(1.0, 1.0), 0.5);
        assert!(normalized_delta_score(0.0, 0.01) > 0.5);
    }

    #[test]
    fn benchmark_fingerprint_changes_when_pool_changes() {
        let a = benchmark_pool_fingerprint(&[bench("a")]);
        let b = benchmark_pool_fingerprint(&[bench("b")]);
        assert_ne!(a, b);
    }

    #[test]
    fn real_eval_cache_key_includes_namespace_instruction_and_pool() {
        let pool = vec![bench("a")];
        let a = real_eval_cache_key("ns-a", "instruction", &pool);
        let b = real_eval_cache_key("ns-b", "instruction", &pool);
        let c = real_eval_cache_key("ns-a", "other instruction", &pool);
        assert_ne!(a, b);
        assert_ne!(a, c);
        assert_eq!(a.len(), 64, "sha256 hex cache key");
    }

    #[test]
    fn cache_round_trips_scores_by_key() {
        let cache = RealEvalCache::default();
        assert!(cache.get("k").is_none());
        cache.insert("k".into(), 0.72, "real eval improved".into());
        let hit = cache.get("k").expect("cached score");
        assert_eq!(hit.score, 0.72);
        assert_eq!(hit.feedback, "real eval improved");
    }
}
