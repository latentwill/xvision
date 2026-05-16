//! Validation diagnostics for agent records. Pure function over the
//! `Agent` value type for v1; uniqueness checks against the store happen
//! separately in `engine::api::agents::validate`.

use serde::{Deserialize, Serialize};

use crate::agents::model::Agent;

#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts-export",
    ts(export, export_to = "../../../frontend/web/src/api/types.gen/")
)]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum Severity {
    Error,
    Warning,
    Info,
}

#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts-export",
    ts(export, export_to = "../../../frontend/web/src/api/types.gen/")
)]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ValidationDiagnostic {
    /// Stable machine-readable code (e.g., "name_empty", "slot_name_duplicate").
    pub code: String,
    pub severity: Severity,
    pub message: String,
    /// Optional pointer into the agent payload (e.g., "name", "slots[0].provider").
    pub field: Option<String>,
}

/// Validate the in-memory shape of an agent. Uniqueness against the
/// workspace (e.g., name collisions with other agents) is NOT checked
/// here — that's the caller's job because it needs a store handle.
pub fn validate_agent(agent: &Agent) -> Vec<ValidationDiagnostic> {
    let mut out = Vec::new();

    if agent.name.trim().is_empty() {
        out.push(ValidationDiagnostic {
            code: "name_empty".into(),
            severity: Severity::Error,
            message: "Agent name is required.".into(),
            field: Some("name".into()),
        });
    }

    if agent.slots.is_empty() {
        out.push(ValidationDiagnostic {
            code: "slots_empty".into(),
            severity: Severity::Error,
            message: "Agent needs at least one slot.".into(),
            field: Some("slots".into()),
        });
    }

    // Slot-name duplicates within the same agent.
    let mut seen_names = std::collections::HashSet::new();
    for (i, slot) in agent.slots.iter().enumerate() {
        let field_prefix = format!("slots[{}]", i);

        if slot.name.trim().is_empty() {
            out.push(ValidationDiagnostic {
                code: "slot_name_empty".into(),
                severity: Severity::Error,
                message: format!("Slot {} needs a name.", i),
                field: Some(format!("{}.name", field_prefix)),
            });
        } else if !seen_names.insert(slot.name.to_lowercase()) {
            out.push(ValidationDiagnostic {
                code: "slot_name_duplicate".into(),
                severity: Severity::Error,
                message: format!("Slot name '{}' is used more than once.", slot.name),
                field: Some(format!("{}.name", field_prefix)),
            });
        }

        if slot.provider.trim().is_empty() {
            out.push(ValidationDiagnostic {
                code: "slot_provider_empty".into(),
                severity: Severity::Error,
                message: format!("Slot '{}' needs a provider.", slot.name),
                field: Some(format!("{}.provider", field_prefix)),
            });
        }

        if slot.model.trim().is_empty() {
            out.push(ValidationDiagnostic {
                code: "slot_model_empty".into(),
                severity: Severity::Error,
                message: format!("Slot '{}' needs a model.", slot.name),
                field: Some(format!("{}.model", field_prefix)),
            });
        }

        if slot.system_prompt.trim().is_empty() {
            out.push(ValidationDiagnostic {
                code: "slot_prompt_empty".into(),
                severity: Severity::Warning,
                message: format!("Slot '{}' has an empty system prompt.", slot.name),
                field: Some(format!("{}.system_prompt", field_prefix)),
            });
        }

        // `max_tokens` is now `Option<u32>`; `None` means
        // "auto from the selected model" at dispatch time (see
        // `agents::model_metadata::resolve_max_tokens`). The previous
        // `slot_max_tokens_zero` error fired against the old u32 field
        // and is no longer reachable.
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agents::model::{Agent, AgentSlot};

    fn good_agent() -> Agent {
        Agent::single_slot_default(
            "01HZ000000000000000000000",
            "demo",
            "anthropic",
            "claude-sonnet-4-6",
        )
    }

    #[test]
    fn good_agent_with_prompt_passes() {
        let mut a = good_agent();
        a.slots[0].system_prompt = "You are a trader.".into();
        let diags = validate_agent(&a);
        assert!(diags.is_empty(), "expected no diagnostics, got {:?}", diags);
    }

    #[test]
    fn empty_name_errors() {
        let mut a = good_agent();
        a.name = "  ".into();
        a.slots[0].system_prompt = "x".into();
        let diags = validate_agent(&a);
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].code, "name_empty");
        assert_eq!(diags[0].severity, Severity::Error);
    }

    #[test]
    fn duplicate_slot_names_error() {
        let mut a = good_agent();
        a.slots = vec![
            AgentSlot {
                name: "trader".into(),
                provider: "anthropic".into(),
                model: "x".into(),
                system_prompt: "p".into(),
                skill_ids: vec![],
                max_tokens: Some(4096),
            },
            AgentSlot {
                name: "TRADER".into(), // case-insensitive duplicate
                provider: "anthropic".into(),
                model: "x".into(),
                system_prompt: "p".into(),
                skill_ids: vec![],
                max_tokens: Some(4096),
            },
        ];
        let diags = validate_agent(&a);
        assert!(diags.iter().any(|d| d.code == "slot_name_duplicate"));
    }

    #[test]
    fn empty_prompt_warns_not_errors() {
        let a = good_agent(); // system_prompt is empty by default
        let diags = validate_agent(&a);
        let prompt_warn = diags
            .iter()
            .find(|d| d.code == "slot_prompt_empty")
            .expect("warn present");
        assert_eq!(prompt_warn.severity, Severity::Warning);
    }

    #[test]
    fn empty_slots_errors() {
        let mut a = good_agent();
        a.slots.clear();
        let diags = validate_agent(&a);
        assert!(diags.iter().any(|d| d.code == "slots_empty"));
    }
}
