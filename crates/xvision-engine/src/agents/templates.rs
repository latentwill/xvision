//! Starter templates for the `/agents/new` template picker.
//!
//! Three shapes, in order of complexity:
//!
//! 1. `single-trader` — one slot, one prompt. The 80% case.
//! 2. `analyst-executor` — two slots demonstrating sequential composition.
//! 3. `risk-checked-trader` — three slots showing a conventional
//!    trader / risk_check / executor pattern.
//!
//! Slot names are example conventions — the user is free to rename
//! anything. Templates seed the form; they don't enforce structure.

use serde::{Deserialize, Serialize};

use crate::agents::model::AgentSlot;

#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts-export",
    ts(export, export_to = "../../../frontend/web/src/api/types.gen/")
)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentTemplate {
    /// Stable identifier (e.g., "single-trader"). Surfaces in URLs as
    /// `/agents/new?template=single-trader` once that's wired.
    pub id: String,
    /// Human-readable label shown on the picker card.
    pub name: String,
    /// One-paragraph blurb describing what the template demonstrates.
    pub description: String,
    /// Pre-filled slots the operator can immediately customize.
    pub slots: Vec<AgentSlot>,
}

pub fn builtin_templates() -> Vec<AgentTemplate> {
    vec![
        AgentTemplate {
            id: "single-trader".into(),
            name: "Single-prompt trader".into(),
            description:
                "One slot, one model, one prompt. The 80% case — start here unless you're \
                 building a multi-stage pipeline."
                    .into(),
            slots: vec![AgentSlot {
                name: "main".into(),
                provider: "".into(),
                model: "".into(),
                system_prompt:
                    "You are a discretionary trader making one decision per cycle. Given the \
                     briefing, output exactly one JSON object matching: \
                     {\"action\":\"long_open|short_open|flat|hold\", \"conviction\":0..1, \
                     \"justification\":\"string\"}. Do not omit action."
                        .into(),
                skill_ids: vec![],
                max_tokens: 4096,
            }],
        },
        AgentTemplate {
            id: "analyst-executor".into(),
            name: "Analyst → Executor".into(),
            description:
                "Two slots demonstrating sequential composition. First slot analyzes the \
                 briefing into a thesis; second slot turns the thesis into an executable \
                 decision."
                    .into(),
            slots: vec![
                AgentSlot {
                    name: "analyst".into(),
                    provider: "".into(),
                    model: "".into(),
                    system_prompt:
                        "You are a market analyst. Read the briefing and output a structured \
                         thesis: regime, dominant signal, contradicting signals, expected \
                         volatility, time horizon."
                            .into(),
                    skill_ids: vec![],
                    max_tokens: 4096,
                },
                AgentSlot {
                    name: "executor".into(),
                    provider: "".into(),
                    model: "".into(),
                    system_prompt:
                        "You are an executor. Given the analyst's thesis, output a single \
                         JSON decision matching: {\"action\":\"long_open|short_open|flat|hold\", \
                         \"conviction\":0..1, \"justification\":\"string\"}. Be conservative \
                         when the analyst flags contradictions. Do not omit action."
                            .into(),
                    skill_ids: vec![],
                    max_tokens: 2048,
                },
            ],
        },
        AgentTemplate {
            id: "risk-checked-trader".into(),
            name: "Risk-checked trader".into(),
            description:
                "Three slots showing one conventional pattern: trader proposes, risk_check \
                 vetoes or modifies, executor commits. Demonstrates how named slots can model \
                 a multi-stage pipeline without enforcing those names."
                    .into(),
            slots: vec![
                AgentSlot {
                    name: "trader".into(),
                    provider: "".into(),
                    model: "".into(),
                    system_prompt:
                        "You are a trader. Propose a decision given the briefing. Output exactly \
                         one JSON object matching: {\"action\":\"long_open|short_open|flat|hold\", \
                         \"conviction\":0..1, \"justification\":\"string\"}. Do not omit action."
                            .into(),
                    skill_ids: vec![],
                    max_tokens: 4096,
                },
                AgentSlot {
                    name: "risk_check".into(),
                    provider: "".into(),
                    model: "".into(),
                    system_prompt:
                        "You are a risk gate. Given the trader's proposed decision and the \
                         current portfolio state, output {verdict: approve|modify|veto, \
                         size_cap_pct, reason}."
                            .into(),
                    skill_ids: vec![],
                    max_tokens: 2048,
                },
                AgentSlot {
                    name: "executor".into(),
                    provider: "".into(),
                    model: "".into(),
                    system_prompt:
                        "You are an executor. Given the trader's decision and the risk gate's \
                         verdict, output exactly one JSON object matching: \
                         {\"action\":\"long_open|short_open|flat|hold\", \"conviction\":0..1, \
                         \"justification\":\"string\"}. Do not omit action."
                            .into(),
                    skill_ids: vec![],
                    max_tokens: 2048,
                },
            ],
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn three_builtin_templates() {
        let t = builtin_templates();
        assert_eq!(t.len(), 3);
        let ids: Vec<&str> = t.iter().map(|x| x.id.as_str()).collect();
        assert!(ids.contains(&"single-trader"));
        assert!(ids.contains(&"analyst-executor"));
        assert!(ids.contains(&"risk-checked-trader"));
    }

    #[test]
    fn single_trader_has_one_slot() {
        let t = builtin_templates();
        let st = t.iter().find(|x| x.id == "single-trader").unwrap();
        assert_eq!(st.slots.len(), 1);
        assert_eq!(st.slots[0].name, "main");
    }

    #[test]
    fn slot_names_demonstrate_user_convention_not_enforcement() {
        let t = builtin_templates();
        let rct = t.iter().find(|x| x.id == "risk-checked-trader").unwrap();
        let names: Vec<&str> = rct.slots.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names, vec!["trader", "risk_check", "executor"]);
    }
}
