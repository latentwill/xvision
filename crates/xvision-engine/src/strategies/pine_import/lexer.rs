//! Tokenizer for the Pine Script v5 subset.
//!
//! Produces a flat `Vec<Token>` from source text. Unknown/unsupported
//! characters are emitted as `Token::Unknown` rather than erroring.

#[derive(Debug, Clone, PartialEq)]
pub struct Span {
    /// Byte offset of the first character (inclusive).
    pub start: usize,
    /// Byte offset just past the last character (exclusive).
    pub end: usize,
    /// 1-based line number of `start`.
    pub line: usize,
    /// 1-based column of `start`.
    pub col: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub enum TokenKind {
    // Literals
    Int(i64),
    Float(f64),
    Bool(bool),
    Str(String),

    // Identifiers & keywords
    Ident(String),
    // Composite namespace tokens we produce for convenience
    /// `ta.sma`, `ta.rsi`, etc.
    TaDot(String),
    /// `input.int`, `input.float`, `input.bool`, `input.string`
    InputDot(String),
    /// `strategy.entry`, `strategy.close`, `strategy.exit`
    StrategyDot(String),

    // Punctuation
    LParen,
    RParen,
    LBracket,
    RBracket,
    LBrace,
    RBrace,
    Comma,
    Dot,
    Colon,
    Semicolon,
    ColonEq, // :=
    Eq,      // =
    EqEq,    // ==
    Neq,     // !=
    Lt,      // <
    Lte,     // <=
    Gt,      // >
    Gte,     // >=
    Plus,
    Minus,
    Star,
    Slash,
    Percent,
    Question, // ?
    Arrow,    // =>

    // Special directives
    /// `//@version=5` directive — carries the version number.
    VersionDirective(u32),

    // Newline — significant in Pine (statement separator).
    Newline,

    // Unknown / unsupported character (passed through as raw bytes).
    Unknown(char),
}

#[derive(Debug, Clone)]
pub struct Token {
    pub kind: TokenKind,
    pub span: Span,
}

pub struct Lexer<'src> {
    src: &'src str,
    /// Current byte position.
    pos: usize,
    line: usize,
    col: usize,
}

impl<'src> Lexer<'src> {
    pub fn new(src: &'src str) -> Self {
        Lexer {
            src,
            pos: 0,
            line: 1,
            col: 1,
        }
    }

    fn remaining(&self) -> &str {
        &self.src[self.pos..]
    }

    fn peek(&self) -> Option<char> {
        self.remaining().chars().next()
    }

    fn peek2(&self) -> Option<char> {
        let mut it = self.remaining().chars();
        it.next();
        it.next()
    }

    fn advance(&mut self) -> Option<char> {
        let c = self.remaining().chars().next()?;
        self.pos += c.len_utf8();
        if c == '\n' {
            self.line += 1;
            self.col = 1;
        } else {
            self.col += 1;
        }
        Some(c)
    }

    fn make_span(&self, start: usize, start_line: usize, start_col: usize) -> Span {
        Span {
            start,
            end: self.pos,
            line: start_line,
            col: start_col,
        }
    }

    fn skip_spaces(&mut self) {
        while let Some(c) = self.peek() {
            if c == ' ' || c == '\t' || c == '\r' {
                self.advance();
            } else {
                break;
            }
        }
    }

    fn lex_comment_or_directive(&mut self, start: usize, sl: usize, sc: usize) -> Option<Token> {
        // We saw '//'. Check for '@version=N'
        let rest = self.remaining();
        if rest.starts_with("@version=") {
            self.pos += "@version=".len();
            self.col += "@version=".len();
            let num_start = self.pos;
            while self.peek().map_or(false, |c| c.is_ascii_digit()) {
                self.advance();
            }
            let num_str = &self.src[num_start..self.pos];
            let version: u32 = num_str.parse().unwrap_or(0);
            // consume rest of line
            while let Some(c) = self.peek() {
                if c == '\n' {
                    break;
                }
                self.advance();
            }
            return Some(Token {
                kind: TokenKind::VersionDirective(version),
                span: self.make_span(start, sl, sc),
            });
        }
        // Regular comment — consume to end of line, emit nothing
        while let Some(c) = self.peek() {
            if c == '\n' {
                break;
            }
            self.advance();
        }
        None
    }

