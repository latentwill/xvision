//! Typed AST for the Pine Script v5 subset understood by xvision.
//!
//! Anything not in the subset is stored as `Statement::Unsupported { source_span, raw }`
//! rather than returning an error or panicking.

use serde::{Deserialize, Serialize};
use std::fmt;

// ── Error type ────────────────────────────────────────────────────────────────

/// Parse error returned by [`super::parse_pine`].
///
/// Carries the 1-based line and column where the error was detected, plus a
/// human-readable message.
#[derive(Debug, Clone, PartialEq)]
pub struct PineParseError {
    /// 1-based line number where the error occurred.
    pub line: usize,
    /// 1-based column where the error occurred (best-effort).
    pub col: usize,
    /// Human-readable error message.
    pub message: String,
}

impl PineParseError {
    pub(crate) fn new(line: usize, col: usize, message: impl Into<String>) -> Self {
        PineParseError {
            line,
            col,
            message: message.into(),
        }
    }
}

impl fmt::Display for PineParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "Pine parse error at {}:{}: {}",
            self.line, self.col, self.message
        )
    }
}

impl std::error::Error for PineParseError {}

// ── Header ───────────────────────────────────────────────────────────────────

/// The `indicator(...)` or `strategy(...)` header call.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PineHeader {
    /// `"indicator"` or `"strategy"`.
    pub kind: String,
    /// First positional or `title=` argument.
    pub title: Option<String>,
    /// All arguments in order of appearance.
    /// Tuple is `(name, value)` where `name` is `None` for positional args.
    pub args: Vec<(Option<String>, Expr)>,
}

// ── Expressions ──────────────────────────────────────────────────────────────

/// A Pine Script expression (subset).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Expr {
    /// Integer literal, e.g. `14`.
    IntLit { value: i64 },
    /// Float literal, e.g. `2.5`.
    FloatLit { value: f64 },
    /// Boolean literal: `true` / `false`.
    BoolLit { value: bool },
    /// String literal (content without surrounding quotes).
    StrLit { value: String },
    /// Variable / built-in reference: `close`, `rsi_length`, …
    Ident { name: String },
    /// `ta.<name>(args)` call.
    TaCall { name: String, args: Vec<Expr> },
    /// `strategy.entry(...)` / `strategy.close(...)` / `strategy.exit(...)` as expression.
    StrategyCall {
        method: String,
        args: Vec<(Option<String>, Expr)>,
    },
    /// `input.int(...)` / `input.float(...)` / `input.bool(...)` / `input.string(...)`.
    InputCall {
        input_type: String,
        args: Vec<(Option<String>, Expr)>,
    },
    /// Binary operation: `left op right`.
    BinOp {
        op: String,
        left: Box<Expr>,
        right: Box<Expr>,
    },
    /// Unary `not expr`.
    Not { expr: Box<Expr> },
    /// Ternary `cond ? then_ : else_`.
    Ternary {
        cond: Box<Expr>,
        then_: Box<Expr>,
        else_: Box<Expr>,
    },
    /// Parenthesised expression.
    Paren { inner: Box<Expr> },
    /// Anything outside the subset — preserved verbatim.
    Unsupported { raw: String },
}

// ── Statements ───────────────────────────────────────────────────────────────

/// A top-level Pine Script statement (subset).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "stmt", rename_all = "snake_case")]
pub enum Statement {
    /// `x = <expr>` or `var x = <expr>`.
    Assignment {
        name: String,
        value: Expr,
        /// `true` when the `var` keyword is present (persistent variable).
        is_var: bool,
    },
    /// `input.int/float/bool/string(...)` assigned to a name.
    Input {
        name: String,
        input_type: String,
        args: Vec<(Option<String>, Expr)>,
    },
    /// `ta.<name>(args)` assigned to a name.
    TaAssignment {
        name: String,
        ta_name: String,
        args: Vec<Expr>,
    },
    /// `strategy.entry("id", direction, ...)`.
    StrategyEntry { args: Vec<(Option<String>, Expr)> },
    /// `strategy.close("id", ...)`.
    StrategyClose { args: Vec<(Option<String>, Expr)> },
    /// `strategy.exit("id", ...)`.
    StrategyExit { args: Vec<(Option<String>, Expr)> },
    /// An `if <condition>` block with a captured body.
    ///
    /// When the `if` guard is a mappable expression (comparison, crossover/crossunder,
    /// boolean operator), the condition is preserved here so the mapper can thread it
    /// into a Filter condition for the enclosed `strategy.entry`/`strategy.exit` calls.
    /// Body statements that are not parseable degrade to `Unsupported` rather than
    /// causing an error.
    ///
    /// `else` / `else if` branches are not yet fully supported — when encountered they
    /// are recorded as `Unsupported` in the body and do not block parsing.
    If {
        /// The guard expression (e.g. `ta.rsi(close,14) < 30`).
        condition: Expr,
        /// Statements in the indented body of the `if` block.
        body: Vec<Statement>,
    },
    /// A construct outside the supported subset.
    ///
    /// `source_span` is a `(start_byte, end_byte)` range in the original
    /// source string. `raw` is the raw text of the unsupported fragment.
    Unsupported {
        source_span: (usize, usize),
        raw: String,
    },
}

// ── Root ─────────────────────────────────────────────────────────────────────

/// The typed root of a parsed Pine Script.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PineScript {
    /// Pine version from `//@version=N`.
    pub version: u32,
    /// `indicator(...)` or `strategy(...)` header call, if present.
    pub header: Option<PineHeader>,
    /// Top-level statements in source order.
    pub statements: Vec<Statement>,
}
