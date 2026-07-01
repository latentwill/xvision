# DSPy Real GEPA Evaluation Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add an opt-in real backtest evaluation tier for GEPA instruction candidates so DSPy prompt compilation can select instructions that improve benchmark strategy outcomes, not just instructions that an LLM says match observations.

**Architecture:** Keep the current LLM scorer as the default and as the cheap first-pass cull. Add configuration and an internal scorer seam to `GepaBridge`; when `gepa_real_eval=true`, candidate instructions above the fast-score threshold are rescored by a deterministic benchmark scorer whose score flows into GEPA selection and `PatternSnapshot` demos. The first implementation uses deterministic test doubles for the expensive mutator/backtest boundary and keeps engine/dashboard free of `xvision-dspy` dependencies.

**Tech Stack:** Rust, async_trait, serde/TOML config, sqlx SQLite-backed stores where already present, existing AutoOptimizer modules under `crates/xvision-engine/src/autooptimizer`, existing CLI/dashboard GEPA wiring.

## Global Constraints

- `gepa_real_eval` is opt-in and defaults to `false`.
- Current LLM-only GEPA behavior must remain unchanged when `gepa_real_eval=false`.
- `xvision-engine`, `xvision-cli`, and `xvision-dashboard` must not import `xvision-dspy`, `dspy-rs`, `rig-core`, or related heavy DSPy dependencies.
- Do not restore standalone DSPy CLI subcommands; the flywheel remains inside `xvn optimize run` / dashboard cycle execution.
- Real-eval implementation must use deterministic unit tests and fakes; no provider keys, network calls, or live broker/data dependencies.
- Real scores must populate `SnapshotDemo.score` when real eval is enabled.
- Broad `cargo test -p xvision-engine` is known to include unrelated migration-fixture failures on this branch family; targeted tests are the acceptance gate unless those unrelated fixtures are fixed in scope.

---

## File Structure

- Modify `crates/xvision-engine/src/autooptimizer/config.rs`
  - Add `gepa_real_eval`, `gepa_real_eval_min_llm_score`, and `gepa_benchmark_pool` fields.
  - Add `GepaBenchmarkWindow` as the benchmark-pool item struct.
  - Add unit tests for defaults and validation.
- Modify `crates/xvision-engine/src/autooptimizer/gepa.rs`
  - Add internal scorer abstraction.
  - Preserve existing LLM score logic.
  - Route candidates through fast score then optional real score.
  - Add deterministic tests proving real score changes winner selection and low fast scores skip real scoring.
- Create `crates/xvision-engine/src/autooptimizer/gepa_eval.rs`
  - Add real-score math, benchmark fingerprinting, and in-memory cache helpers.
  - Add test-double-friendly `BenchmarkEvaluator` trait.
  - Add unit tests for score clamping, cache key stability, and failure-to-low-score behavior.
- Modify `crates/xvision-engine/src/autooptimizer/mod.rs`
  - Register `pub mod gepa_eval;`.
- Modify `crates/xvision-cli/src/commands/optimize.rs`
  - Pass real-eval config into `GepaBridge` construction.
- Modify `crates/xvision-dashboard/src/routes/autooptimizer_cycle.rs`
  - Pass real-eval config into dashboard-created `GepaBridge` construction.
- Test targets:
  - `cargo test -p xvision-engine autooptimizer::config::tests::gepa_`
  - `cargo test -p xvision-engine autooptimizer::gepa::tests::real_eval_`
  - `cargo test -p xvision-engine autooptimizer::gepa_eval::tests::`
  - `cargo test -p xvision-engine --lib autooptimizer::gepa`

---

### Task 1: Add real GEPA eval configuration

**Files:**
- Modify: `crates/xvision-engine/src/autooptimizer/config.rs:23-33`
- Modify: `crates/xvision-engine/src/autooptimizer/config.rs:177-204`
- Modify: `crates/xvision-engine/src/autooptimizer/config.rs:346-390`
- Modify: `crates/xvision-engine/src/autooptimizer/config.rs:899-1010`

