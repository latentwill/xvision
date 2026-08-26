use std::sync::Arc;

use anyhow::Result;
use serde::{Deserialize, Serialize};

use crate::agent::llm::{LlmDispatch, LlmRequest, Message};
use crate::autooptimizer::mutator::MutationDiff;
use crate::autooptimizer::program_view::to_markdown;
use crate::strategies::Strategy;

const JUDGE_PROMPT: &str = include_str!("../../prompts/autooptimizer/judge-v1.md");

/// Cortex namespace the Judge recalls prior distilled findings from and
/// writes new ones back to. Subsurface (developer-facing) name per the
/// autooptimizer terminology lock; never collapses to bare `optimizer`.
pub const JUDGE_MEMORY_NS: &str = "autooptimizer:judge";

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

/// Returns an error when dynamic review evidence contains forbidden performance
/// metric tokens. Static strategy JSON/markdown is allowed to mention risk
/// controls such as `max_drawdown_usd`; those are not hidden backtest scores.
pub fn ensure_metrics_blind(label: &str, text: &str) -> Result<()> {
    let lower = text
        .to_lowercase()
        // These are risk-control configuration keys, not outcome metrics.
        // They are present in every current RiskConfig serialization and must
        // not make an otherwise metrics-blind Judge prompt fail closed.
        .replace("max_drawdown_usd", "")
        .replace("max_drawdown_pct", "");
    for token in FORBIDDEN_METRIC_TOKENS {
        if lower.contains(token) {
            return Err(anyhow::anyhow!(
                "judge prompt must not include metrics in {label}: found '{token}'"
            ));
        }
    }
    Ok(())
}

