use std::sync::Arc;

use anyhow::Context;
use serde::{Deserialize, Serialize};

use crate::agent::llm::{LlmDispatch, LlmRequest, Message};
use crate::autooptimizer::config::AutoOptimizerConfig;
use crate::autooptimizer::content_hash::ContentHash;
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

    /// Apply this diff to `base`, returning the candidate strategy.
    ///
    /// This is the **canonical** apply used by the cycle orchestrator, the
    /// `mutate-once` CLI verb, and the mutator's own identity check, so all
    /// three agree on what a diff actually changes. It applies:
    ///   - `params`: dot-path keys into `mechanical_params` (nested objects are
    ///     created as needed), so a change to `risk.max_positions` lands at the
    ///     right depth instead of a flat top-level key.
    ///   - `tools`: add/remove against `manifest.required_tools`.
    ///
    /// Prose edits are intentionally **not** applied here: a `Strategy`
    /// references library agents by `AgentRef`, so an agent-prompt edit has no
    /// home in the strategy artifact's content hash. A prose-only diff is
    /// therefore an identity (no-op) at the strategy level — [`Mutator::propose`]
    /// detects that and retries for a real change rather than emitting a
    /// guaranteed-zero candidate (F14, QA 2026-06-04). Before this was unified,
    /// the cycle path applied params only (flat, no tools), so a valid tool or
    /// nested-param experiment was silently dropped as "identity".
    pub fn apply_to(&self, base: &Strategy) -> Strategy {
        let mut s = base.clone();
        for change in &self.params {
            set_param_value(&mut s.mechanical_params, &change.key, change.after.clone());
        }
        for added in &self.tools.added {
            if !s.manifest.required_tools.contains(added) {
                s.manifest.required_tools.push(added.clone());
            }
        }
        for removed in &self.tools.removed {
            s.manifest.required_tools.retain(|t| t != removed);
        }
        s
    }
}

/// Set `params[key] = value`, where `key` is a dot path (`a.b.c`). Missing
/// intermediate objects are created. A path that traverses a non-object value
/// is left unchanged rather than clobbering it.
fn set_param_value(params: &mut serde_json::Value, key: &str, value: serde_json::Value) {
    if key.is_empty() {
        return;
    }
    let parts: Vec<&str> = key.splitn(16, '.').collect();
    let (last, prefix) = parts.split_last().expect("splitn yields at least one part");
    if !params.is_object() {
        *params = serde_json::Value::Object(serde_json::Map::new());
    }
    let mut cur = params;
    for &part in prefix {
        let map = match cur.as_object_mut() {
            Some(m) => m,
            None => return,
        };
        cur = map
            .entry(part.to_string())
            .or_insert_with(|| serde_json::Value::Object(serde_json::Map::new()));
    }
    if let Some(map) = cur.as_object_mut() {
        map.insert(last.to_string(), value);
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
                    Ok(()) if is_identity_diff(&diff, base) => {
                        // F14: a diff that leaves the strategy byte-identical is
                        // a guaranteed 0.0-delta no-op and hashes to the parent
                        // (corrupting lineage — F12). Feed it back as an error so
                        // the next attempt proposes a real change rather than the
                        // mutator "succeeding" with nothing to gate.
                        last_errors = Some(vec![ValidationError {
                            code: "identity_diff".into(),
                            message: "the proposed change does not alter the strategy (no-op); \
                                      propose a concrete parameter or tool change"
                                .into(),
                            path: None,
                        }]);
                    }
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

/// True when applying `diff` to `base` produces a strategy with the same
/// content hash — i.e. the diff is a no-op at the strategy-artifact level.
fn is_identity_diff(diff: &MutationDiff, base: &Strategy) -> bool {
    let candidate = diff.apply_to(base);
    match (serde_json::to_value(base), serde_json::to_value(&candidate)) {
        (Ok(b), Ok(c)) => ContentHash::of_json(&b) == ContentHash::of_json(&c),
        // If either fails to serialize we can't prove it's identity; let the
        // downstream gate handle it rather than spuriously rejecting.
        _ => false,
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

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture_strategy() -> Strategy {
        let v = serde_json::json!({
            "manifest": {
                "id": "01HZTEST00000000000000000A",
                "display_name": "Apply Test Strategy",
                "plain_summary": "Minimal strategy for apply/identity tests.",
                "creator": "@test",
                "template": "custom",
                "regime_fit": [],
                "asset_universe": ["BTC/USD"],
                "decision_cadence_minutes": 60,
                "required_tools": ["rsi"],
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
            "mechanical_params": { "ema_fast": 12, "ema_slow": 26 }
        });
        serde_json::from_value(v).expect("fixture strategy must deserialise")
    }

    fn diff_with(params: Vec<ParamChange>, added: Vec<String>, removed: Vec<String>) -> MutationDiff {
        MutationDiff {
            kind: MutationKind::Param,
            prose: Vec::new(),
            params,
            tools: ToolDiff { added, removed },
            rationale: "test".into(),
        }
    }

    #[test]
    fn apply_to_sets_top_level_param() {
        let base = fixture_strategy();
        let diff = diff_with(
            vec![ParamChange {
                key: "ema_fast".into(),
                before: serde_json::json!(12),
                after: serde_json::json!(20),
            }],
            vec![],
            vec![],
        );
        let child = diff.apply_to(&base);
        assert_eq!(child.mechanical_params["ema_fast"], serde_json::json!(20));
        assert_eq!(child.mechanical_params["ema_slow"], serde_json::json!(26));
    }

    #[test]
    fn apply_to_creates_nested_param_path() {
        let base = fixture_strategy();
        let diff = diff_with(
            vec![ParamChange {
                key: "signals.rsi.period".into(),
                before: serde_json::Value::Null,
                after: serde_json::json!(14),
            }],
            vec![],
            vec![],
        );
        let child = diff.apply_to(&base);
        assert_eq!(
            child.mechanical_params["signals"]["rsi"]["period"],
            serde_json::json!(14)
        );
    }

    #[test]
    fn apply_to_adds_and_removes_tools() {
        let base = fixture_strategy();
        let diff = diff_with(vec![], vec!["macd".into()], vec!["rsi".into()]);
        let child = diff.apply_to(&base);
        assert!(child.manifest.required_tools.contains(&"macd".to_string()));
        assert!(!child.manifest.required_tools.contains(&"rsi".to_string()));
    }

    #[test]
    fn identity_diff_detected_for_noop_change() {
        let base = fixture_strategy();
        // Setting a param to its current value is a no-op at the hash level.
        let noop = diff_with(
            vec![ParamChange {
                key: "ema_fast".into(),
                before: serde_json::json!(12),
                after: serde_json::json!(12),
            }],
            vec![],
            vec![],
        );
        assert!(
            is_identity_diff(&noop, &base),
            "no-op param change must be identity"
        );

        // An empty diff is also identity.
        assert!(is_identity_diff(&empty_mutation(), &base));

        // A real change is not identity.
        let real = diff_with(
            vec![ParamChange {
                key: "ema_fast".into(),
                before: serde_json::json!(12),
                after: serde_json::json!(99),
            }],
            vec![],
            vec![],
        );
        assert!(!is_identity_diff(&real, &base));
    }
}
