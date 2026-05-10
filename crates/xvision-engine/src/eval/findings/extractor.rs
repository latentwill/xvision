//! `extract_findings` — drive the v1 OSShip-style prompt against a
//! finished run + decision/equity summaries, parse the LLM's JSON array
//! reply, attach run-id + timestamps + ULIDs, return `Vec<Finding>`.
//!
//! The extractor is robust to a small amount of pre/post-prose: it locates
//! the first `[` and last `]` in the response and parses the slice between
//! them. Anything outside is ignored. This is intentional — real models
//! occasionally violate the "ONLY the JSON array" instruction in the
//! prompt, and a tight retry loop is more annoying than slicing.

use std::sync::Arc;

use anyhow::{anyhow, Context, Result};
use chrono::Utc;
use ulid::Ulid;

use crate::agent::llm::{LlmDispatch, LlmRequest, Message};
use crate::eval::findings::{Finding, Severity};
use crate::eval::run::Run;

const PROMPT: &str = include_str!("prompts/extractor-v1.md");

#[derive(serde::Deserialize)]
struct RawFinding {
    kind: String,
    severity: Severity,
    summary: String,
    evidence: serde_json::Value,
}

pub async fn extract_findings(
    run: &Run,
    decisions_summary: serde_json::Value,
    equity_summary: serde_json::Value,
    dispatch: Arc<dyn LlmDispatch>,
    model: &str,
) -> Result<Vec<Finding>> {
    let user_payload = serde_json::json!({
        "run_metrics": run.metrics,
        "decisions_summary": decisions_summary,
        "equity_curve_summary": equity_summary,
    });

    let user_text = serde_json::to_string_pretty(&user_payload)
        .context("serialize findings extractor user payload")?;
    let req = LlmRequest {
        model: model.to_string(),
        system_prompt: PROMPT.to_string(),
        messages: vec![Message::user_text(user_text)],
        max_tokens: 2000,
        tools: vec![],
    };
    let resp = dispatch.complete(req).await?;
    let text = resp.text();
    let text = text.as_str();

    let json_start = text
        .find('[')
        .ok_or_else(|| anyhow!("findings extractor response has no JSON array"))?;
    let json_end = text
        .rfind(']')
        .map(|i| i + 1)
        .ok_or_else(|| anyhow!("findings extractor response missing closing ']'"))?;
    if json_end <= json_start {
        return Err(anyhow!(
            "findings extractor response: ']' before '[' (text: {text:?})",
        ));
    }

    let raw: Vec<RawFinding> = serde_json::from_str(&text[json_start..json_end])
        .with_context(|| format!("parse findings JSON array (sliced text: {:?})", &text[json_start..json_end]))?;
    let now = Utc::now();
    Ok(raw
        .into_iter()
        .map(|r| Finding {
            id: Ulid::new().to_string(),
            run_id: run.id.clone(),
            kind: r.kind,
            severity: r.severity,
            summary: r.summary,
            evidence: r.evidence,
            extracted_at: now,
            schema_version: "1".into(),
        })
        .collect())
}
