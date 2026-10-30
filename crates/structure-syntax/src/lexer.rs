//! Lexical analysis (2.1, 2.2, B.1.1, B.1.2, B.1.4.1).
//!
//! The whole file is tokenised up front. Lexical problems are attached to
//! [`TokenKind::Error`] tokens and are only reported when the parser consumes
//! them as part of a textual language element; this lets semigraphic (LD,
//! FBD and SFC) bodies, which use characters with no textual meaning, pass
//! through the lexer unharmed. Problems with comments are always reported.

use crate::diag::Diagnostic;
use crate::source::{FileId, Span};
use crate::token::{Keyword, Token, TokenKind};

pub struct LexOutput {
    pub tokens: Vec<Token>,
    pub diagnostics: Vec<Diagnostic>,
    /// Pragmas (2.1.6): span of the whole `{...}` and its contents.
    pub pragmas: Vec<(Span, String)>,
}

pub fn lex(file: FileId, text: &str) -> LexOutput {
    let mut lx = Lexer::new(file, text);
    lx.run();
    LexOutput { tokens: lx.tokens, diagnostics: lx.diags, pragmas: lx.pragmas }
}

struct Lexer<'a> {
    file: FileId,
    text: &'a str,
    chars: Vec<char>,
    offs: Vec<u32>,
    pos: usize,
    tokens: Vec<Token>,
    diags: Vec<Diagnostic>,
    pragmas: Vec<(Span, String)>,
    nl: bool,
    ws: bool,
}

fn is_eol(c: char) -> bool {
    c == '\n' || c == '\r' || c == '\u{2029}'
}

fn is_ident_start(c: char) -> bool {
    c.is_ascii_alphabetic() || c == '_' || (!c.is_ascii() && c.is_alphabetic())
}

fn is_ident_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_' || (!c.is_ascii() && c.is_alphanumeric())
}

/// '#' or its alternative representation (table 1, feature 3b).
fn is_hash(c: char) -> bool {
    c == '#' || c == '£'
}

impl<'a> Lexer<'a> {
    fn new(file: FileId, text: &'a str) -> Self {
        let mut chars = Vec::with_capacity(text.len());
        let mut offs = Vec::with_capacity(text.len() + 1);
        for (i, c) in text.char_indices() {
            chars.push(c);
            offs.push(i as u32);
        }
        offs.push(text.len() as u32);
        Lexer { file, text, chars, offs, pos: 0, tokens: Vec::new(), diags: Vec::new(), pragmas: Vec::new(), nl: true, ws: true }
    }

    fn peek(&self, n: usize) -> char {
        self.chars.get(self.pos + n).copied().unwrap_or('\0')
    }

    fn at_end(&self) -> bool {
        self.pos >= self.chars.len()
    }

    fn span(&self, start: usize, end: usize) -> Span {
        Span::new(self.file, self.offs[start], self.offs[end])
    }

    fn push(&mut self, kind: TokenKind, start: usize) {
        let span = self.span(start, self.pos);
        self.tokens.push(Token { kind, span, nl_before: self.nl, ws_before: self.ws, error: None });
        self.nl = false;
        self.ws = false;
    }

    fn push_err(&mut self, start: usize, msg: impl Into<String>) {
        let span = self.span(start, self.pos);
        self.tokens.push(Token { kind: TokenKind::Error, span, nl_before: self.nl, ws_before: self.ws, error: Some(msg.into()) });
        self.nl = false;
        self.ws = false;
    }

    fn run(&mut self) {
        while !self.at_end() {
            let c = self.peek(0);
            if is_eol(c) {
                self.nl = true;
                self.ws = true;
                self.pos += 1;
                continue;
            }
            if c.is_whitespace() || c == '\u{feff}' {
                self.ws = true;
                self.pos += 1;
                continue;
            }
            if c == '(' && self.peek(1) == '*' {
                self.comment();
                continue;
            }
            if c == '{' {
                self.pragma();
                continue;
            }
            self.token();
        }
        let end = self.chars.len();
        let span = self.span(end, end);
        self.tokens.push(Token { kind: TokenKind::Eof, span, nl_before: true, ws_before: true, error: None });
    }

