//! Recursive-descent parser for the Pine Script v5 subset.
//!
//! Produces a [`PineScript`] AST. Constructs outside the supported subset
//! are captured as [`Statement::Unsupported`] rather than returning errors.
//! Only truly malformed syntax (e.g. unclosed parentheses in a header call)
//! results in a [`PineParseError`].
//!
//! Strategy: we parse **line by line**. Each line is independently classified.
//! This means `strategy.entry(...)` inside an `if` block is still captured as
//! a `StrategyEntry` statement — the condition is captured as `Unsupported`.

use super::ast::{Expr, PineHeader, PineParseError, PineScript, Statement};
use super::lexer::{Lexer, Token, TokenKind};

// ── Token stream ─────────────────────────────────────────────────────────────

struct TokenStream {
    tokens: Vec<Token>,
    pos: usize,
}

impl TokenStream {
    fn new(tokens: Vec<Token>) -> Self {
        TokenStream { tokens, pos: 0 }
    }

    fn peek_raw(&self) -> Option<&Token> {
        self.tokens.get(self.pos)
    }

    fn next_raw(&mut self) -> Option<&Token> {
        let tok = self.tokens.get(self.pos)?;
        self.pos += 1;
        Some(tok)
    }

    fn skip_newlines(&mut self) {
        while self.pos < self.tokens.len() && self.tokens[self.pos].kind == TokenKind::Newline {
            self.pos += 1;
        }
    }

    fn mark(&self) -> usize {
        self.pos
    }

    fn restore(&mut self, mark: usize) {
        self.pos = mark;
    }

    fn current_byte_offset(&self) -> usize {
        self.tokens.get(self.pos).map_or(0, |t| t.span.start)
    }

    fn last_byte_end(&self) -> usize {
        if self.pos == 0 {
            0
        } else {
            self.tokens
                .get(self.pos.saturating_sub(1))
                .map_or(0, |t| t.span.end)
        }
    }

    fn current_line(&self) -> usize {
        self.tokens.get(self.pos).map_or(1, |t| t.span.line)
    }

    fn current_col(&self) -> usize {
        self.tokens.get(self.pos).map_or(1, |t| t.span.col)
    }
}

// ── Helpers ───────────────────────────────────────────────────────────────────

fn collect_line_raw(src: &str, start_byte: usize) -> String {
    let tail = &src[start_byte.min(src.len())..];
    let end = tail.find('\n').unwrap_or(tail.len());
    tail[..end].trim().to_string()
}

/// Skip tokens until we hit a newline (consuming it) or EOF.
fn skip_to_eol(ts: &mut TokenStream) {
    while let Some(tok) = ts.peek_raw() {
        let is_nl = tok.kind == TokenKind::Newline;
        ts.next_raw();
        if is_nl {
            return;
        }
    }
}

// ── Argument parsing ─────────────────────────────────────────────────────────

/// Parse `( arg, name=arg, ... )`. Returns `Err` only for unclosed `(`.
fn parse_arg_list(ts: &mut TokenStream) -> Result<Vec<(Option<String>, Expr)>, PineParseError> {
    ts.skip_newlines();
    let (line, col) = (ts.current_line(), ts.current_col());
    match ts.peek_raw() {
        Some(t) if t.kind == TokenKind::LParen => {
            ts.next_raw();
        }
        Some(t) => {
            return Err(PineParseError::new(
                t.span.line,
                t.span.col,
                format!("expected '(' got {:?}", t.kind),
            ))
        }
        None => return Err(PineParseError::new(line, col, "expected '(' but got EOF")),
    }

    let mut args: Vec<(Option<String>, Expr)> = Vec::new();
    loop {
        ts.skip_newlines();
        match ts.peek_raw() {
            Some(t) if t.kind == TokenKind::RParen => {
                ts.next_raw();
                break;
            }
            None => return Err(PineParseError::new(0, 0, "unclosed argument list")),
            _ => {}
        }

        let name = try_parse_named_arg(ts);
        let expr = parse_expr(ts)?;
        args.push((name, expr));

        ts.skip_newlines();
        match ts.peek_raw() {
            Some(t) if t.kind == TokenKind::Comma => {
                ts.next_raw();
            }
            Some(t) if t.kind == TokenKind::RParen => {
                ts.next_raw();
                break;
            }
            None => {
                return Err(PineParseError::new(
                    0,
                    0,
                    "unclosed argument list: expected ')' but got EOF",
                ))
            }
            Some(t) => {
                return Err(PineParseError::new(
                    t.span.line,
                    t.span.col,
                    format!("unclosed argument list: expected ')' or ',' but got {:?}", t.kind),
                ));
            }
        }
    }
    Ok(args)
}

fn try_parse_named_arg(ts: &mut TokenStream) -> Option<String> {
    let mark = ts.mark();
    let name = match ts.peek_raw() {
        Some(Token {
            kind: TokenKind::Ident(n),
            ..
        }) => n.clone(),
        _ => return None,
    };
    ts.next_raw();
    match ts.peek_raw() {
        Some(Token {
            kind: TokenKind::Eq, ..
        }) => {
            ts.next_raw();
            Some(name)
        }
        _ => {
            ts.restore(mark);
            None
        }
    }
}

