//! `AgentRef` + `PipelineDef` — the building blocks of the post-refactor
//! `Strategy`. Per the
//! `2026-05-12-strategies-refactor-agent-composition.md` plan, strategies
//! stop carrying fixed `regime/trader` slots and instead reference
//! N agents from the workspace agent library, each playing a
//! user-defined role in the strategy's pipeline.
//!
//! This file is the strategy-side half of that refactor. The agent records
//! themselves live in `crates/xvision-engine/src/agents/`.

use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::agents::capability::Capability;

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

/// Reference from a strategy's `AgentRef` to a trained nanochat checkpoint.
/// When `Some`, the slot runs a local nanochat model instead of an LLM.
/// Mutually exclusive with `model_override` at strategy-save validation time.
/// Absent (the default) → omitted from wire → all existing strategy JSON and
/// content hashes are byte-stable.
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts-export",
    ts(export, export_to = "../../../frontend/web/src/api/types.gen/")
)]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CheckpointRef {
    /// FK → `trained_models.model_id` (ULID).
    pub model_id: String,
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
    /// Which capability of the referenced agent this position
    /// activates. Phase A field — Phase B's unified dispatcher reads
    /// this to pick the slot's handler when an agent advertises more
    /// than one capability.
    ///
    /// `None` (the default) means "let the runtime pick the slot's
    /// first capability in `BTreeSet` order" — which is `Trader` for
    /// every legacy/pre-033 slot. Spec Decision 2.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts-export", ts(optional))]
    pub activates: Option<Capability>,
    /// Optional per-strategy override of the referenced agent's trader-slot
    /// system prompt. `None` (the default) = use the shared agent library
    /// prompt verbatim. `Some(p)` makes THIS strategy run with prompt `p`
    /// without mutating the shared `Agent` record — so the override lands in
    /// the `Strategy` content hash (proper lineage) and never leaks into other
    /// strategies that reference the same agent. This is the "home" that makes
    /// `prose` optimizer mutations reachable (run-7 finding; F25 design pass).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts-export", ts(optional))]
    pub prompt_override: Option<String>,
    /// Optional per-strategy override of the referenced agent's trader-slot
    /// `(provider/)model`. Same rationale as `prompt_override`. Reserved for the
    /// deferred F25 model-swap mutation axis; honored at resolution today so the
    /// axis is a pure mutator/validator add later.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts-export", ts(optional))]
    pub model_override: Option<String>,
    /// NEW: when present, this slot runs a local nanochat checkpoint instead of
    /// an LLM. Absent (the default) → omitted from the wire so every existing
    /// strategy's JSON and content hash is byte-stable.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts-export", ts(optional))]
    pub checkpoint: Option<CheckpointRef>,
    /// NEW: hard-gate (true) vs advisory (false) for nanochat filter slots.
    /// None → omitted from wire (default semantics = hard gate for nanochat
    /// slots, resolved at dispatch time). Omitted from wire when None.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts-export", ts(optional))]
    pub veto: Option<bool>,
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
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum PipelineKind {
    /// New strategies default to Single. The operator picks Sequential
    /// when adding the second agent in the inspector.
    #[default]
    Single,
    Sequential,
    Graph,
}

/// Predicate evaluated against an upstream agent's `FilterSignal.payload`
/// to decide whether a `PipelineEdge` fires. Closed set per Decision 5
/// of the capability-first agent model spec.
///
/// Phase A persists the shape only. The Phase B unified dispatcher
/// implements the evaluator — when an edge has `condition = Some(p)`,
/// the dispatcher resolves `p` against the from-side agent's most
/// recent `FilterSignal.payload` and drops the edge if the predicate
/// evaluates to `false`. `signal_field` is a dotted path into the
/// payload JSON (e.g. `"regime"`, `"confidence.value"`).
///
/// The serde tag is `snake_case` so on-disk JSON reads the lowercase
/// variant name verbatim:
///
/// ```json
/// { "eq": { "signal_field": "regime", "value": "trend" } }
/// { "any": [ { "eq": { ... } }, { "eq": { ... } } ] }
/// ```
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts-export",
    ts(export, export_to = "../../../frontend/web/src/api/types.gen/")
)]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum EdgePredicate {
    /// `payload.<signal_field> == value`
    Eq {
        signal_field: String,
        #[cfg_attr(feature = "ts-export", ts(type = "unknown"))]
        value: serde_json::Value,
    },
    /// `payload.<signal_field> != value`
    Neq {
        signal_field: String,
        #[cfg_attr(feature = "ts-export", ts(type = "unknown"))]
        value: serde_json::Value,
    },
    /// `payload.<signal_field> >= value` (numeric)
    Gte {
        signal_field: String,
        #[cfg_attr(feature = "ts-export", ts(type = "unknown"))]
        value: serde_json::Value,
    },
    /// `payload.<signal_field> <= value` (numeric)
    Lte {
        signal_field: String,
        #[cfg_attr(feature = "ts-export", ts(type = "unknown"))]
        value: serde_json::Value,
    },
    /// `payload.<signal_field>` ∈ `values`
    In {
        signal_field: String,
        #[cfg_attr(feature = "ts-export", ts(type = "unknown[]"))]
        values: Vec<serde_json::Value>,
    },
    /// All inner predicates must evaluate to `true`.
    All(Vec<EdgePredicate>),
    /// At least one inner predicate must evaluate to `true`.
    Any(Vec<EdgePredicate>),
    /// Inner predicate must evaluate to `false`.
    Not(Box<EdgePredicate>),
}

