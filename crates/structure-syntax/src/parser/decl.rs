//! Data type and variable declarations (B.1.3, B.1.4.3).

use super::Parser;
use crate::ast::*;
use crate::diag::Diagnostic;
use crate::ident::Ident;
use crate::token::{Keyword, TokenKind};

impl<'a> Parser<'a> {
    /// `TYPE type_declaration ';' {type_declaration ';'} END_TYPE`
    pub(crate) fn data_type_decl(&mut self) -> DataTypeDecl {
        let start = self.span();
        self.bump();
        let mut types = Vec::new();
        while !self.at_kw(Keyword::EndType) && !self.at_eof() && !self.at_library_start() {
            let s = self.span();
            let Some(name) = self.expect_ident("a type name") else {
                self.recover(&[TokenKind::Kw(Keyword::EndType)]);
                continue;
            };
            if !self.expect(TokenKind::Colon) {
                self.recover(&[TokenKind::Kw(Keyword::EndType)]);
                continue;
            }
            let spec = self.type_spec();
            let init = if self.eat(TokenKind::Assign) { Some(self.initializer()) } else { None };
            let end = self.prev_span();
            if !self.expect(TokenKind::Semi) {
                self.recover(&[TokenKind::Kw(Keyword::EndType)]);
            }
            types.push(TypeDecl { name, spec, init, span: s.to(end) });
        }
        if types.is_empty() {
            self.diags.push(Diagnostic::syntax("a TYPE declaration shall declare at least one type", start));
        }
        let end = self.span();
        self.expect_kw(Keyword::EndType);
        DataTypeDecl { types, span: start.to(end) }
    }

    /// Parses a type specification (without an initial value).
    pub(crate) fn type_spec(&mut self) -> TypeSpec {
        let start = self.span();
        match self.kind(0) {
            TokenKind::Kw(Keyword::Array) => {
                self.bump();
                let close = if self.eat(TokenKind::LBracket) {
                    TokenKind::RBracket
                } else if self.at(TokenKind::LParen) {
                    let s = self.span();
                    self.bump();
                    self.diags.push(
                        Diagnostic::extension("array bounds should be enclosed in '[' and ']'", s).with_clause("B.1.3.3"),
                    );
                    TokenKind::RParen
                } else {
                    self.expected("'['");
                    TokenKind::RBracket
                };
                let mut dims = Vec::new();
                loop {
                    let lo = self.expr();
                    self.expect(TokenKind::DotDot);
                    let hi = self.expr();
                    dims.push((lo, hi));
                    if !self.eat(TokenKind::Comma) {
                        break;
                    }
                }
                self.expect(close);
                self.expect_kw(Keyword::Of);
                let elem = self.type_spec();
                let end = self.prev_span();
                TypeSpec::Array { dims, elem: Box::new(elem), span: start.to(end) }
            }
            TokenKind::Kw(Keyword::Struct) => {
                self.bump();
                let mut fields = Vec::new();
                while !self.at_kw(Keyword::EndStruct) && !self.at_eof() {
                    let s = self.span();
                    let Some(name) = self.expect_ident("a structure element name") else {
                        self.recover(&[TokenKind::Kw(Keyword::EndStruct)]);
                        continue;
                    };
                    if !self.expect(TokenKind::Colon) {
                        self.recover(&[TokenKind::Kw(Keyword::EndStruct)]);
                        continue;
                    }
                    let spec = self.type_spec();
                    let init = if self.eat(TokenKind::Assign) { Some(self.initializer()) } else { None };
                    let end = self.prev_span();
                    if !self.expect(TokenKind::Semi) {
                        self.recover(&[TokenKind::Kw(Keyword::EndStruct)]);
                    }
                    fields.push(StructField { name, spec, init, span: s.to(end) });
                }
                let end = self.span();
                self.expect_kw(Keyword::EndStruct);
                TypeSpec::Struct { fields, span: start.to(end) }
            }
            TokenKind::LParen => {
                self.bump();
                let mut values = Vec::new();
                loop {
                    if self.at(TokenKind::TypedPrefix) {
                        let t = self.bump();
                        self.diags.push(
                            Diagnostic::syntax("a type prefix shall not be used inside an enumeration list", t.span).with_clause("2.3.3.1"),
                        );
                    }
                    match self.expect_ident("an enumerated value") {
                        Some(i) => values.push(i),
                        None => break,
                    }
                    if !self.eat(TokenKind::Comma) {
                        break;
                    }
                }
                let end = self.span();
                self.expect(TokenKind::RParen);
                TypeSpec::Enum { values, span: start.to(end) }
            }
            TokenKind::Ident => {
                let name = self.ident().unwrap();
                let upper = name.key();
                if upper == "STRING" || upper == "WSTRING" {
                    let wide = upper == "WSTRING";
                    let mut len = None;
                    if self.eat(TokenKind::LBracket) {
                        len = Some(Box::new(self.expr()));
                        self.expect(TokenKind::RBracket);
                    } else if self.at(TokenKind::LParen) && !self.paren_starts_subrange() {
                        let s = self.span();
                        self.bump();
                        self.diags.push(
                            Diagnostic::extension("string length should be enclosed in '[' and ']'", s).with_clause("B.1.4.3"),
                        );
                        len = Some(Box::new(self.expr()));
                        self.expect(TokenKind::RParen);
                    }
                    let end = self.prev_span();
                    return TypeSpec::String { wide, len, span: start.to(end) };
                }
                if self.at(TokenKind::LParen) {
                    self.bump();
                    let lo = self.expr();
                    self.expect(TokenKind::DotDot);
                    let hi = self.expr();
                    let end = self.span();
                    self.expect(TokenKind::RParen);
                    return TypeSpec::Subrange { base: name, lo: Box::new(lo), hi: Box::new(hi), span: start.to(end) };
                }
                TypeSpec::Named(name)
            }
            _ => {
                self.expected("a data type");
                TypeSpec::Named(Ident::new("?", start))
            }
        }
    }

