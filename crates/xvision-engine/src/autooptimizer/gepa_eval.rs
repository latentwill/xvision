use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;

use serde::Serialize;
use sha2::{Digest, Sha256};

use crate::autooptimizer::config::GepaBenchmarkWindow;

#[derive(Debug, Clone, PartialEq)]
pub struct CachedRealEvalScore {
    pub score: f64,
    pub feedback: String,
}

#[derive(Debug, Clone)]
pub struct RealEvalOutcome {
    pub label: String,
    pub parent_sharpe: f64,
    pub child_sharpe: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RealEvalCandidateScore {
    pub score: f64,
    pub feedback: String,
}

#[async_trait]
pub trait BenchmarkEvaluator: Send + Sync {
    async fn evaluate(
        &self,
        instruction: &str,
        benchmark: &GepaBenchmarkWindow,
    ) -> anyhow::Result<RealEvalOutcome>;
}

#[derive(Clone, Default)]
pub struct RealEvalCache {
    inner: Arc<Mutex<HashMap<String, CachedRealEvalScore>>>,
}

impl RealEvalCache {
    pub fn get(&self, key: &str) -> Option<CachedRealEvalScore> {
        self.inner
            .lock()
            .expect("real eval cache poisoned")
            .get(key)
            .cloned()
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

pub async fn score_real_eval_candidate(
    evaluator: &dyn BenchmarkEvaluator,
    instruction: &str,
    pool: &[GepaBenchmarkWindow],
) -> anyhow::Result<RealEvalCandidateScore> {
    let mut scores = Vec::with_capacity(pool.len());
    let mut parts = Vec::with_capacity(pool.len());

    for benchmark in pool {
        match evaluator.evaluate(instruction, benchmark).await {
            Ok(outcome) => {
                let score = normalized_delta_score(outcome.parent_sharpe, outcome.child_sharpe);
                scores.push(score);
                parts.push(format!(
                    "{}: parent Sharpe {:.2}, child Sharpe {:.2}, score {:.2}",
                    outcome.label, outcome.parent_sharpe, outcome.child_sharpe, score
                ));
            }
            Err(e) => {
                scores.push(0.0);
                parts.push(format!("{}: real eval failed: {e}", benchmark.label));
            }
        }
    }

    let score = if scores.is_empty() {
        0.0
    } else {
        scores.iter().sum::<f64>() / scores.len() as f64
    };

    Ok(RealEvalCandidateScore {
        score,
        feedback: format!("Real eval mean score {:.2}. {}", score, parts.join("; ")),
    })
}

pub fn benchmark_pool_fingerprint(pool: &[GepaBenchmarkWindow]) -> String {
    hash_json(&pool)
}

pub fn real_eval_cache_key(namespace: &str, instruction: &str, pool: &[GepaBenchmarkWindow]) -> String {
    let mut hasher = Sha256::new();
    update_framed(&mut hasher, b"xvision.gepa.real-eval.v2");
    update_framed(&mut hasher, namespace.as_bytes());
    update_framed(&mut hasher, instruction.as_bytes());
    update_framed(&mut hasher, benchmark_pool_fingerprint(pool).as_bytes());
    format!("{:x}", hasher.finalize())
}

fn update_framed(hasher: &mut Sha256, field: &[u8]) {
    let len = u64::try_from(field.len()).expect("cache key field length fits in u64");
    hasher.update(len.to_be_bytes());
    hasher.update(field);
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

    #[derive(Clone)]
    struct FakeBenchmarkEvaluator {
        outcomes: Arc<Mutex<Vec<anyhow::Result<RealEvalOutcome>>>>,
    }

    impl FakeBenchmarkEvaluator {
        fn new(outcomes: Vec<RealEvalOutcome>) -> Self {
            Self {
                outcomes: Arc::new(Mutex::new(outcomes.into_iter().map(Ok).collect())),
            }
        }

        fn failing(message: &str) -> Self {
            Self {
                outcomes: Arc::new(Mutex::new(vec![Err(anyhow::anyhow!(message.to_owned()))])),
            }
        }
    }

    #[async_trait::async_trait]
    impl BenchmarkEvaluator for FakeBenchmarkEvaluator {
        async fn evaluate(
            &self,
            _instruction: &str,
            benchmark: &GepaBenchmarkWindow,
        ) -> anyhow::Result<RealEvalOutcome> {
            let outcome = self
                .outcomes
                .lock()
                .expect("fake benchmark outcomes poisoned")
                .remove(0)?;
            Ok(RealEvalOutcome {
                label: benchmark.label.clone(),
                ..outcome
            })
        }
    }

    #[tokio::test]
    async fn benchmark_scores_average_across_windows() {
        let evaluator = FakeBenchmarkEvaluator::new(vec![
            RealEvalOutcome {
                label: "bull".into(),
                parent_sharpe: 1.0,
                child_sharpe: 1.5,
            },
            RealEvalOutcome {
                label: "bear".into(),
                parent_sharpe: 1.0,
                child_sharpe: 0.5,
            },
        ]);
        let pool = vec![bench("bull"), bench("bear")];
        let scored = score_real_eval_candidate(&evaluator, "candidate instruction", &pool)
            .await
            .unwrap();
        assert_eq!(
            scored.score, 0.5,
            "one +0.5 normalized and one -0.5 normalized average to neutral"
        );
        assert!(scored.feedback.contains("bull"));
        assert!(scored.feedback.contains("bear"));
    }

    #[tokio::test]
    async fn benchmark_failure_returns_low_score_with_feedback() {
        let evaluator = FakeBenchmarkEvaluator::failing("missing parent strategy");
        let pool = vec![bench("bull")];
        let scored = score_real_eval_candidate(&evaluator, "candidate instruction", &pool)
            .await
            .unwrap();
        assert_eq!(scored.score, 0.0);
        assert!(scored.feedback.contains("missing parent strategy"));
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
    fn real_eval_cache_key_frames_nul_containing_fields_unambiguously() {
        let pool = vec![bench("a")];

        let namespace_contains_boundary = real_eval_cache_key("a\0b", "c", &pool);
        let instruction_contains_boundary = real_eval_cache_key("a", "b\0c", &pool);

        assert_ne!(namespace_contains_boundary, instruction_contains_boundary);
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