// ── Expression parser ─────────────────────────────────────────────────────────

fn parse_expr(ts: &mut TokenStream) -> Result<Expr, PineParseError> {
    parse_ternary(ts)
}

fn parse_ternary(ts: &mut TokenStream) -> Result<Expr, PineParseError> {
    let cond = parse_or(ts)?;
    if matches!(ts.peek_raw(), Some(t) if t.kind == TokenKind::Question) {
        ts.next_raw();
        let then_ = parse_ternary(ts)?;
        match ts.peek_raw() {
            Some(t) if t.kind == TokenKind::Colon => {
                ts.next_raw();
            }
            _ => {}
        }
        let else_ = parse_ternary(ts)?;
        return Ok(Expr::Ternary {
            cond: Box::new(cond),
            then_: Box::new(then_),
            else_: Box::new(else_),
        });
    }
    Ok(cond)
}

fn parse_or(ts: &mut TokenStream) -> Result<Expr, PineParseError> {
    let mut left = parse_and(ts)?;
    loop {
        match ts.peek_raw() {
            Some(Token {
                kind: TokenKind::Ident(k),
                ..
            }) if k == "or" => {
                ts.next_raw();
                let right = parse_and(ts)?;
                left = Expr::BinOp {
                    op: "or".into(),
                    left: Box::new(left),
                    right: Box::new(right),
                };
            }
            _ => break,
        }
    }
    Ok(left)
}

fn parse_and(ts: &mut TokenStream) -> Result<Expr, PineParseError> {
    let mut left = parse_not(ts)?;
    loop {
        match ts.peek_raw() {
            Some(Token {
                kind: TokenKind::Ident(k),
                ..
            }) if k == "and" => {
                ts.next_raw();
                let right = parse_not(ts)?;
                left = Expr::BinOp {
                    op: "and".into(),
                    left: Box::new(left),
                    right: Box::new(right),
                };
            }
            _ => break,
        }
    }
    Ok(left)
}

fn parse_not(ts: &mut TokenStream) -> Result<Expr, PineParseError> {
    if let Some(Token {
        kind: TokenKind::Ident(k),
        ..
    }) = ts.peek_raw()
    {
        if k == "not" {
            ts.next_raw();
            let inner = parse_not(ts)?;
            return Ok(Expr::Not {
                expr: Box::new(inner),
            });
        }
    }
    parse_comparison(ts)
}

fn parse_comparison(ts: &mut TokenStream) -> Result<Expr, PineParseError> {
    let mut left = parse_additive(ts)?;
    loop {
        let op = match ts.peek_raw() {
            Some(Token {
                kind: TokenKind::EqEq,
                ..
            }) => "==",
            Some(Token {
                kind: TokenKind::Neq, ..
            }) => "!=",
            Some(Token {
                kind: TokenKind::Lt, ..
            }) => "<",
            Some(Token {
                kind: TokenKind::Lte, ..
            }) => "<=",
            Some(Token {
                kind: TokenKind::Gt, ..
            }) => ">",
            Some(Token {
                kind: TokenKind::Gte, ..
            }) => ">=",
            _ => break,
        }
        .to_string();
        ts.next_raw();
        let right = parse_additive(ts)?;
        left = Expr::BinOp {
            op,
            left: Box::new(left),
            right: Box::new(right),
        };
    }
    Ok(left)
}

fn parse_additive(ts: &mut TokenStream) -> Result<Expr, PineParseError> {
    let mut left = parse_multiplicative(ts)?;
    loop {
        let op = match ts.peek_raw() {
            Some(Token {
                kind: TokenKind::Plus,
                ..
            }) => "+",
            Some(Token {
                kind: TokenKind::Minus,
                ..
            }) => "-",
            _ => break,
        }
        .to_string();
        ts.next_raw();
        let right = parse_multiplicative(ts)?;
        left = Expr::BinOp {
            op,
            left: Box::new(left),
            right: Box::new(right),
        };
    }
    Ok(left)
}

fn parse_multiplicative(ts: &mut TokenStream) -> Result<Expr, PineParseError> {
    let mut left = parse_unary(ts)?;
    loop {
        let op = match ts.peek_raw() {
            Some(Token {
                kind: TokenKind::Star,
                ..
            }) => "*",
            Some(Token {
                kind: TokenKind::Slash,
                ..
            }) => "/",
            Some(Token {
                kind: TokenKind::Percent,
                ..
            }) => "%",
            _ => break,
        }
        .to_string();
        ts.next_raw();
        let right = parse_unary(ts)?;
        left = Expr::BinOp {
            op,
            left: Box::new(left),
            right: Box::new(right),
        };
    }
    Ok(left)
}

