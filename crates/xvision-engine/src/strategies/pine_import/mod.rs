//! Pine Script v5 ingestion — WU1: lexer + recursive-descent parser → typed AST.
//!
//! Public surface for WU1. WU2 (Strategy mapping) and WU4 (fidelity reports)
//! are NOT implemented here; this module is self-contained.

mod ast;
mod lexer;
mod parser;

pub use ast::{Expr, PineHeader, PineParseError, PineScript, Statement};

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