    /// Comments (2.1.5). Nested comments are an error (table E.1); nesting is
    /// tracked so that recovery after the error is sensible.
    fn comment(&mut self) {
        let start = self.pos;
        self.pos += 2;
        let mut depth = 1;
        while !self.at_end() {
            let c = self.peek(0);
            if c == '(' && self.peek(1) == '*' {
                let s = self.pos;
                self.pos += 2;
                depth += 1;
                self.diags.push(Diagnostic::annex_e("2.1.5", "nested comments are not allowed", self.span(s, self.pos)));
                continue;
            }
            if c == '*' && self.peek(1) == ')' {
                self.pos += 2;
                depth -= 1;
                if depth == 0 {
                    break;
                }
                continue;
            }
            if is_eol(c) {
                self.nl = true;
            }
            self.pos += 1;
        }
        if depth > 0 {
            self.diags.push(Diagnostic::syntax("unterminated comment", self.span(start, start + 2)));
        }
        self.ws = true;
    }

    /// Pragmas (2.1.6): `{ ... }`. Their syntax and semantics are
    /// implementation dependent; they are collected and otherwise ignored.
    fn pragma(&mut self) {
        let start = self.pos;
        self.pos += 1;
        while !self.at_end() && self.peek(0) != '}' {
            if is_eol(self.peek(0)) {
                self.nl = true;
            }
            self.pos += 1;
        }
        if self.at_end() {
            self.diags.push(Diagnostic::syntax("unterminated pragma", self.span(start, start + 1)));
        } else {
            self.pos += 1;
        }
        let span = self.span(start, self.pos);
        let inner_end = if self.pos > start + 1 && self.chars[self.pos - 1] == '}' { self.pos - 1 } else { self.pos };
        let content = self.text[self.offs[start + 1] as usize..self.offs[inner_end] as usize].to_string();
        self.pragmas.push((span, content));
        self.ws = true;
    }

    fn token(&mut self) {
        let start = self.pos;
        let c = self.peek(0);
        if is_ident_start(c) {
            self.ident_or_keyword();
            return;
        }
        if c.is_ascii_digit() {
            self.number();
            return;
        }
        match c {
            '\'' => {
                self.string('\'');
                return;
            }
            '"' => {
                self.string('"');
                return;
            }
            '%' => {
                self.direct_var();
                return;
            }
            _ => {}
        }
        let (kind, len) = match (c, self.peek(1)) {
            (':', '=') => (TokenKind::Assign, 2),
            ('=', '>') => (TokenKind::Arrow, 2),
            ('*', '*') => (TokenKind::Power, 2),
            ('<', '=') => (TokenKind::Le, 2),
            ('>', '=') => (TokenKind::Ge, 2),
            ('<', '>') => (TokenKind::Ne, 2),
            ('.', '.') => (TokenKind::DotDot, 2),
            (':', _) => (TokenKind::Colon, 1),
            (';', _) => (TokenKind::Semi, 1),
            (',', _) => (TokenKind::Comma, 1),
            ('.', _) => (TokenKind::Dot, 1),
            ('(', _) => (TokenKind::LParen, 1),
            (')', _) => (TokenKind::RParen, 1),
            ('[', _) => (TokenKind::LBracket, 1),
            (']', _) => (TokenKind::RBracket, 1),
            ('+', _) => (TokenKind::Plus, 1),
            ('-', _) => (TokenKind::Minus, 1),
            ('*', _) => (TokenKind::Star, 1),
            ('/', _) => (TokenKind::Slash, 1),
            ('&', _) => (TokenKind::Amp, 1),
            ('=', _) => (TokenKind::Eq, 1),
            ('<', _) => (TokenKind::Lt, 1),
            ('>', _) => (TokenKind::Gt, 1),
            ('|', _) => (TokenKind::Pipe, 1),
            ('!', _) => (TokenKind::Bang, 1),
            ('#', _) | ('£', _) => (TokenKind::Hash, 1),
            _ => {
                self.pos += 1;
                self.push_err(start, format!("unexpected character '{}'", c));
                return;
            }
        };
        self.pos += len;
        self.push(kind, start);
    }

