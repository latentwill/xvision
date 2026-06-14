//! Pine Script v5 ingestion — WU1 (parser) + WU2 (mapper) + WU4 (fidelity diff).
//!
//! ## WU1 public surface
//!
//! [`parse_pine`] — parse a Pine Script v5 source string into a typed
//! [`PineScript`] AST. Unsupported constructs become [`Statement::Unsupported`]
//! nodes rather than errors.
//!
//! ## WU2 public surface
//!
//! [`map_script`] — map a parsed [`PineScript`] to an xvision [`Strategy`].
//! Returns a [`MapOutcome`] carrying the always-valid mapped strategy and any
//! [`UnmappedNode`]s that could not be deterministically converted.
//!
//! ## WU4 public surface
//!
//! [`import_pine`] — the single entry-point called by WU6 (CLI) and WU7 (HTTP
//! route). Runs parse → map → fidelity and returns an [`ImportOutcome`]
//! carrying both the validated strategy and the [`FidelityReport`].
//!
//! [`FidelityReport`] / [`FidelityItem`] — serializable diff report.
//! [`PineImportError`] — structured error wrapping parse failures and the
//! "nothing mappable" case.

mod ast;
pub mod fidelity;
pub mod inputs;
mod lexer;
pub mod library;
pub mod map;
mod parser;

pub use ast::{Expr, PineHeader, PineParseError, PineScript, Statement};
pub use fidelity::{build_fidelity_report, CostModelReference, FidelityItem, FidelityReport};
pub use inputs::{input_mutation_targets, InputKind, InputTarget};
pub use map::{map_script, MapOutcome, UnmappedNode};
// Re-export BriefingIndicator from its canonical home in strategies::mod
pub use crate::strategies::BriefingIndicator;

use crate::strategies::{Strategy, TunableBound};
use std::fmt;

impl From<InputTarget> for TunableBound {
    fn from(t: InputTarget) -> Self {
        TunableBound {
            path: t.path,
            min: t.min,
            max: t.max,
            step: t.step,
            kind: t.kind,
        }
    }
}

// ── WU4 types ─────────────────────────────────────────────────────────────────

/// The result of a successful Pine Script import.
///
/// Returned by [`import_pine`]. Both fields are always populated:
/// `strategy` has passed `validate_strategy` and `fidelity` describes
/// the semantic fidelity of the conversion.
#[derive(Debug, Clone)]
pub struct ImportOutcome {
    /// The mapped, validated xvision strategy.
    pub strategy: Strategy,
    /// Per-element fidelity classification (captured / approximated / dropped).
    pub fidelity: FidelityReport,
}

/// A structured error from [`import_pine`].
#[derive(Debug)]
pub enum PineImportError {
    /// The source could not be parsed at all (structural syntax error).
    ParseError(PineParseError),
    /// The script parsed successfully but contained nothing that could be
    /// mapped to an xvision strategy (no `strategy.entry` calls and no
    /// filter conditions — effectively an empty or purely visual script).
    NothingMappable(String),
}

impl fmt::Display for PineImportError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PineImportError::ParseError(e) => write!(f, "Pine parse error: {e}"),
            PineImportError::NothingMappable(msg) => write!(f, "Nothing mappable: {msg}"),
        }
    }
}

impl std::error::Error for PineImportError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            PineImportError::ParseError(e) => Some(e),
            PineImportError::NothingMappable(_) => None,
        }
    }
}

// ── WU4 entry-point ───────────────────────────────────────────────────────────

/// Parse a Pine Script v5 source string, map it to an xvision [`Strategy`],
/// and produce a [`FidelityReport`] describing the semantic fidelity of the
/// conversion.
///
/// This is **the single import entry-point** called by:
/// - WU6: `xvn strategy import-pine <file>` CLI command
/// - WU7: `POST /api/strategy/import/pine` HTTP route
///
/// # Errors
///
/// - [`PineImportError::ParseError`] when the source is structurally broken
///   (the parser could not produce any meaningful AST).
///
/// No error is returned for scripts with unsupported constructs — those are
/// recorded as `dropped` in the [`FidelityReport`] and the returned strategy
/// is a minimal valid Agentic strategy (the "honest starting point" goal).
pub fn import_pine(src: &str) -> Result<ImportOutcome, PineImportError> {
    // Step 1: Parse
    let script = parser::parse(src).map_err(PineImportError::ParseError)?;

    // Step 2: Map AST → Strategy
    let outcome = map::map_script(&script);

    // Step 3: Build fidelity report
    let fidelity = fidelity::build_fidelity_report(&script, &outcome);

    // Step 4 (WU-A): Collect input mutation targets → tunable bounds
    let targets = inputs::input_mutation_targets(&script, &outcome);
    let tunable_bounds: Vec<TunableBound> = targets.into_iter().map(TunableBound::from).collect();
    let mut strategy = outcome.strategy;
    strategy.tunable_bounds = tunable_bounds;

    Ok(ImportOutcome { strategy, fidelity })
}

// ── WU1 entry-point ───────────────────────────────────────────────────────────

/// Parse a Pine Script v5 source string into a typed [`PineScript`] AST.
///
/// # Errors
///
/// Returns [`PineParseError`] only when the source is structurally broken in a
/// way that prevents producing any meaningful AST (e.g. an unclosed
/// parenthesis in the `indicator(...)` / `strategy(...)` header call).
///
/// Unsupported or unrecognised constructs are captured as
/// [`Statement::Unsupported`] nodes — they do not cause an error and do not
/// panic.
pub fn parse_pine(src: &str) -> Result<PineScript, PineParseError> {
    parser::parse(src)
}
