//! Three-candidate tournament for candidate-generation upgrade (Phase 4).
//!
//! Replaces the single-shot mutator with: incumbent (no change) + adversarial
//! + synthesis, judged blind via Borda count. Numeric gate still runs after
//! the tournament picks a winner.

use std::sync::Arc;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::agent::llm::{LlmDispatch, LlmRequest, Message, ResponseSchema};
use crate::autooptimizer::config::AutoOptimizerConfig;
use crate::autooptimizer::mutator::{
    annotated_filter_paths_section, applicable_mutation_kinds, empty_mutation, filter_create_directive,
    filter_tunable_paths, kind_focus_directive, tunable_param_keys, MutationDiff, Mutator,
};
use crate::autooptimizer::program_view;
use crate::autooptimizer::validator::{validate_mutation_diff, ValidationError};
use crate::strategies::Strategy;

const ADVERSARIAL_PROMPT: &str = include_str!("../../prompts/autooptimizer/tournament-adversarial-v1.md");
const SYNTHESIS_PROMPT: &str = include_str!("../../prompts/autooptimizer/tournament-synthesis-v1.md");
const JUDGE_NUMERIC_PROMPT: &str = include_str!("../../prompts/autooptimizer/tournament-judge-numeric-v1.md");
const JUDGE_CONSISTENCY_PROMPT: &str =
    include_str!("../../prompts/autooptimizer/tournament-judge-consistency-v1.md");
const JUDGE_RISK_PROMPT: &str = include_str!("../../prompts/autooptimizer/tournament-judge-risk-v1.md");

pub const CANDIDATE_COUNT: usize = 3;
const JUDGE_COUNT: usize = 3;
const MAX_PARAMS_APPLY: usize = 64;

/// Judge persona — each evaluates candidates through a different lens.
/// Maps to the multi-persona review pattern from the AutoResearch self-play paper.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JudgePersona {
    Numeric,
    Consistency,
    Risk,
}