    fn lex_string(&mut self, quote: char, start: usize, sl: usize, sc: usize) -> Token {
        let mut s = String::new();
        loop {
            match self.peek() {
                None => break,
                Some(c) if c == quote => {
                    self.advance();
                    break;
                }
                Some('\\') => {
                    self.advance();
                    match self.advance() {
                        Some('n') => s.push('\n'),
                        Some('t') => s.push('\t'),
                        Some(c) => s.push(c),
                        None => break,
                    }
                }
                Some(c) => {
                    self.advance();
                    s.push(c);
                }
            }
        }
        Token {
            kind: TokenKind::Str(s),
            span: self.make_span(start, sl, sc),
        }
    }

    fn lex_number(&mut self, start: usize, sl: usize, sc: usize) -> Token {
        // Consume digits
        while self.peek().map_or(false, |c| c.is_ascii_digit()) {
            self.advance();
        }
        // Check for decimal point followed by digit (not .. operator)
        let is_float = self.peek() == Some('.') && self.peek2().map_or(false, |c| c.is_ascii_digit());
        if is_float {
            self.advance(); // consume '.'
            while self.peek().map_or(false, |c| c.is_ascii_digit()) {
                self.advance();
            }
        }
        let text = &self.src[start..self.pos];
        let kind = if is_float {
            TokenKind::Float(text.parse().unwrap_or(0.0))
        } else {
            // Try int, fall back to float
            if let Ok(i) = text.parse::<i64>() {
                TokenKind::Int(i)
            } else {
                TokenKind::Float(text.parse().unwrap_or(0.0))
            }
        };
        Token {
            kind,
            span: self.make_span(start, sl, sc),
        }
    }

    fn lex_ident_or_keyword(&mut self, start: usize, sl: usize, sc: usize) -> Token {
        while self.peek().map_or(false, |c| c.is_alphanumeric() || c == '_') {
            self.advance();
        }
        let word = &self.src[start..self.pos];

        // Check for namespace tokens: ta., input., strategy.
        if self.peek() == Some('.') {
            let saved_pos = self.pos;
            let saved_line = self.line;
            let saved_col = self.col;
            self.advance(); // consume '.'
            let ns_start = self.pos;
            while self.peek().map_or(false, |c| c.is_alphanumeric() || c == '_') {
                self.advance();
            }
            let method = &self.src[ns_start..self.pos];
            if !method.is_empty() {
                match word {
                    "ta" => {
                        return Token {
                            kind: TokenKind::TaDot(method.to_string()),
                            span: self.make_span(start, sl, sc),
                        }
                    }
                    "input" => {
                        return Token {
                            kind: TokenKind::InputDot(method.to_string()),
                            span: self.make_span(start, sl, sc),
                        }
                    }
                    "strategy" => {
                        return Token {
                            kind: TokenKind::StrategyDot(method.to_string()),
                            span: self.make_span(start, sl, sc),
                        }
                    }
                    _ => {}
                }
            }
            // Not a recognised namespace — backtrack to after the word (dot stays unread? no,
            // emit ident and let the dot be re-lexed). We restore pos/line/col to right after the word.
            self.pos = saved_pos;
            self.line = saved_line;
            self.col = saved_col;
        }

        let kind = match word {
            "true" => TokenKind::Bool(true),
            "false" => TokenKind::Bool(false),
            _ => TokenKind::Ident(word.to_string()),
        };
        Token {
            kind,
            span: self.make_span(start, sl, sc),
        }
    }

