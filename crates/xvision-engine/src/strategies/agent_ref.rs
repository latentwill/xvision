//! `AgentRef` + `PipelineDef` — the building blocks of the post-refactor
//! `Strategy`. Per the
//! `2026-05-12-strategies-refactor-agent-composition.md` plan, strategies
//! stop carrying fixed `regime/intern/trader` slots and instead reference
//! N agents from the workspace agent library, each playing a
//! user-defined role in the strategy's pipeline.
//!
//! This file is the strategy-side half of that refactor. The agent records
//! themselves live in `crates/xvision-engine/src/agents/`.

use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// Canonical form of a role string used as the comparison key across
/// the engine: trimmed of leading/trailing whitespace and lowercased
/// (ASCII). Pick the single helper so every comparison site agrees;
/// previously, sites were split between `trim()`,
/// `eq_ignore_ascii_case()`, and combinations of the two — which let a
/// `Trader` slot run as trader but drop its output (see QA finding #5,
/// 2026-05-17).
pub fn canonical_role(s: &str) -> String {
    s.trim().to_ascii_lowercase()
}

/// Serde deserializer for role fields: normalizes the on-disk value to
/// canonical form on load. Old strategy JSON with whitespace-padded or
/// mixed-case role values self-heals on the next read instead of
/// requiring a migration. Save also runs the same canonicalizer so disk
/// strings carry the canonical form going forward.
fn deserialize_role<'de, D>(d: D) -> Result<String, D::Error>
where
    D: Deserializer<'de>,
{
    let raw = String::deserialize(d)?;
    Ok(canonical_role(&raw))
}

fn serialize_role<S>(value: &str, s: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    s.serialize_str(&canonical_role(value))
}

/// One agent's appearance inside a strategy. `agent_id` is an FK to the
/// `Agent` record in the workspace agent library; `role` is the
/// user-defined role this agent plays in this particular strategy. The
/// same agent can appear in different strategies under different role
/// names — role lives on the reference, not the referent.
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts-export",
    ts(export, export_to = "../../../frontend/web/src/api/types.gen/")
)]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AgentRef {
    pub agent_id: String,
    #[serde(deserialize_with = "deserialize_role", serialize_with = "serialize_role")]
    pub role: String,
}

impl AgentRef {
    /// Canonical comparison key for this ref's role. Use this rather
    /// than reading `self.role` directly when comparing against
    /// pipeline-stage names like `"trader"`.
    pub fn canonical_role(&self) -> String {
        canonical_role(&self.role)
    }
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
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts-export",
    ts(export, export_to = "../../../frontend/web/src/api/types.gen/")
)]
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
/// owning strategy's `agents` list.
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts-export",
    ts(export, export_to = "../../../frontend/web/src/api/types.gen/")
)]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PipelineEdge {
    #[serde(deserialize_with = "deserialize_role", serialize_with = "serialize_role")]
    pub from_role: String,
    #[serde(deserialize_with = "deserialize_role", serialize_with = "serialize_role")]
    pub to_role: String,
}

/// Wiring spec for a strategy's agents.
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts-export",
    ts(export, export_to = "../../../frontend/web/src/api/types.gen/")
)]
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
    fn canonical_role_trims_and_lowercases() {
        assert_eq!(canonical_role(" Trader "), "trader");
        assert_eq!(canonical_role("TRADER"), "trader");
        assert_eq!(canonical_role("trader"), "trader");
        assert_eq!(canonical_role("   "), "");
    }

    #[test]
    fn agent_ref_deserialize_normalizes_role() {
        // Old strategy JSON with a whitespace-padded mixed-case role
        // self-heals on load.
        let r: AgentRef = serde_json::from_value(json!({
            "agent_id": "01HZAGENT",
            "role": " Trader ",
        }))
        .unwrap();
        assert_eq!(r.role, "trader");
        assert_eq!(r.canonical_role(), "trader");
    }

    #[test]
    fn agent_ref_serialize_emits_canonical_role() {
        // A programmatic construction that bypassed the deserializer
        // still serializes with the canonical form on the wire, so
        // round-tripped data is canonical.
        let r = AgentRef {
            agent_id: "01HZAGENT".into(),
            role: " Trader ".into(),
        };
        let s = serde_json::to_string(&r).unwrap();
        assert!(s.contains("\"trader\""), "expected canonical role on wire, got `{s}`");
        assert!(!s.contains(" Trader"), "expected no whitespace on wire");
    }

    #[test]
    fn pipeline_edge_deserialize_normalizes_roles() {
        let e: PipelineEdge = serde_json::from_value(json!({
            "from_role": " Scout ",
            "to_role": "TRADER",
        }))
        .unwrap();
        assert_eq!(e.from_role, "scout");
        assert_eq!(e.to_role, "trader");
    }

    #[test]
    fn pipeline_def_missing_edges_field_defaults_to_empty() {
        // Old strategies written before this refactor have no `edges` —
        // serde(default) lets them parse.
        let d: PipelineDef = serde_json::from_value(json!({
            "kind": "sequential"
        }))
        .unwrap();
        assert_eq!(d.kind, PipelineKind::Sequential);
        assert!(d.edges.is_empty());
    }
}