    fn paren_starts_subrange(&self) -> bool {
        // `STRING (...)` is never a subrange; kept for symmetry.
        false
    }

    /// Initial value: constant, enumerated value, array or structure
    /// initialisation.
    pub(crate) fn initializer(&mut self) -> Initializer {
        let start = self.span();
        if self.at(TokenKind::LBracket) {
            self.bump();
            let mut elems = Vec::new();
            if !self.at(TokenKind::RBracket) {
                loop {
                    elems.push(self.array_init_elem());
                    if !self.eat(TokenKind::Comma) {
                        break;
                    }
                }
            }
            let end = self.span();
            self.expect(TokenKind::RBracket);
            return Initializer::Array(elems, start.to(end));
        }
        if self.at(TokenKind::LParen) && self.kind(1) == TokenKind::Ident && self.kind(2) == TokenKind::Assign {
            return self.struct_init();
        }
        Initializer::Expr(self.expr())
    }

    pub(crate) fn struct_init(&mut self) -> Initializer {
        let start = self.span();
        self.expect(TokenKind::LParen);
        let mut elems = Vec::new();
        while let Some(name) = self.expect_ident("a structure element name") {
            self.expect(TokenKind::Assign);
            let v = self.initializer();
            elems.push((name, v));
            if !self.eat(TokenKind::Comma) {
                break;
            }
        }
        let end = self.span();
        self.expect(TokenKind::RParen);
        Initializer::Struct(elems, start.to(end))
    }

    fn array_init_elem(&mut self) -> ArrayInitElem {
        let start = self.span();
        if matches!(self.kind(0), TokenKind::Int) && self.kind(1) == TokenKind::LParen {
            let count = self.primary();
            self.bump(); // '('
            let value = if self.at(TokenKind::RParen) { None } else { Some(self.initializer()) };
            let end = self.span();
            self.expect(TokenKind::RParen);
            return ArrayInitElem::Repeat { count, value, span: start.to(end) };
        }
        ArrayInitElem::Single(self.initializer())
    }

