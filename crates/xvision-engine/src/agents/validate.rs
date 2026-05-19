//! Validation diagnostics for agent records. Pure function over the
//! `Agent` value type for v1; uniqueness checks against the store happen
//! separately in `engine::api::agents::validate`.

use std::collections::HashSet;

use serde::{Deserialize, Serialize};

use crate::agents::model::Agent;

// ---------------------------------------------------------------------------
// Save-gate constants
// ---------------------------------------------------------------------------

/// Minimum character length for a slot's system_prompt before a save is
/// accepted. Prompts shorter than this are either placeholder stubs or
/// clearly unfit for production use.
pub const MIN_SYSTEM_PROMPT_CHARS: usize = 200;

/// Leading text that identifies the factory default placeholder prompt.
/// We match on the leading sentence (tolerant of trailing edits) rather
/// than a content hash so a saved prompt that starts with this string but
/// has had junk appended is still caught.
const DEFAULT_PLACEHOLDER_LEADING: &str =
    "You are a trading agent. Decide based on the inputs provided.";

/// Asset ticker slugs that the name↔prompt mismatch check recognises.
const ASSET_SLUGS: &[&str] = &[
    "BTC", "ETH", "SOL", "AVAX", "DOGE", "LINK", "MATIC", "DOT", "ADA", "XRP",
];

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

/// Return the set of `ASSET_SLUGS` that appear in `text` (case-insensitive).
fn assets_in(text: &str) -> HashSet<&'static str> {
    let upper = text.to_uppercase();
    ASSET_SLUGS
        .iter()
        .filter(|s| upper.contains(*s))
        .copied()
        .collect()
}

/// If the agent `name` references an asset slug that is absent from `prompt`,
/// return an error message string for the first such slug. Returns `None`
/// when no mismatch is detected (including when the name has no asset slug).
fn name_prompt_asset_mismatch(name: &str, prompt: &str) -> Option<String> {
    let in_name = assets_in(name);
    if in_name.is_empty() {
        return None;
    }
    let in_prompt = assets_in(prompt);
    // Sort for deterministic ordering so the first reported slug is stable.
    let mut missing: Vec<&'static str> = in_name
        .iter()
        .filter(|slug| !in_prompt.contains(*slug))
        .copied()
        .collect();
    missing.sort_unstable();
    missing
        .first()
        .map(|slug| format!("agent name mentions {slug} but system_prompt does not"))
}

/// Return `true` when `prompt` is the unmodified factory default placeholder
/// or is too short to be a useful system prompt.
fn is_default_placeholder(prompt: &str) -> bool {
    let normalized = prompt.trim();
    normalized.starts_with(DEFAULT_PLACEHOLDER_LEADING) || normalized.len() < MIN_SYSTEM_PROMPT_CHARS
}

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

/// Validate an agent before it is persisted (create or update). Runs all
/// structural checks from `validate_agent` **plus** two save-gate rules:
///
/// 1. **Name↔prompt asset mismatch** — if the agent name references a
///    known ticker slug (BTC, ETH, SOL, …) that is absent from every
///    slot's `system_prompt`, the save is refused. Rationale: the audit
///    found a "SOL agent" whose prompt opened with "ETH/USD 4-hour swing
///    trader" — silent misconfiguration.
///
/// 2. **Default-placeholder / too-short prompt** — refuse to save any slot
///    whose `system_prompt` still contains the factory default text or is
///    shorter than `MIN_SYSTEM_PROMPT_CHARS` characters. A prompt that
///    short cannot contain meaningful trading logic.
///
/// Returns an `Err(String)` with a human-readable message for the first
/// blocking violation, or `Ok(())` when all checks pass. Callers should
/// first call `validate_agent` to surface *all* structural diagnostics;
/// this function adds the hard rejection gates on top.
pub fn validate_agent_for_save(agent: &Agent) -> Result<(), String> {
    // ------------------------------------------------------------------
    // (b) Default-placeholder / too-short prompt check.
    //     Runs over every slot; the first violation blocks the save.
    // ------------------------------------------------------------------
    for slot in &agent.slots {
        if slot.system_prompt.trim().is_empty() {
            // Empty prompt is already a Warning from `validate_agent`; not
            // a hard block here — the store's existing behaviour allows it
            // so as not to break wizard drafts that save before the prompt
            // is written. The minimum-length check below handles the actual
            // gate.
            continue;
        }
        if is_default_placeholder(&slot.system_prompt) {
            return Err(format!(
                "slot '{}': system_prompt is the default placeholder or fewer than \
                 {MIN_SYSTEM_PROMPT_CHARS} characters; replace with a real trading prompt before saving",
                slot.name,
            ));
        }
    }

    // ------------------------------------------------------------------
    // (a) Name ↔ prompt asset mismatch.
    //     Build the combined prompt from all slots so that a multi-slot
    //     agent where one slot mentions SOL and another doesn't still
    //     passes when the name says SOL.
    // ------------------------------------------------------------------
    let combined_prompt: String = agent
        .slots
        .iter()
        .map(|s| s.system_prompt.as_str())
        .collect::<Vec<_>>()
        .join(" ");

    if let Some(msg) = name_prompt_asset_mismatch(&agent.name, &combined_prompt) {
        return Err(msg);
    }

    Ok(())
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
                prompt_version: String::new(),
            },
            AgentSlot {
                name: "TRADER".into(), // case-insensitive duplicate
                provider: "anthropic".into(),
                model: "x".into(),
                system_prompt: "p".into(),
                skill_ids: vec![],
                max_tokens: Some(4096),
                prompt_version: String::new(),
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
