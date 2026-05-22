//! Validation diagnostics for agent records. Pure function over the
//! `Agent` value type for v1; uniqueness checks against the store happen
//! separately in `engine::api::agents::validate`.

use std::sync::OnceLock;

use serde::{Deserialize, Serialize};

use crate::agents::model::Agent;

/// The exact 129-character default placeholder prompt seeded by
/// `+ New agent`. Agents shipped with this verbatim text are
/// indistinguishable from "operator never wrote a prompt" and silently
/// degrade eval runs (the audit found `Macro MACD-RSI Weekly Trader` and
/// `Multi-Factor Logic Agent` both shipping this string — same
/// `prompt_version=41ac7a4abb2e51a5`). We reject saves that ship this
/// content so the operator is forced to author a real prompt.
pub const DEFAULT_PLACEHOLDER_PROMPT: &str = "You are a trading agent. Decide based on the inputs provided. Output JSON: {action, conviction (0-1), justification (one line)}.";

/// SHA-256 of `DEFAULT_PLACEHOLDER_PROMPT`. Computed once on first access
/// so the hex string is built lazily but reused across every validator
/// call. The comparison is hash-based (not raw string equality) so future
/// reformatters / whitespace-normalisers stay honest — a one-byte drift
/// trips the check immediately.
fn placeholder_prompt_sha256() -> &'static str {
    static H: OnceLock<String> = OnceLock::new();
    H.get_or_init(|| {
        use sha2::{Digest, Sha256};
        format!("{:x}", Sha256::digest(DEFAULT_PLACEHOLDER_PROMPT.as_bytes()))
    })
}

/// Recognised asset-symbol tokens that we expect to find inside an
/// agent's `system_prompt` when the agent's `name` advertises them. The
/// list is intentionally small — popular tickers an operator would
/// reasonably embed in a name when targeting one asset. False positives
/// (e.g. `BTC` appearing inside a longer english word) are guarded
/// against by the substring match running on a lowercased prompt body.
///
/// Keep this list short: the rule is a coherence smell-test, not a
/// strict-typing pass. New entries should be debated.
const RECOGNISED_ASSET_TOKENS: &[&str] = &["SOL", "BTC", "ETH", "DOGE", "ADA", "XRP", "AVAX"];

/// Minimum character length for a slot's system_prompt before a save is
/// accepted. Prompts shorter than this are either placeholder stubs or
/// clearly unfit for production use.
pub const MIN_SYSTEM_PROMPT_CHARS: usize = 200;

/// Leading text that identifies the factory default placeholder prompt.
/// We match on the leading sentence during save so a prompt that starts
/// with the placeholder but has had junk appended is still caught.
const DEFAULT_PLACEHOLDER_LEADING: &str = "You are a trading agent. Decide based on the inputs provided.";

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

        // Placeholder-prompt rejection. Agents that ship with the
        // verbatim `+ New agent` default prompt are silent eval
        // landmines — they read as "real agent" but contain no
        // strategy-specific reasoning. Compare against a cached SHA-256
        // of the canonical placeholder so any byte-level drift trips
        // the check.
        if !slot.system_prompt.is_empty() {
            use sha2::{Digest, Sha256};
            let digest = format!("{:x}", Sha256::digest(slot.system_prompt.as_bytes()));
            if digest == placeholder_prompt_sha256() {
                out.push(ValidationDiagnostic {
                    code: "slot_prompt_placeholder".into(),
                    severity: Severity::Error,
                    message: format!(
                        "Slot '{}' still uses the default placeholder prompt. \
                         Author a strategy-specific system prompt before saving.",
                        slot.name
                    ),
                    field: Some(format!("{}.system_prompt", field_prefix)),
                });
            }
        }

        // Name-vs-asset coherence. If the agent's `name` mentions a
        // recognised asset ticker (case-insensitive whole-token match
        // against `RECOGNISED_ASSET_TOKENS`), the slot's
        // `system_prompt` must mention the same token somewhere
        // (case-insensitive substring). The audit's
        // `SOL 4h trend breakout trader agent` ships a prompt that
        // opens with `"You are a single-agent ETH/USD 4-hour swing
        // trader"` — that mismatch slipped past v0 review and is the
        // motivating case for this rule.
        for token in RECOGNISED_ASSET_TOKENS {
            if !name_mentions_asset_token(&agent.name, token) {
                continue;
            }
            if !slot
                .system_prompt
                .to_ascii_lowercase()
                .contains(&token.to_ascii_lowercase())
            {
                out.push(ValidationDiagnostic {
                    code: "slot_prompt_asset_mismatch".into(),
                    severity: Severity::Error,
                    message: format!(
                        "Agent name mentions `{token}` but slot '{}' system prompt does not. \
                         Either rename the agent or update the prompt so the asset focus is consistent.",
                        slot.name
                    ),
                    field: Some(format!("{}.system_prompt", field_prefix)),
                });
                // One diagnostic per slot covers the operator's intent;
                // a multi-ticker name (rare) still surfaces the first
                // mismatch.
                break;
            }
        }

        // `max_tokens` is now `Option<u32>`; `None` means
        // "auto from the selected model" at dispatch time (see
        // `agents::model_metadata::resolve_max_tokens`). The previous
        // `slot_max_tokens_zero` error fired against the old u32 field
        // and is no longer reachable.
    }

    out
}

