use std::sync::Arc;

use async_trait::async_trait;

use crate::agent::llm::{LlmDispatch, LlmRequest, Message};
use crate::autooptimizer::config::GepaBenchmarkWindow;
use crate::autooptimizer::dspy_bridge::{CompileResult, DspyBridge};
use crate::autooptimizer::gepa_eval::{
    real_eval_cache_key, score_real_eval_candidate, BenchmarkEvaluator, RealEvalCache,
};
use crate::autooptimizer::pattern_snapshot::{Provenance, SnapshotDemo};

const SYSTEM_PROMPT: &str = "You are an instruction optimizer for an automated trading-strategy \
    experiment writer. Your job is to analyze findings from recent optimization cycles and produce \
    concise instruction prefixes that steer the experiment writer toward what works and away from \
    what does not.";

/// GEPA (Genetic-Pareto Evolutionary) bridge with Pareto frontier selection,
/// minibatch → full eval two-tier pipeline, text feedback scoring, merge/crossover,
/// separate reflection model, and skip-perfect-scores optimization.
///
/// Algorithm per generation:
///   1. REFLECT — uses reflection_dispatch (stronger, higher-temp model)
///   2. PROPOSE — one LLM call per candidate with mutator dispatch
///   3. SCORE  — minibatch tier first; only promote to full pool if minibatch improves
///   4. MERGE  — every merge_frequency gens, crossover two best candidates
///
/// Pareto frontier selection picks the candidate that dominates the most local
/// optima (per-observation best scores), avoiding greedy collapse to a single mode.
#[derive(Clone)]
pub struct RealEvalOptions {
    pub min_fast_score: f64,
    pub benchmark_pool: Vec<GepaBenchmarkWindow>,
    pub cache: RealEvalCache,
    pub evaluator: Arc<dyn BenchmarkEvaluator>,
}

impl RealEvalOptions {
    pub fn new(
        min_fast_score: f64,
        benchmark_pool: Vec<GepaBenchmarkWindow>,
        evaluator: Arc<dyn BenchmarkEvaluator>,
    ) -> Self {
        Self {
            min_fast_score,
            benchmark_pool,
            cache: RealEvalCache::default(),
            evaluator,
        }
    }
}

pub fn real_eval_options_from_config(
    cfg: &crate::autooptimizer::config::AutoOptimizerConfig,
) -> anyhow::Result<Option<RealEvalOptions>> {
    if cfg.gepa_real_eval {
        anyhow::bail!(
            "gepa_real_eval=true is not available until a production benchmark evaluator is wired; \
             set gepa_real_eval=false to use the LLM scorer"
        );
    }
    Ok(None)
}

pub struct GepaBridge {
    pub dispatch: Arc<dyn LlmDispatch + Send + Sync>,
    pub model: String,
    pub provider: String,
    pub candidates: usize,
    pub generations: usize,

    // Phase 4 additions
    /// Separate dispatch for REFLECT — should be a stronger, higher-temperature model.
    /// Falls back to `self.dispatch` when `None`.
    pub reflection_dispatch: Option<Arc<dyn LlmDispatch + Send + Sync>>,
    pub reflection_model: Option<String>,
    /// Pareto frontier selection (`"pareto"`) or greedy best (`"current_best"`).
    pub selection_strategy: GepaSelectionStrategy,
    /// Minibatch size for cheap first-pass scoring. Candidates below the minibatch
    /// mean don't proceed to full eval.
    pub reflection_minibatch_size: usize,
    /// Skip observations scored 1.0 (perfect) from reflection and scoring prompts.
    /// They provide no improvement signal.
    pub skip_perfect: bool,
    /// Enable merge/crossover: every N generations, combine two best candidates.
    pub use_merge: bool,
    pub merge_frequency: usize,
    /// Optional real benchmark evaluator. When present, LLM scores act as the
    /// cheap first-pass cull and surviving candidates receive real scores.
    pub real_eval: Option<RealEvalOptions>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GepaSelectionStrategy {
    Pareto,
    CurrentBest,
}

impl Default for GepaSelectionStrategy {
    fn default() -> Self {
        Self::Pareto
    }
}

/// Score + text feedback per observation. The `why` strings explain
/// why the instruction does or doesn't address each observation, enriching
/// the next generation's REFLECT with qualitative signal.
#[derive(Debug, Clone)]
struct ScoreWithFeedback {
    scores: Vec<f64>,
    feedback: Vec<String>,
    real_eval_skipped: bool,
}

// ── ScoreWithFeedback helpers ──────────────────────────────────────────
impl ScoreWithFeedback {
    fn mean_on_indices(&self, indices: &[usize]) -> f64 {
        if indices.is_empty() {
            return 0.0;
        }
        let sum: f64 = indices
            .iter()
            .map(|&i| self.scores.get(i).copied().unwrap_or(0.0))
            .sum();
        sum / indices.len() as f64
    }

