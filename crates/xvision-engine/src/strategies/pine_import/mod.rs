//! Pine Script v5 ingestion — WU1 (parser) + WU2 (mapper).
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

mod ast;
mod lexer;
mod parser;
pub mod map;

pub use ast::{Expr, PineHeader, PineParseError, PineScript, Statement};
pub use map::{map_script, MapOutcome, UnmappedNode};
// Re-export BriefingIndicator from its canonical home in strategies::mod
pub use crate::strategies::BriefingIndicator;

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