**Interfaces:**
- Produces: `AutoOptimizerConfig.gepa_real_eval: bool`
- Produces: `AutoOptimizerConfig.gepa_real_eval_min_llm_score: f64`
- Produces: `AutoOptimizerConfig.gepa_benchmark_pool: Vec<GepaBenchmarkWindow>`
- Produces: `GepaBenchmarkWindow { label: String, parent_strategy_id: String, day: DayWindow, baseline: BaselineUntouchedWindow }`
- Consumes: existing `AutoOptimizerConfig::validate()` and `MAX_WINDOW_DAYS` validation pattern.

- [ ] **Step 1: Write failing default/validation tests**

Add tests near the existing config tests:

```rust
#[test]
fn gepa_real_eval_defaults_to_disabled_with_empty_pool() {
    let cfg = AutoOptimizerConfig::default();
    assert!(!cfg.gepa_real_eval);
    assert_eq!(cfg.gepa_real_eval_min_llm_score, 0.30);
    assert!(cfg.gepa_benchmark_pool.is_empty());
    assert!(cfg.validate().is_ok());
}

#[test]
fn gepa_real_eval_requires_benchmark_pool_when_enabled() {
    let mut cfg = AutoOptimizerConfig::default();
    cfg.gepa_real_eval = true;
    let err = cfg.validate().unwrap_err();
    assert!(
        err.to_string().contains("gepa_benchmark_pool"),
        "real eval without benchmarks must name gepa_benchmark_pool; got: {err}"
    );
}

#[test]
fn gepa_real_eval_rejects_fast_score_threshold_outside_unit_interval() {
    let mut cfg = AutoOptimizerConfig::default();
    cfg.gepa_real_eval_min_llm_score = 1.1;
    let err = cfg.validate().unwrap_err();
    assert!(
        err.to_string().contains("gepa_real_eval_min_llm_score"),
        "threshold >1 must be rejected by name; got: {err}"
    );

    cfg.gepa_real_eval_min_llm_score = -0.01;
    let err = cfg.validate().unwrap_err();
    assert!(
        err.to_string().contains("gepa_real_eval_min_llm_score"),
        "threshold <0 must be rejected by name; got: {err}"
    );
}

#[test]
fn gepa_benchmark_pool_rejects_overlong_day_window() {
    let mut cfg = AutoOptimizerConfig::default();
    cfg.gepa_real_eval = true;
    cfg.gepa_benchmark_pool = vec![GepaBenchmarkWindow {
        label: "too-wide".into(),
        parent_strategy_id: "parent-a".into(),
        day: DayWindow {
            start: NaiveDate::from_ymd_opt(2024, 1, 1).unwrap(),
            end: NaiveDate::from_ymd_opt(2025, 1, 1).unwrap(),
        },
        baseline: BaselineUntouchedWindow {
            start: NaiveDate::from_ymd_opt(2025, 1, 1).unwrap(),
            end: NaiveDate::from_ymd_opt(2025, 2, 1).unwrap(),
        },
    }];
    let err = cfg.validate().unwrap_err();
    let msg = err.to_string();
    assert!(
        msg.contains("gepa_benchmark_pool") && msg.contains("too-wide") && msg.contains("120"),
        "message must name benchmark pool, label, and day cap; got: {msg}"
    );
}
```

- [ ] **Step 2: Run tests to verify RED**

Run:

```bash
cargo test -p xvision-engine autooptimizer::config::tests::gepa_
```

Expected: compile fails because `gepa_real_eval`, `gepa_real_eval_min_llm_score`, `gepa_benchmark_pool`, and `GepaBenchmarkWindow` do not exist.

- [ ] **Step 3: Add config fields and defaults**

Add helpers near `default_gepa_generations()`:

```rust
fn default_gepa_real_eval_min_llm_score() -> f64 {
    0.30
}
```

Add struct near `ScenarioWindowPair`:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GepaBenchmarkWindow {
    pub label: String,
    pub parent_strategy_id: String,
    pub day: DayWindow,
    pub baseline: BaselineUntouchedWindow,
}
```

Add fields to `AutoOptimizerConfig` after `gepa_generations`:

```rust
    /// Opt-in GEPA scorer that uses benchmark backtests after the cheap LLM
    /// cull. Defaults off so existing DSPy compiles keep their current cost and
    /// behavior.
    #[serde(default)]
    pub gepa_real_eval: bool,

    /// Minimum fast LLM score a candidate must achieve before the optimizer pays
    /// for real benchmark evaluation. Range: 0.0..=1.0.
    #[serde(default = "default_gepa_real_eval_min_llm_score")]
    pub gepa_real_eval_min_llm_score: f64,

    /// Fixed benchmark windows used only to score GEPA instruction candidates.
    /// Empty is valid when `gepa_real_eval=false`; at least one entry is required
    /// when real eval is enabled.
    #[serde(default)]
    pub gepa_benchmark_pool: Vec<GepaBenchmarkWindow>,