    fn constant(total_len: usize, indices: &[usize], score: f64, feedback: String) -> Self {
        let mut scores = vec![0.0; total_len];
        let mut feedbacks = vec![String::new(); total_len];
        for &idx in indices {
            scores[idx] = score;
            feedbacks[idx] = feedback.clone();
        }
        Self {
            scores,
            feedback: feedbacks,
            real_eval_skipped: false,
        }
    }

    fn merge_inactive_from(
        mut self,
        active_indices: &[usize],
        prior_scores: &[f64],
        prior_feedback: &[String],
    ) -> Self {
        if prior_scores.is_empty() {
            return self;
        }

        let mut active = vec![false; self.scores.len()];
        for &idx in active_indices {
            if let Some(slot) = active.get_mut(idx) {
                *slot = true;
            }
        }

        for idx in 0..self.scores.len() {
            if active[idx] {
                continue;
            }
            if let Some(score) = prior_scores.get(idx) {
                self.scores[idx] = *score;
            }
            if let Some(feedback) = prior_feedback.get(idx) {
                self.feedback[idx] = feedback.clone();
            }
        }

        self
    }
}

fn best_scores_mean_on_indices(scores: &[f64], indices: &[usize]) -> f64 {
    if scores.is_empty() {
        f64::NEG_INFINITY
    } else if indices.is_empty() {
        0.0
    } else {
        indices
            .iter()
            .map(|&i| scores.get(i).copied().unwrap_or(0.0))
            .sum::<f64>()
            / indices.len() as f64
    }
}

#[async_trait]
impl DspyBridge for GepaBridge {
    async fn compile(
        &self,
        namespace: &str,
        observations: &[(String, String)],
        base_instruction: Option<&str>,
    ) -> anyhow::Result<CompileResult> {
        if observations.is_empty() {
            return Ok(CompileResult::empty("gepa"));
        }

        let obs_list = observations
            .iter()
            .enumerate()
            .map(|(i, (_id, t))| format!("{}. {}", i + 1, t.trim()))
            .collect::<Vec<_>>()
            .join("\n");

        let mut provenance = Provenance::new(&self.provider, &self.model);
        let mut best_instruction = String::new();
        let mut best_scores: Vec<f64> = vec![];
        let mut best_feedback: Vec<String> = vec![];
        // Warm-start: seed with the current agent system prompt so the optimizer
        // improves FROM the real prompt rather than generating from scratch.
        let seed_prefix = base_instruction
            .filter(|b| !b.is_empty())
            .unwrap_or("")
            .to_string();

        let n_gens = self.generations.max(1);
        let n_candidates = self.candidates.max(1);

        for gen in 0..n_gens {
            // ── 4a: Skip perfect observations (score == 1.0) ─────────
            let active_indices: Vec<usize> = if self.skip_perfect && !best_scores.is_empty() {
                (0..observations.len())
                    .filter(|&i| best_scores.get(i).copied().unwrap_or(0.0) < 1.0)
                    .collect()
            } else {
                (0..observations.len()).collect()
            };
            if active_indices.is_empty() {
                break; // all observations perfect — stop
            }

            // Build the observation list for this generation (skip perfects).
            let _gen_obs_text = active_indices
                .iter()
                .enumerate()
                .map(|(j, &orig_i)| format!("{}. {}", j + 1, observations[orig_i].1.trim()))
                .collect::<Vec<_>>()
                .join("\n");

            // ── 4f: Incorporate prior-gen feedback strings into reflection ──
            let feedback_hint = if !best_feedback.is_empty() {
                let fb = best_feedback
                    .iter()
                    .enumerate()
                    .take(5)
                    .map(|(i, f)| format!("  Obs {}: {}", i + 1, f))
                    .collect::<Vec<_>>()
                    .join("\n");
                format!("\n\nKey insights from prior scoring:\n{fb}\n")
            } else {
                String::new()
            };

            // ── Stage 1: REFLECT (uses reflection dispatch if available) ──
            let reflection_text = if !seed_prefix.is_empty() {
                format!("{obs_list}\n\nCurrent agent system prompt (improve FROM this, do not discard):\n{seed_prefix}{feedback_hint}")
            } else {
                format!("{obs_list}{feedback_hint}")
            };
            let reflection = self.reflect(&reflection_text, &mut provenance).await?;
            let mb_size = self.reflection_minibatch_size.min(active_indices.len()).max(2);

            // Minibatch indices: first `mb_size` active indices (for cheap cull)
            let mb_indices: Vec<usize> = active_indices.iter().copied().take(mb_size).collect();

            let mut scored_candidates: Vec<(String, ScoreWithFeedback)> = Vec::with_capacity(n_candidates);

            // Track per-observation best scores for Pareto frontier (4a).
            let mut per_obs_best: Vec<f64> = vec![f64::NEG_INFINITY; observations.len()];
            let mut frontier_candidates: Vec<(String, ScoreWithFeedback, /* frontier_count */ usize)> =
                Vec::with_capacity(n_candidates);

            for i in 0..n_candidates {
                let instruction = self
                    .propose(&reflection, i, n_candidates, &mut provenance)
                    .await?;

                // Tier 1: minibatch scoring (cheap)
                let mb_scores = self
                    .score_candidate(
                        namespace,
                        &instruction,
                        observations,
                        &mb_indices,
                        &mut provenance,
                    )
                    .await?;
                let mb_mean = mb_scores.mean_on_indices(&mb_indices);
                if mb_scores.real_eval_skipped {
                    continue;
                }
                if mb_mean <= best_scores_mean_on_indices(&best_scores, &active_indices) && gen > 0 {
                    continue; // early cull: minibatch doesn't beat current best
                }

                // Tier 2: full scoring on active indices. If the minibatch already
                // covers the full active set, reuse it so real-eval tests and runs
                // don't issue a duplicate fast-score call for the same candidate.
                let full = if mb_indices == active_indices {
                    mb_scores
                } else {
                    self.score_candidate(
                        namespace,
                        &instruction,
                        observations,
                        &active_indices,
                        &mut provenance,
                    )
                    .await?
                };
                if full.real_eval_skipped {
                    continue;
                }
                let full = full.merge_inactive_from(&active_indices, &best_scores, &best_feedback);

                // Track per-observation best for Pareto frontier.
                for &orig_i in &active_indices {
                    let s = full.scores.get(orig_i).copied().unwrap_or(0.0);
                    if s > per_obs_best[orig_i] {
                        per_obs_best[orig_i] = s;
                    }
                }

                scored_candidates.push((instruction, full));
            }

            // ── 4a: Pareto frontier selection ──
            // Count how many observations each successfully full-scored candidate
            // is tied for best on after the generation's per-observation bests are
            // finalized. Merge/crossover needs the full frontier, not just the
            // single generation winner.
            for (instruction, scores) in scored_candidates {
                let mut frontier_count = 0usize;
                for &orig_i in &active_indices {
                    let s = scores.scores.get(orig_i).copied().unwrap_or(0.0);
                    if (s - per_obs_best[orig_i]).abs() < 1e-6 {
                        frontier_count += 1;
                    }
                }
                frontier_candidates.push((instruction, scores, frontier_count));
            }

            // Pareto-weighted selection: candidate with highest frontier count wins.
            // Falls back to max-mean if frontier counts are all equal.
            let winner = match self.selection_strategy {
                GepaSelectionStrategy::Pareto => frontier_candidates.iter().max_by(|a, b| {
                    a.2.cmp(&b.2).then_with(|| {
                        a.1.mean_on_indices(&active_indices)
                            .partial_cmp(&b.1.mean_on_indices(&active_indices))
                            .unwrap()
                    })
                }),
                GepaSelectionStrategy::CurrentBest => frontier_candidates.iter().max_by(|a, b| {
                    a.1.mean_on_indices(&active_indices)
                        .partial_cmp(&b.1.mean_on_indices(&active_indices))
                        .unwrap()
                }),
            };

            if let Some((instr, scores, _fc)) = winner {
                let mean = scores.mean_on_indices(&active_indices);
                if mean > best_scores_mean_on_indices(&best_scores, &active_indices) {
                    best_instruction = instr.clone();
                    best_scores = scores.scores.clone();
                    best_feedback = scores.feedback.clone();
                }
            }

            // ── 4e: Merge (crossover) every merge_frequency gens ──
            if self.use_merge
                && self.merge_frequency > 0
                && (gen + 1) % self.merge_frequency == 0
                && frontier_candidates.len() >= 2
            {
                // Pick two best candidates by frontier count.
                frontier_candidates.sort_by(|a, b| {
                    b.2.cmp(&a.2).then_with(|| {
                        a.1.mean_on_indices(&active_indices)
                            .partial_cmp(&b.1.mean_on_indices(&active_indices))
                            .unwrap()
                    })
                });
                let parent_a = &frontier_candidates[0].0;
                let parent_b = &frontier_candidates[1].0;
                if let Ok(merged) = self.merge(parent_a, parent_b, &mut provenance).await {
                    let mb_scores = self
                        .score_candidate(namespace, &merged, observations, &mb_indices, &mut provenance)
                        .await?;
                    let mb_mean = mb_scores.mean_on_indices(&mb_indices);
                    if !mb_scores.real_eval_skipped
                        && mb_mean > best_scores_mean_on_indices(&best_scores, &active_indices)
                    {
                        let full = if mb_indices == active_indices {
                            mb_scores
                        } else {
                            self.score_candidate(
                                namespace,
                                &merged,
                                observations,
                                &active_indices,
                                &mut provenance,
                            )
                            .await?
                        };
                        if full.real_eval_skipped {
                            continue;
                        }
                        let full = full.merge_inactive_from(&active_indices, &best_scores, &best_feedback);
                        let mean = full.mean_on_indices(&active_indices);
                        if mean > best_scores_mean_on_indices(&best_scores, &active_indices) {
                            best_instruction = merged;
                            best_scores = full.scores;
                            best_feedback = full.feedback;
                        }
                    }
                }
            }
        }

        let demos = observations
            .iter()
            .enumerate()
            .map(|(i, (id, text))| SnapshotDemo {
                observation_id: id.clone(),
                text: text.clone(),
                score: best_scores.get(i).copied(),
            })
            .collect();

        Ok(CompileResult {
            instruction: best_instruction,
            provenance,
            demos,
            optimizer_name: "gepa".to_string(),
            rng_seed: 0,
        })
    }
}

impl GepaBridge {
    /// REFLECT uses the reflection dispatch (or falls back to the main dispatch).
    async fn reflect(&self, obs_list: &str, provenance: &mut Provenance) -> anyhow::Result<String> {
        let dispatch = self.reflection_dispatch.as_ref().unwrap_or(&self.dispatch);
        let model = self.reflection_model.as_deref().unwrap_or(&self.model);

        let user_text = format!(
            "Here are observations from recent optimization cycles. Identify the 3 most important \
             patterns: what kinds of experiments succeeded, what failed, and why.\n\n\
             Observations:\n{obs_list}"
        );
        let req = LlmRequest {
            model: model.to_string(),
            system_prompt: SYSTEM_PROMPT.to_string(),
            messages: vec![Message::user_text(user_text)],
            max_tokens: None,
            tools: vec![],
            temperature: Some(1.0), // Phase 4c: higher temp for creative pattern recognition
            response_schema: None,
            cache_control: None,
            force_json: false,
        };
        let resp = dispatch.complete(req).await?;
        provenance.record_usage(resp.input_tokens, resp.output_tokens);
        Ok(resp.text().trim().to_string())
    }