    fn ident_or_keyword(&mut self) {
        let start = self.pos;
        while is_ident_char(self.peek(0)) {
            self.pos += 1;
        }
        let word: String = self.chars[start..self.pos].iter().collect();
        let upper = word.to_uppercase();
        if is_hash(self.peek(0)) {
            match upper.as_str() {
                "T" | "TIME" => {
                    self.pos += 1;
                    self.duration_body();
                    self.push(TokenKind::Duration, start);
                    return;
                }
                "TOD" | "TIME_OF_DAY" => {
                    self.pos += 1;
                    self.scan_time_of_day();
                    self.push(TokenKind::TimeOfDay, start);
                    return;
                }
                "D" | "DATE" => {
                    self.pos += 1;
                    self.scan_date();
                    self.push(TokenKind::Date, start);
                    return;
                }
                "DT" | "DATE_AND_TIME" => {
                    self.pos += 1;
                    self.scan_date();
                    if self.peek(0) == '-' {
                        self.pos += 1;
                    }
                    self.scan_time_of_day();
                    self.push(TokenKind::DateAndTime, start);
                    return;
                }
                _ => {
                    self.pos += 1;
                    self.push(TokenKind::TypedPrefix, start);
                    return;
                }
            }
        }
        match Keyword::from_upper(&upper) {
            Some(kw) => self.push(TokenKind::Kw(kw), start),
            None => self.push(TokenKind::Ident, start),
        }
    }

    fn scan_digits(&mut self) {
        while self.peek(0).is_ascii_digit() || self.peek(0) == '_' {
            self.pos += 1;
        }
    }

    fn duration_body(&mut self) {
        if self.peek(0) == '-' {
            self.pos += 1;
        }
        loop {
            if !self.peek(0).is_ascii_digit() {
                break;
            }
            self.scan_digits();
            if self.peek(0) == '.' && self.peek(1).is_ascii_digit() {
                self.pos += 1;
                self.scan_digits();
            }
            // unit
            let c0 = self.peek(0).to_ascii_lowercase();
            let c1 = self.peek(1).to_ascii_lowercase();
            if c0 == 'm' && c1 == 's' {
                self.pos += 2;
            } else if matches!(c0, 'd' | 'h' | 'm' | 's') {
                self.pos += 1;
            } else {
                break;
            }
            if self.peek(0) == '_' && self.peek(1).is_ascii_digit() {
                self.pos += 1;
            }
        }
    }

    fn scan_time_of_day(&mut self) {
        self.scan_digits();
        if self.peek(0) == ':' {
            self.pos += 1;
            self.scan_digits();
            if self.peek(0) == ':' {
                self.pos += 1;
                self.scan_digits();
                if self.peek(0) == '.' && self.peek(1).is_ascii_digit() {
                    self.pos += 1;
                    self.scan_digits();
                }
            }
        }
    }

    fn scan_date(&mut self) {
        self.scan_digits();
        for _ in 0..2 {
            if self.peek(0) == '-' && self.peek(1).is_ascii_digit() {
                self.pos += 1;
                self.scan_digits();
            }
        }
    }

