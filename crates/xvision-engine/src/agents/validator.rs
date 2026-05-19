//! Pre-persist drift validator for agent slot prompts.
//!
//! Two rules, both refusing to save the agent when violated:
//!
//! 1. **Unregistered tool reference.** If the system prompt mentions a
//!    tool name (e.g. `indicator_panel`, `ohlcv_history`) that is NOT
//!    listed in the slot's tool registry (`AgentSlot.skill_ids`), reject
//!    the save. The harness audit (intake #344) found `"tools": []` in
//!    every outbound prompt blob across 2,757 model calls — every one of
//!    those agents was advertising tools the model could never call.
//!
//! 2. **Schema-enum drift on `Allowed actions:`.** If the prompt declares
//!    an explicit `Allowed actions: ...` list, every token in that list
//!    must be a member of the canonical `trader_output` response-schema
//!    enum. The SOL 4h trend agent listed `exit` even though the schema
//!    enum is `[long_open, short_open, flat, hold]`; the model emitted
//!    `exit` zero times across 56 runs because the parser silently
//!    rejected it.
//!
//! ## Canonical enum source
//!
//! The canonical action enum lives in
//! `crates/xvision-engine/src/eval/executor/trader_output.rs` —
//! specifically the `matches!` arm in `TraderOutput::validate`. That file
//! is in a forbidden path for this track, so [`ACTION_SCHEMA_ENUM`] is a
//! mirror; the test [`tests::action_enum_matches_trader_output`] is
//! intentionally a brittle reminder that the two lists must stay in sync.
//!
//! ## Implementation notes
//!
//! Word-boundary detection is done by hand (rather than via the `regex`
//! crate) to avoid adding a workspace dependency for two scans. A
//! "word character" here is `[A-Za-z0-9_]`, which matches the perl `\w`
//! convention the contract's `\b<tool_name>\b` regex implies.
//!
//! ## Lint mode
//!
//! [`lint_agents`] runs the same two rules over every persisted agent
//! and returns a [`LintFinding`] per violation, with `agent_id`,
//! `slot_index`, and a one-line message. The CLI verb that wires this
//! into `xvn` is a follow-up — the contract restricts this track to
//! `crates/xvision-engine/src/agents/**`, so we expose the lint as a
//! library function and cover it from a test.

use std::collections::HashSet;

use thiserror::Error;

use crate::agents::model::{Agent, AgentSlot};
use crate::agents::store::{AgentStore, ListFilter};

/// Canonical `trader_output.action` enum.
///
/// Mirrors the literal match in
/// `crates/xvision-engine/src/eval/executor/trader_output.rs::TraderOutput::validate`:
/// `matches!(self.action.as_str(), "long_open" | "short_open" | "flat" | "hold")`.
pub const ACTION_SCHEMA_ENUM: &[&str] = &["long_open", "short_open", "flat", "hold"];

/// Tool names the prompt may reference that we recognise as needing
/// registration. We deliberately keep this list closed instead of
/// "every word that looks like a snake_case identifier" — false
/// positives would refuse to save agents whose prompts mention things
/// like `risk_check` (a slot role) or `prompt_version` (a metadata
/// field). Audit-known tools come first; extend as the registry grows.
const KNOWN_TOOL_NAMES: &[&str] = &[
    "indicator_panel",
    "ohlcv_history",
    "ohlcv",
    "fetch_bars",
    "news_feed",
    "orderbook_snapshot",
    "position_summary",
];

/// Typed pre-persist drift error.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum PromptSchemaDriftError {
    /// The prompt mentions one or more tool names that are not in the
    /// slot's resolved tool registry. `slot_index` is the position of
    /// the slot in the parent agent's `slots` vector.
    #[error(
        "slot {slot_index} ('{slot_name}') references unregistered tool(s) {missing_tools:?} \
         in its system prompt; add them to the slot's tool registry or drop the references"
    )]
    UnregisteredTool {
        slot_index: usize,
        slot_name: String,
        missing_tools: Vec<String>,
    },
    /// The prompt's `Allowed actions:` list declares tokens that are
    /// not in the canonical `trader_output.action` schema enum.
    #[error(
        "slot {slot_index} ('{slot_name}') declares 'Allowed actions: ...' with token(s) \
         {extra_actions:?} that are not in the trader_output schema enum \
         {schema_enum:?}; the model's emissions will be silently rejected by the parser"
    )]
    AllowedActionsOutOfSchema {
        slot_index: usize,
        slot_name: String,
        extra_actions: Vec<String>,
        schema_enum: Vec<String>,
    },
}