    async fn propose(
        &self,
        reflection: &str,
        candidate_idx: usize,
        total: usize,
        provenance: &mut Provenance,
    ) -> anyhow::Result<String> {
        let user_text = format!(
            "Based on this analysis:\n{reflection}\n\n\
             Write a concise instruction prefix (2-4 sentences) for the experiment writer \
             that steers it toward the wins and away from the failures. \
             Candidate {} of {}: make this proposal distinct in emphasis from other candidates. \
             Output ONLY the instruction text, no preamble.",
            candidate_idx + 1,
            total
        );
        let req = LlmRequest {
            model: self.model.clone(),
            system_prompt: SYSTEM_PROMPT.to_string(),
            messages: vec![Message::user_text(user_text)],
            max_tokens: None,
            tools: vec![],
            temperature: Some(0.7),
            response_schema: None,
            cache_control: None,
            force_json: false,
        };
        let resp = self.dispatch.complete(req).await?;
        provenance.record_usage(resp.input_tokens, resp.output_tokens);
        Ok(resp.text().trim().to_string())
    }

    async fn score_candidate(
        &self,
        namespace: &str,
        instruction: &str,
        observations: &[(String, String)],
        indices: &[usize],
        provenance: &mut Provenance,
    ) -> anyhow::Result<ScoreWithFeedback> {
        let fast = self
            .score_on_indices(instruction, observations, indices, provenance)
            .await?;
        let Some(real_eval) = &self.real_eval else {
            return Ok(fast);
        };

        let fast_mean = fast.mean_on_indices(indices);
        if fast_mean < real_eval.min_fast_score {
            let mut skipped = fast.clone();
            skipped.real_eval_skipped = true;
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
            return Ok(ScoreWithFeedback::constant(
                observations.len(),
                indices,
                hit.score,
                hit.feedback,
            ));
        }

        let (score, feedback) = self
            .real_eval_score_candidate(namespace, instruction, real_eval)
            .await?;
        real_eval.cache.insert(cache_key, score, feedback.clone());
        Ok(ScoreWithFeedback::constant(
            observations.len(),
            indices,
            score,
            feedback,
        ))
    }