```

Add defaults in `impl Default for AutoOptimizerConfig`:

```rust
            gepa_real_eval: false,
            gepa_real_eval_min_llm_score: default_gepa_real_eval_min_llm_score(),
            gepa_benchmark_pool: vec![],
```

- [ ] **Step 4: Add validation**

Inside `AutoOptimizerConfig::validate()`, after existing GEPA candidate/generation validation or near other numeric config checks, add:

```rust
        if !(0.0..=1.0).contains(&self.gepa_real_eval_min_llm_score) {
            bail!(
                "gepa_real_eval_min_llm_score must be between 0.0 and 1.0 inclusive; got {}",
                self.gepa_real_eval_min_llm_score
            );
        }

        if self.gepa_real_eval && self.gepa_benchmark_pool.is_empty() {
            bail!("gepa_benchmark_pool must contain at least one benchmark when gepa_real_eval=true");
        }

        validate_gepa_benchmark_pool(&self.gepa_benchmark_pool, self.effective_max_window_days())?;
```

Add helper close to `validate_regime_set`:

```rust
fn validate_gepa_benchmark_pool(pool: &[GepaBenchmarkWindow], max_window_days: i64) -> anyhow::Result<()> {
    let mut labels = std::collections::HashSet::new();
    for item in pool {
        if item.label.trim().is_empty() {
            bail!("gepa_benchmark_pool contains an entry with an empty label");
        }
        if !labels.insert(item.label.as_str()) {
            bail!("duplicate gepa_benchmark_pool label '{}' — labels must be unique", item.label);
        }
        if item.parent_strategy_id.trim().is_empty() {
            bail!("gepa_benchmark_pool '{}' must set parent_strategy_id", item.label);
        }
        let day_span = (item.day.end - item.day.start).num_days();
        if day_span > max_window_days {
            bail!(
                "gepa_benchmark_pool '{}' day window spans {} days, exceeding the {} day cap",
                item.label,
                day_span,
                max_window_days
            );
        }
        let base_span = (item.baseline.end - item.baseline.start).num_days();
        if base_span > max_window_days {
            bail!(
                "gepa_benchmark_pool '{}' baseline window spans {} days, exceeding the {} day cap",
                item.label,
                base_span,
                max_window_days
            );
        }
        if item.day.end > item.baseline.start {
            bail!(
                "gepa_benchmark_pool '{}' day window overlaps baseline window",
                item.label
            );
        }
    }
    Ok(())
}
```

If `effective_max_window_days()` is not an existing method, use the local logic already present in `validate()` for `max_window_days.unwrap_or(MAX_WINDOW_DAYS)`; do not add a public method unless it simplifies existing code.

- [ ] **Step 5: Run config tests to verify GREEN**

Run:

```bash
cargo test -p xvision-engine autooptimizer::config::tests::gepa_
```

Expected: all new `gepa_` config tests pass.

- [ ] **Step 6: Commit**

```bash
git add crates/xvision-engine/src/autooptimizer/config.rs
git commit -m "feat(optimizer): configure real GEPA evaluation"
```

---

### Task 2: Add real-eval score math and cache helpers

**Files:**
- Create: `crates/xvision-engine/src/autooptimizer/gepa_eval.rs`
- Modify: `crates/xvision-engine/src/autooptimizer/mod.rs`

**Interfaces:**
- Produces: `pub fn normalized_delta_score(parent_metric: f64, child_metric: f64) -> f64`
- Produces: `pub fn benchmark_pool_fingerprint(pool: &[GepaBenchmarkWindow]) -> String`
- Produces: `pub fn real_eval_cache_key(namespace: &str, instruction: &str, pool: &[GepaBenchmarkWindow]) -> String`
- Produces: `pub struct RealEvalCache`
- Consumes: `GepaBenchmarkWindow` from Task 1.

- [ ] **Step 1: Write failing tests**

Create `gepa_eval.rs` with tests first:

```rust
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
```

- [ ] **Step 2: Register module and run RED**

Add to `crates/xvision-engine/src/autooptimizer/mod.rs`:

```rust
pub mod gepa_eval;
```

Run:

```bash
cargo test -p xvision-engine autooptimizer::gepa_eval::tests::
```

Expected: compile fails because functions and `RealEvalCache` do not exist.

- [ ] **Step 3: Implement helpers**

Add implementation in `gepa_eval.rs`:

```rust
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
```

- [ ] **Step 4: Run helper tests to verify GREEN**

Run:

```bash
cargo test -p xvision-engine autooptimizer::gepa_eval::tests::
```

Expected: helper tests pass.

- [ ] **Step 5: Commit**

```bash
git add crates/xvision-engine/src/autooptimizer/gepa_eval.rs crates/xvision-engine/src/autooptimizer/mod.rs
git commit -m "feat(optimizer): add real GEPA score helpers"
```

---

### Task 3: Add scorer seam to `GepaBridge`

**Files:**
- Modify: `crates/xvision-engine/src/autooptimizer/gepa.rs:26-49`
- Modify: `crates/xvision-engine/src/autooptimizer/gepa.rs:94-313`
- Modify: `crates/xvision-engine/src/autooptimizer/gepa.rs:374-443`
- Modify: `crates/xvision-engine/src/autooptimizer/gepa.rs:477-548`

**Interfaces:**
- Produces: `pub struct RealEvalOptions { enabled: bool, min_fast_score: f64, benchmark_pool: Vec<GepaBenchmarkWindow>, cache: RealEvalCache }`
- Produces: `GepaBridge.real_eval: Option<RealEvalOptions>`
- Produces: internal method `score_candidate(...)` that returns fast-score skip or real-score result.
- Consumes: `normalized_delta_score`, `real_eval_cache_key`, and `RealEvalCache` from Task 2.

- [ ] **Step 1: Write failing routing tests**

Add a test-only fake real scorer inside `gepa.rs` tests by keeping the production seam small and injectable. If the implementation chooses not to expose a trait, add `#[cfg(test)] real_eval_scores: Option<Vec<f64>>` only if it does not leak into non-test code. Preferred production-compatible shape:

```rust
#[async_trait]
trait CandidateRealEvaluator: Send + Sync {
    async fn score_candidate(&self, namespace: &str, instruction: &str) -> anyhow::Result<(f64, String)>;
}
```

Then write tests:

```rust
#[tokio::test]
async fn real_eval_skips_candidate_below_fast_threshold() {
    let responses = vec![
        "reflection".to_string(),
        "low fast candidate".to_string(),
        r#"{"results":[{"score":0.10,"why":"weak"},{"score":0.10,"why":"weak"}]}"#.to_string(),
    ];
    let mut gepa = mock_gepa(responses);
    gepa.candidates = 1;
    gepa.real_eval = Some(RealEvalOptions::test_with_scores(0.30, vec![0.99]));

    let result = gepa
        .compile(
            "ns",
            &[("a".into(), "obs a".into()), ("b".into(), "obs b".into())],
            None,
        )
        .await
        .unwrap();

    assert!(result.instruction.is_empty(), "low fast score should not win from real eval");
    assert!(result.demos.iter().all(|d| d.score.unwrap_or(0.0) < 0.30));
}

#[tokio::test]
async fn real_eval_score_can_change_candidate_selection() {
    let responses = vec![
        "reflection".to_string(),
        "candidate with high llm but poor real result".to_string(),
        r#"{"results":[{"score":0.90,"why":"sounds good"},{"score":0.90,"why":"sounds good"}]}"#.to_string(),
        "candidate with lower llm but better real result".to_string(),
        r#"{"results":[{"score":0.80,"why":"still plausible"},{"score":0.80,"why":"still plausible"}]}"#.to_string(),
    ];
    let mut gepa = mock_gepa(responses);
    gepa.real_eval = Some(RealEvalOptions::test_with_scores(0.30, vec![0.20, 0.95]));

    let result = gepa
        .compile(
            "ns",
            &[("a".into(), "obs a".into()), ("b".into(), "obs b".into())],
            None,
        )
        .await
        .unwrap();

    assert!(result.instruction.contains("better real result"));
    assert_eq!(result.demos[0].score, Some(0.95));
    assert_eq!(result.demos[1].score, Some(0.95));
}
```