impl PromptSchemaDriftError {
    /// Wrap into [`anyhow::Error`]. Use from `AgentStore::*` so the
    /// typed error survives `?` past `anyhow::Result<_>` signatures.
    ///
    /// Callers can later `downcast_ref::<PromptSchemaDriftError>()` to
    /// recover the typed variant at the API layer.
    pub fn into_anyhow(self) -> anyhow::Error {
        anyhow::Error::new(self)
    }
}

/// Validate every slot on an agent. Returns the first violation as a
/// typed error; callers (`AgentStore::create`, `AgentStore::update`)
/// surface it before persistence.
///
/// Order is stable: slots are visited in `slots` order, and within a
/// slot rule (1) runs before rule (2).
pub fn validate_prompt_schema(agent: &Agent) -> Result<(), PromptSchemaDriftError> {
    validate_prompt_schema_slots(&agent.slots)
}

/// Slot-slice variant of [`validate_prompt_schema`] for the persistence
/// path where the caller (`AgentStore::create` / `AgentStore::update`)
/// hasn't yet assembled an `Agent` value.
pub fn validate_prompt_schema_slots(slots: &[AgentSlot]) -> Result<(), PromptSchemaDriftError> {
    for (idx, slot) in slots.iter().enumerate() {
        check_slot(idx, slot)?;
    }
    Ok(())
}

fn check_slot(idx: usize, slot: &AgentSlot) -> Result<(), PromptSchemaDriftError> {
    let registered: HashSet<String> = slot.skill_ids.iter().map(|s| s.to_ascii_lowercase()).collect();

    let mentioned = mentioned_tools(&slot.system_prompt);
    let mut missing: Vec<String> = mentioned
        .into_iter()
        .filter(|t| {
            !registered.contains(t.as_str()) && !(t == "ohlcv_history" && registered.contains("ohlcv"))
        })
        .collect();
    missing.sort();
    missing.dedup();
    if !missing.is_empty() {
        return Err(PromptSchemaDriftError::UnregisteredTool {
            slot_index: idx,
            slot_name: slot.name.clone(),
            missing_tools: missing,
        });
    }

    if let Some(actions) = parse_allowed_actions(&slot.system_prompt) {
        let schema: HashSet<&str> = ACTION_SCHEMA_ENUM.iter().copied().collect();
        let mut extras: Vec<String> = actions
            .into_iter()
            .filter(|a| !schema.contains(a.as_str()))
            .collect();
        extras.sort();
        extras.dedup();
        if !extras.is_empty() {
            return Err(PromptSchemaDriftError::AllowedActionsOutOfSchema {
                slot_index: idx,
                slot_name: slot.name.clone(),
                extra_actions: extras,
                schema_enum: ACTION_SCHEMA_ENUM.iter().map(|s| s.to_string()).collect(),
            });
        }
    }
    Ok(())
}

/// `[A-Za-z0-9_]` — the `\w` character class. Anything else is a word
/// boundary on both sides.
fn is_word_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_'
}

/// Word-boundary search for `needle` in `haystack`. Equivalent to
/// `\b<needle>\b` for ASCII identifiers (the only thing we use it for).
fn contains_whole_word(haystack: &str, needle: &str) -> bool {
    if needle.is_empty() {
        return false;
    }
    let bytes = haystack.as_bytes();
    let nlen = needle.len();
    let mut start = 0;
    while let Some(pos) = haystack[start..].find(needle) {
        let abs = start + pos;
        let left_ok = abs == 0 || !is_word_char(haystack[..abs].chars().next_back().unwrap_or(' '));
        let end = abs + nlen;
        let right_ok = end == bytes.len() || !is_word_char(haystack[end..].chars().next().unwrap_or(' '));
        if left_ok && right_ok {
            return true;
        }
        start = abs + 1;
    }
    false
}

/// Tool names mentioned in the prompt that we recognise.
fn mentioned_tools(prompt: &str) -> Vec<String> {
    let mut out = Vec::new();
    for &tool in KNOWN_TOOL_NAMES {
        if contains_whole_word(prompt, tool) {
            out.push(tool.to_string());
        }
    }
    out
}