    async fn real_eval_score_candidate(
        &self,
        _namespace: &str,
        instruction: &str,
        real_eval: &RealEvalOptions,
    ) -> anyhow::Result<(f64, String)> {
        let scored = score_real_eval_candidate(
            real_eval.evaluator.as_ref(),
            instruction,
            &real_eval.benchmark_pool,
        )
        .await?;
        Ok((scored.score, scored.feedback))
    }

    /// Score an instruction against specific observation indices. Returns scores
    /// AND text feedback ("why" strings) per observation (Phase 4d).
    async fn score_on_indices(
        &self,
        instruction: &str,
        all_observations: &[(String, String)],
        indices: &[usize],
        provenance: &mut Provenance,
    ) -> anyhow::Result<ScoreWithFeedback> {
        if indices.is_empty() {
            return Ok(ScoreWithFeedback {
                scores: vec![],
                feedback: vec![],
                real_eval_skipped: false,
            });
        }

        let obs_text = indices
            .iter()
            .enumerate()
            .map(|(j, &orig_i)| format!("{}. {}", j + 1, all_observations[orig_i].1.trim()))
            .collect::<Vec<_>>()
            .join("\n");

        let n = indices.len();
        let user_text = format!(
            "Rate how well this instruction prefix addresses each observation (0.0–1.0). \
             Higher = the instruction would steer the experiment writer to avoid this failure \
             or repeat this success.\n\n\
             For each observation, provide a score AND one sentence explaining why.\n\n\
             Instruction:\n{instruction}\n\n\
             Observations:\n{obs_text}\n\n\
             Return JSON only: {{\"results\": [\
             {{\"score\": 0.8, \"why\": \"Directly addresses the ATR-overfitting pattern\"}},\
             ...\
             ]}} with exactly {n} entries."
        );
        let req = LlmRequest {
            model: self.model.clone(),
            system_prompt: SYSTEM_PROMPT.to_string(),
            messages: vec![Message::user_text(user_text)],
            max_tokens: None,
            tools: vec![],
            temperature: Some(0.0),
            response_schema: None,
            cache_control: None,
            force_json: true,
        };
        let resp = self.dispatch.complete(req).await?;
        provenance.record_usage(resp.input_tokens, resp.output_tokens);
        let text = resp.text();
        let parsed: serde_json::Value = serde_json::from_str(text.trim()).unwrap_or_default();

        let results = parsed["results"].as_array().cloned().unwrap_or_default();

        // Map indexed results back to full observation array positions.
        let mut full_scores = vec![0.5f64; all_observations.len()];
        let mut full_feedback = vec![String::new(); all_observations.len()];

        for (idx_pos, &orig_idx) in indices.iter().enumerate() {
            if let Some(entry) = results.get(idx_pos) {
                full_scores[orig_idx] = entry["score"].as_f64().unwrap_or(0.5).clamp(0.0, 1.0);
                full_feedback[orig_idx] = entry["why"].as_str().unwrap_or("").to_string();
            }
        }

        Ok(ScoreWithFeedback {
            scores: full_scores,
            feedback: full_feedback,
            real_eval_skipped: false,
        })
    }