Use a helper such as `RealEvalOptions::test_with_scores` under `#[cfg(test)]` to avoid building real backtests in these tests.

- [ ] **Step 2: Run tests to verify RED**

Run:

```bash
cargo test -p xvision-engine autooptimizer::gepa::tests::real_eval_
```

Expected: compile fails because `real_eval`, `RealEvalOptions`, and test helpers do not exist.

- [ ] **Step 3: Add options and bridge fields**

At the top of `gepa.rs`, import config/cache helpers:

```rust
use crate::autooptimizer::config::GepaBenchmarkWindow;
use crate::autooptimizer::gepa_eval::{real_eval_cache_key, RealEvalCache};
```

Add structs before `GepaBridge`:

```rust
#[derive(Clone)]
pub struct RealEvalOptions {
    pub min_fast_score: f64,
    pub benchmark_pool: Vec<GepaBenchmarkWindow>,
    pub cache: RealEvalCache,
    #[cfg(test)]
    test_scores: Option<std::sync::Arc<std::sync::Mutex<Vec<f64>>>>,
}

impl RealEvalOptions {
    pub fn new(min_fast_score: f64, benchmark_pool: Vec<GepaBenchmarkWindow>) -> Self {
        Self {
            min_fast_score,
            benchmark_pool,
            cache: RealEvalCache::default(),
            #[cfg(test)]
            test_scores: None,
        }
    }

    #[cfg(test)]
    fn test_with_scores(min_fast_score: f64, scores: Vec<f64>) -> Self {
        Self {
            min_fast_score,
            benchmark_pool: vec![],
            cache: RealEvalCache::default(),
            test_scores: Some(std::sync::Arc::new(std::sync::Mutex::new(scores))),
        }
    }
}
```

Add field to `GepaBridge`:

```rust
    /// Optional real benchmark evaluator. When present, LLM scores act as the
    /// cheap first-pass cull and surviving candidates receive real scores.
    pub real_eval: Option<RealEvalOptions>,
```

Update all constructors/tests in `gepa.rs`, CLI, and dashboard later to include `real_eval: None` initially.

- [ ] **Step 4: Add routing method**

Add method inside `impl GepaBridge`:

```rust
    async fn score_candidate(
        &self,
        namespace: &str,
        instruction: &str,
        observations: &[(String, String)],
        indices: &[usize],
        provenance: &mut Provenance,
    ) -> anyhow::Result<ScoreWithFeedback> {
        let fast = self.score_on_indices(instruction, observations, indices, provenance).await?;
        let Some(real_eval) = &self.real_eval else {
            return Ok(fast);
        };

        let fast_mean = fast.mean_on_indices(indices);
        if fast_mean < real_eval.min_fast_score {
            let mut skipped = fast.clone();
            for &idx in indices {
                if let Some(feedback) = skipped.feedback.get_mut(idx) {
                    *feedback = format!(
                        "Skipped real eval: fast LLM score {:.2} below {:.2} threshold.",
                        fast_mean, real_eval.min_fast_score
                    );
                }
            }
            return Ok(skipped);
        }

        let cache_key = real_eval_cache_key(namespace, instruction, &real_eval.benchmark_pool);
        if let Some(hit) = real_eval.cache.get(&cache_key) {
            return Ok(ScoreWithFeedback::constant(observations.len(), indices, hit.score, hit.feedback));
        }

        let (score, feedback) = self.real_eval_score_candidate(namespace, instruction, real_eval).await?;
        real_eval.cache.insert(cache_key, score, feedback.clone());
        Ok(ScoreWithFeedback::constant(observations.len(), indices, score, feedback))
    }
```

Add helper to `ScoreWithFeedback`:

```rust
    fn constant(total_len: usize, indices: &[usize], score: f64, feedback: String) -> Self {
        let mut scores = vec![0.0; total_len];
        let mut feedbacks = vec![String::new(); total_len];
        for &idx in indices {
            scores[idx] = score;
            feedbacks[idx] = feedback.clone();
        }
        Self { scores, feedback: feedbacks }
    }
```

Add temporary real-eval method with test support and a non-test fallback that returns a clear error until Task 4 supplies the real evaluator:

```rust
    async fn real_eval_score_candidate(
        &self,
        _namespace: &str,
        instruction: &str,
        real_eval: &RealEvalOptions,
    ) -> anyhow::Result<(f64, String)> {
        #[cfg(test)]
        if let Some(scores) = &real_eval.test_scores {
            let score = scores
                .lock()
                .expect("test scores poisoned")
                .remove(0)
                .clamp(0.0, 1.0);
            return Ok((score, format!("Test real eval score {score:.2} for {instruction}")));
        }

        anyhow::bail!("gepa_real_eval scorer is not wired")
    }
```

- [ ] **Step 5: Replace score calls in compile**

In `compile`, change both candidate scoring call sites from:

```rust
self.score_on_indices(&instruction, observations, &mb_indices, &mut provenance).await?
```

and

```rust
self.score_on_indices(&instruction, observations, &active_indices, &mut provenance).await?
```

to:

```rust
self.score_candidate(_namespace, &instruction, observations, &mb_indices, &mut provenance).await?
```

and:

```rust
self.score_candidate(_namespace, &instruction, observations, &active_indices, &mut provenance).await?
```

Rename the `_namespace` argument to `namespace` at the method signature, then use `namespace` in call sites.

Keep merge scoring LLM-only for this task unless the new tests specifically cover merge; real eval for merge candidates can be added in a later refactor once base candidate scoring is correct.

- [ ] **Step 6: Run GEPA routing tests to verify GREEN**

Run:

```bash
cargo test -p xvision-engine autooptimizer::gepa::tests::real_eval_
```

Expected: both new GEPA routing tests pass.

- [ ] **Step 7: Run existing GEPA test**

Run:

```bash
cargo test -p xvision-engine autooptimizer::gepa::tests::single_generation_picks_best_candidate
```

Expected: existing LLM-only behavior still passes with `real_eval: None`.

- [ ] **Step 8: Commit**

```bash
git add crates/xvision-engine/src/autooptimizer/gepa.rs
git commit -m "feat(optimizer): route GEPA through real-eval scorer seam"
```

---

### Task 4: Implement deterministic real evaluator boundary

**Files:**
- Modify: `crates/xvision-engine/src/autooptimizer/gepa_eval.rs`
- Modify: `crates/xvision-engine/src/autooptimizer/gepa.rs`

**Interfaces:**
- Produces: `#[async_trait] pub trait BenchmarkEvaluator`
- Produces: `pub struct RealEvalOutcome { pub parent_sharpe: f64, pub child_sharpe: f64, pub label: String }`
- Produces: production `RealEvalOptions.evaluator: Option<Arc<dyn BenchmarkEvaluator>>`
- Consumes: `normalized_delta_score` from Task 2.

- [ ] **Step 1: Write failing evaluator tests**

In `gepa_eval.rs` tests, add:

```rust
#[tokio::test]
async fn benchmark_scores_average_across_windows() {
    let evaluator = FakeBenchmarkEvaluator::new(vec![
        RealEvalOutcome { label: "bull".into(), parent_sharpe: 1.0, child_sharpe: 1.5 },
        RealEvalOutcome { label: "bear".into(), parent_sharpe: 1.0, child_sharpe: 0.5 },
    ]);
    let pool = vec![bench("bull"), bench("bear")];
    let scored = score_real_eval_candidate(&evaluator, "candidate instruction", &pool)
        .await
        .unwrap();
    assert_eq!(scored.score, 0.5, "one +0.5 normalized and one -0.5 normalized average to neutral");
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
```

- [ ] **Step 2: Run evaluator tests to verify RED**

Run:

```bash
cargo test -p xvision-engine autooptimizer::gepa_eval::tests::benchmark_
```

Expected: compile fails because `BenchmarkEvaluator`, `RealEvalOutcome`, `score_real_eval_candidate`, and fake helpers do not exist.

- [ ] **Step 3: Implement evaluator trait and score function**

In `gepa_eval.rs`:

```rust
use async_trait::async_trait;

#[derive(Debug, Clone)]
pub struct RealEvalOutcome {
    pub label: String,
    pub parent_sharpe: f64,
    pub child_sharpe: f64,
}

#[derive(Debug, Clone)]
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
```

In tests, implement `FakeBenchmarkEvaluator` inside the test module.

- [ ] **Step 4: Wire evaluator into `GepaBridge`**

