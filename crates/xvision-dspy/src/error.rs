//! Typed errors for the offline optimizer surface.

use crate::capability::Capability;

/// Convenience result alias for optimizer operations.
pub type OptimizerResult<T> = Result<T, OptimizerError>;

/// Errors the optimizer surface can return. These are the *contract* errors the
/// caller is expected to branch on; lower-level `dspy-rs`/IO failures are wrapped
/// in [`OptimizerError::Engine`].
#[derive(Debug, thiserror::Error)]
pub enum OptimizerError {
    /// No optimizer signature exists for the requested capability.
    ///
    /// Carries remediation text so the caller can surface an actionable message
    /// rather than a bare "unsupported".
    #[error(
        "no optimizer for capability `{capability}`: {remediation}"
    )]
    MissingCapabilityOptimizer {
        /// The capability key (e.g. `decision_grader`).
        capability: &'static str,
        /// Human-readable next step.
        remediation: String,
    },

    /// The configured model provider could not be reached / is not configured.
    ///
    /// In this crate's offline scope this is only producible by the (stubbed)
    /// live adapter path; the deterministic test model never returns it.
    #[error("provider `{provider}` unavailable: {detail}")]
    ProviderUnavailable {
        /// Provider identity (e.g. `openai`, `anthropic`, or `dummy`).
        provider: String,
        /// Why it's unavailable + how to fix.
        detail: String,
    },

    /// A signature input/output failed parse or validation at the boundary.
    #[error("signature `{signature}` {phase} error: {detail}")]
    Signature {
        /// Signature name (capability key).
        signature: &'static str,
        /// `parse` or `validate`.
        phase: &'static str,
        /// Specifics.
        detail: String,
    },

    /// An underlying dspy-rs / engine failure (compile, IO, etc.).
    #[error("optimizer engine error: {0}")]
    Engine(String),
}

impl OptimizerError {
    /// Build a [`OptimizerError::MissingCapabilityOptimizer`] with standard
    /// remediation text for a capability that has no signature yet.
    pub fn missing_capability(capability: Capability) -> Self {
        let remediation = match capability {
            Capability::DecisionGrader => {
                "the decision_grader signature is a declared stub; implement \
                 `signatures::decision_grader` (define DSRs in/out fields + a \
                 metric) before optimizing this capability"
            }
            Capability::Intern => {
                "the intern signature is a declared stub; implement \
                 `signatures::intern` before optimizing this capability"
            }
            Capability::ChatAuthoring => {
                "the chat_authoring signature is a declared stub; implement \
                 `signatures::chat_authoring` before optimizing this capability"
            }
            // Trader/Filter are implemented; reaching here means a logic bug.
            Capability::Trader | Capability::Filter => {
                "this capability has an implemented signature; if you see this, \
                 the optimizer dispatch is mis-wired — file a bug"
            }
        };
        OptimizerError::MissingCapabilityOptimizer {
            capability: capability.as_key(),
            remediation: remediation.to_string(),
        }
    }
}