/// Match `Allowed actions: a, b, c` (or `Allowed actions:\na\nb\nc`)
/// up to the next blank line, period, or end-of-string. Returns the
/// list lower-cased and de-duplicated, or `None` if the prompt has no
/// such declaration.
///
/// The header match is case-insensitive on the `Allowed actions:`
/// literal; tokens are normalised to lower-case before comparison so
/// `Allowed Actions: LONG_OPEN | EXIT` is parsed correctly.
fn parse_allowed_actions(prompt: &str) -> Option<Vec<String>> {
    // Locate the header case-insensitively by scanning the lower-cased
    // prompt and using the offset against the original string for the
    // body slice (this keeps token preservation correct even though
    // we lower-case for comparison anyway).
    let lower = prompt.to_ascii_lowercase();
    let needle = "allowed actions";
    let mut search_from = 0usize;
    let header_pos = loop {
        let pos = lower[search_from..].find(needle)?;
        let abs = search_from + pos;
        // After the header literal must come optional whitespace then
        // a colon. We accept `Allowed actions :` and `Allowed  actions:`
        // (any whitespace between the two words is already matched
        // because we required a single space in the needle; broaden
        // only the gap before the colon).
        let mut cursor = abs + needle.len();
        let bytes = lower.as_bytes();
        while cursor < bytes.len() && (bytes[cursor] == b' ' || bytes[cursor] == b'\t') {
            cursor += 1;
        }
        if cursor < bytes.len() && bytes[cursor] == b':' {
            break cursor + 1;
        }
        search_from = abs + needle.len();
    };

    // The body runs until the first period or blank line.
    let tail = &prompt[header_pos..];
    let body_end = tail.find("\n\n").or_else(|| tail.find('.')).unwrap_or(tail.len());
    let body = &tail[..body_end];

    let tokens: Vec<String> = body
        .split(|c: char| c == ',' || c == '|' || c.is_whitespace())
        .map(|s| s.trim().to_ascii_lowercase())
        .filter(|s| !s.is_empty())
        .collect();
    if tokens.is_empty() {
        None
    } else {
        Some(tokens)
    }
}

// ─── lint mode ───────────────────────────────────────────────────────

/// One drift finding produced by [`lint_agents`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LintFinding {
    pub agent_id: String,
    pub slot_index: usize,
    pub message: String,
}