fn parse_unary(ts: &mut TokenStream) -> Result<Expr, PineParseError> {
    if let Some(Token {
        kind: TokenKind::Minus,
        ..
    }) = ts.peek_raw()
    {
        ts.next_raw();
        let inner = parse_primary(ts)?;
        return Ok(Expr::BinOp {
            op: "-".into(),
            left: Box::new(Expr::IntLit { value: 0 }),
            right: Box::new(inner),
        });
    }
    parse_postfix(ts)
}

fn parse_postfix(ts: &mut TokenStream) -> Result<Expr, PineParseError> {
    let mut base = parse_primary(ts)?;
    // History reference expr[i] → Unsupported
    loop {
        match ts.peek_raw() {
            Some(Token {
                kind: TokenKind::LBracket,
                ..
            }) => {
                ts.next_raw();
                let mut depth = 1usize;
                while let Some(t) = ts.next_raw() {
                    match &t.kind {
                        TokenKind::LBracket => depth += 1,
                        TokenKind::RBracket => {
                            depth -= 1;
                            if depth == 0 {
                                break;
                            }
                        }
                        _ => {}
                    }
                }
                base = Expr::Unsupported {
                    raw: "history_ref[...]".into(),
                };
            }
            _ => break,
        }
    }
    Ok(base)
}

fn parse_primary(ts: &mut TokenStream) -> Result<Expr, PineParseError> {
    match ts.peek_raw() {
        Some(Token {
            kind: TokenKind::LParen,
            ..
        }) => {
            ts.next_raw();
            let inner = parse_expr(ts)?;
            if matches!(ts.peek_raw(), Some(t) if t.kind == TokenKind::RParen) {
                ts.next_raw();
            }
            Ok(Expr::Paren {
                inner: Box::new(inner),
            })
        }
        Some(Token {
            kind: TokenKind::Int(v),
            ..
        }) => {
            let v = *v;
            ts.next_raw();
            Ok(Expr::IntLit { value: v })
        }
        Some(Token {
            kind: TokenKind::Float(v),
            ..
        }) => {
            let v = *v;
            ts.next_raw();
            Ok(Expr::FloatLit { value: v })
        }
        Some(Token {
            kind: TokenKind::Bool(v),
            ..
        }) => {
            let v = *v;
            ts.next_raw();
            Ok(Expr::BoolLit { value: v })
        }
        Some(Token {
            kind: TokenKind::Str(s),
            ..
        }) => {
            let s = s.clone();
            ts.next_raw();
            Ok(Expr::StrLit { value: s })
        }
        Some(Token {
            kind: TokenKind::TaDot(name),
            ..
        }) => {
            let name = name.clone();
            ts.next_raw();
            let args = parse_positional_args(ts).unwrap_or_default();
            Ok(Expr::TaCall { name, args })
        }
        Some(Token {
            kind: TokenKind::InputDot(itype),
            ..
        }) => {
            let itype = itype.clone();
            ts.next_raw();
            let args = parse_arg_list(ts).unwrap_or_default();
            Ok(Expr::InputCall {
                input_type: itype,
                args,
            })
        }
        Some(Token {
            kind: TokenKind::StrategyDot(method),
            ..
        }) => {
            let method = method.clone();
            ts.next_raw();
            let args = parse_arg_list(ts).unwrap_or_default();
            Ok(Expr::StrategyCall { method, args })
        }
        Some(Token {
            kind: TokenKind::Ident(name),
            ..
        }) => {
            let name = name.clone();
            ts.next_raw();
            // Check for unknown namespaced call: `ns.method(...)` where `ns` is not
            // a recognised namespace (ta/input/strategy — those are handled by the
            // lexer as composite tokens). E.g. `request.security(...)`, `math.max(...)`.
            if matches!(ts.peek_raw(), Some(t) if t.kind == TokenKind::Dot) {
                let dot_mark = ts.mark();
                ts.next_raw(); // consume '.'
                if let Some(Token {
                    kind: TokenKind::Ident(method),
                    ..
                }) = ts.peek_raw()
                {
                    let method = method.clone();
                    ts.next_raw(); // consume method name
                    if matches!(ts.peek_raw(), Some(t) if t.kind == TokenKind::LParen) {
                        // Unknown namespaced call: ns.method(...) → Unsupported with full name
                        let inner = skip_paren_group(ts).unwrap_or_default();
                        return Ok(Expr::Unsupported {
                            raw: format!("{name}.{method}({inner})"),
                        });
                    } else {
                        // ns.property (no call) — restore to after the dot so the
                        // Dot token is re-emitted as a binary Dot on the next parse step.
                        // Return the namespace ident; the property access is not supported.
                        ts.restore(dot_mark);
                        return Ok(Expr::Ident { name });
                    }
                } else {
                    // Dot not followed by ident — restore
                    ts.restore(dot_mark);
                }
            }
            // User-defined function call (no dot) → Unsupported
            if matches!(ts.peek_raw(), Some(t) if t.kind == TokenKind::LParen) {
                if let Ok(inner) = skip_paren_group(ts) {
                    return Ok(Expr::Unsupported {
                        raw: format!("{name}({inner})"),
                    });
                }
            }
            Ok(Expr::Ident { name })
        }
        Some(t) => {
            let raw = format!("{:?}", t.kind);
            ts.next_raw();
            Ok(Expr::Unsupported { raw })
        }
        None => Ok(Expr::Unsupported { raw: "EOF".into() }),
    }
}