fn build_user_body(parent: &Strategy, child: &Strategy, diff: &MutationDiff, tape: &str) -> String {
    let parent_md = to_markdown(parent);
    let child_md = to_markdown(child);
    let diff_json = serde_json::to_string_pretty(diff).unwrap_or_else(|_| String::from("{}"));

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

/// Assemble the Judge system prompt, optionally prefixing recalled
/// Patterns. Returns the plain `JUDGE_PROMPT` whenever `memory` is
/// `None`, no embedder is available, recall is empty, or recall errors —
/// recall is strictly best-effort and never propagates.
async fn build_system_prompt(
    parent_strategy: &Strategy,
    diff: &MutationDiff,
    memory: Option<&crate::agent::memory_recorder::MemoryRecorder>,
    scenario_start: Option<chrono::DateTime<chrono::Utc>>,
) -> String {
    use crate::agent::memory_recorder::{render_recalled_patterns, RecallResult};

    let Some(mem) = memory else {
        return JUDGE_PROMPT.to_string();
    };

    // Concise recall query: parent name + the diff shape. Enough signal
    // to match a relevant prior Pattern without leaking metrics (the diff
    // describes config changes, not outcomes).
    let diff_json = serde_json::to_string(diff).unwrap_or_else(|_| String::from("{}"));
    let query = format!(
        "strategy {} change {}",
        parent_strategy.manifest.display_name, diff_json
    );

    match mem
        .recall_in_namespace(JUDGE_MEMORY_NS, &query, 3, scenario_start)
        .await
    {
        Ok(RecallResult::Hits { matches, .. }) if !matches.is_empty() => {
            format!("{}\n\n{}", render_recalled_patterns(&matches), JUDGE_PROMPT)
        }
        Ok(RecallResult::Hits { .. }) => {
            tracing::info!("judge recall: no prior patterns matched; using plain prompt");
            JUDGE_PROMPT.to_string()
        }
        Ok(RecallResult::NoEmbedder { .. }) => {
            // Demoted to debug: fires on every judge call during optimizer runs,
            // so at the default `info` level it repeated the no-embedder notice
            // per call. Silent by default; visible with RUST_LOG=debug.
            tracing::debug!("judge recall: no embedder; using plain prompt");
            JUDGE_PROMPT.to_string()
        }
        Ok(RecallResult::Skipped) => JUDGE_PROMPT.to_string(),
        Err(e) => {
            tracing::warn!("judge recall failed (best-effort, ignoring): {e}");
            JUDGE_PROMPT.to_string()
        }
    }
}

/// Run the qualitative Judge over an accepted experiment.
///
/// Cortex recall (default-off; enabled when `memory` is `Some`): before
/// dispatch, the Judge recalls prior distilled Patterns from
/// [`JUDGE_MEMORY_NS`] and prepends the case-law `<prior_observations>`
/// block to the **system** prompt. Recall is best-effort — a missing
/// embedder, empty result, or any store error degrades silently to the
/// plain `JUDGE_PROMPT` (logged, never propagated), so a memory-disabled
/// or failing recall is byte-for-byte today's behavior.
///
/// `scenario_start` is forwarded to the store for temporal safety: the
/// Judge runs in eval context, so the cycle passes
/// `Some(scenario.time_window.start)` to exclude Patterns trained inside
/// the scenario window.
///
/// `assert_metrics_blind` stays on the **user body** only — recalled
/// text lands in the system prompt, so a Pattern that mentions a metric
/// can't trip the body invariant.
pub async fn run_judge(
    judge: &Judge,
    parent_strategy: &Strategy,
    child_strategy: &Strategy,
    diff: &MutationDiff,
    trade_tape_excerpt: &str,
    memory: Option<&crate::agent::memory_recorder::MemoryRecorder>,
    scenario_start: Option<chrono::DateTime<chrono::Utc>>,
) -> Result<Vec<Finding>> {
    ensure_metrics_blind("trade activity sample", trade_tape_excerpt)?;
    let body = build_user_body(parent_strategy, child_strategy, diff, trade_tape_excerpt);

    let system_prompt = build_system_prompt(parent_strategy, diff, memory, scenario_start).await;

    let req = LlmRequest {
        model: judge.model.clone(),
        system_prompt,
        messages: vec![Message::user_text(body)],
        max_tokens: None,
        tools: vec![],
        temperature: None,
        response_schema: None,
        cache_control: None,
        force_json: true,
    };

    let resp = judge.dispatch.complete(req).await?;
    Ok(extract_findings(&resp.text()))
}

#[cfg(test)]
mod tests {
    use crate::agent::llm::{ContentBlock, StopReason};

    struct StubDispatch;

    #[async_trait::async_trait]
    impl LlmDispatch for StubDispatch {
        async fn complete(&self, _req: LlmRequest) -> anyhow::Result<crate::agent::llm::LlmResponse> {
            Ok(crate::agent::llm::LlmResponse {
                content: vec![ContentBlock::Text {
                    text: "[]".to_string(),
                }],
                stop_reason: StopReason::EndTurn,
                input_tokens: 0,
                output_tokens: 0,
            })
        }
    }

    fn stub_judge() -> Judge {
        Judge {
            dispatch: Arc::new(StubDispatch),
            provider: "test".to_string(),
            model: "test".to_string(),
        }
    }

    use super::*;
    use serde_json::json;

    fn strategy_with_drawdown_risk_fields() -> Strategy {
        let v = json!({
            "manifest": {
                "id": "01HZTEST00000000000000000J",
                "display_name": "Judge Test Strategy",
                "plain_summary": "",
                "creator": "@test",
                "template": "custom",
                "regime_fit": [],
                "asset_universe": ["BTC/USD"],
                "decision_cadence_minutes": 60,
                "required_tools": ["rsi"],
                "risk_preset_or_config": "balanced"
            },
            "agents": [{"agent_id": "01HZAGENT0000000000000000J", "role": "trader"}],
            "risk": {
                "risk_pct_per_trade": 0.015,
                "max_concurrent_positions": 2,
                "max_leverage": 1.0,
                "stop_loss_atr_multiple": 2.0,
                "daily_loss_kill_pct": 0.05,
                "max_total_exposure_pct": 150.0,
                "max_drawdown_usd": 0.0,
                "max_drawdown_pct": null
            },
            "activation_mode": "every_bar"
        });
        serde_json::from_value(v).expect("fixture strategy must deserialize")
    }

    #[tokio::test]
    async fn run_judge_allows_strategy_schema_drawdown_fields() {
        let strategy = strategy_with_drawdown_risk_fields();
        let diff = crate::autooptimizer::mutator::empty_mutation();

        let findings = run_judge(&stub_judge(), &strategy, &strategy, &diff, "", None, None)
            .await
            .expect("strategy risk-control drawdown fields must not trip the prompt gate");

        assert!(findings.is_empty());
    }

    #[test]
    fn full_body_would_trip_the_old_broad_validator() {
        let strategy = strategy_with_drawdown_risk_fields();
        let diff = crate::autooptimizer::mutator::empty_mutation();

        let body = build_user_body(&strategy, &strategy, &diff, "");

        assert!(body.contains("max_drawdown_usd"));
        // Risk-control configuration keys are not outcome metrics: the
        // metrics-blind check allowlists them instead of failing closed.
        ensure_metrics_blind("full judge body", &body)
            .expect("drawdown risk keys must not trip the metrics-blind check");
    }

    #[test]
    fn trade_tape_metric_leak_returns_error() {
        let err = ensure_metrics_blind("trade activity sample", "sharpe improved")
            .expect_err("metric names must stay out of the dynamic trade sample");

        assert!(err
            .to_string()
            .contains("judge prompt must not include metrics in trade activity sample"));
    }
}
