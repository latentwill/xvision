//! Three-candidate tournament for candidate-generation upgrade (Phase 4).
//!
//! Replaces the single-shot mutator with: incumbent (no change) + adversarial
//! + synthesis, judged blind via Borda count. Numeric gate still runs after
//! the tournament picks a winner.

use std::sync::Arc;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::agent::llm::{LlmDispatch, LlmRequest, Message};
use crate::autooptimizer::config::AutoOptimizerConfig;
use crate::autooptimizer::mutator::{empty_mutation, MutationDiff, Mutator};
use crate::autooptimizer::program_view;
use crate::autooptimizer::validator::{validate_mutation_diff, ValidationError};
use crate::strategies::Strategy;

const ADVERSARIAL_PROMPT: &str = include_str!("../../prompts/autooptimizer/tournament-adversarial-v1.md");
const SYNTHESIS_PROMPT: &str = include_str!("../../prompts/autooptimizer/tournament-synthesis-v1.md");
const JUDGE_PROMPT: &str = include_str!("../../prompts/autooptimizer/tournament-judge-v1.md");

pub const CANDIDATE_COUNT: usize = 3;
const JUDGE_COUNT: usize = 3;
const MAX_PARAMS_APPLY: usize = 64;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CandidateKind {
    Incumbent,
    Adversarial,
    Synthesis,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TournamentCandidate {
    pub kind: CandidateKind,
    pub strategy: Strategy,
    pub diff: MutationDiff,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BordaVote {
    /// Candidate indices ordered best-first. `ranking[0]` = 1st place.
    pub ranking: [usize; CANDIDATE_COUNT],
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TournamentResult {
    pub winner_kind: CandidateKind,
    pub winner_diff: MutationDiff,
    pub winner_strategy: Strategy,
    pub incumbent_wins: bool,
    pub borda_scores: [u32; CANDIDATE_COUNT],
}

pub struct TournamentRunner {
    pub dispatch: Arc<dyn LlmDispatch + Send + Sync>,
    pub model: String,
    pub provider: String,
    pub max_retries: u32,
}

impl TournamentRunner {
    pub fn from_mutator(m: &Mutator) -> Self {
        Self {
            dispatch: Arc::clone(&m.dispatch),
            model: m.model.clone(),
            provider: m.provider.clone(),
            max_retries: m.max_retries,
        }
    }

    pub async fn generate_candidates(
        &self,
        parent: &Strategy,
        config: &AutoOptimizerConfig,
    ) -> Result<Vec<TournamentCandidate>> {
        let incumbent = TournamentCandidate {
            kind: CandidateKind::Incumbent,
            strategy: parent.clone(),
            diff: empty_mutation(),
        };
        let adversarial_sys = system_section(ADVERSARIAL_PROMPT);
        let synthesis_sys = system_section(SYNTHESIS_PROMPT);
        let (adv_diff, syn_diff) = tokio::try_join!(
            self.propose_diff(parent, config, &adversarial_sys),
            self.propose_diff(parent, config, &synthesis_sys),
        )?;
        let adv_strategy = apply_params(parent, &adv_diff);
        let syn_strategy = apply_params(parent, &syn_diff);
        Ok(vec![
            incumbent,
            TournamentCandidate {
                kind: CandidateKind::Adversarial,
                strategy: adv_strategy,
                diff: adv_diff,
            },
            TournamentCandidate {
                kind: CandidateKind::Synthesis,
                strategy: syn_strategy,
                diff: syn_diff,
            },
        ])
    }

    async fn propose_diff(
        &self,
        parent: &Strategy,
        config: &AutoOptimizerConfig,
        system_prompt: &str,
    ) -> Result<MutationDiff> {
        let program_md = program_view::to_markdown(parent);
        let mut last_err: Option<String> = None;
        let max_attempts = self.max_retries.saturating_add(1);
        assert!(max_attempts >= 1, "max_attempts must be at least 1");
        for attempt in 0..max_attempts {
            let user_text =
                build_proposal_user(&program_md, &config.allowed_mutation_kinds, last_err.as_deref());
            let req = LlmRequest {
                model: self.model.clone(),
                system_prompt: system_prompt.to_string(),
                messages: vec![Message::user_text(user_text)],
                max_tokens: None,
                tools: vec![],
                temperature: None,
                response_schema: None,
                cache_control: None,
                force_json: true,
            };
            let resp = self
                .dispatch
                .complete(req)
                .await
                .with_context(|| format!("tournament propose failed on attempt {attempt}"))?;
            let diff = match extract_diff(&resp.text()) {
                Ok(d) => d,
                Err(e) => {
                    last_err = Some(e.to_string());
                    continue;
                }
            };
            match validate_mutation_diff(&diff, parent) {
                Ok(()) => return Ok(diff),
                Err(errs) => {
                    last_err = Some(fmt_errors(&errs));
                }
            }
        }
        anyhow::bail!("tournament propose failed after {max_attempts} attempt(s)")
    }

    pub async fn borda_vote(&self, candidates: &[TournamentCandidate]) -> Result<Vec<BordaVote>> {
        assert_eq!(
            candidates.len(),
            CANDIDATE_COUNT,
            "borda_vote requires {CANDIDATE_COUNT} candidates"
        );
        let summary = build_candidate_summary(candidates);
        let (v0, v1, v2) = tokio::try_join!(
            self.one_judge_vote(&summary),
            self.one_judge_vote(&summary),
            self.one_judge_vote(&summary),
        )?;
        Ok(vec![v0, v1, v2])
    }

    async fn one_judge_vote(&self, candidate_summary: &str) -> Result<BordaVote> {
        let req = LlmRequest {
            model: self.model.clone(),
            system_prompt: JUDGE_PROMPT.to_string(),
            messages: vec![Message::user_text(candidate_summary.to_string())],
            max_tokens: None,
            tools: vec![],
            temperature: None,
            response_schema: None,
            cache_control: None,
            force_json: true,
        };
        let resp = self
            .dispatch
            .complete(req)
            .await
            .context("judge dispatch failed")?;
        parse_borda_vote(&resp.text())
    }

    pub fn tally(votes: &[BordaVote]) -> [u32; CANDIDATE_COUNT] {
        assert_eq!(
            votes.len(),
            JUDGE_COUNT,
            "tally requires exactly {JUDGE_COUNT} votes"
        );
        let mut scores = [0u32; CANDIDATE_COUNT];
        for vote in votes {
            for (rank_pos, &candidate_idx) in vote.ranking.iter().enumerate() {
                assert!(candidate_idx < CANDIDATE_COUNT, "candidate_idx out of bounds");
                let points = (CANDIDATE_COUNT - 1 - rank_pos) as u32;
                scores[candidate_idx] = scores[candidate_idx].saturating_add(points);
            }
        }
        scores
    }

    pub async fn run_tournament(
        &self,
        parent: &Strategy,
        config: &AutoOptimizerConfig,
    ) -> Result<TournamentResult> {
        let candidates = self.generate_candidates(parent, config).await?;
        assert_eq!(candidates.len(), CANDIDATE_COUNT);
        let votes = self.borda_vote(&candidates).await?;
        let borda_scores = Self::tally(&votes);
        let winner_index = pick_winner(&borda_scores);
        let winner = &candidates[winner_index];
        Ok(TournamentResult {
            winner_kind: winner.kind,
            winner_diff: winner.diff.clone(),
            winner_strategy: winner.strategy.clone(),
            incumbent_wins: winner.kind == CandidateKind::Incumbent,
            borda_scores,
        })
    }
}

fn pick_winner(scores: &[u32; CANDIDATE_COUNT]) -> usize {
    scores
        .iter()
        .enumerate()
        .max_by_key(|&(i, &s)| (s, usize::MAX - i))
        .map(|(i, _)| i)
        .unwrap_or(0)
}

fn apply_params(base: &Strategy, diff: &MutationDiff) -> Strategy {
    assert!(
        diff.params.len() <= MAX_PARAMS_APPLY,
        "params count exceeds bound"
    );
    let mut s = base.clone();
    if let serde_json::Value::Object(ref mut map) = s.mechanical_params {
        for change in &diff.params {
            map.insert(change.key.clone(), change.after.clone());
        }
    }
    s
}

fn system_section(prompt_template: &str) -> String {
    let marker = "# USER";
    if let Some(idx) = prompt_template.find(marker) {
        prompt_template[..idx].trim().to_string()
    } else {
        prompt_template.to_string()
    }
}

fn build_proposal_user(program_md: &str, allowed_kinds: &[String], prev_err: Option<&str>) -> String {
    let kinds = allowed_kinds.join(", ");
    let err_section = prev_err
        .map(|e| format!("\n\nPrevious attempt errors — fix all:\n\n{e}"))
        .unwrap_or_default();
    format!("Strategy program view:\n\n{program_md}\n\nAllowed experiment kinds: {kinds}{err_section}\n\nPropose ONE experiment as a JSON object.")
}

fn build_candidate_summary(candidates: &[TournamentCandidate]) -> String {
    assert_eq!(candidates.len(), CANDIDATE_COUNT);
    let mut out = String::from("Rank the following strategy experiment candidates.\n\n");
    for (i, c) in candidates.iter().enumerate() {
        let label = match c.kind {
            CandidateKind::Incumbent => "No change (incumbent)",
            CandidateKind::Adversarial => "Bold experiment",
            CandidateKind::Synthesis => "Focused experiment",
        };
        out.push_str(&format!("## Candidate {i} — {label}\n\n"));
        if c.diff.is_empty() {
            out.push_str("(Strategy left unchanged.)\n\n");
        } else {
            out.push_str(&format!("Rationale: {}\n\n", c.diff.rationale));
            let diff_json = serde_json::to_string_pretty(&c.diff).unwrap_or_default();
            out.push_str(&format!("```json\n{diff_json}\n```\n\n"));
        }
    }
    out.push_str("Respond with {\"ranking\": [best_index, second_index, third_index]}.");
    out
}

fn extract_diff(text: &str) -> anyhow::Result<MutationDiff> {
    let s = strip_json_fences(text);
    serde_json::from_str::<MutationDiff>(s).context("failed to parse MutationDiff from tournament response")
}

fn strip_json_fences(text: &str) -> &str {
    let t = text.trim();
    t.strip_prefix("```json")
        .or_else(|| t.strip_prefix("```"))
        .map(|s| s.trim_start())
        .and_then(|s| s.strip_suffix("```"))
        .map(|s| s.trim_end())
        .unwrap_or(t)
}

fn parse_borda_vote(text: &str) -> Result<BordaVote> {
    let s = strip_json_fences(text);
    #[derive(Deserialize)]
    struct Raw {
        ranking: Vec<usize>,
    }
    let raw: Raw = serde_json::from_str(s).context("failed to parse BordaVote")?;
    if raw.ranking.len() != CANDIDATE_COUNT {
        anyhow::bail!("BordaVote ranking must have {CANDIDATE_COUNT} entries");
    }
    let mut seen = [false; CANDIDATE_COUNT];
    for &idx in &raw.ranking {
        if idx >= CANDIDATE_COUNT {
            anyhow::bail!("BordaVote ranking index {idx} out of range");
        }
        if seen[idx] {
            anyhow::bail!("BordaVote ranking contains duplicate index {idx}");
        }
        seen[idx] = true;
    }
    Ok(BordaVote {
        ranking: [raw.ranking[0], raw.ranking[1], raw.ranking[2]],
    })
}

fn fmt_errors(errors: &[ValidationError]) -> String {
    assert!(!errors.is_empty());
    errors
        .iter()
        .map(|e| {
            if let Some(p) = &e.path {
                format!("- [{}] {} (at {})", e.code, e.message, p)
            } else {
                format!("- [{}] {}", e.code, e.message)
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::llm::MockDispatch;
    use crate::autooptimizer::config::AutoOptimizerConfig;
    use crate::strategies::Strategy;

    fn stub_strategy() -> Strategy {
        let v = serde_json::json!({
            "manifest": {
                "id": "01HZTEST00000000000000000A",
                "display_name": "Tournament Test Strategy",
                "plain_summary": "Minimal fixture for tournament tests.",
                "creator": "@test",
                "template": "custom",
                "regime_fit": [],
                "asset_universe": ["BTC/USD"],
                "decision_cadence_minutes": 60,
                "required_tools": [],
                "risk_preset_or_config": "balanced"
            },
            "agents": [{"agent_id": "01HZAGENT0000000000000000A", "role": "trader"}],
            "risk": {
                "risk_pct_per_trade": 0.01,
                "max_concurrent_positions": 1,
                "max_leverage": 1.0,
                "stop_loss_atr_multiple": 2.0,
                "daily_loss_kill_pct": 0.05
            },
            "mechanical_params": {}
        });
        serde_json::from_value(v).expect("stub_strategy fixture must deserialise")
    }

    fn valid_diff_json() -> String {
        serde_json::to_string(&MutationDiff {
            kind: crate::autooptimizer::mutator::MutationKind::Prose,
            prose: vec![crate::autooptimizer::mutator::ProseEdit {
                agent_role: "trader".into(),
                before: "analyze market".into(),
                after: "analyze market trends carefully".into(),
            }],
            params: vec![],
            tools: crate::autooptimizer::mutator::ToolDiff {
                added: vec![],
                removed: vec![],
            },
            filter: vec![],
            rationale: "test rationale".into(),
        })
        .unwrap()
    }

    fn make_runner(dispatch: MockDispatch) -> TournamentRunner {
        TournamentRunner {
            dispatch: Arc::new(dispatch),
            model: "test-model".into(),
            provider: "test".into(),
            max_retries: 0,
        }
    }

    #[test]
    fn borda_tally_picks_winner() {
        let votes = vec![
            BordaVote { ranking: [1, 0, 2] },
            BordaVote { ranking: [1, 2, 0] },
            BordaVote { ranking: [1, 0, 2] },
        ];
        let scores = TournamentRunner::tally(&votes);
        assert_eq!(scores[1], 6, "candidate 1 should have 6 pts (1st in all 3 votes)");
        assert!(scores[1] > scores[0] && scores[1] > scores[2]);
    }

    #[test]
    fn borda_tally_tie_prefers_incumbent() {
        let votes = vec![
            BordaVote { ranking: [0, 1, 2] },
            BordaVote { ranking: [1, 0, 2] },
            BordaVote { ranking: [2, 0, 1] },
        ];
        let scores = TournamentRunner::tally(&votes);
        let winner = pick_winner(&scores);
        assert_eq!(winner, 0, "incumbent wins on tie (same score)");
    }

    #[test]
    fn parse_borda_vote_valid() {
        let v = parse_borda_vote(r#"{"ranking": [2, 0, 1]}"#).unwrap();
        assert_eq!(v.ranking, [2, 0, 1]);
    }

    #[test]
    fn parse_borda_vote_duplicate_rejected() {
        assert!(parse_borda_vote(r#"{"ranking": [0, 0, 1]}"#).is_err());
    }

    #[test]
    fn parse_borda_vote_out_of_range_rejected() {
        assert!(parse_borda_vote(r#"{"ranking": [0, 1, 3]}"#).is_err());
    }

    #[tokio::test]
    async fn tournament_produces_3_candidates() {
        let config = AutoOptimizerConfig::default();
        let adv_json = valid_diff_json();
        let syn_json = valid_diff_json();
        let judge_json = r#"{"ranking": [0, 1, 2]}"#;
        let dispatch = MockDispatch::sequence(vec![
            crate::agent::llm::LlmResponse {
                content: vec![crate::agent::llm::ContentBlock::Text { text: adv_json }],
                stop_reason: crate::agent::llm::StopReason::EndTurn,
                input_tokens: 1,
                output_tokens: 1,
            },
            crate::agent::llm::LlmResponse {
                content: vec![crate::agent::llm::ContentBlock::Text { text: syn_json }],
                stop_reason: crate::agent::llm::StopReason::EndTurn,
                input_tokens: 1,
                output_tokens: 1,
            },
            crate::agent::llm::LlmResponse {
                content: vec![crate::agent::llm::ContentBlock::Text {
                    text: judge_json.to_string(),
                }],
                stop_reason: crate::agent::llm::StopReason::EndTurn,
                input_tokens: 1,
                output_tokens: 1,
            },
        ]);
        let runner = make_runner(dispatch);
        let strategy = stub_strategy();
        let candidates = runner.generate_candidates(&strategy, &config).await.unwrap();
        assert_eq!(candidates.len(), 3);
        assert_eq!(candidates[0].kind, CandidateKind::Incumbent);
        assert_eq!(candidates[1].kind, CandidateKind::Adversarial);
        assert_eq!(candidates[2].kind, CandidateKind::Synthesis);
        assert!(candidates[0].diff.is_empty());
    }

    #[tokio::test]
    async fn incumbent_wins_when_ranked_first_by_all_judges() {
        let config = AutoOptimizerConfig::default();
        let adv_json = valid_diff_json();
        let syn_json = valid_diff_json();
        let incumbent_first = r#"{"ranking": [0, 1, 2]}"#;
        let dispatch = MockDispatch::sequence(vec![
            crate::agent::llm::LlmResponse {
                content: vec![crate::agent::llm::ContentBlock::Text { text: adv_json }],
                stop_reason: crate::agent::llm::StopReason::EndTurn,
                input_tokens: 1,
                output_tokens: 1,
            },
            crate::agent::llm::LlmResponse {
                content: vec![crate::agent::llm::ContentBlock::Text { text: syn_json }],
                stop_reason: crate::agent::llm::StopReason::EndTurn,
                input_tokens: 1,
                output_tokens: 1,
            },
            crate::agent::llm::LlmResponse {
                content: vec![crate::agent::llm::ContentBlock::Text {
                    text: incumbent_first.to_string(),
                }],
                stop_reason: crate::agent::llm::StopReason::EndTurn,
                input_tokens: 1,
                output_tokens: 1,
            },
        ]);
        let runner = make_runner(dispatch);
        let strategy = stub_strategy();
        let result = runner.run_tournament(&strategy, &config).await.unwrap();
        assert!(
            result.incumbent_wins,
            "incumbent should win when ranked first by all judges"
        );
    }
}