fn parse_positional_args(ts: &mut TokenStream) -> Result<Vec<Expr>, PineParseError> {
    let args = parse_arg_list(ts)?;
    Ok(args.into_iter().map(|(_, e)| e).collect())
}

fn skip_paren_group(ts: &mut TokenStream) -> Result<String, PineParseError> {
    match ts.peek_raw() {
        Some(t) if t.kind == TokenKind::LParen => {
            ts.next_raw();
        }
        Some(t) => return Err(PineParseError::new(t.span.line, t.span.col, "expected '('")),
        None => return Err(PineParseError::new(0, 0, "expected '(' but got EOF")),
    }
    let mut depth = 1usize;
    let mut parts = String::new();
    while let Some(tok) = ts.next_raw() {
        match &tok.kind {
            TokenKind::LParen => {
                depth += 1;
                parts.push('(');
            }
            TokenKind::RParen => {
                depth -= 1;
                if depth == 0 {
                    break;
                }
                parts.push(')');
            }
            TokenKind::Ident(s) => {
                parts.push_str(s);
                parts.push(' ');
            }
            _ => {
                parts.push_str("_ ");
            }
        }
    }
    Ok(parts)
}

// ── Header parser ─────────────────────────────────────────────────────────────

fn parse_header(ts: &mut TokenStream, kind: &str) -> Result<PineHeader, PineParseError> {
    let args = parse_arg_list(ts)?;
    let title = args.first().and_then(|(name, expr)| {
        if name.as_deref().map_or(true, |n| n == "title") {
            if let Expr::StrLit { value } = expr {
                Some(value.clone())
            } else {
                None
            }
        } else {
            None
        }
    });
    Ok(PineHeader {
        kind: kind.to_string(),
        title,
        args,
    })
}

// ── Line classifier ───────────────────────────────────────────────────────────
//
// The core strategy: rather than a fully structural block parser (which would
// need a real indentation-sensitive grammar), we classify each logical line.
// Lines inside `if` / `for` / `switch` blocks are still classified and their
// `strategy.*` calls, assignments, and inputs are captured. The `if` condition
// line becomes `Unsupported`, and the body lines are individually classified.
// This mirrors how many Pine analysis tools work.

/// Keywords that introduce block-level constructs we capture as Unsupported
/// (their body lines are still parsed individually).
/// Note: `"if"` is NOT in this list — it has its own handler that produces
/// `Statement::If { condition, body }` for condition capture.
const UNSUPPORTED_BLOCK_KEYWORDS: &[&str] = &["for", "while", "switch", "type", "method", "import", "export"];

// ── Main parse entry ──────────────────────────────────────────────────────────