    /// Merge two instruction candidates into one that captures both strengths
    /// (Phase 4e: crossover operation from GEPA's `use_merge`).
    async fn merge(
        &self,
        instruction_a: &str,
        instruction_b: &str,
        provenance: &mut Provenance,
    ) -> anyhow::Result<String> {
        let user_text = format!(
            "Combine these two instruction prefixes into one that captures both strengths. \
             Remove redundancy, keep the most actionable guidance.\n\n\
             Instruction A:\n{instruction_a}\n\n\
             Instruction B:\n{instruction_b}\n\n\
             Output ONLY the merged instruction, no preamble."
        );
        let req = LlmRequest {
            model: self.model.clone(),
            system_prompt: SYSTEM_PROMPT.to_string(),
            messages: vec![Message::user_text(user_text)],
            max_tokens: None,
            tools: vec![],
            temperature: Some(0.3),
            response_schema: None,
            cache_control: None,
            force_json: false,
        };
        let resp = self.dispatch.complete(req).await?;
        provenance.record_usage(resp.input_tokens, resp.output_tokens);
        Ok(resp.text().trim().to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{
        atomic::{AtomicUsize, Ordering},
        Arc, Mutex,
    };

    use chrono::NaiveDate;

    use crate::agent::llm::{ContentBlock, LlmResponse, MockDispatch, StopReason};
    use crate::autooptimizer::config::{BaselineUntouchedWindow, DayWindow, GepaBenchmarkWindow};
    use crate::autooptimizer::gepa_eval::{real_eval_cache_key, RealEvalOutcome};

    fn to_response(text: impl Into<String>) -> LlmResponse {
        LlmResponse {
            content: vec![ContentBlock::Text { text: text.into() }],
            stop_reason: StopReason::EndTurn,
            input_tokens: 1,
            output_tokens: 1,
        }
    }

    fn mock_gepa(responses: Vec<String>) -> GepaBridge {
        let canned: Vec<LlmResponse> = responses.into_iter().map(to_response).collect();
        GepaBridge {
            dispatch: Arc::new(MockDispatch::sequence(canned)),
            model: "test-model".into(),
            provider: "test-provider".into(),
            candidates: 2,
            generations: 1,
            reflection_dispatch: None,
            reflection_model: None,
            selection_strategy: GepaSelectionStrategy::CurrentBest,
            reflection_minibatch_size: 2,
            skip_perfect: false,
            use_merge: false,
            merge_frequency: 3,
            real_eval: None,
        }
    }

    fn benchmark(label: &str) -> GepaBenchmarkWindow {
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
    struct SequenceBenchmarkEvaluator {
        scores: Arc<Mutex<Vec<f64>>>,
        calls: Arc<AtomicUsize>,
    }

    impl SequenceBenchmarkEvaluator {
        fn new(scores: Vec<f64>) -> Self {
            Self {
                scores: Arc::new(Mutex::new(scores)),
                calls: Arc::new(AtomicUsize::new(0)),
            }
        }

        fn call_count(&self) -> usize {
            self.calls.load(Ordering::SeqCst)
        }
    }

    #[async_trait::async_trait]
    impl BenchmarkEvaluator for SequenceBenchmarkEvaluator {
        async fn evaluate(
            &self,
            _instruction: &str,
            benchmark: &GepaBenchmarkWindow,
        ) -> anyhow::Result<RealEvalOutcome> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            let score = self
                .scores
                .lock()
                .expect("benchmark scores poisoned")
                .remove(0)
                .clamp(0.0, 1.0);
            Ok(RealEvalOutcome {
                label: benchmark.label.clone(),
                parent_sharpe: 1.0,
                child_sharpe: 2.0 * score,
            })
        }
    }

    #[test]
    fn real_eval_options_from_config_respects_disabled_default() {
        let cfg = crate::autooptimizer::config::AutoOptimizerConfig::default();
        assert!(real_eval_options_from_config(&cfg).unwrap().is_none());
    }

    #[test]
    fn real_eval_options_from_config_rejects_enabled_without_production_evaluator() {
        let mut cfg = crate::autooptimizer::config::AutoOptimizerConfig::default();
        cfg.gepa_real_eval = true;

        let err = match real_eval_options_from_config(&cfg) {
            Ok(_) => panic!("enabled config must not produce real eval options without an evaluator"),
            Err(err) => err,
        };
        let msg = err.to_string();
        assert!(
            msg.contains("gepa_real_eval=true")
                && msg.contains("production benchmark evaluator")
                && msg.contains("gepa_real_eval=false"),
            "enabled config must fail before constructing unusable real eval options; got: {msg}"
        );
    }

    #[tokio::test]
    async fn empty_observations_returns_empty_result() {
        let gepa = mock_gepa(vec![]);
        let result = gepa.compile("ns", &[], None).await.unwrap();
        assert!(result.instruction.is_empty());
        assert_eq!(result.demos.len(), 0);
    }

    #[tokio::test]
    async fn single_generation_picks_best_candidate() {
        // Mock responses:
        // 1. REFLECT
        // 2. PROPOSE candidate 0
        // 3. SCORE candidate 0
        // 4. PROPOSE candidate 1
        // 5. SCORE candidate 1
        let responses = vec![
            "reflection: favor wider stops".to_string(), // reflect
            "Use wider ATR-based stops".to_string(),     // propose 0
            r#"{"results":[{"score":0.9,"why":"good"},{"score":0.3,"why":"meh"}]}"#.to_string(), // score 0
            "Tighten stops, use momentum".to_string(),   // propose 1
            r#"{"results":[{"score":0.2,"why":"bad"},{"score":0.1,"why":"worse"}]}"#.to_string(), // score 1
        ];
        let gepa = mock_gepa(responses);
        let result = gepa
            .compile(
                "ns",
                &[("a".into(), "obs a".into()), ("b".into(), "obs b".into())],
                None,
            )
            .await
            .unwrap();
        assert!(!result.instruction.is_empty());
        // Best should be candidate 0 (mean 0.6 > 0.15).
        assert!(result.instruction.contains("ATR"));
    }

    #[tokio::test]
    async fn real_eval_skips_candidate_below_full_fast_threshold() {
        let responses = vec![
            "reflection".to_string(),
            "full skip candidate".to_string(),
            r#"{"results":[{"score":0.80,"why":"strong minibatch"},{"score":0.80,"why":"strong minibatch"}]}"#.to_string(),
            r#"{"results":[{"score":0.10,"why":"weak full"},{"score":0.10,"why":"weak full"},{"score":0.10,"why":"weak full"}]}"#.to_string(),
        ];
        let mut gepa = mock_gepa(responses);
        gepa.candidates = 1;
        let evaluator = SequenceBenchmarkEvaluator::new(vec![0.99]);
        gepa.real_eval = Some(RealEvalOptions::new(
            0.30,
            vec![benchmark("bench-a")],
            Arc::new(evaluator),
        ));

        let result = gepa
            .compile(
                "ns",
                &[
                    ("a".into(), "obs a".into()),
                    ("b".into(), "obs b".into()),
                    ("c".into(), "obs c".into()),
                ],
                None,
            )
            .await
            .unwrap();

        assert!(
            result.instruction.is_empty(),
            "candidate skipped by full active-set fast score should not win from fast LLM scores"
        );
        assert!(result.demos.iter().all(|d| d.score.unwrap_or(0.0) < 0.30));
    }

    #[tokio::test]
    async fn real_eval_score_can_change_candidate_selection() {
        let responses = vec![
            "reflection".to_string(),
            "candidate with high llm but poor real result".to_string(),
            r#"{"results":[{"score":0.90,"why":"sounds good"},{"score":0.90,"why":"sounds good"}]}"#
                .to_string(),
            "candidate with lower llm but better real result".to_string(),
            r#"{"results":[{"score":0.80,"why":"still plausible"},{"score":0.80,"why":"still plausible"}]}"#
                .to_string(),
        ];
        let mut gepa = mock_gepa(responses);
        let evaluator = SequenceBenchmarkEvaluator::new(vec![0.20, 0.95]);
        gepa.real_eval = Some(RealEvalOptions::new(
            0.30,
            vec![benchmark("bench-a")],
            Arc::new(evaluator),
        ));

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

    #[tokio::test]
    async fn real_eval_reuses_cached_score_for_full_active_pass() {
        let responses = vec![
            "reflection".to_string(),
            "candidate cached real eval".to_string(),
            r#"{"results":[{"score":0.80,"why":"strong minibatch"},{"score":0.80,"why":"strong minibatch"}]}"#.to_string(),
            r#"{"results":[{"score":0.80,"why":"strong full"},{"score":0.80,"why":"strong full"},{"score":0.80,"why":"strong full"}]}"#.to_string(),
        ];
        let evaluator = SequenceBenchmarkEvaluator::new(vec![0.77]);
        let mut gepa = mock_gepa(responses);
        gepa.candidates = 1;
        gepa.real_eval = Some(RealEvalOptions::new(
            0.30,
            vec![benchmark("bench-a")],
            Arc::new(evaluator.clone()),
        ));

        let result = gepa
            .compile(
                "ns",
                &[
                    ("a".into(), "obs a".into()),
                    ("b".into(), "obs b".into()),
                    ("c".into(), "obs c".into()),
                ],
                None,
            )
            .await
            .unwrap();

        assert!(result.instruction.contains("cached real eval"));
        assert_eq!(
            evaluator.call_count(),
            1,
            "full active pass must hit real-eval cache"
        );
        assert!(result
            .demos
            .iter()
            .all(|demo| (demo.score.unwrap() - 0.77).abs() < 1e-9));
    }

    #[tokio::test]
    async fn real_eval_merge_candidate_must_pass_real_scorer_before_winning() {
        let responses = vec![
            "reflection".to_string(),
            "candidate with best real score".to_string(),
            r#"{"results":[{"score":0.70,"why":"ok"},{"score":0.70,"why":"ok"}]}"#.to_string(),
            "candidate with second real score".to_string(),
            r#"{"results":[{"score":0.60,"why":"ok"},{"score":0.60,"why":"ok"}]}"#.to_string(),
            "merged high fast score low real score".to_string(),
            r#"{"results":[{"score":0.99,"why":"sounds merged"},{"score":0.99,"why":"sounds merged"}]}"#
                .to_string(),
        ];
        let evaluator = SequenceBenchmarkEvaluator::new(vec![0.70, 0.60, 0.20]);
        let benchmark_pool = vec![benchmark("bench-a")];
        let real_eval = RealEvalOptions::new(0.30, benchmark_pool.clone(), Arc::new(evaluator.clone()));
        let real_eval_cache = real_eval.cache.clone();
        let mut gepa = mock_gepa(responses);
        gepa.use_merge = true;
        gepa.merge_frequency = 1;
        gepa.real_eval = Some(real_eval);

        let result = gepa
            .compile(
                "ns",
                &[("a".into(), "obs a".into()), ("b".into(), "obs b".into())],
                None,
            )
            .await
            .unwrap();

        assert!(result.instruction.contains("best real score"));
        assert!(!result.instruction.contains("merged high fast"));
        assert_eq!(evaluator.call_count(), 3, "merge path must invoke real evaluator");
        assert_eq!(result.demos[0].score, Some(0.70));
        assert_eq!(result.demos[1].score, Some(0.70));
        let merged_cache_key =
            real_eval_cache_key("ns", "merged high fast score low real score", &benchmark_pool);
        let merged_cached_score = real_eval_cache
            .get(&merged_cache_key)
            .expect("merged candidate must be scored through real-eval cache path");
        assert!((merged_cached_score.score - 0.20).abs() < 1e-9);
    }

    #[tokio::test]
    async fn skip_perfect_compares_active_scores_and_preserves_inactive_best_scores() {
        let responses = vec![
            "reflection one".to_string(),
            "first generation instruction".to_string(),
            r#"{"results":[{"score":1.0,"why":"perfect a"},{"score":1.0,"why":"perfect b"},{"score":0.4,"why":"weak c"},{"score":0.4,"why":"weak d"}]}"#.to_string(),
            "reflection two".to_string(),
            "second generation improves remaining observations".to_string(),
            r#"{"results":[{"score":0.95,"why":"better c"},{"score":0.95,"why":"better d"}]}"#.to_string(),
        ];
        let mut gepa = mock_gepa(responses);
        gepa.candidates = 1;
        gepa.generations = 2;
        gepa.reflection_minibatch_size = 4;
        gepa.skip_perfect = true;

        let result = gepa
            .compile(
                "ns",
                &[
                    ("a".into(), "obs a".into()),
                    ("b".into(), "obs b".into()),
                    ("c".into(), "obs c".into()),
                    ("d".into(), "obs d".into()),
                ],
                None,
            )
            .await
            .unwrap();

        assert!(result
            .instruction
            .contains("second generation improves remaining observations"));
        assert_eq!(result.demos[0].score, Some(1.0));
        assert_eq!(result.demos[1].score, Some(1.0));
        assert_eq!(result.demos[2].score, Some(0.95));
        assert_eq!(result.demos[3].score, Some(0.95));
    }
}
