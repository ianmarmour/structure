//! Recursive-descent parser for the textual elements of annex B.

mod config;
mod decl;
mod il;
mod sfc;
mod st;

use crate::ast::*;
use crate::diag::Diagnostic;
use crate::ident::{check_identifier, Ident};
use crate::lexer::lex;
use crate::source::{SourceFile, Span};
use crate::token::{Keyword, Token, TokenKind};

pub struct ParseResult {
    pub unit: SourceUnit,
    pub diagnostics: Vec<Diagnostic>,
    pub pragmas: Vec<(Span, String)>,
}

/// Parses a complete source file.
pub fn parse(file: &SourceFile) -> ParseResult {
    let lexed = lex(file.id, &file.text);
    let mut p = Parser::new(file, lexed.tokens);
    p.diags.extend(lexed.diagnostics);
    let unit = p.source_unit();
    ParseResult { unit, diagnostics: p.diags, pragmas: lexed.pragmas }
}

/// Parses a single ST expression (used by tools and tests).
pub fn parse_expression(file: &SourceFile) -> (Option<Expr>, Vec<Diagnostic>) {
    let lexed = lex(file.id, &file.text);
    let mut p = Parser::new(file, lexed.tokens);
    p.diags.extend(lexed.diagnostics);
    let e = p.expr();
    if !p.at(TokenKind::Eof) {
        p.error_here("unexpected token after expression");
    }
    (Some(e), p.diags)
}

/// Parses an ST expression contained in `text`, which starts at byte
/// `offset` of `file` (used for transition conditions in graphical SFC).
pub fn parse_expression_at(file: &SourceFile, start: u32, end: u32) -> (Expr, Vec<Diagnostic>) {
    let lexed = lex(file.id, &file.text);
    let toks: Vec<Token> = lexed
        .tokens
        .into_iter()
        .filter(|t| (t.span.start >= start && t.span.end <= end) || t.kind == TokenKind::Eof)
        .collect();
    let mut p = Parser::new(file, toks);
    let e = p.expr();
    if !p.at(TokenKind::Eof) {
        p.error_here("unexpected token after transition condition");
    }
    (e, p.diags)
}

/// Parses ST statements contained in the byte range `start..end` of `file`.
pub fn parse_statements_at(file: &SourceFile, start: u32, end: u32) -> (Vec<Stmt>, Vec<Diagnostic>) {
    let lexed = lex(file.id, &file.text);
    let toks: Vec<Token> = lexed
        .tokens
        .into_iter()
        .filter(|t| (t.span.start >= start && t.span.end <= end) || t.kind == TokenKind::Eof)
        .collect();
    let mut p = Parser::new(file, toks);
    let stmts = p.stmt_list(&[]);
    (stmts, p.diags)
}

/// Parses a body (any language) contained in the byte range `start..end`.
pub fn parse_body_at(file: &SourceFile, start: u32, end: u32) -> (Body, Vec<Diagnostic>) {
    let lexed = lex(file.id, &file.text);
    let toks: Vec<Token> = lexed
        .tokens
        .into_iter()
        .filter(|t| (t.span.start >= start && t.span.end <= end) || t.kind == TokenKind::Eof)
        .collect();
    let mut p = Parser::new(file, toks);
    let n = p.toks.len() - 1;
    let body = p.body_region(0, n, start, end);
    (body, p.diags)
}

pub(crate) struct Parser<'a> {
    pub(crate) file: &'a SourceFile,
    pub(crate) toks: Vec<Token>,
    pub(crate) pos: usize,
    /// Tokens at or beyond this index are treated as end of input.
    pub(crate) limit: usize,
    pub(crate) diags: Vec<Diagnostic>,
    eof: Token,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum BodyLang {
    Empty,
    St,
    Il,
    SfcText,
    Graphic,
}

impl<'a> Parser<'a> {
    pub(crate) fn new(file: &'a SourceFile, toks: Vec<Token>) -> Self {
        let limit = toks.len().saturating_sub(1);
        let eof = toks.last().cloned().unwrap_or(Token {
            kind: TokenKind::Eof,
            span: Span::new(file.id, file.text.len() as u32, file.text.len() as u32),
            nl_before: true,
            ws_before: true,
            error: None,
        });
        Parser { file, toks, pos: 0, limit, diags: Vec::new(), eof }
    }

    // ------------------------------------------------------------ token access

