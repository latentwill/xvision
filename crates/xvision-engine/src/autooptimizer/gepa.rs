use std::sync::Arc;

use async_trait::async_trait;

use crate::agent::llm::{LlmDispatch, LlmRequest, Message};
use crate::autooptimizer::dspy_bridge::{CompileResult, DspyBridge};
use crate::autooptimizer::pattern_snapshot::{Provenance, SnapshotDemo};

const SYSTEM_PROMPT: &str = "You are an instruction optimizer for an automated trading-strategy \
    experiment writer. Your job is to analyze findings from recent optimization cycles and produce \
    concise instruction prefixes that steer the experiment writer toward what works and away from \
    what does not.";

/// GEPA (Genetic-Pareto Evolutionary) bridge: two-stage reflection+proposal loop
/// with LLM-based per-candidate scoring. Replaces the single-call `LiveDspyBridge`
/// summarizer when `gepa_enabled = true`.
///
/// Algorithm per generation:
///   1. REFLECT — single LLM call: analyze all observations, identify patterns
///   2. PROPOSE — one LLM call per candidate: generate instruction from reflection
///   3. SCORE   — one LLM call per candidate: rate instruction against observations
///
/// The candidate with the highest mean score wins. Runs for `generations` iterations;
/// each generation seeds its reflection from the full observation pool.
pub struct GepaBridge {
    pub dispatch: Arc<dyn LlmDispatch + Send + Sync>,
    pub model: String,
    pub provider: String,
    pub candidates: usize,
    pub generations: usize,
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
        let mut best_score = f64::NEG_INFINITY;
        let mut best_scores: Vec<f64> = vec![];