Update `RealEvalOptions`:

```rust
use crate::autooptimizer::gepa_eval::{score_real_eval_candidate, BenchmarkEvaluator};

pub struct RealEvalOptions {
    pub min_fast_score: f64,
    pub benchmark_pool: Vec<GepaBenchmarkWindow>,
    pub cache: RealEvalCache,
    pub evaluator: Option<Arc<dyn BenchmarkEvaluator>>,
    #[cfg(test)]
    test_scores: Option<Arc<Mutex<Vec<f64>>>>,
}
```

Update `RealEvalOptions::new` to accept an evaluator:

```rust
pub fn new(
    min_fast_score: f64,
    benchmark_pool: Vec<GepaBenchmarkWindow>,
    evaluator: Option<Arc<dyn BenchmarkEvaluator>>,
) -> Self { ... }
```

Update `real_eval_score_candidate`:

```rust
        if let Some(evaluator) = &real_eval.evaluator {
            let scored = score_real_eval_candidate(evaluator.as_ref(), instruction, &real_eval.benchmark_pool).await?;
            return Ok((scored.score, scored.feedback));
        }

        anyhow::bail!("gepa_real_eval=true but no benchmark evaluator is configured")
```

Keep the `#[cfg(test)] test_scores` early return so GEPA routing unit tests stay simple.

- [ ] **Step 5: Run evaluator and GEPA tests**

Run:

```bash
cargo test -p xvision-engine autooptimizer::gepa_eval::tests::benchmark_ && cargo test -p xvision-engine autooptimizer::gepa::tests::real_eval_
```

Expected: all tests pass.

- [ ] **Step 6: Commit**

```bash
git add crates/xvision-engine/src/autooptimizer/gepa_eval.rs crates/xvision-engine/src/autooptimizer/gepa.rs
git commit -m "feat(optimizer): score GEPA candidates with benchmark evaluator"
```

---

### Task 5: Wire config into CLI/dashboard GEPA construction without enabling production backtests yet

**Files:**
- Modify: `crates/xvision-cli/src/commands/optimize.rs:1047-1065`
- Modify: `crates/xvision-dashboard/src/routes/autooptimizer_cycle.rs:439-456`
- Modify: `crates/xvision-engine/src/autooptimizer/gepa.rs`

**Interfaces:**
- Consumes: `cfg.gepa_real_eval`, `cfg.gepa_real_eval_min_llm_score`, `cfg.gepa_benchmark_pool`
- Produces: `GepaBridge.real_eval` set to `Some(RealEvalOptions::new(..., None))` only when config enables it.

- [ ] **Step 1: Write failing construction test in `gepa.rs`**

Add a pure unit test that verifies constructor intent through a helper function. First add the test expecting a helper:

```rust
#[test]
fn real_eval_options_from_config_respects_disabled_default() {
    let cfg = crate::autooptimizer::config::AutoOptimizerConfig::default();
    assert!(real_eval_options_from_config(&cfg).is_none());
}
```

Add another test:

```rust
#[test]
fn real_eval_options_from_config_carries_threshold_and_pool() {
    use chrono::NaiveDate;
    use crate::autooptimizer::config::{AutoOptimizerConfig, BaselineUntouchedWindow, DayWindow, GepaBenchmarkWindow};

    let mut cfg = AutoOptimizerConfig::default();
    cfg.gepa_real_eval = true;
    cfg.gepa_real_eval_min_llm_score = 0.42;
    cfg.gepa_benchmark_pool = vec![GepaBenchmarkWindow {
        label: "bench-a".into(),
        parent_strategy_id: "parent-a".into(),
        day: DayWindow {
            start: NaiveDate::from_ymd_opt(2025, 1, 1).unwrap(),
            end: NaiveDate::from_ymd_opt(2025, 1, 15).unwrap(),
        },
        baseline: BaselineUntouchedWindow {
            start: NaiveDate::from_ymd_opt(2025, 1, 15).unwrap(),
            end: NaiveDate::from_ymd_opt(2025, 2, 1).unwrap(),
        },
    }];

    let options = real_eval_options_from_config(&cfg).expect("enabled real eval options");
    assert_eq!(options.min_fast_score, 0.42);
    assert_eq!(options.benchmark_pool.len(), 1);
    assert!(options.evaluator.is_none(), "production evaluator is wired in a later task");
}
```

