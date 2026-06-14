//! Closed enum of the capability classes an agent slot can advertise.
//!
//! Capabilities are the dispatch axis for the post-2026-05-22 agent
//! graph (see
//! `docs/superpowers/specs/2026-05-22-capability-first-agent-model-and-graph-composition.md`).
//! Phase A — this file — only persists the type; the unified
//! `dispatch_capability` seam that consumes it lands in Phase B.
//!
//! Wire form (JSON): one of `"trader"`, `"filter"`, `"router"`. The serde tag
//! is `lowercase` so on-disk JSON reads the lowercase string verbatim; the DB
//! column on `agent_slots.capabilities` stores a JSON array of these strings.

use serde::{Deserialize, Serialize};

/// One capability class an agent slot can play in a strategy pipeline.
///
/// Closed set per Decision 1 of the capability-first agent model spec.
/// Adding a new variant is a schema change — the wave-coordinator must
/// reserve a migration row + update the dispatcher.
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts-export",
    ts(export, export_to = "../../../frontend/web/src/api/types.gen/")
)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Capability {
    /// Emits a `TraderDecision`. The legacy default — every pre-033
    /// slot is implicitly a `Trader` on the back-compat path.
    Trader,
    /// Emits a `FilterSignal` consumed by downstream agents via
    /// `PipelineEdge.condition` predicates.
    Filter,
    /// Picks which downstream branch of the pipeline executes next.
    Router,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    #[test]
    fn capability_wire_format_is_lowercase() {
        // All three variants must serialize as the lowercase variant
        // name so the on-disk JSON column shape stays stable.
        for v in [Capability::Trader, Capability::Filter, Capability::Router] {
            let s = serde_json::to_string(&v).unwrap();
            let back: Capability = serde_json::from_str(&s).unwrap();
            assert_eq!(v, back);
        }
        assert_eq!(serde_json::to_string(&Capability::Trader).unwrap(), "\"trader\"");
        assert_eq!(serde_json::to_string(&Capability::Filter).unwrap(), "\"filter\"");
        assert_eq!(serde_json::to_string(&Capability::Router).unwrap(), "\"router\"");
    }

    #[test]
    fn btreeset_ordering_is_stable() {
        // `BTreeSet<Capability>` is the persisted shape; the iteration
        // order must be deterministic so persisted JSON is byte-stable.
        // PartialOrd/Ord on the variants follow declaration order.
        let set: BTreeSet<Capability> = [Capability::Router, Capability::Trader, Capability::Filter]
            .into_iter()
            .collect();
        let collected: Vec<Capability> = set.into_iter().collect();
        assert_eq!(
            collected,
            vec![Capability::Trader, Capability::Filter, Capability::Router,]
        );
    }

    #[test]
    fn rejects_unknown_variant() {
        // A future column-value typo or hand-edited JSON file with an
        // unknown capability string must NOT silently parse to the
        // wrong variant — better to surface the parse error so the
        // operator notices.
        let err = serde_json::from_str::<Capability>("\"wat\"").unwrap_err();
        assert!(
            err.to_string().contains("unknown variant"),
            "expected 'unknown variant' error, got {err}",
        );
    }
}
