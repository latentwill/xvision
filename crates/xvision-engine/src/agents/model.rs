//! Agent + AgentSlot value types.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts-export",
    ts(export, export_to = "../../../frontend/web/src/api/types.gen/")
)]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Agent {
    pub agent_id: String,
    pub name: String,
    pub description: String,
    pub tags: Vec<String>,
    pub slots: Vec<AgentSlot>,
    pub archived: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts-export",
    ts(export, export_to = "../../../frontend/web/src/api/types.gen/")
)]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AgentSlot {
    pub name: String,
    pub provider: String,
    pub model: String,
    pub system_prompt: String,
    /// Forward-compat hook for the v1.1 skill registry (see
    /// `docs/superpowers/plans/2026-05-11-agents-page-v1.md` §Skills).
    /// Each entry is a ULID into the workspace skill registry; entries
    /// of `kind = tool | prompt_fragment | evaluator` compose onto this
    /// slot at runtime. The picker is hidden in v1 until `/settings/skills`
    /// ships — but the field is persisted so existing agents survive the
    /// registry landing without a schema migration. Not related to the
    /// Plan 2b `xvn skill` surface that was removed in ADR 0012.
    pub skill_ids: Vec<String>,
    pub max_tokens: u32,
}

impl Agent {
    /// Construct a single-slot agent with sensible defaults — the default
    /// shape produced by `+ New agent` on `/agents/new`.
    pub fn single_slot_default(
        agent_id: impl Into<String>,
        name: impl Into<String>,
        provider: impl Into<String>,
        model: impl Into<String>,
    ) -> Self {
        let now = Utc::now();
        Self {
            agent_id: agent_id.into(),
            name: name.into(),
            description: String::new(),
            tags: Vec::new(),
            slots: vec![AgentSlot {
                name: "main".to_string(),
                provider: provider.into(),
                model: model.into(),
                system_prompt: String::new(),
                skill_ids: Vec::new(),
                max_tokens: 4096,
            }],
            archived: false,
            created_at: now,
            updated_at: now,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn single_slot_default_has_one_slot_named_main() {
        let a = Agent::single_slot_default(
            "01HZ000000000000000000000",
            "demo",
            "anthropic",
            "claude-sonnet-4-6",
        );
        assert_eq!(a.slots.len(), 1);
        assert_eq!(a.slots[0].name, "main");
        assert_eq!(a.slots[0].max_tokens, 4096);
        assert!(a.slots[0].system_prompt.is_empty());
        assert!(!a.archived);
        assert!(a.tags.is_empty());
    }
}