- [ ] **Step 2: Run construction tests to verify RED**

Run:

```bash
cargo test -p xvision-engine autooptimizer::gepa::tests::real_eval_options_from_config_
```

Expected: compile fails because `real_eval_options_from_config` does not exist.

- [ ] **Step 3: Implement helper**

In `gepa.rs`, add:

```rust
pub fn real_eval_options_from_config(
    cfg: &crate::autooptimizer::config::AutoOptimizerConfig,
) -> Option<RealEvalOptions> {
    cfg.gepa_real_eval.then(|| {
        RealEvalOptions::new(
            cfg.gepa_real_eval_min_llm_score,
            cfg.gepa_benchmark_pool.clone(),
            None,
        )
    })
}
```

This helper deliberately leaves `evaluator=None` until the production benchmark evaluator is wired. Config validation still prevents accidental enablement without a benchmark pool; if enabled before evaluator wiring, compile returns a clear error instead of silently falling back to LLM scoring.

- [ ] **Step 4: Wire CLI/dashboard bridge construction**

In `crates/xvision-cli/src/commands/optimize.rs`, add field in `GepaBridge { ... }`:

```rust
                real_eval: xvision_engine::autooptimizer::gepa::real_eval_options_from_config(&cfg),
```

In `crates/xvision-dashboard/src/routes/autooptimizer_cycle.rs`, add the same field:

```rust
                    real_eval: xvision_engine::autooptimizer::gepa::real_eval_options_from_config(&cfg),
```

Also update `mock_gepa` test helper in `gepa.rs` and any other `GepaBridge` literals to include `real_eval: None`.

- [ ] **Step 5: Run targeted compile tests**

Run:

```bash
cargo test -p xvision-engine autooptimizer::gepa::tests::real_eval_options_from_config_ && cargo test -p xvision-engine autooptimizer::gepa::tests::single_generation_picks_best_candidate
```

Expected: tests pass.

- [ ] **Step 6: Run CLI/dashboard type checks via existing tests where available**

Run:

```bash
cargo test -p xvision-engine --lib autooptimizer::gepa
```

Expected: GEPA lib tests pass. If CLI/dashboard crates fail to compile due missing new `GepaBridge` field, add the field exactly at their `GepaBridge` literals and rerun.

- [ ] **Step 7: Commit**

```bash
git add crates/xvision-engine/src/autooptimizer/gepa.rs crates/xvision-cli/src/commands/optimize.rs crates/xvision-dashboard/src/routes/autooptimizer_cycle.rs
git commit -m "feat(optimizer): wire real GEPA eval config"
```

---

### Task 6: Final verification and dependency boundary check

**Files:**
- No production file changes expected unless verification exposes a targeted issue.

**Interfaces:**
- Consumes all prior tasks.
- Produces verified branch ready for review/PR.

- [ ] **Step 1: Format**

Run:

```bash
cargo fmt
```

Expected: no output, exit 0.

- [ ] **Step 2: Run targeted tests**

Run:

```bash
cargo test -p xvision-engine autooptimizer::config::tests::gepa_ \
  && cargo test -p xvision-engine autooptimizer::gepa_eval::tests:: \
  && cargo test -p xvision-engine autooptimizer::gepa::tests::real_eval_ \
  && cargo test -p xvision-engine autooptimizer::gepa::tests::single_generation_picks_best_candidate
```

Expected: all targeted tests pass.

- [ ] **Step 3: Run dependency boundary checks**

Run:

```bash
cargo tree -p xvision-engine | grep -iE "dspy-rs|xvision-dspy|rig-core" || true
```

Expected: no matching dependency lines. If matches appear, remove the dependency leak before proceeding.

Run:

```bash
cargo tree -p xvision-dashboard | grep -iE "dspy-rs|xvision-dspy|rig-core" || true
```

Expected: no matching dependency lines.

- [ ] **Step 4: Inspect git status**

Run:

```bash
git status --short
```

Expected: only intentional tracked changes are present. Untracked local Zenith installation artifacts may remain and must not be staged.

- [ ] **Step 5: Commit any final fixes**

If formatting or verification required edits:

```bash
git add <intentional-files>
git commit -m "fix(optimizer): stabilize real GEPA evaluation"
```

If no edits were needed, do not create an empty commit.