pub fn parse(src: &str) -> Result<PineScript, PineParseError> {
    let tokens = Lexer::new(src).tokenize();
    let mut ts = TokenStream::new(tokens);

    let mut version: u32 = 5;
    let mut header: Option<PineHeader> = None;
    let mut statements: Vec<Statement> = Vec::new();

    loop {
        ts.skip_newlines();
        if ts.peek_raw().is_none() {
            break;
        }

        let span_start = ts.current_byte_offset();

        // Clone the kind to avoid borrow issues
        let kind = match ts.peek_raw() {
            Some(t) => t.kind.clone(),
            None => break,
        };

        match kind {
            TokenKind::VersionDirective(v) => {
                ts.next_raw();
                version = v;
            }

            // `indicator(...)` / `strategy(...)` header
            TokenKind::Ident(ref kw) if kw == "indicator" || kw == "strategy" => {
                let kw = kw.clone();
                ts.next_raw();
                if matches!(ts.peek_raw(), Some(t) if t.kind == TokenKind::LParen) {
                    // Check if it's the header (not yet set) or a bare statement
                    if header.is_none() {
                        match parse_header(&mut ts, &kw) {
                            Ok(h) => {
                                header = Some(h);
                            }
                            Err(e) => return Err(e),
                        }
                        skip_to_eol(&mut ts);
                    } else {
                        // Second strategy() call — treat as Unsupported
                        let _ = skip_paren_group(&mut ts);
                        let raw = collect_line_raw(src, span_start);
                        let span_end = span_start + raw.len();
                        skip_to_eol(&mut ts);
                        statements.push(Statement::Unsupported {
                            source_span: (span_start, span_end),
                            raw,
                        });
                    }
                } else {
                    let raw = collect_line_raw(src, span_start);
                    let span_end = span_start + raw.len();
                    skip_to_eol(&mut ts);
                    statements.push(Statement::Unsupported {
                        source_span: (span_start, span_end),
                        raw,
                    });
                }
            }

            // `var name = expr`
            TokenKind::Ident(ref kw) if kw == "var" => {
                ts.next_raw(); // consume 'var'
                ts.skip_newlines();
                let name = match ts.peek_raw() {
                    Some(Token {
                        kind: TokenKind::Ident(n),
                        ..
                    }) => {
                        let n = n.clone();
                        ts.next_raw();
                        n
                    }
                    _ => {
                        let raw = collect_line_raw(src, span_start);
                        let span_end = span_start + raw.len();
                        skip_to_eol(&mut ts);
                        statements.push(Statement::Unsupported {
                            source_span: (span_start, span_end),
                            raw,
                        });
                        continue;
                    }
                };
                // Expect '='
                ts.skip_newlines();
                match ts.peek_raw() {
                    Some(Token {
                        kind: TokenKind::Eq, ..
                    }) => {
                        ts.next_raw();
                    }
                    _ => {
                        let raw = collect_line_raw(src, span_start);
                        let span_end = span_start + raw.len();
                        skip_to_eol(&mut ts);
                        statements.push(Statement::Unsupported {
                            source_span: (span_start, span_end),
                            raw,
                        });
                        continue;
                    }
                }
                ts.skip_newlines();
                let stmt = parse_rhs_stmt(&mut ts, src, span_start, name, true);
                skip_to_eol(&mut ts);
                statements.push(stmt);
            }

            // `if <condition>` block — capture condition + indented body
            TokenKind::Ident(ref kw) if kw == "if" => {
                ts.next_raw(); // consume 'if'
                ts.skip_newlines();
                // Try to parse the guard expression on the same line.
                // We attempt parse_expr; if it fails we fall back to Unsupported.
                let mark = ts.mark();
                let condition = match parse_expr(&mut ts) {
                    Ok(expr) => {
                        // consume remainder of the if-header line
                        skip_to_eol(&mut ts);
                        expr
                    }
                    Err(_) => {
                        ts.restore(mark);
                        let raw = collect_line_raw(src, span_start);
                        let span_end = span_start + raw.len();
                        skip_to_eol(&mut ts);
                        // Fallback: emit Unsupported for the entire if line and
                        // let the main loop handle the body lines individually.
                        statements.push(Statement::Unsupported {
                            source_span: (span_start, span_end),
                            raw,
                        });
                        continue;
                    }
                };
                // Collect the indented body (lines indented more than the if keyword).
                let body = parse_if_body(&mut ts, src);
                statements.push(Statement::If { condition, body });
            }

            // Block-level unsupported keywords: `for`, `while`, etc.
            // We capture the keyword line as Unsupported but DON'T skip the body —
            // body lines are parsed individually on the next iterations.
            TokenKind::Ident(ref kw) if UNSUPPORTED_BLOCK_KEYWORDS.contains(&kw.as_str()) => {
                let raw = collect_line_raw(src, span_start);
                let span_end = span_start + raw.len();
                skip_to_eol(&mut ts);
                statements.push(Statement::Unsupported {
                    source_span: (span_start, span_end),
                    raw,
                });
                // Do NOT skip the body — let the main loop handle each line.
            }

            // User-defined function definition `name(params) => expr`
            // We need a 2-token lookahead to distinguish from plain assignment.
            TokenKind::Ident(_) => {
                let mark = ts.mark();
                let name_tok = ts.next_raw().cloned().unwrap();
                let name = match &name_tok.kind {
                    TokenKind::Ident(n) => n.clone(),
                    _ => unreachable!(),
                };

                let is_funcdef = if matches!(ts.peek_raw(), Some(t) if t.kind == TokenKind::LParen) {
                    let inner_mark = ts.mark();
                    ts.next_raw(); // consume '('
                    let mut depth = 1usize;
                    while let Some(t) = ts.peek_raw() {
                        match t.kind {
                            TokenKind::LParen => {
                                depth += 1;
                                ts.next_raw();
                            }
                            TokenKind::RParen => {
                                depth -= 1;
                                ts.next_raw();
                                if depth == 0 {
                                    break;
                                }
                            }
                            _ => {
                                ts.next_raw();
                            }
                        }
                    }
                    let result = matches!(ts.peek_raw(), Some(t) if t.kind == TokenKind::Arrow);
                    ts.restore(inner_mark); // restore after the '('
                    result
                } else {
                    false
                };

                if is_funcdef {
                    ts.restore(mark);
                    let raw = collect_line_raw(src, span_start);
                    let span_end = span_start + raw.len();
                    skip_to_eol(&mut ts);
                    statements.push(Statement::Unsupported {
                        source_span: (span_start, span_end),
                        raw,
                    });
                    continue;
                }

                // Not a funcdef — process as assignment or other
                // We've already consumed the ident. Check what follows.
                ts.skip_newlines();
                match ts.peek_raw().map(|t| t.kind.clone()) {
                    // `name = <rhs>`
                    Some(TokenKind::Eq) => {
                        ts.next_raw(); // consume '='
                        ts.skip_newlines();
                        let stmt = parse_rhs_stmt(&mut ts, src, span_start, name, false);
                        skip_to_eol(&mut ts);
                        statements.push(stmt);
                    }
                    // `name := expr` (re-assignment)
                    Some(TokenKind::ColonEq) => {
                        ts.next_raw();
                        match parse_expr(&mut ts) {
                            Ok(expr) => {
                                skip_to_eol(&mut ts);
                                statements.push(Statement::Assignment {
                                    name,
                                    value: expr,
                                    is_var: false,
                                });
                            }
                            Err(_) => {
                                let raw = collect_line_raw(src, span_start);
                                let span_end = span_start + raw.len();
                                skip_to_eol(&mut ts);
                                statements.push(Statement::Unsupported {
                                    source_span: (span_start, span_end),
                                    raw,
                                });
                            }
                        }
                    }
                    // `name(args)` — bare call without assignment
                    Some(TokenKind::LParen) => {
                        let inner = skip_paren_group(&mut ts).unwrap_or_default();
                        let raw = format!("{name}({inner})");
                        let span_end = ts.last_byte_end();
                        skip_to_eol(&mut ts);
                        statements.push(Statement::Unsupported {
                            source_span: (span_start, span_end),
                            raw,
                        });
                    }
                    // Anything else — bare ident or other construct
                    _ => {
                        let raw = collect_line_raw(src, span_start);
                        let span_end = span_start + raw.len();
                        skip_to_eol(&mut ts);
                        statements.push(Statement::Unsupported {
                            source_span: (span_start, span_end),
                            raw,
                        });
                    }
                }
            }

            // `strategy.entry(...)` / `strategy.close(...)` / `strategy.exit(...)`
            TokenKind::StrategyDot(method) => {
                ts.next_raw();
                let args = parse_arg_list(&mut ts).unwrap_or_default();
                skip_to_eol(&mut ts);
                statements.push(match method.as_str() {
                    "entry" => Statement::StrategyEntry { args },
                    "close" => Statement::StrategyClose { args },
                    "exit" => Statement::StrategyExit { args },
                    _ => Statement::Unsupported {
                        source_span: (span_start, ts.last_byte_end()),
                        raw: format!("strategy.{method}(...)"),
                    },
                });
            }

            // `ta.<name>(...)` as bare statement
            TokenKind::TaDot(n) => {
                ts.next_raw();
                let _ = parse_positional_args(&mut ts).ok();
                let span_end = ts.last_byte_end();
                skip_to_eol(&mut ts);
                statements.push(Statement::Unsupported {
                    source_span: (span_start, span_end),
                    raw: format!("ta.{n}(...)"),
                });
            }

            // `input.*` as bare statement
            TokenKind::InputDot(_) => {
                ts.next_raw();
                let _ = parse_arg_list(&mut ts).ok();
                let span_end = ts.last_byte_end();
                skip_to_eol(&mut ts);
                statements.push(Statement::Unsupported {
                    source_span: (span_start, span_end),
                    raw: "input.(...)".into(),
                });
            }

            TokenKind::Newline => {
                ts.next_raw();
            }

            _ => {
                let raw = collect_line_raw(src, span_start);
                let span_end = span_start + raw.len();
                skip_to_eol(&mut ts);
                if !raw.is_empty() {
                    statements.push(Statement::Unsupported {
                        source_span: (span_start, span_end),
                        raw,
                    });
                }
            }
        }
    }

    Ok(PineScript {
        version,
        header,
        statements,
    })
}

