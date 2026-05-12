//! `AgentRef` + `PipelineDef` — the building blocks of the post-refactor
//! `Strategy`. Per the
//! `2026-05-12-strategies-refactor-agent-composition.md` plan, strategies
//! stop carrying fixed `regime/intern/trader` slots and instead reference
//! N agents from the workspace agent library, each playing a
//! user-defined role in the strategy's pipeline.
//!
//! This file is the bundle-side half of that refactor. The agent records
//! themselves live in `crates/xvision-engine/src/agents/`.

use serde::{Deserialize, Serialize};

/// One agent's appearance inside a strategy. `agent_id` is an FK to the
/// `Agent` record in the workspace agent library; `role` is the
/// user-defined role this agent plays in this particular strategy. The
/// same agent can appear in different strategies under different role
/// names — role lives on the reference, not the referent.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AgentRef {
    pub agent_id: String,
    pub role: String,
}

/// How the agents in a strategy wire together.
///
/// - `Single`: exactly one agent; no edges.
/// - `Sequential`: agents execute in the order they appear in
///   `Strategy.agents`. Edges are derived from that order so
///   `edges` is empty under this kind.
/// - `Graph`: arbitrary DAG defined by `edges`. v1 does not ship a graph
///   editor — the variant exists so on-disk JSON can carry a graph from
///   a future version without breaking parse for current builds.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PipelineKind {
    Single,
    Sequential,
    Graph,
}

impl Default for PipelineKind {
    fn default() -> Self {
        // New strategies default to Single. The operator picks Sequential
        // when adding the second agent in the inspector.
        Self::Single
    }
}

/// One directed edge in a `Graph` pipeline. Ignored for `Single` and
/// `Sequential`. Roles refer to `AgentRef.role` values present on the
/// owning bundle's `agents` list.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PipelineEdge {
    pub from_role: String,
    pub to_role: String,
}

/// Wiring spec for a strategy's agents.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PipelineDef {
    pub kind: PipelineKind,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub edges: Vec<PipelineEdge>,
}

impl Default for PipelineDef {
    fn default() -> Self {
        Self {
            kind: PipelineKind::default(),
            edges: Vec::new(),
        }
    }
}

impl PipelineDef {
    /// Convenience: the default for a fresh single-agent strategy.
    pub fn single() -> Self {
        Self {
            kind: PipelineKind::Single,
            edges: Vec::new(),
        }
    }

    /// Convenience: ordered sequential pipeline. Edges stay empty —
    /// callers read order off `Strategy.agents`.
    pub fn sequential() -> Self {
        Self {
            kind: PipelineKind::Sequential,
            edges: Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn pipeline_kind_default_is_single() {
        assert_eq!(PipelineKind::default(), PipelineKind::Single);
    }

    #[test]
    fn pipeline_def_default_is_single_empty_edges() {
        let d = PipelineDef::default();
        assert_eq!(d.kind, PipelineKind::Single);
        assert!(d.edges.is_empty());
    }

    #[test]
    fn pipeline_kind_serializes_snake_case() {
        // The serde tag must be `snake_case` so on-disk JSON reads
        // "single" / "sequential" / "graph" — matches the CLI flag
        // values in the plan.
        let v = serde_json::to_value(PipelineKind::Sequential).unwrap();
        assert_eq!(v, json!("sequential"));
        let v: PipelineKind = serde_json::from_value(json!("graph")).unwrap();
        assert_eq!(v, PipelineKind::Graph);
    }

    #[test]
    fn agent_ref_round_trips() {
        let r = AgentRef {
            agent_id: "01HZAGENT".into(),
            role: "trader".into(),
        };
        let s = serde_json::to_string(&r).unwrap();
        let back: AgentRef = serde_json::from_str(&s).unwrap();
        assert_eq!(back, r);
    }

    #[test]
    fn pipeline_def_round_trips_with_edges() {
        let d = PipelineDef {
            kind: PipelineKind::Graph,
            edges: vec![
                PipelineEdge {
                    from_role: "scout".into(),
                    to_role: "trader".into(),
                },
                PipelineEdge {
                    from_role: "risk".into(),
                    to_role: "trader".into(),
                },
            ],
        };
        let s = serde_json::to_string(&d).unwrap();
        let back: PipelineDef = serde_json::from_str(&s).unwrap();
        assert_eq!(back, d);
    }

    #[test]
    fn pipeline_def_round_trips_with_empty_edges_omitted() {
        // Single / Sequential omit `edges` from the wire shape so the
        // disk JSON stays compact.
        let d = PipelineDef::sequential();
        let s = serde_json::to_string(&d).unwrap();
        assert!(
            !s.contains("\"edges\""),
            "expected edges field omitted when empty, got `{s}`"
        );
        let back: PipelineDef = serde_json::from_str(&s).unwrap();
        assert_eq!(back, d);
    }

    #[test]
    fn pipeline_def_missing_edges_field_defaults_to_empty() {
        // Old bundles written before this refactor have no `edges` —
        // serde(default) lets them parse.
        let d: PipelineDef = serde_json::from_value(json!({
            "kind": "sequential"
        }))
        .unwrap();
        assert_eq!(d.kind, PipelineKind::Sequential);
        assert!(d.edges.is_empty());
    }
}