    /// One `VAR_xxx ... END_VAR` block.
    pub(crate) fn var_block(&mut self) -> VarBlock {
        let start = self.span();
        let kind = match self.kind(0) {
            TokenKind::Kw(Keyword::Var) => VarKind::Var,
            TokenKind::Kw(Keyword::VarInput) => VarKind::Input,
            TokenKind::Kw(Keyword::VarOutput) => VarKind::Output,
            TokenKind::Kw(Keyword::VarInOut) => VarKind::InOut,
            TokenKind::Kw(Keyword::VarExternal) => VarKind::External,
            TokenKind::Kw(Keyword::VarGlobal) => VarKind::Global,
            TokenKind::Kw(Keyword::VarTemp) => VarKind::Temp,
            TokenKind::Kw(Keyword::VarAccess) => VarKind::Access,
            TokenKind::Kw(Keyword::VarConfig) => VarKind::Config,
            _ => unreachable!(),
        };
        self.bump();
        let mut qualifier = None;
        loop {
            let q = match self.kind(0) {
                TokenKind::Kw(Keyword::Constant) => VarQualifier::Constant,
                TokenKind::Kw(Keyword::Retain) => VarQualifier::Retain,
                TokenKind::Kw(Keyword::NonRetain) => VarQualifier::NonRetain,
                _ => break,
            };
            let s = self.span();
            self.bump();
            if qualifier.is_some() {
                self.diags.push(Diagnostic::syntax("at most one of CONSTANT, RETAIN and NON_RETAIN may be given", s).with_clause("2.4.3"));
            }
            qualifier = Some(q);
        }
        let items = match kind {
            VarKind::Access => VarItems::Access(self.access_decls()),
            VarKind::Config => VarItems::Config(self.config_inits()),
            _ => VarItems::Vars(self.var_decls()),
        };
        let end = self.span();
        self.expect_kw(Keyword::EndVar);
        VarBlock { kind, qualifier, items, span: start.to(end) }
    }

    fn at_block_end(&self) -> bool {
        self.at_kw(Keyword::EndVar) || self.at_eof() || self.at_library_start()
    }

    fn var_decls(&mut self) -> Vec<VarDecl> {
        let mut decls = Vec::new();
        while !self.at_block_end() {
            if matches!(
                self.kind(0),
                TokenKind::Kw(Keyword::Var)
                    | TokenKind::Kw(Keyword::VarInput)
                    | TokenKind::Kw(Keyword::VarOutput)
                    | TokenKind::Kw(Keyword::VarInOut)
                    | TokenKind::Kw(Keyword::VarTemp)
                    | TokenKind::Kw(Keyword::VarExternal)
            ) {
                // missing END_VAR
                break;
            }
            let before = self.pos;
            if let Some(d) = self.var_decl() {
                decls.push(d);
            }
            if self.pos == before {
                self.bump();
            }
        }
        decls
    }

    fn var_decl(&mut self) -> Option<VarDecl> {
        let start = self.span();
        let mut names = Vec::new();
        if self.at(TokenKind::Ident) {
            while let Some(i) = self.expect_ident("a variable name") {
                names.push(i);
                if !self.eat(TokenKind::Comma) {
                    break;
                }
            }
        } else if !self.at_kw(Keyword::At) {
            if let TokenKind::Kw(k) = self.kind(0) {
                self.error_here(format!("keyword {} cannot be used as a variable name (2.1.3)", k.as_str()));
                self.recover(&[TokenKind::Kw(Keyword::EndVar)]);
                return None;
            }
            self.expected("a variable declaration");
            self.recover(&[TokenKind::Kw(Keyword::EndVar)]);
            return None;
        }
        let mut location = None;
        if self.eat_kw(Keyword::At) {
            location = self.direct_var_token();
        }
        if !self.expect(TokenKind::Colon) {
            self.recover(&[TokenKind::Kw(Keyword::EndVar)]);
            return None;
        }
        let spec = self.type_spec();
        let edge = if self.eat_kw(Keyword::REdge) {
            Some(Edge::Rising)
        } else if self.eat_kw(Keyword::FEdge) {
            Some(Edge::Falling)
        } else {
            None
        };
        let init = if self.eat(TokenKind::Assign) { Some(self.initializer()) } else { None };
        let end = self.prev_span();
        if !self.expect(TokenKind::Semi) {
            self.recover(&[TokenKind::Kw(Keyword::EndVar)]);
        }
        Some(VarDecl { names, location, spec, edge, init, span: start.to(end) })
    }

    /// Parses a `%...` token into a [`DirectVar`].
    pub(crate) fn direct_var_token(&mut self) -> Option<DirectVar> {
        if !self.at(TokenKind::DirectVar) {
            self.expected("a directly represented variable");
            return None;
        }
        let t = self.bump();
        Some(parse_direct_var(self.text(&t), t.span))
    }