/// Parse the indented body of an `if` block.
///
/// Reads statements as long as the next non-newline token is indented
/// (column > 1). Stops at EOF, at a top-level (`col == 1`) token, or at
/// an `else`/`else if` keyword on a non-indented line (which is recorded
/// as Unsupported and consumed so the main loop doesn't re-parse it).
///
/// Each body statement goes through the full statement parser so that
/// `strategy.entry`, `strategy.close`, `strategy.exit`, assignments, and
/// inputs are captured properly. Unrecognised body constructs become
/// `Statement::Unsupported`.
fn parse_if_body(ts: &mut TokenStream, src: &str) -> Vec<Statement> {
    let mut body: Vec<Statement> = Vec::new();

    loop {
        // Skip newlines between body lines
        while matches!(ts.peek_raw(), Some(t) if t.kind == TokenKind::Newline) {
            ts.next_raw();
        }

        // Check if next token is at col > 1 (indented — part of body)
        match ts.peek_raw() {
            None => break,
            Some(t) => {
                if t.span.col <= 1 {
                    // Non-indented token: body ends.
                    // Check if it's an `else` keyword — if so, consume the else
                    // branch as Unsupported so the main loop doesn't re-parse it.
                    if let TokenKind::Ident(ref kw) = t.kind.clone() {
                        if kw == "else" {
                            let span_start = t.span.start;
                            ts.next_raw(); // consume 'else'
                            let raw = collect_line_raw(src, span_start);
                            let span_end = span_start + raw.len();
                            skip_to_eol(ts);
                            body.push(Statement::Unsupported {
                                source_span: (span_start, span_end),
                                raw: format!("else {raw}").trim().to_string(),
                            });
                            // Consume the else body (indented lines after else)
                            let _else_body = parse_if_body(ts, src);
                            // Else body is discarded — we don't try to map it.
                        }
                    }
                    break;
                }
            }
        }

        let span_start = ts.current_byte_offset();

        // Clone the token kind to avoid borrow issues
        let kind = match ts.peek_raw() {
            Some(t) => t.kind.clone(),
            None => break,
        };

        let stmt = match kind {
            // `strategy.entry(...)` / `strategy.close(...)` / `strategy.exit(...)`
            TokenKind::StrategyDot(method) => {
                ts.next_raw();
                let args = parse_arg_list(ts).unwrap_or_default();
                skip_to_eol(ts);
                match method.as_str() {
                    "entry" => Statement::StrategyEntry { args },
                    "close" => Statement::StrategyClose { args },
                    "exit" => Statement::StrategyExit { args },
                    _ => {
                        let span_end = ts.last_byte_end();
                        Statement::Unsupported {
                            source_span: (span_start, span_end),
                            raw: format!("strategy.{method}(...)"),
                        }
                    }
                }
            }

            // `ta.*` as bare statement in body
            TokenKind::TaDot(n) => {
                ts.next_raw();
                let _ = parse_positional_args(ts).ok();
                let span_end = ts.last_byte_end();
                skip_to_eol(ts);
                Statement::Unsupported {
                    source_span: (span_start, span_end),
                    raw: format!("ta.{n}(...)"),
                }
            }

            // `input.*` as bare statement in body
            TokenKind::InputDot(_) => {
                ts.next_raw();
                let _ = parse_arg_list(ts).ok();
                let span_end = ts.last_byte_end();
                skip_to_eol(ts);
                Statement::Unsupported {
                    source_span: (span_start, span_end),
                    raw: "input.(...)".into(),
                }
            }

            // `var name = <rhs>`
            TokenKind::Ident(ref kw) if kw == "var" => {
                ts.next_raw(); // consume 'var'
                ts.skip_newlines();
                let name = match ts.peek_raw() {
                    Some(Token {
                        kind: TokenKind::Ident(n),
                        ..
                    }) => {
                        let n = n.clone();
                        ts.next_raw();
                        n
                    }
                    _ => {
                        let raw = collect_line_raw(src, span_start);
                        let span_end = span_start + raw.len();
                        skip_to_eol(ts);
                        body.push(Statement::Unsupported {
                            source_span: (span_start, span_end),
                            raw,
                        });
                        continue;
                    }
                };
                match ts.peek_raw() {
                    Some(Token {
                        kind: TokenKind::Eq, ..
                    }) => {
                        ts.next_raw();
                    }
                    _ => {
                        let raw = collect_line_raw(src, span_start);
                        let span_end = span_start + raw.len();
                        skip_to_eol(ts);
                        body.push(Statement::Unsupported {
                            source_span: (span_start, span_end),
                            raw,
                        });
                        continue;
                    }
                }
                let stmt = parse_rhs_stmt(ts, src, span_start, name, true);
                skip_to_eol(ts);
                stmt
            }

            // Generic ident: may be assignment (`name =`), re-assignment (`name :=`),
            // or bare call.
            TokenKind::Ident(_) => {
                let mark = ts.mark();
                let name = match ts.next_raw().map(|t| t.kind.clone()) {
                    Some(TokenKind::Ident(n)) => n,
                    _ => {
                        ts.restore(mark);
                        let raw = collect_line_raw(src, span_start);
                        let span_end = span_start + raw.len();
                        skip_to_eol(ts);
                        body.push(Statement::Unsupported {
                            source_span: (span_start, span_end),
                            raw,
                        });
                        continue;
                    }
                };

                match ts.peek_raw().map(|t| t.kind.clone()) {
                    Some(TokenKind::Eq) => {
                        ts.next_raw();
                        let stmt = parse_rhs_stmt(ts, src, span_start, name, false);
                        skip_to_eol(ts);
                        stmt
                    }
                    Some(TokenKind::ColonEq) => {
                        ts.next_raw();
                        match parse_expr(ts) {
                            Ok(expr) => {
                                skip_to_eol(ts);
                                Statement::Assignment {
                                    name,
                                    value: expr,
                                    is_var: false,
                                }
                            }
                            Err(_) => {
                                let raw = collect_line_raw(src, span_start);
                                let span_end = span_start + raw.len();
                                skip_to_eol(ts);
                                Statement::Unsupported {
                                    source_span: (span_start, span_end),
                                    raw,
                                }
                            }
                        }
                    }
                    Some(TokenKind::LParen) => {
                        let inner = skip_paren_group(ts).unwrap_or_default();
                        let span_end = ts.last_byte_end();
                        skip_to_eol(ts);
                        Statement::Unsupported {
                            source_span: (span_start, span_end),
                            raw: format!("{name}({inner})"),
                        }
                    }
                    _ => {
                        let raw = collect_line_raw(src, span_start);
                        let span_end = span_start + raw.len();
                        skip_to_eol(ts);
                        Statement::Unsupported {
                            source_span: (span_start, span_end),
                            raw,
                        }
                    }
                }
            }

            // Newline — skip
            TokenKind::Newline => {
                ts.next_raw();
                continue;
            }

            // Anything else → Unsupported
            _ => {
                let raw = collect_line_raw(src, span_start);
                let span_end = span_start + raw.len();
                skip_to_eol(ts);
                if raw.is_empty() {
                    continue;
                }
                Statement::Unsupported {
                    source_span: (span_start, span_end),
                    raw,
                }
            }
        };

        body.push(stmt);
    }

    body
}