/// One directed edge in a `Graph` pipeline. Ignored for `Single` and
/// `Sequential`. Roles refer to `AgentRef.role` values present on the
/// owning strategy's `agents` list.
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts-export",
    ts(export, export_to = "../../../frontend/web/src/api/types.gen/")
)]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PipelineEdge {
    #[serde(deserialize_with = "deserialize_role", serialize_with = "serialize_role")]
    pub from_role: String,
    #[serde(deserialize_with = "deserialize_role", serialize_with = "serialize_role")]
    pub to_role: String,
    /// Optional predicate evaluated against the upstream agent's
    /// `FilterSignal.payload`. `None` (the default) = unconditional
    /// edge — today's behavior. `Some(p)` fires the edge only when `p`
    /// evaluates to `true`. Phase A persists the shape; Phase B
    /// implements the evaluator.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts-export", ts(optional))]
    pub condition: Option<EdgePredicate>,
}

/// Wiring spec for a strategy's agents.
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts-export",
    ts(export, export_to = "../../../frontend/web/src/api/types.gen/")
)]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct PipelineDef {
    pub kind: PipelineKind,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub edges: Vec<PipelineEdge>,
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
    fn agent_ref_overrides_default_to_none_and_omit_from_wire() {
        // A ref with no overrides must serialize WITHOUT the override keys, so
        // existing strategy JSON and content hashes are byte-stable.
        let r = AgentRef {
            agent_id: "01HZAGENT".into(),
            role: "trader".into(),
            activates: None,
            prompt_override: None,
            model_override: None,
            checkpoint: None,
            veto: None,
        };
        let s = serde_json::to_string(&r).unwrap();
        assert!(
            !s.contains("prompt_override"),
            "absent override must be omitted: {s}"
        );
        assert!(
            !s.contains("model_override"),
            "absent override must be omitted: {s}"
        );
        assert!(!s.contains("checkpoint"), "absent checkpoint must be omitted: {s}");
        assert!(!s.contains("veto"), "absent veto must be omitted: {s}");
    }

    #[test]
    fn agent_ref_overrides_round_trip_when_present() {
        let r = AgentRef {
            agent_id: "01HZAGENT".into(),
            role: "trader".into(),
            activates: None,
            prompt_override: Some("You are a disciplined momentum trader...".into()),
            model_override: Some("openrouter/google/gemini-3.1-flash-lite".into()),
            checkpoint: None,
            veto: None,
        };
        let s = serde_json::to_string(&r).unwrap();
        let back: AgentRef = serde_json::from_str(&s).unwrap();
        assert_eq!(back, r);
    }

    #[test]
    fn agent_ref_legacy_json_without_overrides_parses() {
        // Strategies written before this field exists must still load.
        let r: AgentRef = serde_json::from_value(json!({
            "agent_id": "01HZAGENT", "role": "trader"
        }))
        .unwrap();
        assert_eq!(r.prompt_override, None);
        assert_eq!(r.model_override, None);
    }

    #[test]
    fn agent_ref_round_trips() {
        let r = AgentRef {
            agent_id: "01HZAGENT".into(),
            role: "trader".into(),
            activates: None,
            prompt_override: None,
            model_override: None,
            checkpoint: None,
            veto: None,
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
                    condition: None,
                },
                PipelineEdge {
                    from_role: "risk".into(),
                    to_role: "trader".into(),
                    condition: None,
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
            activates: None,
            prompt_override: None,
            model_override: None,
            checkpoint: None,
            veto: None,
        };
        let s = serde_json::to_string(&r).unwrap();
        assert!(
            s.contains("\"trader\""),
            "expected canonical role on wire, got `{s}`"
        );
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

    #[test]
    fn checkpoint_ref_absent_omitted_from_wire() {
        // Existing strategies with no checkpoint must serialize identically to
        // before: new fields absent ⇒ omitted from wire ⇒ content hash stable.
        let r = AgentRef {
            agent_id: "01HZAGENT".into(),
            role: "filter".into(),
            activates: None,
            prompt_override: None,
            model_override: None,
            checkpoint: None,
            veto: None,
        };
        let s = serde_json::to_string(&r).unwrap();
        assert!(!s.contains("checkpoint"), "absent checkpoint must be omitted: {s}");
        assert!(!s.contains("veto"), "absent veto must be omitted: {s}");
    }

    #[test]
    fn checkpoint_ref_round_trips_when_present() {
        let r = AgentRef {
            agent_id: "01HZAGENT".into(),
            role: "filter".into(),
            activates: None,
            prompt_override: None,
            model_override: None,
            checkpoint: Some(CheckpointRef {
                model_id: "01JNANO00000000000000000000".into(),
            }),
            veto: Some(true),
        };
        let s = serde_json::to_string(&r).unwrap();
        let back: AgentRef = serde_json::from_str(&s).unwrap();
        assert_eq!(back, r);
        assert_eq!(back.checkpoint.as_ref().unwrap().model_id, "01JNANO00000000000000000000");
        assert_eq!(back.veto, Some(true));
    }

    #[test]
    fn agent_ref_legacy_json_without_checkpoint_or_veto_parses() {
        // JSON written before these fields exist must still load with None defaults.
        let r: AgentRef = serde_json::from_value(serde_json::json!({
            "agent_id": "01HZAGENT",
            "role": "trader"
        }))
        .unwrap();
        assert_eq!(r.checkpoint, None);
        assert_eq!(r.veto, None);
    }
}