    pub(crate) fn tok(&self, n: usize) -> &Token {
        let i = self.pos + n;
        if i >= self.limit {
            if i == self.limit && self.limit < self.toks.len() {
                // Synthesise an EOF at the position of the limiting token.
                return &self.eof;
            }
            return &self.eof;
        }
        &self.toks[i]
    }

    pub(crate) fn kind(&self, n: usize) -> TokenKind {
        if self.pos + n >= self.limit {
            TokenKind::Eof
        } else {
            self.toks[self.pos + n].kind
        }
    }

    pub(crate) fn at(&self, k: TokenKind) -> bool {
        self.kind(0) == k
    }

    pub(crate) fn at_kw(&self, kw: Keyword) -> bool {
        self.kind(0) == TokenKind::Kw(kw)
    }

    pub(crate) fn at_eof(&self) -> bool {
        self.kind(0) == TokenKind::Eof
    }

    pub(crate) fn text(&self, t: &Token) -> &'a str {
        &self.file.text[t.span.start as usize..t.span.end as usize]
    }

    pub(crate) fn cur_text(&self) -> &'a str {
        if self.at_eof() {
            ""
        } else {
            self.text(&self.toks[self.pos])
        }
    }

    pub(crate) fn cur_upper(&self) -> String {
        self.cur_text().to_uppercase()
    }

    pub(crate) fn span(&self) -> Span {
        if self.pos < self.limit {
            self.toks[self.pos].span
        } else if self.limit < self.toks.len() {
            let s = self.toks[self.limit].span.start;
            Span::new(self.file.id, s, s)
        } else {
            self.eof.span
        }
    }

    /// Span of the most recently consumed token.
    pub(crate) fn prev_span(&self) -> Span {
        if self.pos == 0 {
            self.span()
        } else {
            self.toks[(self.pos - 1).min(self.toks.len() - 1)].span
        }
    }

    pub(crate) fn bump(&mut self) -> Token {
        let t = self.tok(0).clone();
        if t.kind == TokenKind::Error {
            if let Some(msg) = &t.error {
                self.diags.push(Diagnostic::syntax(msg.clone(), t.span));
            }
        }
        if self.pos < self.limit {
            self.pos += 1;
        }
        t
    }

    pub(crate) fn eat(&mut self, k: TokenKind) -> bool {
        if self.at(k) {
            self.bump();
            true
        } else {
            false
        }
    }

    pub(crate) fn eat_kw(&mut self, kw: Keyword) -> bool {
        self.eat(TokenKind::Kw(kw))
    }

    pub(crate) fn error_here(&mut self, msg: impl Into<String>) {
        let span = self.span();
        if self.kind(0) == TokenKind::Error {
            if let Some(m) = self.tok(0).error.clone() {
                self.diags.push(Diagnostic::syntax(m, span));
                return;
            }
        }
        self.diags.push(Diagnostic::syntax(msg, span));
    }

    pub(crate) fn expected(&mut self, what: &str) {
        let found = if self.at_eof() { "end of input".to_string() } else { format!("'{}'", self.cur_text()) };
        self.error_here(format!("expected {}, found {}", what, found));
    }

    pub(crate) fn expect(&mut self, k: TokenKind) -> bool {
        if self.eat(k) {
            true
        } else {
            self.expected(k.describe());
            false
        }
    }

    pub(crate) fn expect_kw(&mut self, kw: Keyword) -> bool {
        if self.eat_kw(kw) {
            true
        } else {
            self.expected(kw.as_str());
            false
        }
    }

    /// Accepts an identifier token, checking the rules of 2.1.2.
    pub(crate) fn ident(&mut self) -> Option<Ident> {
        if self.at(TokenKind::Ident) {
            let t = self.bump();
            let text = self.text(&t).to_string();
            if let Some(problem) = check_identifier(&text) {
                self.diags.push(Diagnostic::syntax(format!("invalid identifier '{}': {}", text, problem), t.span).with_clause("2.1.2"));
            }
            if text.chars().any(|c| !c.is_ascii()) {
                self.diags.push(
                    Diagnostic::extension(format!("identifier '{}' uses characters outside the ISO 646 IRV character set", text), t.span)
                        .with_clause("2.1.1"),
                );
            }
            Some(Ident::new(text, t.span))
        } else {
            None
        }
    }

    pub(crate) fn expect_ident(&mut self, what: &str) -> Option<Ident> {
        match self.ident() {
            Some(i) => Some(i),
            None => {
                if let TokenKind::Kw(k) = self.kind(0) {
                    self.error_here(format!("keyword {} cannot be used as {} (2.1.3)", k.as_str(), what));
                    let t = self.bump();
                    return Some(Ident::new(self.text(&t).to_string(), t.span));
                }
                self.expected(what);
                None
            }
        }
    }

    /// Skips tokens until one of `stops` (not consumed) or a semicolon
    /// (consumed) is found.
    pub(crate) fn recover(&mut self, stops: &[TokenKind]) {
        while !self.at_eof() {
            let k = self.kind(0);
            if stops.contains(&k) {
                return;
            }
            if k == TokenKind::Semi {
                self.bump();
                return;
            }
            self.pos += 1;
        }
    }

    // ------------------------------------------------------------ top level

    fn source_unit(&mut self) -> SourceUnit {
        let mut unit = SourceUnit::default();
        while !self.at_eof() {
            let start = self.pos;
            match self.kind(0) {
                TokenKind::Kw(Keyword::Type) => {
                    let d = self.data_type_decl();
                    unit.elements.push(LibraryElement::DataTypes(d));
                }
                TokenKind::Kw(Keyword::Function) => {
                    let p = self.pou(Keyword::Function);
                    unit.elements.push(LibraryElement::Pou(p));
                }
                TokenKind::Kw(Keyword::FunctionBlock) => {
                    let p = self.pou(Keyword::FunctionBlock);
                    unit.elements.push(LibraryElement::Pou(p));
                }
                TokenKind::Kw(Keyword::Program) => {
                    let p = self.pou(Keyword::Program);
                    unit.elements.push(LibraryElement::Pou(p));
                }
                TokenKind::Kw(Keyword::Configuration) => {
                    let c = self.configuration();
                    unit.elements.push(LibraryElement::Configuration(c));
                }
                _ => {
                    self.expected("TYPE, FUNCTION, FUNCTION_BLOCK, PROGRAM or CONFIGURATION");
                    self.bump();
                    while !self.at_eof() && !self.at_library_start() {
                        self.pos += 1;
                    }
                }
            }
            if self.pos == start {
                self.bump();
            }
        }
        unit
    }

    pub(crate) fn at_library_start(&self) -> bool {
        matches!(
            self.kind(0),
            TokenKind::Kw(Keyword::Type)
                | TokenKind::Kw(Keyword::Function)
                | TokenKind::Kw(Keyword::FunctionBlock)
                | TokenKind::Kw(Keyword::Program)
                | TokenKind::Kw(Keyword::Configuration)
        )
    }

    fn pou(&mut self, kw: Keyword) -> Pou {
        let start = self.span();
        self.bump();
        let name = self.expect_ident("a name").unwrap_or_else(|| Ident::new("?", start));
        let (kind, end_kw) = match kw {
            Keyword::Function => {
                let result = if self.expect(TokenKind::Colon) {
                    self.type_spec()
                } else {
                    TypeSpec::Named(Ident::new("?", self.span()))
                };
                (PouKind::Function { result }, Keyword::EndFunction)
            }
            Keyword::FunctionBlock => (PouKind::FunctionBlock, Keyword::EndFunctionBlock),
            _ => (PouKind::Program, Keyword::EndProgram),
        };
        let var_blocks = self.var_blocks();
        let body = self.body(end_kw);
        let end = self.span();
        if !self.eat_kw(end_kw) {
            self.expected(end_kw.as_str());
        }
        Pou { kind, name, var_blocks, body, span: start.to(end) }
    }

    pub(crate) fn var_blocks(&mut self) -> Vec<VarBlock> {
        let mut blocks = Vec::new();
        while let TokenKind::Kw(k) = self.kind(0) {
            if !matches!(
                k,
                Keyword::Var
                    | Keyword::VarInput
                    | Keyword::VarOutput
                    | Keyword::VarInOut
                    | Keyword::VarExternal
                    | Keyword::VarGlobal
                    | Keyword::VarTemp
                    | Keyword::VarAccess
                    | Keyword::VarConfig
            ) {
                break;
            }
            blocks.push(self.var_block());
        }
        blocks
    }

    // ------------------------------------------------------------ bodies

    /// Index of the token terminating a body that ends with `end_kw`.
    fn find_end(&self, end_kw: Keyword) -> usize {
        let mut i = self.pos;
        while i < self.limit {
            let k = self.toks[i].kind;
            if k == TokenKind::Kw(end_kw) {
                return i;
            }
            if self.toks[i].nl_before
                && matches!(
                    k,
                    TokenKind::Kw(Keyword::Type)
                        | TokenKind::Kw(Keyword::Function)
                        | TokenKind::Kw(Keyword::FunctionBlock)
                        | TokenKind::Kw(Keyword::Program)
                        | TokenKind::Kw(Keyword::Configuration)
                )
                && end_kw != Keyword::EndConfiguration
            {
                return i;
            }
            i += 1;
        }
        self.limit
    }

    /// Parses the body of a POU, action or transition up to (not including)
    /// the terminating keyword.
    pub(crate) fn body(&mut self, end_kw: Keyword) -> Body {
        let end = self.find_end(end_kw);
        let start_off = self.span().start;
        let end_off = if end < self.toks.len() { self.toks[end].span.start } else { self.file.text.len() as u32 };
        let start = self.pos;
        self.body_region(start, end, start_off, end_off)
    }

    pub(crate) fn body_region(&mut self, start: usize, end: usize, start_off: u32, end_off: u32) -> Body {
        let lang = self.classify(start, end);
        let saved_limit = self.limit;
        self.limit = end;
        self.pos = start;
        let body = match lang {
            BodyLang::Empty => Body::Empty,
            BodyLang::St => {
                let stmts = self.stmt_list(&[]);
                Body::St(stmts)
            }
            BodyLang::Il => {
                let instrs = self.il_list();
                Body::Il(instrs)
            }
            BodyLang::SfcText => Body::Sfc(self.sfc_text()),
            BodyLang::Graphic => {
                let first = if start < end { self.toks[start].span.start } else { start_off };
                let g = self.graphic_region(first, end_off);
                self.pos = end;
                Body::Graphic(g)
            }
        };
        if self.pos < end {
            self.error_here("unexpected text in body");
        }
        self.limit = saved_limit;
        self.pos = end;
        body
    }

    pub(crate) fn classify(&self, start: usize, end: usize) -> BodyLang {
        if start >= end {
            return BodyLang::Empty;
        }
        if matches!(
            self.toks[start].kind,
            TokenKind::Kw(Keyword::InitialStep) | TokenKind::Kw(Keyword::Step) | TokenKind::Kw(Keyword::Transition) | TokenKind::Kw(Keyword::Action)
        ) {
            return BodyLang::SfcText;
        }
        let toks = &self.toks[start..end];
        if toks.iter().any(|t| matches!(t.kind, TokenKind::Pipe | TokenKind::Bang)) {
            return BodyLang::Graphic;
        }
        if toks.iter().any(|t| t.kind == TokenKind::Semi) {
            return BodyLang::St;
        }
        let adjacent_graphic = toks.windows(2).any(|w| {
            !w[1].ws_before
                && matches!(
                    (w[0].kind, w[1].kind),
                    (TokenKind::Minus, TokenKind::Minus)
                        | (TokenKind::Plus, TokenKind::Minus)
                        | (TokenKind::Minus, TokenKind::Plus)
                        | (TokenKind::Eq, TokenKind::Eq)
                        | (TokenKind::Minus, TokenKind::Gt)
                        | (TokenKind::Gt, TokenKind::Gt)
                        | (TokenKind::Minus, TokenKind::Lt)
                )
        });
        if adjacent_graphic {
            return BodyLang::Graphic;
        }
        BodyLang::Il
    }

    /// Extracts the semigraphic text between two byte offsets (keeping
    /// column positions) and hands it to the graphical parser.
    pub(crate) fn graphic_region(&mut self, start_off: u32, end_off: u32) -> GraphicBody {
        let (line, _) = self.file.line_col(start_off);
        let line_start = self.file.line_start(line as usize - 1);
        let mut text = String::new();
        // Blank out anything on the first line that precedes the body.
        for c in self.file.text[line_start as usize..start_off as usize].chars() {
            text.push(if c == '\t' { '\t' } else { ' ' });
        }
        text.push_str(&self.file.text[start_off as usize..end_off as usize]);
        let (body, diags) = crate::graphic::parse_graphic(self.file, line_start, &text);
        self.diags.extend(diags);
        body
    }
}