    fn access_decls(&mut self) -> Vec<AccessDecl> {
        let mut decls = Vec::new();
        while !self.at_block_end() {
            let start = self.span();
            let Some(name) = self.expect_ident("an access path name") else {
                self.recover(&[TokenKind::Kw(Keyword::EndVar)]);
                continue;
            };
            if !self.expect(TokenKind::Colon) {
                self.recover(&[TokenKind::Kw(Keyword::EndVar)]);
                continue;
            }
            let path = self.access_path();
            if !self.expect(TokenKind::Colon) {
                self.recover(&[TokenKind::Kw(Keyword::EndVar)]);
                continue;
            }
            let spec = self.type_spec();
            let direction = if self.eat_kw(Keyword::ReadWrite) {
                Some(Direction::ReadWrite)
            } else if self.eat_kw(Keyword::ReadOnly) {
                Some(Direction::ReadOnly)
            } else {
                None
            };
            let end = self.prev_span();
            if !self.expect(TokenKind::Semi) {
                self.recover(&[TokenKind::Kw(Keyword::EndVar)]);
            }
            decls.push(AccessDecl { name, path, spec, direction, span: start.to(end) });
        }
        decls
    }

    /// `[resource '.'] direct_variable | [resource '.'] [program '.'] {fb '.'} symbolic_variable`
    pub(crate) fn access_path(&mut self) -> AccessPath {
        let start = self.span();
        let mut prefix = Vec::new();
        if self.at(TokenKind::DirectVar) {
            let d = self.direct_var_token().unwrap();
            return AccessPath { prefix, target: AccessTarget::Direct(d), span: start.to(self.prev_span()) };
        }
        // Collect identifiers separated by dots; a trailing direct variable
        // or selectors finish the path.
        if self.at(TokenKind::Ident) && self.kind(1) == TokenKind::Dot && self.kind(2) == TokenKind::DirectVar {
            prefix.push(self.ident().unwrap());
            self.bump();
            let d = self.direct_var_token().unwrap();
            return AccessPath { prefix, target: AccessTarget::Direct(d), span: start.to(self.prev_span()) };
        }
        let e = self.variable_expr();
        AccessPath { prefix, target: AccessTarget::Symbolic(e), span: start.to(self.prev_span()) }
    }

    fn config_inits(&mut self) -> Vec<ConfigInit> {
        let mut inits = Vec::new();
        while !self.at_block_end() {
            let start = self.span();
            let mut path = Vec::new();
            while let Some(i) = self.expect_ident("an instance path") {
                path.push(i);
                if !self.eat(TokenKind::Dot) {
                    break;
                }
            }
            if path.is_empty() {
                self.recover(&[TokenKind::Kw(Keyword::EndVar)]);
                continue;
            }
            let location = if self.eat_kw(Keyword::At) { self.direct_var_token() } else { None };
            if !self.expect(TokenKind::Colon) {
                self.recover(&[TokenKind::Kw(Keyword::EndVar)]);
                continue;
            }
            let spec = self.type_spec();
            let init = if self.eat(TokenKind::Assign) { Some(self.initializer()) } else { None };
            let end = self.prev_span();
            if !self.expect(TokenKind::Semi) {
                self.recover(&[TokenKind::Kw(Keyword::EndVar)]);
            }
            inits.push(ConfigInit { path, location, spec, init, span: start.to(end) });
        }
        inits
    }
}

/// Interprets the text of a `%...` token (B.1.4.1).
pub fn parse_direct_var(text: &str, span: crate::source::Span) -> DirectVar {
    let mut chars = text[1..].chars().peekable();
    let location = match chars.next().map(|c| c.to_ascii_uppercase()) {
        Some('I') => LocationPrefix::Input,
        Some('Q') => LocationPrefix::Output,
        _ => LocationPrefix::Memory,
    };
    let mut size = SizePrefix::None;
    if let Some(c) = chars.peek().map(|c| c.to_ascii_uppercase()) {
        size = match c {
            'X' => SizePrefix::X,
            'B' => SizePrefix::B,
            'W' => SizePrefix::W,
            'D' => SizePrefix::D,
            'L' => SizePrefix::L,
            _ => SizePrefix::None,
        };
        if size != SizePrefix::None {
            chars.next();
        }
    }
    let rest: String = chars.collect();
    if rest == "*" {
        return DirectVar { location, size, address: Vec::new(), incomplete: true, span };
    }
    let address = rest.split('.').filter_map(|p| p.parse::<u32>().ok()).collect();
    DirectVar { location, size, address, incomplete: false, span }
}

#[allow(dead_code)]
fn _unused(_: Ident) {}
