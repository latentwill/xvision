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

/// Numeric `RiskConfig` fields the mutator may tune via `risk.<field>` param
/// keys. F14/F20 (QA 2026-06-04): the real strategies on the node all have an
/// empty `mechanical_params`; their only tunable knobs live in `risk`, so
/// without this the optimizer could never produce a valid param experiment for
/// any real strategy. Keep in sync with `RiskConfig` (xvision-risk).
pub const RISK_PARAM_FIELDS: &[&str] = &[
    "risk_pct_per_trade",
    "max_concurrent_positions",
    "max_leverage",
    "stop_loss_atr_multiple",
    "daily_loss_kill_pct",
    "max_position_pct_nav",
];

/// If `key` addresses a tunable `risk` field — either `risk.<field>` or a bare
/// `<field>` that isn't shadowed by a `mechanical_params` key — return the field
/// name; otherwise `None` (the key targets `mechanical_params`).
pub fn risk_field_for_key(base: &Strategy, key: &str) -> Option<String> {
    if let Some(field) = key.strip_prefix("risk.") {
        return RISK_PARAM_FIELDS.contains(&field).then(|| field.to_string());
    }
    let shadowed_by_mechanical = base
        .mechanical_params
        .as_object()
        .map(|m| m.contains_key(key))
        .unwrap_or(false);
    if !shadowed_by_mechanical && RISK_PARAM_FIELDS.contains(&key) {
        return Some(key.to_string());
    }
    None
}

/// The param keys an experiment may target on `base`: every `mechanical_params`
/// top-level key plus `risk.<field>` for each tunable risk knob. Used to tell
/// the experiment writer which keys exist (F21) and to render a helpful
/// `unknown_param` error.
pub fn tunable_param_keys(base: &Strategy) -> Vec<String> {
    let mut keys = Vec::new();
    if let Some(mp) = base.mechanical_params.as_object() {
        for (k, v) in mp {
            // Only scalar leaves are directly tunable.
            if !v.is_object() && !v.is_array() {
                keys.push(k.clone());
            }
        }
    }
    for f in RISK_PARAM_FIELDS {
        keys.push(format!("risk.{f}"));
    }
    keys
}

