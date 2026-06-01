use std::sync::Arc;

use anyhow::Result;
use serde::{Deserialize, Serialize};

use crate::agent::llm::{LlmDispatch, LlmRequest, Message};
use crate::autooptimizer::mutator::MutationDiff;
use crate::autooptimizer::program_view::to_markdown;
use crate::strategies::Strategy;

const JUDGE_PROMPT: &str = include_str!("../../prompts/autooptimizer/judge-v1.md");

const FORBIDDEN_METRIC_TOKENS: &[&str] = &[
    "sharpe",
    "drawdown",
    "profit_factor",
    "win_rate",
    "equity_usd",
    "pnl",
    "max_drawdown",
    "calmar",
    "sortino",
];

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FindingSeverity {
    Info,
    Warn,
    Risk,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Finding {
    pub code: String,
    pub severity: FindingSeverity,
    pub summary: String,
    pub detail: Option<String>,
}

pub struct Judge {
    pub dispatch: Arc<dyn LlmDispatch + Send + Sync>,
    pub provider: String,
    pub model: String,
}

/// Asserts that the given text contains none of the forbidden metric tokens.
/// Called on the user body before dispatch to ensure the judge is metrics-blind.
pub fn assert_metrics_blind(text: &str) {
    let lower = text.to_lowercase();
    for token in FORBIDDEN_METRIC_TOKENS {
        assert!(
            !lower.contains(token),
            "judge prompt must not include metrics: found '{token}'"
        );
    }
}

fn build_user_body(
    parent: &Strategy,
    child: &Strategy,
    diff: &MutationDiff,
    tape: &str,
) -> String {
    let parent_md = to_markdown(parent);
    let child_md = to_markdown(child);
    let diff_json =
        serde_json::to_string_pretty(diff).unwrap_or_else(|_| String::from("{}"));

    format!(
        "## Parent experiment\n\n{parent_md}\n\n\
         ## Accepted experiment (child)\n\n{child_md}\n\n\
         ## Changes applied\n\n```json\n{diff_json}\n```\n\n\
         ## Trade activity sample\n\n{tape}\n\n\
         Produce qualitative findings about this accepted experiment."
    )
}

fn extract_findings(text: &str) -> Vec<Finding> {
    let trimmed = text.trim();
    let json_str = trimmed
        .strip_prefix("```json")
        .or_else(|| trimmed.strip_prefix("```"))
        .map(|s| s.trim_start())
        .and_then(|s| s.strip_suffix("```"))
        .map(|s| s.trim_end())
        .unwrap_or(trimmed);

    match serde_json::from_str::<Vec<Finding>>(json_str) {
        Ok(findings) => findings,
        Err(e) => vec![Finding {
            code: "parse_error".into(),
            severity: FindingSeverity::Info,
            summary: format!("could not parse judge response: {e}"),
            detail: None,
        }],
    }
}

pub async fn run_judge(
    judge: &Judge,
    parent_strategy: &Strategy,
    child_strategy: &Strategy,
    diff: &MutationDiff,
    trade_tape_excerpt: &str,
) -> Result<Vec<Finding>> {
    let body = build_user_body(parent_strategy, child_strategy, diff, trade_tape_excerpt);
    assert_metrics_blind(&body);

    let req = LlmRequest {
        model: judge.model.clone(),
        system_prompt: JUDGE_PROMPT.to_string(),
        messages: vec![Message::user_text(body)],
        max_tokens: None,
        tools: vec![],
        temperature: None,
        response_schema: None,
        cache_control: None,
    };

    let resp = judge.dispatch.complete(req).await?;
    Ok(extract_findings(&resp.text()))
}
