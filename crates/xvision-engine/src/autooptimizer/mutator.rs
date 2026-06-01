use std::sync::Arc;

use anyhow::Context;
use serde::{Deserialize, Serialize};

use crate::agent::llm::{LlmDispatch, LlmRequest, Message};
use crate::autooptimizer::config::AutoOptimizerConfig;
use crate::autooptimizer::program_view;
use crate::autooptimizer::validator::{validate_mutation_diff, ValidationError};
use crate::strategies::Strategy;

const PROMPT_TEMPLATE: &str = include_str!("../../prompts/autooptimizer/mutator-v1.md");

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MutationKind {
    Prose,
    Param,
    Tool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProseEdit {
    pub agent_role: String,
    pub before: String,
    pub after: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParamChange {
    pub key: String,
    pub before: serde_json::Value,
    pub after: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDiff {
    pub added: Vec<String>,
    pub removed: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MutationDiff {
    pub kind: MutationKind,
    pub prose: Vec<ProseEdit>,
    pub params: Vec<ParamChange>,
    pub tools: ToolDiff,
    pub rationale: String,
}

pub fn empty_mutation() -> MutationDiff {
    MutationDiff {
        kind: MutationKind::Prose,
        prose: Vec::new(),
        params: Vec::new(),
        tools: ToolDiff {
            added: Vec::new(),
            removed: Vec::new(),
        },
        rationale: String::new(),
    }
}

impl MutationDiff {
    pub fn is_empty(&self) -> bool {
        self.prose.is_empty()
            && self.params.is_empty()
            && self.tools.added.is_empty()
            && self.tools.removed.is_empty()
    }
}

pub struct Mutator {
    pub provider: String,
    pub model: String,
    pub dispatch: Arc<dyn LlmDispatch + Send + Sync>,
    pub max_retries: u32,
}

impl Mutator {
    pub async fn propose(
        &self,
        base: &Strategy,
        config: &AutoOptimizerConfig,
        dsr_prefix: Option<&str>,
    ) -> anyhow::Result<MutationDiff> {
        let program_md = program_view::to_markdown(base);
        let mut last_errors: Option<Vec<ValidationError>> = None;
        let max_attempts = self.max_retries.saturating_add(1);

        assert!(max_attempts >= 1, "max_attempts must be at least 1");

        for attempt in 0..max_attempts {
            let user_text = build_user_payload(
                &program_md,
                &config.allowed_mutation_kinds,
                last_errors.as_deref(),
            );
            let req = LlmRequest {
                model: self.model.clone(),
                system_prompt: build_system_prompt(dsr_prefix),
                messages: vec![Message::user_text(user_text)],
                max_tokens: None,
                tools: vec![],
                temperature: None,
                response_schema: None,
                cache_control: None,
            };

            let resp = self
                .dispatch
                .complete(req)
                .await
                .with_context(|| format!("mutator dispatch failed on attempt {attempt}"))?;
            let raw_text = resp.text();

            let parse_result = extract_and_parse(&raw_text);
            match parse_result {
                Err(parse_err) => {
                    let synthetic = vec![ValidationError {
                        code: "parse_error".into(),
                        message: parse_err.to_string(),
                        path: None,
                    }];
                    last_errors = Some(synthetic);
                }
                Ok(diff) => match validate_mutation_diff(&diff, base) {
                    Ok(()) => return Ok(diff),
                    Err(errors) => {
                        last_errors = Some(errors);
                    }
                },
            }
        }

        let error_text = last_errors
            .as_deref()
            .map(format_validation_errors)
            .unwrap_or_else(|| "unknown error".into());

        anyhow::bail!("mutator failed after {} attempt(s): {}", max_attempts, error_text)
    }
}

fn system_prompt_text() -> String {
    let marker = "# USER";
    if let Some(idx) = PROMPT_TEMPLATE.find(marker) {
        PROMPT_TEMPLATE[..idx].trim().to_string()
    } else {
        PROMPT_TEMPLATE.to_string()
    }
}

fn build_system_prompt(dsr_prefix: Option<&str>) -> String {
    let base = system_prompt_text();
    match dsr_prefix {
        None | Some("") => base,
        Some(prefix) => format!("{prefix}\n\n---\n\n{base}"),
    }
}

fn build_user_payload(
    program_md: &str,
    allowed_kinds: &[String],
    previous_errors: Option<&[ValidationError]>,
) -> String {
    let kinds_text = allowed_kinds.join(", ");
    let errors_section = match previous_errors {
        None => String::new(),
        Some(errs) => {
            format!(
                "\n\nPrevious attempt errors — you MUST fix all of these:\n\n{}",
                format_validation_errors(errs)
            )
        }
    };

    format!(
        "Strategy program view:\n\n{program_md}\n\nAllowed experiment kinds: {kinds_text}{errors_section}\n\nPropose ONE experiment as a JSON object."
    )
}

fn extract_and_parse(text: &str) -> anyhow::Result<MutationDiff> {
    let json_str = extract_json_from_response(text);
    serde_json::from_str::<MutationDiff>(json_str).context("failed to parse MutationDiff from LLM response")
}

fn extract_json_from_response(text: &str) -> &str {
    let trimmed = text.trim();
    let stripped = trimmed
        .strip_prefix("```json")
        .or_else(|| trimmed.strip_prefix("```"))
        .map(|s| s.trim_start())
        .and_then(|s| s.strip_suffix("```"))
        .map(|s| s.trim_end());
    stripped.unwrap_or(trimmed)
}

fn format_validation_errors(errors: &[ValidationError]) -> String {
    assert!(
        !errors.is_empty(),
        "format_validation_errors called with empty slice"
    );
    errors
        .iter()
        .map(|e| {
            if let Some(path) = &e.path {
                format!("- [{}] {} (at {})", e.code, e.message, path)
            } else {
                format!("- [{}] {}", e.code, e.message)
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}
