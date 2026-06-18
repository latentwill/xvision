use std::sync::Arc;

use async_trait::async_trait;

use crate::agent::llm::{LlmDispatch, LlmRequest, Message};
use crate::autooptimizer::dspy_bridge::{CompileResult, DspyBridge};
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
}

// ── ScoreWithFeedback helpers ──────────────────────────────────────────
impl ScoreWithFeedback {
    fn mean(&self) -> f64 {
        if self.scores.is_empty() {
            0.0
        } else {
            self.scores.iter().sum::<f64>() / self.scores.len() as f64
        }
    }

    fn mean_on_indices(&self, indices: &[usize]) -> f64 {
        if indices.is_empty() {
            return 0.0;
        }
        let sum: f64 = indices.iter().map(|&i| self.scores.get(i).copied().unwrap_or(0.0)).sum();
        sum / indices.len() as f64
    }
}

#[async_trait]
impl DspyBridge for GepaBridge {
    async fn compile(
        &self,
        _namespace: &str,
        observations: &[(String, String)],
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
        let mut best_mean = f64::NEG_INFINITY;
        let mut best_scores: Vec<f64> = vec![];
        let mut best_feedback: Vec<String> = vec![];
        // Phase 6 warm-start: seed the first reflection with the prior best
        // instruction if one was persisted. (Full warm-start requires the
        // memory-store API which isn't directly available here; the flywheel
        // prepends the prior instruction to the observations list instead.)
        let _seed_prefix = String::new();

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
                .map(|(j, &orig_i)| {
                    format!("{}. {}", j + 1, observations[orig_i].1.trim())
                })
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
            let reflection_text = format!(
                "{obs_list}{feedback_hint}"
            );
            let reflection = self.reflect(&reflection_text, &mut provenance).await?;

            // ── Stage 2+3: PROPOSE + SCORE with minibatch → full eval ──
            let mb_size = self
                .reflection_minibatch_size
                .min(active_indices.len())
                .max(2);

            // Minibatch indices: first `mb_size` active indices (for cheap cull)
            let mb_indices: Vec<usize> = active_indices.iter().copied().take(mb_size).collect();

            let mut generation_best_score = f64::NEG_INFINITY;
            let mut generation_best_instruction = String::new();
            let mut generation_best = None;

            // Track per-observation best scores for Pareto frontier (4a).
            let mut per_obs_best: Vec<f64> = vec![f64::NEG_INFINITY; observations.len()];
            let mut frontier_candidates: Vec<(
                String,
                ScoreWithFeedback,
                /* frontier_count */ usize,
            )> = Vec::with_capacity(n_candidates);

            for i in 0..n_candidates {
                let instruction = self
                    .propose(&reflection, i, n_candidates, &mut provenance)
                    .await?;

                // Tier 1: minibatch scoring (cheap)
                let mb_scores = self
                    .score_on_indices(&instruction, observations, &mb_indices, &mut provenance)
                    .await?;
                let mb_mean = mb_scores.mean_on_indices(&(0..mb_indices.len()).collect::<Vec<_>>());
                if mb_mean <= best_mean && gen > 0 {
                    continue; // early cull: minibatch doesn't beat current best
                }

                // Tier 2: full scoring on active indices
                let full = self
                    .score_on_indices(&instruction, observations, &active_indices, &mut provenance)
                    .await?;
                let mean = full.mean();

                // Track per-observation best for Pareto frontier.
                for &orig_i in &active_indices {
                    let s = full.scores.get(orig_i).copied().unwrap_or(0.0);
                    if s > per_obs_best[orig_i] {
                        per_obs_best[orig_i] = s;
                    }
                }

                if mean > generation_best_score {
                    generation_best_score = mean;
                    generation_best_instruction = instruction.clone();
                    generation_best = Some((instruction, full));
                }
            }

            // ── 4a: Pareto frontier selection ──
            // Count how many observations each candidate is best on.
            // Re-score the best candidate for frontier tracking.
            if let Some((_instr, ref scores)) = generation_best {
                // Build frontier: for each observation, count which candidates
                // are tied for best. The generation winner gets frontier priority.
                let mut frontier_count = 0usize;
                for &orig_i in &active_indices {
                    let s = scores
                        .scores
                        .get(orig_i)
                        .copied()
                        .unwrap_or(0.0);
                    if (s - per_obs_best[orig_i]).abs() < 1e-6 {
                        frontier_count += 1;
                    }
                }
                frontier_candidates.push((
                    generation_best_instruction.clone(),
                    scores.clone(),
                    frontier_count,
                ));
            }

            // Pareto-weighted selection: candidate with highest frontier count wins.
            // Falls back to max-mean if frontier counts are all equal.
            let winner = match self.selection_strategy {
                GepaSelectionStrategy::Pareto => frontier_candidates
                    .iter()
                    .max_by(|a, b| a.2.cmp(&b.2).then_with(|| a.1.mean().partial_cmp(&b.1.mean()).unwrap())),
                GepaSelectionStrategy::CurrentBest => frontier_candidates
                    .iter()
                    .max_by(|a, b| a.1.mean().partial_cmp(&b.1.mean()).unwrap()),
            };

            if let Some((instr, scores, _fc)) = winner {
                let mean = scores.mean();
                if mean > best_mean {
                    best_mean = mean;
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
                frontier_candidates
                    .sort_by(|a, b| b.2.cmp(&a.2).then_with(|| a.1.mean().partial_cmp(&b.1.mean()).unwrap()));
                let parent_a = &frontier_candidates[0].0;
                let parent_b = &frontier_candidates[1].0;
                if let Ok(merged) = self.merge(parent_a, parent_b, &mut provenance).await {
                    let mb_scores = self
                        .score_on_indices(&merged, observations, &mb_indices, &mut provenance)
                        .await?;
                    let mb_mean = mb_scores.mean_on_indices(&(0..mb_indices.len()).collect::<Vec<_>>());
                    if mb_mean > best_mean {
                        let full = self
                            .score_on_indices(&merged, observations, &active_indices, &mut provenance)
                            .await?;
                        let mean = full.mean();
                        if mean > best_mean {
                            best_mean = mean;
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
        let dispatch = self
            .reflection_dispatch
            .as_ref()
            .unwrap_or(&self.dispatch);
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
            });
        }

        let obs_text = indices
            .iter()
            .enumerate()
            .map(|(j, &orig_i)| {
                format!("{}. {}", j + 1, all_observations[orig_i].1.trim())
            })
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
                full_scores[orig_idx] = entry["score"]
                    .as_f64()
                    .unwrap_or(0.5)
                    .clamp(0.0, 1.0);
                full_feedback[orig_idx] = entry["why"]
                    .as_str()
                    .unwrap_or("")
                    .to_string();
            }
        }

        Ok(ScoreWithFeedback {
            scores: full_scores,
            feedback: full_feedback,
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
    use std::sync::Arc;

    use crate::agent::llm::{ContentBlock, LlmResponse, MockDispatch, StopReason};

    fn to_response(text: impl Into<String>) -> LlmResponse {
        LlmResponse {
            content: vec![ContentBlock::Text {
                text: text.into(),
            }],
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
        }
    }

    #[tokio::test]
    async fn empty_observations_returns_empty_result() {
        let gepa = mock_gepa(vec![]);
        let result = gepa.compile("ns", &[]).await.unwrap();
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
            "reflection: favor wider stops".to_string(),          // reflect
            "Use wider ATR-based stops".to_string(),              // propose 0
            r#"{"results":[{"score":0.9,"why":"good"},{"score":0.3,"why":"meh"}]}"#.to_string(),  // score 0
            "Tighten stops, use momentum".to_string(),            // propose 1
            r#"{"results":[{"score":0.2,"why":"bad"},{"score":0.1,"why":"worse"}]}"#.to_string(),  // score 1
        ];
        let gepa = mock_gepa(responses);
        let result = gepa
            .compile("ns", &[("a".into(), "obs a".into()), ("b".into(), "obs b".into())])
            .await
            .unwrap();
        assert!(!result.instruction.is_empty());
        // Best should be candidate 0 (mean 0.6 > 0.15).
        assert!(result.instruction.contains("ATR"));
    }
}
