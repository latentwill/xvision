//! Per-capability DSRs signatures (Phase 3.4).
//!
//! Each [`crate::capability::Capability`] maps to at most one DSPy signature.
//! A signature declares the typed input/output fields the optimizer searches
//! instructions/demos over, plus parse/validate boundaries for turning a model's
//! raw output back into a structured verdict.
//!
//! Implemented today: [`trader_decision`] and [`filter_signal`]. The remaining
//! three capabilities (`decision_grader`, `intern`, `chat_authoring`) are
//! **declared stubs** — calling [`signature_for`] on them returns a typed
//! [`OptimizerError::MissingCapabilityOptimizer`] with remediation text. The stub
//! modules below document the intended field shape so the next task can fill them
//! in without re-deriving the contract.
//!
//! ## Why `#[Signature]` with primitive fields
//!
//! The `dsrs_macros::Signature` attribute macro generates, for complex
//! (non-primitive) field types, calls to `schemars::schema_for!`. We keep every
//! field primitive (`String`, `f64`, `bool`) so the crate does not need a
//! `schemars` dependency and the signatures stay trivially serializable. Richer
//! structured outputs are parsed *out of* the model's text at the validate
//! boundary (see [`TraderVerdict`] / [`FilterVerdict`]) rather than encoded as
//! DSRs field types.

use dspy_rs::core::MetaSignature;
use dspy_rs::Signature;

use crate::capability::Capability;
use crate::error::{OptimizerError, OptimizerResult};

/// `trader_decision` — the trader capability's optimizer signature.
///
/// Inputs are the market briefing context; outputs are the structured call the
/// risk gate consumes. The instruction string (the doc comment on the first
/// input field) is what the optimizer rewrites.
#[Signature]
pub struct TraderDecisionSignature {
    /// You are a disciplined systematic trader. Given the market briefing and
    /// current position context, decide the action and size it. Be explicit
    /// about your rationale and keep size within the stated risk budget.
    #[input(desc = "Market briefing: prices, indicators, regime, news digest")]
    briefing: String,

    #[input(desc = "Current position + risk budget context")]
    position_context: String,

    #[output(desc = "One of: buy, sell, hold")]
    action: String,

    #[output(desc = "Fraction of risk budget to deploy, 0.0..=1.0")]
    size_fraction: f64,

    #[output(desc = "Short free-text rationale for the call")]
    rationale: String,
}

/// `filter_signal` — the filter capability's optimizer signature.
///
/// A filter scores/admits a candidate signal. Output is a keep/drop decision
/// plus a confidence score the strategy can threshold on.
#[Signature]
pub struct FilterSignalSignature {
    /// You are a signal-quality filter. Given a candidate trading signal and its
    /// supporting features, decide whether to keep it and how confident you are.
    /// Reject low-quality or contradicted signals.
    #[input(desc = "Candidate signal description")]
    signal: String,

    #[input(desc = "Supporting features / evidence for the signal")]
    features: String,

    #[output(desc = "true to keep the signal, false to drop it")]
    keep: bool,

    #[output(desc = "Confidence in the keep/drop decision, 0.0..=1.0")]
    confidence: f64,
}

/// Structured trader verdict parsed at the validate boundary from a model's
/// raw `action` output.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TraderAction {
    Buy,
    Sell,
    Hold,
}

impl TraderAction {
    /// Parse + validate a raw `action` string. Case-insensitive; trims
    /// whitespace. Anything else is a typed validate error.
    pub fn parse(raw: &str) -> OptimizerResult<TraderAction> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "buy" => Ok(TraderAction::Buy),
            "sell" => Ok(TraderAction::Sell),
            "hold" => Ok(TraderAction::Hold),
            other => Err(OptimizerError::Signature {
                signature: "trader_decision",
                phase: "validate",
                detail: format!("unrecognized action `{other}`; expected buy|sell|hold"),
            }),
        }
    }
}

/// Validate a `size_fraction` lies in `0.0..=1.0`.
pub fn validate_size_fraction(value: f64) -> OptimizerResult<f64> {
    if (0.0..=1.0).contains(&value) && value.is_finite() {
        Ok(value)
    } else {
        Err(OptimizerError::Signature {
            signature: "trader_decision",
            phase: "validate",
            detail: format!("size_fraction {value} out of range 0.0..=1.0"),
        })
    }
}

/// Validate a confidence score lies in `0.0..=1.0`.
pub fn validate_confidence(value: f64, signature: &'static str) -> OptimizerResult<f64> {
    if (0.0..=1.0).contains(&value) && value.is_finite() {
        Ok(value)
    } else {
        Err(OptimizerError::Signature {
            signature,
            phase: "validate",
            detail: format!("confidence {value} out of range 0.0..=1.0"),
        })
    }
}

/// A boxed [`MetaSignature`] for dynamic dispatch off a [`Capability`].
pub type BoxedSignature = Box<dyn MetaSignature>;

/// Return the optimizer signature for a capability, or a typed
/// [`OptimizerError::MissingCapabilityOptimizer`] for unimplemented ones.
///
/// This is the single dispatch point capability → signature. The three stub
/// capabilities fail here rather than silently producing an empty signature.
pub fn signature_for(capability: Capability) -> OptimizerResult<BoxedSignature> {
    match capability {
        Capability::Trader => Ok(Box::new(TraderDecisionSignature::new())),
        Capability::Filter => Ok(Box::new(FilterSignalSignature::new())),
        Capability::DecisionGrader | Capability::Intern | Capability::ChatAuthoring => {
            Err(OptimizerError::missing_capability(capability))
        }
    }
}

// ---------------------------------------------------------------------------
// Declared stubs (notes only). These document the intended signature shape for
// the follow-up task. They deliberately do NOT exist as `#[Signature]` structs
// yet so that `has_optimizer()` / `signature_for()` report them unsupported.
// ---------------------------------------------------------------------------

/// `decision_grader` (STUB — not yet implemented).
///
/// Intended shape:
/// * inputs: `cycle_summary: String` (briefing + decision + realized outcome),
///   `rubric: String`.
/// * outputs: `score: f64` (0.0..=1.0 reward), `critique: String`.
///
/// This is the offline reward signal an optimizer maximizes; it must be cheap
/// and deterministic enough to run over a whole corpus.
pub mod decision_grader {
    //! See module-level doc on [`super::decision_grader`].
}

/// `intern` (STUB — not yet implemented).
///
/// Intended shape:
/// * inputs: `task: String`, `raw_context: String`.
/// * outputs: `digest: String` (condensed context for the trader stage).
pub mod intern {
    //! See module-level doc on [`super::intern`].
}

/// `chat_authoring` (STUB — not yet implemented).
///
/// Intended shape:
/// * inputs: `user_request: String`, `app_state: String`.
/// * outputs: `reply: String`. Non-trading; lowest-risk capability.
pub mod chat_authoring {
    //! See module-level doc on [`super::chat_authoring`].
}