    fn number(&mut self) {
        let start = self.pos;
        self.scan_digits();
        if is_hash(self.peek(0)) {
            // based integer
            self.pos += 1;
            let ds = self.pos;
            while self.peek(0).is_ascii_hexdigit() || self.peek(0) == '_' {
                self.pos += 1;
            }
            if self.pos == ds {
                self.push_err(start, "missing digits in based integer literal");
            } else {
                self.push(TokenKind::BasedInt, start);
            }
            return;
        }
        let mut real = false;
        if self.peek(0) == '.' && self.peek(1).is_ascii_digit() {
            real = true;
            self.pos += 1;
            self.scan_digits();
        }
        if matches!(self.peek(0), 'e' | 'E') {
            let sign = matches!(self.peek(1), '+' | '-');
            let d = if sign { self.peek(2) } else { self.peek(1) };
            if d.is_ascii_digit() {
                self.pos += if sign { 2 } else { 1 };
                self.scan_digits();
                if !real {
                    self.push_err(start, "a real literal shall contain a decimal point (B.1.2.1)");
                    return;
                }
                real = true;
            }
        }
        self.push(if real { TokenKind::Real } else { TokenKind::Int }, start);
    }

    fn string(&mut self, quote: char) {
        let start = self.pos;
        self.pos += 1;
        loop {
            if self.at_end() || is_eol(self.peek(0)) {
                self.push_err(start, "unterminated character string literal");
                return;
            }
            let c = self.peek(0);
            if c == '$' || c == '¤' {
                self.pos += 1;
                if !self.at_end() && !is_eol(self.peek(0)) {
                    self.pos += 1;
                }
                continue;
            }
            self.pos += 1;
            if c == quote {
                break;
            }
        }
        self.push(if quote == '\'' { TokenKind::SString } else { TokenKind::DString }, start);
    }

    fn direct_var(&mut self) {
        let start = self.pos;
        self.pos += 1;
        let loc = self.peek(0).to_ascii_uppercase();
        if !matches!(loc, 'I' | 'Q' | 'M') {
            self.push_err(start, "a directly represented variable needs a location prefix I, Q or M");
            return;
        }
        self.pos += 1;
        if self.peek(0) == '*' {
            self.pos += 1;
            self.push(TokenKind::DirectVar, start);
            return;
        }
        let size = self.peek(0).to_ascii_uppercase();
        if matches!(size, 'X' | 'B' | 'W' | 'D' | 'L') {
            self.pos += 1;
        }
        if self.peek(0) == '*' {
            self.pos += 1;
            self.push(TokenKind::DirectVar, start);
            return;
        }
        if !self.peek(0).is_ascii_digit() {
            self.push_err(start, "incomplete directly represented variable");
            return;
        }
        while self.peek(0).is_ascii_digit() {
            self.pos += 1;
        }
        while self.peek(0) == '.' && self.peek(1).is_ascii_digit() {
            self.pos += 1;
            while self.peek(0).is_ascii_digit() {
                self.pos += 1;
            }
        }
        self.push(TokenKind::DirectVar, start);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kinds(s: &str) -> Vec<TokenKind> {
        lex(FileId(0), s).tokens.iter().map(|t| t.kind).collect()
    }

    #[test]
    fn literals() {
        use TokenKind::*;
        assert_eq!(kinds("16#FF 2#1111_0000 1.0E+6 12"), vec![BasedInt, BasedInt, Real, Int, Eof]);
        assert_eq!(kinds("T#14ms t#5d14h12m18s3.5ms TIME#-14ms"), vec![Duration, Duration, Duration, Eof]);
        assert_eq!(kinds("DT#1984-06-25-15:36:55.36 TOD#15:36:55 D#1984-06-25"), vec![DateAndTime, TimeOfDay, Date, Eof]);
        assert_eq!(kinds("INT#-5 SPEED#SLOW"), vec![TypedPrefix, Minus, Int, TypedPrefix, Ident, Eof]);
        assert_eq!(kinds("1..16"), vec![Int, DotDot, Int, Eof]);
        assert_eq!(kinds("%IX1.2 %QW5 %Q* %IW2.5.7.1"), vec![DirectVar, DirectVar, DirectVar, DirectVar, Eof]);
        assert_eq!(kinds("'$'' \"$\"\""), vec![SString, DString, Eof]);
    }

    #[test]
    fn nested_comment_is_error() {
        let out = lex(FileId(0), "(* (* NESTED *) *) x");
        assert_eq!(out.diagnostics.len(), 1);
        assert_eq!(out.tokens.len(), 2);
    }
}
