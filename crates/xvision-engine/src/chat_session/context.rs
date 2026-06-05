//! `ContextScope` — the per-route discriminator that drives the chat rail's
//! quick-reply chip set, composer placeholder, and header label. Pure data
//! today; the WizardLoop (Phase B) will read it from a `ChatSession` and
//! inject the matching context into the system prompt.
//!
//! Per the plan's §1.4 chip table.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(tag = "scope", rename_all = "snake_case")]
pub enum ContextScope {
    /// "Whole workspace" — the default for routes without specific context.
    #[default]
    Workspace,
    /// Auto-set from the URL when no more-specific scope is selected.
    Route { route: String },
    /// `/eval/runs/<id>` — focused on a single run.
    Run { run_id: String },
    /// `/authoring/<id>` — focused on the strategy being authored.
    Strategy { draft_id: String },
    /// `/live/<id>` — focused on a deployed strategy.
    Deployment { deployment_id: String },
    /// `/eval/compare?ids=…` — focused on a compared set of runs.
    Compare { run_ids: Vec<String> },
    /// `/journal` filtered to a finding-kind set.
    JournalFilter { kinds: Vec<String> },
    /// User-selected items via the rail's "Selected items" affordance.
    Selection { items: Vec<String> },
    /// `/setup?seed=<seed_id>` — cross-cycle entry point.
    Seed { seed_id: String },
}

impl ContextScope {
    /// Header line shown in the rail's "Context: …" affordance.
    pub fn header_label(&self) -> String {
        match self {
            ContextScope::Workspace => "Whole workspace".into(),
            ContextScope::Route { route } => format!("This page · {route}"),
            ContextScope::Run { run_id } => format!("Run · {run_id}"),
            ContextScope::Strategy { draft_id } => format!("Editing · {draft_id}"),
            ContextScope::Deployment { deployment_id } => format!("Deployment · {deployment_id}"),
            ContextScope::Compare { run_ids } => format!("Comparing {} runs", run_ids.len()),
            ContextScope::JournalFilter { kinds } => {
                if kinds.is_empty() {
                    "Journal".into()
                } else {
                    format!("Journal · {}", kinds.join(", "))
                }
            }
            ContextScope::Selection { items } => format!("Selection · {} items", items.len()),
            ContextScope::Seed { seed_id } => format!("Seed · {seed_id}"),
        }
    }

