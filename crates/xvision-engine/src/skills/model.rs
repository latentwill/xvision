//! Skill + SkillKind types.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts-export",
    ts(export, export_to = "../../../frontend/web/src/api/types.gen/")
)]
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SkillKind {
    /// MCP-style callable — gets a tool slot in the agent's tool list.
    Tool,
    /// String prepended to the agent's system prompt.
    PromptFragment,
    /// Post-decision check that can veto / annotate.
    Evaluator,
}

impl SkillKind {
    pub fn as_str(self) -> &'static str {
        match self {
            SkillKind::Tool => "tool",
            SkillKind::PromptFragment => "prompt_fragment",
            SkillKind::Evaluator => "evaluator",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "tool" => Some(SkillKind::Tool),
            "prompt_fragment" => Some(SkillKind::PromptFragment),
            "evaluator" => Some(SkillKind::Evaluator),
            _ => None,
        }
    }
}

#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts-export",
    ts(export, export_to = "../../../frontend/web/src/api/types.gen/")
)]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Skill {
    pub skill_id: String,
    pub name: String,
    pub description: String,
    pub kind: SkillKind,
    /// Free-form per-kind config blob. v1 doesn't validate against a
    /// schema — that's a per-kind concern that lands when a kind's
    /// runtime application surfaces.
    #[cfg_attr(feature = "ts-export", ts(type = "Record<string, unknown>"))]
    pub config: serde_json::Value,
    pub archived: bool,
    #[cfg_attr(feature = "ts-export", ts(type = "string"))]
    pub created_at: DateTime<Utc>,
    #[cfg_attr(feature = "ts-export", ts(type = "string"))]
    pub updated_at: DateTime<Utc>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn skill_kind_round_trips() {
        for k in [
            SkillKind::Tool,
            SkillKind::PromptFragment,
            SkillKind::Evaluator,
        ] {
            let s = k.as_str();
            let back = SkillKind::parse(s).unwrap();
            assert_eq!(back, k);
        }
    }

    #[test]
    fn unknown_kind_string_returns_none() {
        assert!(SkillKind::parse("foobar").is_none());
    }
}