/// The mutation kinds that are *structurally applicable* to `base`, intersected
/// with the operator-allowed kinds (F21). `param` is applicable whenever the
/// strategy exposes a tunable key (always, since every strategy has a `risk`
/// config). `tool` stays as allowed. `prose` is excluded: a `Strategy`
/// references library agents by `AgentRef`, so a prompt edit cannot change the
/// strategy artifact — proposing it only ever yields a no-op (identity) the
/// gate must reject. Excluding it steers the experiment writer to a lever that
/// can actually move the strategy instead of burning the retry budget.
pub fn applicable_mutation_kinds(base: &Strategy, allowed: &[String]) -> Vec<String> {
    let has_params = !tunable_param_keys(base).is_empty();
    allowed
        .iter()
        .filter(|k| match k.as_str() {
            "param" => has_params,
            "tool" => true,
            "prose" => false,
            _ => false,
        })
        .cloned()
        .collect()
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
    /// inversion-pair check, the `mutate-once` CLI verb, and the mutator's own
    /// identity check, so all of them agree on what a diff actually changes. It
    /// applies:
    ///   - `params` targeting `risk.<field>` (or a bare risk-field name): routed
    ///     into the typed `risk` config via a JSON round-trip (F14/F20 — this is
    ///     the only tunable surface real strategies have).
    ///   - `params` otherwise: dot-path keys into `mechanical_params` (nested
    ///     objects are created as needed).
    ///   - `tools`: add/remove against `manifest.required_tools`.
    ///
    /// Prose edits are intentionally **not** applied here: a `Strategy`
    /// references library agents by `AgentRef`, so an agent-prompt edit has no
    /// home in the strategy artifact's content hash. A prose-only diff is
    /// therefore an identity (no-op) at the strategy level — [`Mutator::propose`]
    /// detects that and retries for a real change rather than emitting a
    /// guaranteed-zero candidate.
    pub fn apply_to(&self, base: &Strategy) -> Strategy {
        let mut s = base.clone();
        // Route risk-targeted params through a single JSON round-trip of the
        // typed risk config so an invalid value can't half-apply.
        let mut risk_json = serde_json::to_value(&s.risk).unwrap_or(serde_json::Value::Null);
        let mut risk_touched = false;
        for change in &self.params {
            if let Some(field) = risk_field_for_key(base, &change.key) {
                if let Some(obj) = risk_json.as_object_mut() {
                    obj.insert(field, change.after.clone());
                    risk_touched = true;
                }
            } else {
                set_param_value(&mut s.mechanical_params, &change.key, change.after.clone());
            }
        }
        if risk_touched {
            if let Ok(new_risk) = serde_json::from_value(risk_json) {
                s.risk = new_risk;
            }
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
        exploration_seed: u64,
    ) -> anyhow::Result<MutationDiff> {
        let program_md = program_view::to_markdown(base);
        let mut last_errors: Option<Vec<ValidationError>> = None;
        let max_attempts = self.max_retries.saturating_add(1);

        assert!(max_attempts >= 1, "max_attempts must be at least 1");

        // F21: only offer the experiment writer the kinds that can actually
        // change this strategy, and tell it exactly which param keys exist
        // (mechanical + risk.*), so it stops proposing non-existent params or a
        // prose edit that can't be applied.
        let kinds = applicable_mutation_kinds(base, &config.allowed_mutation_kinds);
        let kinds = if kinds.is_empty() {
            // Defensive: never send an empty kind list. `param` is universally
            // applicable because every strategy carries a risk config.
            vec!["param".to_string()]
        } else {
            kinds
        };
        let param_keys = tunable_param_keys(base);

        for attempt in 0..max_attempts {
            let user_text = build_user_payload(
                &program_md,
                &kinds,
                &param_keys,
                last_errors.as_deref(),
                exploration_seed,
            );
            let req = LlmRequest {
                model: self.model.clone(),
                system_prompt: build_system_prompt(dsr_prefix),
                messages: vec![Message::user_text(user_text)],
                max_tokens: None,
                tools: vec![],
                // F32: the experiment writer was deterministic (temperature None +
                // a fixed prompt), so the same parent produced the IDENTICAL
                // candidate every cycle — the optimizer could never explore or
                // converge. Sample with a non-zero, per-cycle-jittered temperature
                // and a per-cycle exploration nonce in the prompt so successive
                // cycles propose diverse candidates.
                temperature: Some(exploration_temperature(exploration_seed)),
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

/// F32: per-cycle sampling temperature for the experiment writer. Jittered by the
/// exploration seed within an exploratory band (0.7–1.1) so different cycles
/// sample differently — the deterministic `temperature: None` produced the same
/// candidate every cycle. The band stays below fully-random so proposals remain
/// coherent JSON edits.
fn exploration_temperature(exploration_seed: u64) -> f64 {
    0.7 + (exploration_seed % 5) as f64 * 0.1
}

fn build_user_payload(
    program_md: &str,
    allowed_kinds: &[String],
    param_keys: &[String],
    previous_errors: Option<&[ValidationError]>,
    exploration_seed: u64,
) -> String {
    let kinds_text = allowed_kinds.join(", ");
    let keys_section = if param_keys.is_empty() {
        "\n\nThis strategy exposes no tunable parameter keys; do not propose a `param` experiment."
            .to_string()
    } else {
        format!(
            "\n\nTunable parameter keys (a `param` experiment's `key` MUST be exactly one of these):\n{}",
            param_keys
                .iter()
                .map(|k| format!("  - {k}"))
                .collect::<Vec<_>>()
                .join("\n")
        )
    };
    let errors_section = match previous_errors {
        None => String::new(),
        Some(errs) => {
            format!(
                "\n\nPrevious attempt errors — you MUST fix all of these:\n\n{}",
                format_validation_errors(errs)
            )
        }
    };

    // F32: a per-cycle exploration directive. The variant id varies per cycle
    // (and per mutation), steering the writer to a DIFFERENT starting point each
    // run instead of re-proposing the single most obvious tweak forever. Combined
    // with the non-zero temperature, this makes successive cycles diverge so the
    // optimizer can actually search the space.
    let exploration_section = format!(
        "\n\nExploration directive (variant {exploration_seed}): do NOT default to the single \
         most obvious change. Use this variant id as a hint to pick a different parameter and/or a \
         different direction/magnitude than you would by default, so that repeated runs on this \
         same strategy explore the space rather than re-proposing one fixed tweak."
    );

    format!(
        "Strategy program view:\n\n{program_md}\n\nAllowed experiment kinds: {kinds_text}{keys_section}{errors_section}{exploration_section}\n\nPropose ONE experiment as a JSON object."
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
    fn apply_to_routes_risk_param_into_risk_config() {
        // F14/F20: the real tunable surface. `risk.<field>` (and the bare field)
        // must land on the typed risk config, not be dumped into mechanical_params.
        let base = fixture_strategy();
        let before = base.risk.stop_loss_atr_multiple;
        for key in ["risk.stop_loss_atr_multiple", "stop_loss_atr_multiple"] {
            let diff = diff_with(
                vec![ParamChange {
                    key: key.into(),
                    before: serde_json::json!(before),
                    after: serde_json::json!(3.5),
                }],
                vec![],
                vec![],
            );
            let child = diff.apply_to(&base);
            assert_eq!(
                child.risk.stop_loss_atr_multiple, 3.5,
                "key {key} must update risk"
            );
            assert!(
                child.mechanical_params.get("stop_loss_atr_multiple").is_none(),
                "risk param must not leak into mechanical_params for key {key}"
            );
            // And it's a real change, not an identity no-op.
            assert!(
                !is_identity_diff(&diff, &base),
                "risk change must not be identity for {key}"
            );
        }
    }

    #[test]
    fn tunable_keys_include_risk_fields() {
        let base = fixture_strategy();
        let keys = tunable_param_keys(&base);
        assert!(keys.contains(&"risk.stop_loss_atr_multiple".to_string()));
        assert!(keys.contains(&"risk.risk_pct_per_trade".to_string()));
        // mechanical_params scalar keys are included too.
        assert!(keys.contains(&"ema_fast".to_string()));
    }

    #[test]
    fn applicable_kinds_drop_prose_and_keep_param() {
        let base = fixture_strategy();
        let allowed = vec!["prose".into(), "param".into(), "tool".into()];
        let kinds = applicable_mutation_kinds(&base, &allowed);
        assert!(
            kinds.contains(&"param".to_string()),
            "param is always applicable (risk exists)"
        );
        assert!(
            !kinds.contains(&"prose".to_string()),
            "prose is a structural no-op, excluded"
        );
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