    /// Suggested chip strings rendered above the composer.
    pub fn quick_replies(&self) -> &'static [&'static str] {
        match self {
            ContextScope::Workspace => &[
                "What needs my attention?",
                "Pick a draft to work on",
                "Summarize this week",
            ],
            ContextScope::Run { .. } => &[
                "Why did it underperform?",
                "Compare to its baseline",
                "Suggest a variant to draft",
            ],
            ContextScope::Strategy { .. } => &[
                "Improve this prompt",
                "Why is this slot expensive?",
                "Suggest a tool to add",
                "Diff vs template",
            ],
            ContextScope::Deployment { .. } => &[
                "Is this drift real?",
                "Should I pause it?",
                "Draft a variant from yesterday's vetoes",
            ],
            ContextScope::Compare { .. } => &[
                "What do the winners share?",
                "Why did the worst run underperform?",
                "Suggest a synthesis variant",
            ],
            ContextScope::JournalFilter { .. } => &[
                "Summarize what I've learned this week",
                "What's my most repeated mistake?",
                "Suggest a variant based on recent findings",
            ],
            ContextScope::Selection { .. } => &[
                "Compare these",
                "What do they have in common?",
                "Draft a variant that synthesizes them",
            ],
            ContextScope::Seed { .. } => &["Use this seed as the starting point", "Show what was different"],
            ContextScope::Route { route } => match route.as_str() {
                "/strategies" => &[
                    "Help me pick which to work on",
                    "Which has the worst recent eval?",
                    "Suggest a fork from the top-of-list",
                ],
                "/eval/runs" => &[
                    "Pick the most suspicious run",
                    "Find runs that disagree on the same scenario",
                    "Suggest a new scenario to test",
                ],
                _ => &[],
            },
        }
    }

    /// Cortex-memory namespace for chat-rail recall/write-back.
    ///
    /// The namespace is derived from the **scope**, not the session id, on
    /// purpose: a scope (a strategy draft, an eval run, the workspace) long
    /// outlives any single chat session, so binding memory to the scope lets
    /// salient facts survive session deletion / a fresh session opened on the
    /// same page. A session-keyed namespace would orphan every prior
    /// observation the moment the operator started a new conversation.
    ///
    /// Every namespace is prefixed `chat:` so it can never collide with the
    /// agent slot namespaces (`global`, `agent:{id}`) or the autooptimizer
    /// subsurface namespaces (`autooptimizer:judge`, `autooptimizer:mutations`)
    /// that share the same `MemoryStore`.
    pub fn memory_namespace(&self) -> String {
        match self {
            ContextScope::Workspace => "chat:workspace".into(),
            ContextScope::Route { route } => format!("chat:route:{route}"),
            ContextScope::Run { run_id } => format!("chat:run:{run_id}"),
            ContextScope::Strategy { draft_id } => format!("chat:strategy:{draft_id}"),
            ContextScope::Deployment { deployment_id } => {
                format!("chat:deployment:{deployment_id}")
            }
            ContextScope::Compare { run_ids } => {
                format!("chat:compare:{}", run_ids.join(","))
            }
            ContextScope::JournalFilter { kinds } => {
                format!("chat:journal:{}", kinds.join(","))
            }
            ContextScope::Selection { items } => {
                format!("chat:selection:{}", items.join(","))
            }
            ContextScope::Seed { seed_id } => format!("chat:seed:{seed_id}"),
        }
    }

    /// Composer placeholder text matching the active scope.
    pub fn placeholder(&self) -> &'static str {
        match self {
            ContextScope::Workspace => "Ask anything about your workspace…",
            ContextScope::Route { .. } => "Ask about this page…",
            ContextScope::Run { .. } => "Ask about this run…",
            ContextScope::Strategy { .. } => "Edit this slot…",
            ContextScope::Deployment { .. } => "Ask about this deployment…",
            ContextScope::Compare { .. } => "Ask about this comparison…",
            ContextScope::JournalFilter { .. } => "Ask about your journal…",
            ContextScope::Selection { .. } => "Ask about your selection…",
            ContextScope::Seed { .. } => "Refine this seed…",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn workspace_is_default() {
        assert_eq!(ContextScope::default(), ContextScope::Workspace);
    }

    #[test]
    fn run_scope_has_three_quick_replies() {
        let s = ContextScope::Run { run_id: "abc".into() };
        assert_eq!(s.quick_replies().len(), 3);
    }

    #[test]
    fn strategy_scope_has_four_quick_replies() {
        let s = ContextScope::Strategy {
            draft_id: "btc-momentum".into(),
        };
        assert_eq!(s.quick_replies().len(), 4);
    }

    #[test]
    fn route_scope_falls_back_to_empty_chips_for_unknown_routes() {
        let s = ContextScope::Route {
            route: "/unknown".into(),
        };
        assert_eq!(s.quick_replies().len(), 0);
    }

    #[test]
    fn route_scope_has_chips_for_known_routes() {
        let s = ContextScope::Route {
            route: "/strategies".into(),
        };
        assert_eq!(s.quick_replies().len(), 3);
    }

    #[test]
    fn header_label_includes_run_id() {
        let s = ContextScope::Run {
            run_id: "01HABC".into(),
        };
        assert!(s.header_label().contains("01HABC"));
    }

    #[test]
    fn placeholder_differs_per_scope() {
        let workspace = ContextScope::Workspace.placeholder();
        let run = ContextScope::Run { run_id: "x".into() }.placeholder();
        assert_ne!(workspace, run);
    }

    #[test]
    fn memory_namespace_is_scope_based_and_chat_prefixed() {
        assert_eq!(ContextScope::Workspace.memory_namespace(), "chat:workspace");
        assert_eq!(
            ContextScope::Strategy {
                draft_id: "s1".into()
            }
            .memory_namespace(),
            "chat:strategy:s1"
        );
        assert_eq!(
            ContextScope::Run {
                run_id: "01HABC".into()
            }
            .memory_namespace(),
            "chat:run:01HABC"
        );
        // Every variant must carry the `chat:` prefix so it can't collide
        // with agent (`global` / `agent:*`) or autooptimizer namespaces.
        for s in [
            ContextScope::Workspace,
            ContextScope::Route { route: "/x".into() },
            ContextScope::Deployment {
                deployment_id: "d1".into(),
            },
            ContextScope::Compare {
                run_ids: vec!["a".into(), "b".into()],
            },
            ContextScope::Seed { seed_id: "z".into() },
        ] {
            assert!(
                s.memory_namespace().starts_with("chat:"),
                "namespace must be chat-prefixed: {}",
                s.memory_namespace()
            );
        }
    }

    #[test]
    fn json_round_trips_with_serde_tag() {
        let s = ContextScope::Run { run_id: "abc".into() };
        let json = serde_json::to_string(&s).unwrap();
        assert!(json.contains("\"scope\":\"run\""));
        let back: ContextScope = serde_json::from_str(&json).unwrap();
        assert_eq!(back, s);
    }
}