        for _gen in 0..self.generations.max(1) {
            // Stage 1: REFLECT
            let reflection = self.reflect(&obs_list, &mut provenance).await?;

            // Stage 2+3: PROPOSE + SCORE (one candidate at a time to stay simple)
            let mut candidates = Vec::with_capacity(self.candidates.max(1));
            for i in 0..self.candidates.max(1) {
                let instruction = self
                    .propose(&reflection, i, self.candidates.max(1), &mut provenance)
                    .await?;
                let scores = self.score(&instruction, observations, &mut provenance).await?;
                let mean = if scores.is_empty() {
                    0.0
                } else {
                    scores.iter().sum::<f64>() / scores.len() as f64
                };
                candidates.push((instruction, scores, mean));
            }

            // Pick the best candidate this generation.
            if let Some((instr, scores, mean)) = candidates
                .into_iter()
                .max_by(|a, b| a.2.partial_cmp(&b.2).unwrap())
            {
                if mean > best_score {
                    best_score = mean;
                    best_instruction = instr;
                    best_scores = scores;
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
    async fn reflect(&self, obs_list: &str, provenance: &mut Provenance) -> anyhow::Result<String> {
        let user_text = format!(
            "Here are observations from recent optimization cycles. Identify the 3 most important \
             patterns: what kinds of experiments succeeded, what failed, and why.\n\n\
             Observations:\n{obs_list}"
        );
        let req = LlmRequest {
            model: self.model.clone(),
            system_prompt: SYSTEM_PROMPT.to_string(),
            messages: vec![Message::user_text(user_text)],
            max_tokens: None,
            tools: vec![],
            temperature: Some(0.5),
            response_schema: None,
            cache_control: None,
            force_json: false,
        };
        let resp = self.dispatch.complete(req).await?;
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

    async fn score(
        &self,
        instruction: &str,
        observations: &[(String, String)],
        provenance: &mut Provenance,
    ) -> anyhow::Result<Vec<f64>> {
        let obs_list = observations
            .iter()
            .enumerate()
            .map(|(i, (_id, t))| format!("{}. {}", i + 1, t.trim()))
            .collect::<Vec<_>>()
            .join("\n");
        let n = observations.len();
        let user_text = format!(
            "Rate how well this instruction prefix addresses each observation (0.0–1.0). \
             Higher = the instruction would steer the experiment writer to avoid this failure \
             or repeat this success.\n\n\
             Instruction:\n{instruction}\n\n\
             Observations:\n{obs_list}\n\n\
             Return JSON only: {{\"scores\": [0.0, 0.8, ...]}} with exactly {n} values."
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
        let scores: Vec<f64> = parsed["scores"]
            .as_array()
            .map(|arr| {
                arr.iter()
                    .map(|v| v.as_f64().unwrap_or(0.5).clamp(0.0, 1.0))
                    .collect()
            })
            .unwrap_or_else(|| vec![0.5; n]);
        Ok(scores)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use async_trait::async_trait;

    use super::*;
    use crate::agent::llm::{ContentBlock, LlmResponse, StopReason};

    /// Returns scripted responses in sequence; repeats last when exhausted.
    struct ScriptedDispatch {
        responses: Mutex<Vec<String>>,
    }

    impl ScriptedDispatch {
        fn new(responses: Vec<impl Into<String>>) -> Arc<Self> {
            Arc::new(Self {
                responses: Mutex::new(responses.into_iter().map(|s| s.into()).collect()),
            })
        }
    }

    #[async_trait]
    impl LlmDispatch for ScriptedDispatch {
        async fn complete(&self, _req: LlmRequest) -> anyhow::Result<LlmResponse> {
            let mut q = self.responses.lock().unwrap();
            let text = if q.len() > 1 { q.remove(0) } else { q[0].clone() };
            Ok(LlmResponse {
                content: vec![ContentBlock::Text { text }],
                stop_reason: StopReason::EndTurn,
                input_tokens: 5,
                output_tokens: 10,
            })
        }
    }

    #[tokio::test]
    async fn gepa_returns_empty_for_empty_observations() {
        let bridge = GepaBridge {
            dispatch: ScriptedDispatch::new(vec!["reflection"]),
            model: "test".to_string(),
            provider: "test".to_string(),
            candidates: 1,
            generations: 1,
        };
        let result = bridge.compile("ns", &[]).await.unwrap();
        assert!(result.instruction.is_empty());
        assert_eq!(result.optimizer_name, "gepa");
    }

    #[tokio::test]
    async fn gepa_calls_reflect_propose_score_and_returns_best() {
        // Script: reflect → proposal_A → score_A (0.9) → proposal_B → score_B (0.3)
        let dispatch = ScriptedDispatch::new(vec![
            "Reflection: high conviction wins.",
            "Candidate A: prefer high-conviction setups.",
            r#"{"scores": [0.9, 0.8]}"#,
            "Candidate B: focus on regime fit.",
            r#"{"scores": [0.3, 0.4]}"#,
        ]);
        let bridge = GepaBridge {
            dispatch,
            model: "test".to_string(),
            provider: "test".to_string(),
            candidates: 2,
            generations: 1,
        };
        let obs = vec![
            ("obs1".to_string(), "high conviction improved Sharpe".to_string()),
            ("obs2".to_string(), "low conviction hurt win rate".to_string()),
        ];
        let result = bridge.compile("autooptimizer:dspy", &obs).await.unwrap();
        assert_eq!(result.instruction, "Candidate A: prefer high-conviction setups.");
        assert_eq!(result.optimizer_name, "gepa");
        assert_eq!(result.demos.len(), 2);
        assert_eq!(result.demos[0].score, Some(0.9));
        assert_eq!(result.demos[1].score, Some(0.8));
        assert!(result.provenance.total_tokens() > 0);
    }

    #[tokio::test]
    async fn gepa_picks_highest_mean_score() {
        // Two candidates: A gets [0.2, 0.4] mean=0.3, B gets [0.8, 0.9] mean=0.85
        let dispatch = ScriptedDispatch::new(vec![
            "Reflection text.",
            "Candidate A instruction.",
            r#"{"scores": [0.2, 0.4]}"#,
            "Candidate B instruction.",
            r#"{"scores": [0.8, 0.9]}"#,
        ]);
        let bridge = GepaBridge {
            dispatch,
            model: "test".to_string(),
            provider: "test".to_string(),
            candidates: 2,
            generations: 1,
        };
        let obs = vec![
            ("o1".to_string(), "obs1".to_string()),
            ("o2".to_string(), "obs2".to_string()),
        ];
        let result = bridge.compile("ns", &obs).await.unwrap();
        assert_eq!(result.instruction, "Candidate B instruction.");
        assert_eq!(result.demos[0].score, Some(0.8));
    }

    #[tokio::test]
    async fn gepa_accumulates_provenance_across_calls() {
        // 1 generation × 1 candidate = 3 LLM calls (reflect, propose, score)
        // Each MockResponse: input_tokens=5, output_tokens=10 → total=45
        let dispatch = ScriptedDispatch::new(vec![
            "Reflection.",
            "Candidate instruction.",
            r#"{"scores": [0.7]}"#,
        ]);
        let bridge = GepaBridge {
            dispatch,
            model: "test".to_string(),
            provider: "test".to_string(),
            candidates: 1,
            generations: 1,
        };
        let obs = vec![("o1".to_string(), "obs".to_string())];
        let result = bridge.compile("ns", &obs).await.unwrap();
        assert_eq!(result.provenance.prompt_tokens, 15); // 3 calls × 5
        assert_eq!(result.provenance.completion_tokens, 30); // 3 calls × 10
    }
}