impl JudgePersona {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Numeric => "numeric",
            Self::Consistency => "consistency",
            Self::Risk => "risk",
        }
    }

    fn prompt(&self) -> &'static str {
        match self {
            Self::Numeric => JUDGE_NUMERIC_PROMPT,
            Self::Consistency => JUDGE_CONSISTENCY_PROMPT,
            Self::Risk => JUDGE_RISK_PROMPT,
        }
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CandidateKind {
    Incumbent,
    Adversarial,
    Synthesis,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BordaVote {
    /// Candidate indices ordered best-first. `ranking[0]` = 1st place.
    pub ranking: [usize; CANDIDATE_COUNT],
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TournamentCandidate {
    pub kind: CandidateKind,
    pub strategy: Strategy,
    pub diff: MutationDiff,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TournamentResult {
    pub winner_kind: CandidateKind,
    pub winner_diff: MutationDiff,
    pub winner_strategy: Strategy,
    pub incumbent_wins: bool,
    pub borda_scores: [u32; CANDIDATE_COUNT],
    /// Per-persona rankings from each judge (Numeric, Consistency, Risk).
    pub per_persona_votes: Vec<(JudgePersona, BordaVote)>,
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
        resolved_agent_prompts: Option<&std::collections::HashMap<String, String>>,
    ) -> Result<Vec<TournamentCandidate>> {
        let incumbent = TournamentCandidate {
            kind: CandidateKind::Incumbent,
            strategy: parent.clone(),
            diff: empty_mutation(),
        };
        let adversarial_sys = system_section(ADVERSARIAL_PROMPT);
        let synthesis_sys = system_section(SYNTHESIS_PROMPT);
        // xvision-ds0: give each candidate a distinct rotation slot (0 / 1) so
        // B6 kind-rotation focuses different levers across the two writers, the
        // same way `mutation_idx` does on the single-shot path.
        let (adv_diff, syn_diff) = tokio::try_join!(
            self.propose_diff(parent, config, &adversarial_sys, resolved_agent_prompts, 0),
            self.propose_diff(parent, config, &synthesis_sys, resolved_agent_prompts, 1),
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
        resolved_agent_prompts: Option<&std::collections::HashMap<String, String>>,
        candidate_slot: usize,
    ) -> Result<MutationDiff> {
        let empty_map = std::collections::HashMap::new();
        let resolved = resolved_agent_prompts.unwrap_or(&empty_map);
        let program_md = program_view::to_markdown_with_resolved_prompts(parent, resolved);

        // xvision-ds0: compute the same per-parent lever inputs the single-shot
        // mutator uses, so the tournament prompt carries the B17 filter
        // domain-constraint hints and the B6 kind-rotation directive instead of a
        // bare kind list. Enumerated once (filter/kinds don't change per attempt).
        let kinds = applicable_mutation_kinds(parent, &config.allowed_mutation_kinds);
        let kinds = if kinds.is_empty() {
            vec!["param".to_string()]
        } else {
            kinds
        };
        let param_keys = tunable_param_keys(parent);
        let filter_paths: Vec<(String, serde_json::Value)> = if kinds.iter().any(|k| k == "filter") {
            parent
                .filter
                .as_ref()
                .map(filter_tunable_paths)
                .unwrap_or_default()
        } else {
            Vec::new()
        };
        let prose_roles: Vec<String> = if kinds.iter().any(|k| k == "prose") {
            parent.agents.iter().map(|a| a.role.clone()).collect()
        } else {
            Vec::new()
        };

        let mut last_err: Option<String> = None;
        let max_attempts = self.max_retries.saturating_add(1);
        assert!(max_attempts >= 1, "max_attempts must be at least 1");
        for attempt in 0..max_attempts {
            // Seed the target-within-kind rotation off the candidate slot so the
            // two tournament writers explore different concrete levers.
            let exploration_seed = (candidate_slot as u64)
                .wrapping_mul(31)
                .wrapping_add(attempt as u64);
            let user_text = build_proposal_user(
                &program_md,
                &kinds,
                &param_keys,
                &filter_paths,
                &prose_roles,
                candidate_slot,
                attempt as usize,
                exploration_seed,
                parent.filter.is_some(),
                last_err.as_deref(),
            );
            let req = LlmRequest {
                model: self.model.clone(),
                system_prompt: system_prompt.to_string(),
                messages: vec![Message::user_text(user_text)],
                max_tokens: None,
                tools: vec![],
                temperature: None,
                // B3: constrain to the `mutation_diff` schema so OpenAI-compat
                // dispatchers (Ollama) grammar-constrain the JSON output, matching
                // the single-shot mutator path.
                response_schema: Some(ResponseSchema::mutation_diff()),
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

    pub async fn borda_vote(
        &self,
        candidates: &[TournamentCandidate],
    ) -> Result<(Vec<BordaVote>, Vec<(JudgePersona, BordaVote)>)> {
        assert_eq!(
            candidates.len(),
            CANDIDATE_COUNT,
            "borda_vote requires {CANDIDATE_COUNT} candidates"
        );
        let summary = build_candidate_summary(candidates);
        // Dispatch 3 persona-differentiated judges in parallel.
        let (v_numeric, v_consistency, v_risk) = tokio::try_join!(
            self.one_judge_vote(&summary, JudgePersona::Numeric),
            self.one_judge_vote(&summary, JudgePersona::Consistency),
            self.one_judge_vote(&summary, JudgePersona::Risk),
        )?;
        let per_persona = vec![
            (JudgePersona::Numeric, v_numeric.clone()),
            (JudgePersona::Consistency, v_consistency.clone()),
            (JudgePersona::Risk, v_risk.clone()),
        ];
        Ok((vec![v_numeric, v_consistency, v_risk], per_persona))
    }

    async fn one_judge_vote(&self, candidate_summary: &str, persona: JudgePersona) -> Result<BordaVote> {
        let req = LlmRequest {
            model: self.model.clone(),
            system_prompt: persona.prompt().to_string(),
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
        resolved_agent_prompts: Option<&std::collections::HashMap<String, String>>,
    ) -> Result<TournamentResult> {
        let candidates = self
            .generate_candidates(parent, config, resolved_agent_prompts)
            .await?;
        assert_eq!(candidates.len(), CANDIDATE_COUNT);
        let (votes, per_persona_votes) = self.borda_vote(&candidates).await?;
        let borda_scores = Self::tally(&votes);
        let winner_index = pick_winner(&borda_scores);
        let winner = &candidates[winner_index];
        Ok(TournamentResult {
            winner_kind: winner.kind,
            winner_diff: winner.diff.clone(),
            winner_strategy: winner.strategy.clone(),
            incumbent_wins: winner.kind == CandidateKind::Incumbent,
            borda_scores,
            per_persona_votes,
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
    // Delegate to the canonical applier so adversarial/synthesis candidates get
    // the same risk.* / mechanistic.* / filter / prose routing as normal
    // experiments. (Previously this inserted into the now-removed
    // `mechanical_params` blob, bypassing that routing.)
    diff.apply_to(base)
}

fn system_section(prompt_template: &str) -> String {
    let marker = "# USER";
    if let Some(idx) = prompt_template.find(marker) {
        prompt_template[..idx].trim().to_string()
    } else {
        prompt_template.to_string()
    }
}

#[allow(clippy::too_many_arguments)]
fn build_proposal_user(
    program_md: &str,
    allowed_kinds: &[String],
    param_keys: &[String],
    filter_paths: &[(String, serde_json::Value)],
    prose_roles: &[String],
    candidate_slot: usize,
    attempt: usize,
    exploration_seed: u64,
    // xvision-vxn: whether the parent already has a filter (drives create-vs-tune
    // guidance, shared with the single-shot path via `filter_create_directive`).
    filter_exists: bool,
    prev_err: Option<&str>,
) -> String {
    let kinds = allowed_kinds.join(", ");
    // B17 (xvision-ds0): list tunable filter paths with their domain-constraint
    // hints, shared verbatim with the single-shot mutator path.
    let filter_section = annotated_filter_paths_section(allowed_kinds, filter_paths);
    // xvision-vxn: when the parent has no filter, invite a structural create.
    let create_section = filter_create_directive(allowed_kinds, filter_exists);
    // B6 (xvision-ds0): rotate the focused mutation kind across retries / slots.
    let focus_section = kind_focus_directive(
        allowed_kinds,
        param_keys,
        filter_paths,
        prose_roles,
        candidate_slot,
        attempt,
        exploration_seed,
    );
    let err_section = prev_err
        .map(|e| format!("\n\nPrevious attempt errors — fix all:\n\n{e}"))
        .unwrap_or_default();
    format!(
        "Strategy program view:\n\n{program_md}\n\nAllowed experiment kinds: \
         {kinds}{filter_section}{create_section}{focus_section}{err_section}\n\nPropose ONE experiment as a JSON object."
    )
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
            }
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
            create_filter: None,
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

    // ── xvision-ds0: tournament proposer mirrors B6 + B17 ────────────────────

    #[test]
    fn tournament_proposal_user_annotates_filter_path_domain_constraints() {
        // B17: the tournament prompt must carry the same per-path domain hints as
        // the single-shot mutator, so the writer stops proposing out-of-domain
        // filter values that the validator rejects.
        let allowed = vec!["filter".to_string()];
        let filter_paths = vec![
            ("conditions.0.op.zscore_lt".to_string(), serde_json::json!(3)),
            ("conditions.1.op.within_pct".to_string(), serde_json::json!(1.5)),
            ("conditions.2.rhs.numeric".to_string(), serde_json::json!(25.0)),
        ];
        let out = build_proposal_user(
            "PROGRAM",
            &allowed,
            &[],
            &filter_paths,
            &[],
            0,
            0,
            0,
            /* filter_exists */ true,
            None,
        );
        assert!(
            out.contains("conditions.0.op.zscore_lt: 3  (positive integer >= 1)"),
            "zscore_lt must be annotated as a positive integer; got:\n{out}"
        );
        assert!(
            out.contains("conditions.1.op.within_pct: 1.5  (positive number > 0)"),
            "within_pct must be annotated as a positive number; got:\n{out}"
        );
        assert!(
            out.contains("conditions.2.rhs.numeric: 25.0\n")
                || out.trim_end().ends_with("conditions.2.rhs.numeric: 25.0"),
            "plain numeric paths must NOT get a domain annotation; got:\n{out}"
        );
    }

    #[test]
    fn tournament_proposal_user_rotates_focus_kind_across_attempts() {
        // B6: holding the candidate slot fixed and varying the retry attempt must
        // rotate the focused mutation KIND, so retries don't re-hammer one
        // failing kind until the attempt budget is gone.
        let allowed = vec!["prose".to_string(), "filter".to_string(), "param".to_string()];
        let param_keys = vec!["risk.risk_pct_per_trade".to_string()];
        let filter_paths = vec![("conditions.0.rhs.numeric".to_string(), serde_json::json!(25.0))];
        let prose_roles = vec!["trader".to_string()];

        let focused_kind = |out: &str| -> &'static str {
            if out.contains("agent's system prompt") {
                "prose"
            } else if out.contains("filter path") {
                "filter"
            } else if out.contains("parameter `") {
                "param"
            } else {
                "none"
            }
        };

        let mut kinds_seen = std::collections::HashSet::new();
        for attempt in 0..3usize {
            let out = build_proposal_user(
                "PROGRAM",
                &allowed,
                &param_keys,
                &filter_paths,
                &prose_roles,
                0,
                attempt,
                attempt as u64,
                /* filter_exists */ true,
                None,
            );
            kinds_seen.insert(focused_kind(&out));
        }
        assert!(
            kinds_seen.len() >= 2,
            "retries must rotate across at least 2 distinct kinds; saw {kinds_seen:?}"
        );
        assert!(
            !kinds_seen.contains("none"),
            "every attempt must focus a concrete kind; saw {kinds_seen:?}"
        );
    }

    #[test]
    fn tournament_proposal_user_offers_filter_creation_when_no_filter() {
        // xvision-vxn: the tournament proposer, like the single-shot path, must
        // invite the writer to AUTHOR a filter (create_filter) when the parent
        // has none and `filter` is an allowed kind.
        let allowed = vec!["filter".to_string(), "param".to_string()];
        let out = build_proposal_user(
            "PROGRAM",
            &allowed,
            &[],
            &[],
            &[],
            0,
            0,
            0,
            /* filter_exists */ false,
            None,
        );
        assert!(
            out.contains("create_filter"),
            "filterless tournament parent + filter allowed must invite create_filter; got:\n{out}"
        );
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
        let candidates = runner
            .generate_candidates(&strategy, &config, None)
            .await
            .unwrap();
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
        let result = runner.run_tournament(&strategy, &config, None).await.unwrap();
        assert!(
            result.incumbent_wins,
            "incumbent should win when ranked first by all judges"
        );
    }

    /// Capturing dispatch for B3: records every request so the test can assert the
    /// tournament's proposal request carries the constrained `mutation_diff`
    /// schema. Always returns the same canned valid diff.
    struct CapturingDispatch {
        canned: String,
        captured: std::sync::Mutex<Vec<LlmRequest>>,
    }

    #[async_trait::async_trait]
    impl LlmDispatch for CapturingDispatch {
        async fn complete(&self, req: LlmRequest) -> Result<crate::agent::llm::LlmResponse> {
            self.captured.lock().unwrap().push(req);
            Ok(crate::agent::llm::LlmResponse {
                content: vec![crate::agent::llm::ContentBlock::Text {
                    text: self.canned.clone(),
                }],
                stop_reason: crate::agent::llm::StopReason::EndTurn,
                input_tokens: 1,
                output_tokens: 1,
            })
        }
    }

    #[tokio::test]
    async fn tournament_proposal_request_carries_mutation_diff_schema() {
        // B3: the tournament's propose_diff path must request the constrained
        // `mutation_diff` schema (mirrors the single-shot mutator).
        let dispatch = Arc::new(CapturingDispatch {
            canned: valid_diff_json(),
            captured: std::sync::Mutex::new(Vec::new()),
        });
        let runner = TournamentRunner {
            dispatch: dispatch.clone() as Arc<dyn LlmDispatch + Send + Sync>,
            model: "test-model".into(),
            provider: "test".into(),
            max_retries: 0,
        };
        let strategy = stub_strategy();
        let config = AutoOptimizerConfig::default();
        let _ = runner
            .propose_diff(&strategy, &config, "sys", None, 0)
            .await
            .expect("propose_diff should succeed on a valid diff");

        let captured = dispatch.captured.lock().unwrap();
        let schema = captured[0]
            .response_schema
            .as_ref()
            .expect("tournament request must carry a response_schema (B3)");
        assert_eq!(schema.name, "mutation_diff");
        assert!(
            schema.schema.pointer("/properties/kind").is_some(),
            "mutation_diff schema must enumerate `kind`"
        );
    }
}