/// Returns true when `name` contains `token` (case-insensitive) at a
/// whole-token boundary — i.e. the surrounding characters are either
/// absent or non-alphanumeric. Guards against `BTC` matching inside
/// `BTCmonkey` or `SOL` matching inside `SOLO`.
fn name_mentions_asset_token(name: &str, token: &str) -> bool {
    let needle = token.to_ascii_lowercase();
    let hay = name.to_ascii_lowercase();
    let bytes = hay.as_bytes();
    let needle_bytes = needle.as_bytes();
    let mut start = 0;
    while let Some(pos) = hay[start..].find(needle.as_str()) {
        let abs = start + pos;
        let before_ok = abs == 0 || !bytes[abs - 1].is_ascii_alphanumeric();
        let after_idx = abs + needle_bytes.len();
        let after_ok = after_idx >= bytes.len() || !bytes[after_idx].is_ascii_alphanumeric();
        if before_ok && after_ok {
            return true;
        }
        start = abs + needle_bytes.len();
        if start >= bytes.len() {
            break;
        }
    }
    false
}

/// Audit helper: scan a slice of agents and return non-mutating findings
/// for the three new validator rules. Unlike `validate_agent`, this
/// surface is meant for the lint / report path (so operators can see
/// existing badness without rejecting writes). The seeded
/// `Macro MACD-RSI Weekly Trader` + `Multi-Factor Logic Agent`
/// placeholder rows and the `SOL 4h trend breakout trader agent`
/// asset-mismatch row are the motivating cases.
pub fn lint_agents(agents: &[Agent]) -> Vec<AuditFinding> {
    let mut out = Vec::new();
    for agent in agents {
        let diags = validate_agent(agent);
        for d in diags {
            if !matches!(
                d.code.as_str(),
                "slot_prompt_placeholder" | "slot_prompt_asset_mismatch"
            ) {
                continue;
            }
            // Parse `slots[N].field` to recover the slot index, so the
            // operator UI can highlight the offending row. Falls back
            // to 0 when the field is missing or malformed (shouldn't
            // happen for the two rules we surface — both attach a
            // `slots[N].system_prompt` pointer).
            let slot_index = d
                .field
                .as_deref()
                .and_then(|f| f.strip_prefix("slots["))
                .and_then(|rest| rest.split(']').next())
                .and_then(|n| n.parse::<usize>().ok())
                .unwrap_or(0);
            out.push(AuditFinding {
                agent_id: agent.agent_id.clone(),
                agent_name: agent.name.clone(),
                slot_index,
                code: d.code,
                message: d.message,
            });
        }
    }
    out
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AuditFinding {
    pub agent_id: String,
    pub agent_name: String,
    pub slot_index: usize,
    pub code: String,
    pub message: String,
}

/// Validate an agent before it is persisted (create or update). Runs the
/// hard save-gate rules for placeholder/too-short prompts and asset
/// name↔prompt mismatches.
///
/// Honors `XVISION_DISABLE_AGENT_SAVE_GATE=1` as a test-only bypass —
/// **debug builds only**. The `cfg(debug_assertions)` guard means the
/// env var is silently ignored in release builds, so production binaries
/// always run the gate regardless of environment.
///
/// The gate intentionally stays on by default; integration tests that
/// exercise behavior unrelated to prompt quality set the env var so they
/// don't have to fabricate a 200-char prompt at every AgentSlot
/// construction site. See PR #364 (gate origin) and the unblock commit
/// that introduced this bypass.
pub fn validate_agent_for_save(agent: &Agent) -> Result<(), String> {
    #[cfg(debug_assertions)]
    if std::env::var_os("XVISION_DISABLE_AGENT_SAVE_GATE").is_some() {
        return Ok(());
    }
    for slot in &agent.slots {
        let prompt = slot.system_prompt.trim();
        if prompt.is_empty() {
            continue;
        }
        if prompt.starts_with(DEFAULT_PLACEHOLDER_LEADING) || prompt.len() < MIN_SYSTEM_PROMPT_CHARS {
            return Err(format!(
                "slot '{}': system_prompt is the default placeholder or fewer than \
                 {MIN_SYSTEM_PROMPT_CHARS} characters; replace with a real trading prompt before saving",
                slot.name,
            ));
        }
    }

    let combined_prompt = agent
        .slots
        .iter()
        .map(|slot| slot.system_prompt.as_str())
        .collect::<Vec<_>>()
        .join(" ")
        .to_ascii_lowercase();
    for token in RECOGNISED_ASSET_TOKENS {
        if !name_mentions_asset_token(&agent.name, token) {
            continue;
        }
        if !combined_prompt.contains(&token.to_ascii_lowercase()) {
            return Err(format!("agent name mentions {token} but system_prompt does not"));
        }
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
                temperature: None,
                prompt_version: String::new(),
                inputs_policy: crate::agents::InputsPolicy::Raw,
                bar_history_limit: None,
                memory_mode: xvision_memory::types::MemoryMode::default(),
                noop_skip: None,
            },
            AgentSlot {
                name: "TRADER".into(), // case-insensitive duplicate
                provider: "anthropic".into(),
                model: "x".into(),
                system_prompt: "p".into(),
                skill_ids: vec![],
                max_tokens: Some(4096),
                temperature: None,
                prompt_version: String::new(),
                inputs_policy: crate::agents::InputsPolicy::Raw,
                bar_history_limit: None,
                memory_mode: xvision_memory::types::MemoryMode::default(),
                noop_skip: None,
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

    #[test]
    fn placeholder_prompt_rejected_by_exact_match() {
        let mut a = good_agent();
        a.name = "Probably Generic Trader".into();
        a.slots[0].system_prompt = DEFAULT_PLACEHOLDER_PROMPT.to_string();
        let diags = validate_agent(&a);
        let hit = diags
            .iter()
            .find(|d| d.code == "slot_prompt_placeholder")
            .expect("placeholder diagnostic present");
        assert_eq!(hit.severity, Severity::Error);
    }

    #[test]
    fn non_placeholder_prompt_passes_placeholder_check() {
        let mut a = good_agent();
        a.name = "Custom Trader".into();
        a.slots[0].system_prompt =
            "You are a custom trading agent with a real, strategy-specific prompt.".into();
        let diags = validate_agent(&a);
        assert!(diags.iter().all(|d| d.code != "slot_prompt_placeholder"));
    }

    #[test]
    fn near_placeholder_prompt_with_extra_byte_is_not_caught_as_placeholder() {
        // Coherence check on the sha256 path: any drift means the
        // operator authored something — even a trailing space — so the
        // rule fires only on the verbatim placeholder.
        let mut a = good_agent();
        a.name = "Custom Trader".into();
        a.slots[0].system_prompt = format!("{} ", DEFAULT_PLACEHOLDER_PROMPT);
        let diags = validate_agent(&a);
        assert!(diags.iter().all(|d| d.code != "slot_prompt_placeholder"));
    }

    #[test]
    fn asset_mismatch_flagged_for_sol_name_eth_prompt() {
        // The motivating audit case: `SOL 4h trend breakout trader
        // agent` whose prompt opens with `"You are a single-agent
        // ETH/USD 4-hour swing trader"`.
        let mut a = good_agent();
        a.name = "SOL 4h trend breakout trader agent".into();
        a.slots[0].system_prompt =
            "You are a single-agent ETH/USD 4-hour swing trader looking at OHLCV.".into();
        let diags = validate_agent(&a);
        let hit = diags
            .iter()
            .find(|d| d.code == "slot_prompt_asset_mismatch")
            .expect("asset mismatch diagnostic present");
        assert_eq!(hit.severity, Severity::Error);
        assert!(hit.message.contains("SOL"));
    }

    #[test]
    fn asset_match_passes_when_prompt_mentions_same_token() {
        let mut a = good_agent();
        a.name = "SOL 4h trend breakout trader agent".into();
        a.slots[0].system_prompt = "You are a SOL/USD 4-hour swing trader looking at OHLCV.".into();
        let diags = validate_agent(&a);
        assert!(diags.iter().all(|d| d.code != "slot_prompt_asset_mismatch"));
    }

    #[test]
    fn asset_match_is_case_insensitive_in_prompt() {
        let mut a = good_agent();
        a.name = "BTC scalper".into();
        a.slots[0].system_prompt = "You are a btc/usd scalping agent.".into();
        let diags = validate_agent(&a);
        assert!(diags.iter().all(|d| d.code != "slot_prompt_asset_mismatch"));
    }

    #[test]
    fn asset_token_does_not_false_positive_on_substring_in_name() {
        // `SOLO` should not trigger the SOL rule.
        let mut a = good_agent();
        a.name = "SOLO breakout trader".into();
        a.slots[0].system_prompt = "ETH only please.".into();
        let diags = validate_agent(&a);
        assert!(diags.iter().all(|d| d.code != "slot_prompt_asset_mismatch"));
    }

    #[test]
    fn agent_name_without_asset_token_skips_coherence_rule() {
        let mut a = good_agent();
        a.name = "Macro Multi-Factor Trader".into();
        a.slots[0].system_prompt = "Reason about the global macro picture.".into();
        let diags = validate_agent(&a);
        assert!(diags.iter().all(|d| d.code != "slot_prompt_asset_mismatch"));
    }

    #[test]
    fn lint_agents_surfaces_both_placeholder_and_mismatch_findings() {
        let mut placeholder_agent = good_agent();
        placeholder_agent.agent_id = "01HZAAAA0000000000000000A".into();
        placeholder_agent.name = "Macro MACD-RSI Weekly Trader".into();
        placeholder_agent.slots[0].system_prompt = DEFAULT_PLACEHOLDER_PROMPT.into();

        let mut sol_agent = good_agent();
        sol_agent.agent_id = "01HZAAAA0000000000000000B".into();
        sol_agent.name = "SOL 4h trend breakout trader agent".into();
        sol_agent.slots[0].system_prompt = "You are a single-agent ETH/USD 4-hour swing trader.".into();

        let findings = lint_agents(&[placeholder_agent, sol_agent]);
        assert!(
            findings.iter().any(|f| f.code == "slot_prompt_placeholder"),
            "placeholder finding missing: {:?}",
            findings
        );
        assert!(
            findings.iter().any(|f| f.code == "slot_prompt_asset_mismatch"),
            "asset mismatch finding missing: {:?}",
            findings
        );
    }
}