/// Parse the right-hand side of an assignment after `=` has been consumed.
fn parse_rhs_stmt(
    ts: &mut TokenStream,
    src: &str,
    span_start: usize,
    name: String,
    is_var: bool,
) -> Statement {
    ts.skip_newlines();
    match ts.peek_raw().map(|t| t.kind.clone()) {
        Some(TokenKind::InputDot(itype)) => {
            ts.next_raw();
            let args = parse_arg_list(ts).unwrap_or_default();
            Statement::Input {
                name,
                input_type: itype,
                args,
            }
        }
        Some(TokenKind::TaDot(ta_name)) => {
            ts.next_raw();
            let args = parse_positional_args(ts).unwrap_or_default();
            Statement::TaAssignment { name, ta_name, args }
        }
        _ => match parse_expr(ts) {
            Ok(expr) => Statement::Assignment {
                name,
                value: expr,
                is_var,
            },
            Err(_) => {
                let raw = collect_line_raw(src, span_start);
                let span_end = span_start + raw.len();
                Statement::Unsupported {
                    source_span: (span_start, span_end),
                    raw,
                }
            }
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::strategies::pine_import::ast::Statement;

    #[test]
    fn parse_version_directive() {
        let src = "//@version=5\nindicator(\"Test\", overlay=true)\nx = close\n";
        let script = parse(src).unwrap();
        assert_eq!(script.version, 5);
        assert!(script.header.is_some());
        assert_eq!(script.header.as_ref().unwrap().kind, "indicator");
    }

    #[test]
    fn parse_simple_assignment() {
        let src = "//@version=5\nindicator(\"T\")\nx = close\n";
        let script = parse(src).unwrap();
        assert!(script
            .statements
            .iter()
            .any(|s| matches!(s, Statement::Assignment { name, .. } if name == "x")));
    }

    #[test]
    fn parse_input_int() {
        let src = "//@version=5\nindicator(\"T\")\nlen = input.int(14, title=\"Length\")\n";
        let script = parse(src).unwrap();
        assert!(script.statements.iter().any(
            |s| matches!(s, Statement::Input { name, input_type, .. } if name == "len" && input_type == "int")
        ));
    }

    #[test]
    fn parse_ta_assignment() {
        let src = "//@version=5\nindicator(\"T\")\nrsi_val = ta.rsi(close, 14)\n";
        let script = parse(src).unwrap();
        assert!(script
            .statements
            .iter()
            .any(|s| matches!(s, Statement::TaAssignment { ta_name, .. } if ta_name == "rsi")));
    }

    #[test]
    fn parse_strategy_entry() {
        let src = "//@version=5\nstrategy(\"T\")\nstrategy.entry(\"Long\", strategy.long)\n";
        let script = parse(src).unwrap();
        assert!(script
            .statements
            .iter()
            .any(|s| matches!(s, Statement::StrategyEntry { .. })));
    }

    #[test]
    fn var_keyword_sets_is_var_true() {
        let src = "//@version=5\nindicator(\"T\")\nvar x = 0\n";
        let script = parse(src).unwrap();
        assert!(script
            .statements
            .iter()
            .any(|s| matches!(s, Statement::Assignment { is_var: true, .. })));
    }

    #[test]
    fn malformed_header_returns_err() {
        let src = "//@version=5\nindicator(\"Broken\"\n";
        let result = parse(src);
        assert!(
            result.is_err(),
            "malformed header should return Err, got: {result:?}"
        );
    }

    #[test]
    fn unsupported_block_does_not_panic() {
        let src = "//@version=5\nindicator(\"T\")\nfor i = 0 to 10\n    x := x + i\nx = 5\n";
        let script = parse(src).expect("should not error");
        assert!(script
            .statements
            .iter()
            .any(|s| matches!(s, Statement::Unsupported { .. })));
    }

    #[test]
    fn strategy_calls_inside_if_block_are_captured() {
        // With the if-guard feature, `strategy.entry` inside an `if` block is
        // captured inside `Statement::If { body }`, NOT as a bare top-level StrategyEntry.
        // The entry is accessible via the If body, not lost.
        let src =
            "//@version=5\nstrategy(\"T\")\nif close > 100\n    strategy.entry(\"Long\", strategy.long)\n";
        let script = parse(src).expect("should not error");
        // The if block should produce Statement::If with the entry in the body.
        let has_entry_in_if_body = script.statements.iter().any(|s| {
            if let Statement::If { body, .. } = s {
                body.iter().any(|b| matches!(b, Statement::StrategyEntry { .. }))
            } else {
                false
            }
        });
        assert!(
            has_entry_in_if_body,
            "strategy.entry inside if should be captured in Statement::If body: {script:?}"
        );
    }
}