    /// Lex one token (or None at EOF). Skips `None`-producing comment lines
    /// by trying again.
    pub fn next_token(&mut self) -> Option<Token> {
        loop {
            self.skip_spaces();
            let start = self.pos;
            let sl = self.line;
            let sc = self.col;
            let c = self.peek()?;

            // Newline
            if c == '\n' {
                self.advance();
                return Some(Token {
                    kind: TokenKind::Newline,
                    span: self.make_span(start, sl, sc),
                });
            }

            // Comments / directives
            if c == '/' && self.remaining().starts_with("//") {
                self.advance();
                self.advance(); // consume '//'
                if let Some(tok) = self.lex_comment_or_directive(start, sl, sc) {
                    return Some(tok);
                }
                // comment — loop again
                continue;
            }

            // String literals
            if c == '"' || c == '\'' {
                self.advance();
                return Some(self.lex_string(c, start, sl, sc));
            }

            // Numbers (starts with digit, or '-' followed by digit handled in parser)
            if c.is_ascii_digit() {
                return Some(self.lex_number(start, sl, sc));
            }

            // Identifiers / keywords / namespace tokens
            if c.is_alphabetic() || c == '_' {
                return Some(self.lex_ident_or_keyword(start, sl, sc));
            }

            // Operators and punctuation
            self.advance();
            let kind = match c {
                '(' => TokenKind::LParen,
                ')' => TokenKind::RParen,
                '[' => TokenKind::LBracket,
                ']' => TokenKind::RBracket,
                '{' => TokenKind::LBrace,
                '}' => TokenKind::RBrace,
                ',' => TokenKind::Comma,
                '.' => TokenKind::Dot,
                ';' => TokenKind::Semicolon,
                '?' => TokenKind::Question,
                ':' => {
                    if self.peek() == Some('=') {
                        self.advance();
                        TokenKind::ColonEq
                    } else {
                        TokenKind::Colon
                    }
                }
                '=' => {
                    if self.peek() == Some('>') {
                        self.advance();
                        TokenKind::Arrow
                    } else if self.peek() == Some('=') {
                        self.advance();
                        TokenKind::EqEq
                    } else {
                        TokenKind::Eq
                    }
                }
                '!' => {
                    if self.peek() == Some('=') {
                        self.advance();
                        TokenKind::Neq
                    } else {
                        TokenKind::Unknown('!')
                    }
                }
                '<' => {
                    if self.peek() == Some('=') {
                        self.advance();
                        TokenKind::Lte
                    } else {
                        TokenKind::Lt
                    }
                }
                '>' => {
                    if self.peek() == Some('=') {
                        self.advance();
                        TokenKind::Gte
                    } else {
                        TokenKind::Gt
                    }
                }
                '+' => TokenKind::Plus,
                '-' => TokenKind::Minus,
                '*' => TokenKind::Star,
                '/' => TokenKind::Slash,
                '%' => TokenKind::Percent,
                other => TokenKind::Unknown(other),
            };
            return Some(Token {
                kind,
                span: self.make_span(start, sl, sc),
            });
        }
    }

    /// Lex all tokens, collecting them into a Vec.
    pub fn tokenize(mut self) -> Vec<Token> {
        let mut tokens = Vec::new();
        while let Some(tok) = self.next_token() {
            tokens.push(tok);
        }
        tokens
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_directive_is_lexed() {
        let src = "//@version=5\n";
        let toks = Lexer::new(src).tokenize();
        assert!(
            toks.iter()
                .any(|t| matches!(t.kind, TokenKind::VersionDirective(5))),
            "expected VersionDirective(5), got: {toks:?}"
        );
    }

    #[test]
    fn ta_dot_is_combined_token() {
        let src = "ta.sma(close, 14)";
        let toks = Lexer::new(src).tokenize();
        assert!(
            toks.iter()
                .any(|t| matches!(&t.kind, TokenKind::TaDot(n) if n == "sma")),
            "expected TaDot(\"sma\"), got: {toks:?}"
        );
    }

    #[test]
    fn input_dot_is_combined_token() {
        let src = "x = input.int(14, title=\"Length\")";
        let toks = Lexer::new(src).tokenize();
        assert!(
            toks.iter()
                .any(|t| matches!(&t.kind, TokenKind::InputDot(n) if n == "int")),
            "expected InputDot(\"int\"), got: {toks:?}"
        );
    }

    #[test]
    fn strategy_dot_entry() {
        let src = "strategy.entry(\"Long\", strategy.long)";
        let toks = Lexer::new(src).tokenize();
        assert!(
            toks.iter()
                .any(|t| matches!(&t.kind, TokenKind::StrategyDot(n) if n == "entry")),
            "expected StrategyDot(\"entry\")"
        );
    }

    #[test]
    fn colon_eq_token() {
        let src = "x := 5";
        let toks = Lexer::new(src).tokenize();
        assert!(toks.iter().any(|t| t.kind == TokenKind::ColonEq));
    }

    #[test]
    fn string_literal_unquoted() {
        let src = r#"x = "hello world""#;
        let toks = Lexer::new(src).tokenize();
        assert!(toks
            .iter()
            .any(|t| matches!(&t.kind, TokenKind::Str(s) if s == "hello world")),);
    }

    #[test]
    fn float_literal() {
        let src = "x = 3.14";
        let toks = Lexer::new(src).tokenize();
        assert!(toks
            .iter()
            .any(|t| matches!(&t.kind, TokenKind::Float(v) if (*v - 3.14).abs() < 1e-6)),);
    }

    #[test]
    fn bool_literals() {
        let src = "a = true\nb = false";
        let toks = Lexer::new(src).tokenize();
        assert!(toks.iter().any(|t| t.kind == TokenKind::Bool(true)));
        assert!(toks.iter().any(|t| t.kind == TokenKind::Bool(false)));
    }
}