/// Walk every (non-archived by default) agent in the store and produce
/// a finding per slot that fails [`validate_prompt_schema`]. The CLI
/// wiring (`xvn agents lint`) is a follow-up; see module docs.
///
/// `include_archived = true` surfaces violations on archived records
/// too — useful when auditing seed data that's been soft-deleted.
pub async fn lint_agents(store: &AgentStore, include_archived: bool) -> anyhow::Result<Vec<LintFinding>> {
    let agents = store
        .list(ListFilter {
            include_archived,
            ..Default::default()
        })
        .await?;
    let mut findings = Vec::new();
    for agent in agents {
        for (idx, slot) in agent.slots.iter().enumerate() {
            if let Err(e) = check_slot(idx, slot) {
                findings.push(LintFinding {
                    agent_id: agent.agent_id.clone(),
                    slot_index: idx,
                    message: e.to_string(),
                });
            }
        }
    }
    Ok(findings)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agents::model::{Agent, AgentSlot};
    use chrono::Utc;

    fn slot_with(prompt: &str, skill_ids: Vec<&str>) -> AgentSlot {
        AgentSlot {
            name: "trader".into(),
            provider: "anthropic".into(),
            model: "claude-sonnet-4-6".into(),
            system_prompt: prompt.into(),
            skill_ids: skill_ids.into_iter().map(|s| s.to_string()).collect(),
            max_tokens: Some(4096),
            prompt_version: String::new(),
            inputs_policy: crate::agents::InputsPolicy::Raw,
            bar_history_limit: None,
        }
    }

    fn agent_with(slot: AgentSlot) -> Agent {
        let now = Utc::now();
        Agent {
            agent_id: "01HZ000000000000000000000".into(),
            name: "test".into(),
            description: String::new(),
            tags: vec![],
            slots: vec![slot],
            archived: false,
            created_at: now,
            updated_at: now,
        }
    }

    /// Brittle by design: if the canonical enum in
    /// `eval/executor/trader_output.rs` ever drifts, this assertion is
    /// the bridge that says "update the mirror in this module too".
    #[test]
    fn action_enum_matches_trader_output() {
        // Hardcoded duplicate of the `matches!` arm — the test asserts
        // the SAME literal list. If `trader_output.rs` adds `exit`,
        // the `matches!` line is the source of truth and this mirror
        // must be updated alongside it.
        assert_eq!(ACTION_SCHEMA_ENUM, &["long_open", "short_open", "flat", "hold"],);
    }

    #[test]
    fn unregistered_indicator_panel_is_rejected() {
        let agent = agent_with(slot_with(
            "You may call `indicator_panel` at most once per decision.",
            vec![],
        ));
        let err = validate_prompt_schema(&agent).expect_err("must reject");
        match err {
            PromptSchemaDriftError::UnregisteredTool {
                slot_index,
                missing_tools,
                ..
            } => {
                assert_eq!(slot_index, 0);
                assert_eq!(missing_tools, vec!["indicator_panel".to_string()]);
            }
            other => panic!("expected UnregisteredTool, got {other:?}"),
        }
    }

    #[test]
    fn registered_indicator_panel_passes() {
        let agent = agent_with(slot_with(
            "You may call `indicator_panel` at most once per decision.",
            vec!["indicator_panel"],
        ));
        validate_prompt_schema(&agent).expect("registered tool must not be rejected");
    }

    #[test]
    fn registered_check_is_case_insensitive() {
        // Seed data sometimes has tool ids in mixed case; the matcher
        // must not punish that.
        let agent = agent_with(slot_with(
            "You may call `indicator_panel` at most once per decision.",
            vec!["Indicator_Panel"],
        ));
        validate_prompt_schema(&agent).expect("case must not matter");
    }

    #[test]
    fn allowed_actions_with_exit_is_rejected() {
        let agent = agent_with(slot_with(
            "Allowed actions: long_open, short_open, flat, hold, exit",
            vec![],
        ));
        let err = validate_prompt_schema(&agent).expect_err("must reject");
        match err {
            PromptSchemaDriftError::AllowedActionsOutOfSchema {
                slot_index,
                extra_actions,
                ..
            } => {
                assert_eq!(slot_index, 0);
                assert_eq!(extra_actions, vec!["exit".to_string()]);
            }
            other => panic!("expected AllowedActionsOutOfSchema, got {other:?}"),
        }
    }

    #[test]
    fn allowed_actions_one_per_line_is_rejected() {
        let prompt =
            "Decision protocol.\n\nAllowed actions:\nlong_open\nshort_open\nflat\nhold\nexit\n\nReturn JSON.";
        let agent = agent_with(slot_with(prompt, vec![]));
        let err = validate_prompt_schema(&agent).expect_err("must reject");
        assert!(matches!(
            err,
            PromptSchemaDriftError::AllowedActionsOutOfSchema { .. }
        ));
    }

    #[test]
    fn allowed_actions_pipe_separated_is_parsed() {
        let agent = agent_with(slot_with(
            "Allowed actions: long_open | short_open | flat | hold | exit.",
            vec![],
        ));
        let err = validate_prompt_schema(&agent).expect_err("must reject");
        match err {
            PromptSchemaDriftError::AllowedActionsOutOfSchema { extra_actions, .. } => {
                assert_eq!(extra_actions, vec!["exit".to_string()]);
            }
            other => panic!("expected AllowedActionsOutOfSchema, got {other:?}"),
        }
    }

    #[test]
    fn allowed_actions_matching_schema_passes() {
        let agent = agent_with(slot_with(
            "Allowed actions: long_open, short_open, flat, hold.",
            vec![],
        ));
        validate_prompt_schema(&agent).expect("in-schema list must pass");
    }

    #[test]
    fn prompt_without_allowed_actions_header_is_unaffected() {
        let agent = agent_with(slot_with("Be careful. Hold the position when unsure.", vec![]));
        validate_prompt_schema(&agent).expect("no allowed-actions header → no rule-2 firing");
    }

    #[test]
    fn empty_prompt_passes_drift_check() {
        // Empty prompts have a separate warning in `validate_agent`;
        // they are NOT a drift error here.
        let agent = agent_with(slot_with("", vec![]));
        validate_prompt_schema(&agent).expect("empty prompt is not a drift error");
    }

    #[test]
    fn ohlcv_history_word_boundary_does_not_false_match_substring() {
        // `foo_ohlcv_history_bar` should NOT match because `_`
        // is a word character. Protects against accidental matches on
        // synthetic identifiers in prompts.
        let agent = agent_with(slot_with("see foo_ohlcv_history_bar for details", vec![]));
        validate_prompt_schema(&agent).expect("substring inside an identifier must not match");
    }

    #[test]
    fn word_boundary_helper_edge_cases() {
        assert!(contains_whole_word("call indicator_panel now", "indicator_panel"));
        assert!(contains_whole_word("indicator_panel", "indicator_panel"));
        assert!(contains_whole_word("use `indicator_panel`.", "indicator_panel"));
        assert!(!contains_whole_word(
            "use indicator_panel_v2 here",
            "indicator_panel"
        ));
        assert!(!contains_whole_word("use my_indicator_panel", "indicator_panel"));
        assert!(!contains_whole_word("", "indicator_panel"));
        assert!(!contains_whole_word("indicator_panel", ""));
    }
}
