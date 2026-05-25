//! Local capability enum for the offline optimizer.
//!
//! This is intentionally a **small local copy** rather than a re-export from
//! `xvision-engine`. Depending on the engine from here would violate the
//! offline-isolation invariant (and drag the optimizer into the runtime
//! dependency graph). The variants mirror the capability-first agent roles
//! documented in the engine; if the engine's set drifts, this enum is updated
//! by hand — there is deliberately no compile-time coupling.

use serde::{Deserialize, Serialize};

/// A capability an agent slot can fulfil. Each capability maps to (at most) one
/// optimizer signature; capabilities without a signature return a typed
/// [`crate::error::OptimizerError::MissingCapabilityOptimizer`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Capability {
    /// Produces a `TraderDecision` (the trader's call into the risk gate).
    Trader,
    /// Boolean / scored filter over a candidate signal.
    Filter,
    /// Grades a completed decision cycle (offline reward signal).
    DecisionGrader,
    /// Cheap pre-pass / context gatherer.
    Intern,
    /// Authors chat-rail prose (non-trading).
    ChatAuthoring,
}

impl Capability {
    /// Stable string key used in provenance, signature hashing, and error text.
    pub fn as_key(self) -> &'static str {
        match self {
            Capability::Trader => "trader",
            Capability::Filter => "filter",
            Capability::DecisionGrader => "decision_grader",
            Capability::Intern => "intern",
            Capability::ChatAuthoring => "chat_authoring",
        }
    }

    /// Whether an optimizer signature is implemented for this capability today.
    ///
    /// `trader` and `filter` are fully implemented; the remaining three are
    /// declared stubs (see [`crate::signatures`]) and currently report
    /// unsupported via [`crate::error::OptimizerError::MissingCapabilityOptimizer`].
    pub fn has_optimizer(self) -> bool {
        matches!(self, Capability::Trader | Capability::Filter)
    }
}
