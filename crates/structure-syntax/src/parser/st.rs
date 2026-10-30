//! Structured Text expressions and statements (B.3).

use super::Parser;
use crate::ast::*;
use crate::diag::Diagnostic;
use crate::ident::Ident;
use crate::literal;
use crate::token::{Keyword, TokenKind};

impl<'a> Parser<'a> {
    // ------------------------------------------------------------ expressions

    pub(crate) fn expr(&mut self) -> Expr {
        self.binary(1)
    }

    fn binary_op(&self) -> Option<BinaryOp> {
        Some(match self.kind(0) {
            TokenKind::Kw(Keyword::Or) => BinaryOp::Or,
            TokenKind::Kw(Keyword::Xor) => BinaryOp::Xor,
            TokenKind::Kw(Keyword::And) | TokenKind::Amp => BinaryOp::And,
            TokenKind::Eq => BinaryOp::Eq,
            TokenKind::Ne => BinaryOp::Ne,
            TokenKind::Lt => BinaryOp::Lt,
            TokenKind::Gt => BinaryOp::Gt,
            TokenKind::Le => BinaryOp::Le,
            TokenKind::Ge => BinaryOp::Ge,
            TokenKind::Plus => BinaryOp::Add,
            TokenKind::Minus => BinaryOp::Sub,
            TokenKind::Star => BinaryOp::Mul,
            TokenKind::Slash => BinaryOp::Div,
            TokenKind::Kw(Keyword::Mod) => BinaryOp::Mod,
            TokenKind::Power => BinaryOp::Pow,
            _ => return None,
        })
    }

    fn binary(&mut self, min_prec: u8) -> Expr {
        let mut lhs = self.unary();
        while let Some(op) = self.binary_op() {
            let prec = op.precedence();
            if prec < min_prec {
                break;
            }
            self.bump();
            let rhs = self.binary(prec + 1);
            let span = lhs.span.to(rhs.span);
            lhs = Expr { kind: ExprKind::Binary { op, lhs: Box::new(lhs), rhs: Box::new(rhs) }, span };
        }
        lhs
    }

    fn unary(&mut self) -> Expr {
        let start = self.span();
        match self.kind(0) {
            TokenKind::Minus => {
                self.bump();
                let operand = self.unary();
                let span = start.to(operand.span);
                Expr { kind: ExprKind::Unary { op: UnaryOp::Neg, operand: Box::new(operand) }, span }
            }
            TokenKind::Kw(Keyword::Not) => {
                self.bump();
                let operand = self.unary();
                let span = start.to(operand.span);
                Expr { kind: ExprKind::Unary { op: UnaryOp::Not, operand: Box::new(operand) }, span }
            }
            TokenKind::Plus if matches!(self.kind(1), TokenKind::Int | TokenKind::Real) => {
                // signed_integer / real literal with explicit '+'
                self.bump();
                let mut e = self.primary();
                e.span = start.to(e.span);
                e
            }
            _ => self.primary(),
        }
    }

    fn literal(&mut self, kind: LiteralKind, type_prefix: Option<Ident>, span: crate::source::Span) -> Expr {
        Expr { kind: ExprKind::Literal(Literal { kind, type_prefix }), span }
    }

    /// Numeric literal after an optional sign.
    fn numeric_literal(&mut self, negative: bool, prefix: Option<Ident>, start: crate::source::Span) -> Expr {
        let t = self.bump();
        let text = self.text(&t);
        let span = start.to(t.span);
        match t.kind {
            TokenKind::Int => match literal::parse_decimal(text) {
                Ok(v) => self.literal(LiteralKind::Int { value: v, negative, based: false }, prefix, span),
                Err(e) => {
                    self.diags.push(Diagnostic::syntax(e, t.span).with_clause("2.2.1"));
                    Expr { kind: ExprKind::Error, span }
                }
            },
            TokenKind::BasedInt => {
                if negative {
                    self.diags.push(Diagnostic::syntax("based numbers shall not contain a leading sign", span).with_clause("2.2.1"));
                }
                match literal::parse_based(text) {
                    Ok(v) => self.literal(LiteralKind::Int { value: v, negative: false, based: true }, prefix, span),
                    Err(e) => {
                        self.diags.push(Diagnostic::syntax(e, t.span).with_clause("2.2.1"));
                        Expr { kind: ExprKind::Error, span }
                    }
                }
            }
            TokenKind::Real => match literal::parse_real(text) {
                Ok(s) => self.literal(LiteralKind::Real { text: s, negative }, prefix, span),
                Err(e) => {
                    self.diags.push(Diagnostic::syntax(e, t.span).with_clause("2.2.1"));
                    Expr { kind: ExprKind::Error, span }
                }
            },
            _ => unreachable!(),
        }
    }

    pub(crate) fn primary(&mut self) -> Expr {
        let start = self.span();
        match self.kind(0) {
            TokenKind::Int | TokenKind::BasedInt | TokenKind::Real => self.numeric_literal(false, None, start),
            TokenKind::SString | TokenKind::DString => {
                let t = self.bump();
                let wide = t.kind == TokenKind::DString;
                match literal::parse_string(self.text(&t), wide) {
                    Ok(chars) => self.literal(LiteralKind::String { wide, chars }, None, t.span),
                    Err(e) => {
                        self.diags.push(Diagnostic::syntax(e, t.span).with_clause("2.2.2"));
                        Expr { kind: ExprKind::Error, span: t.span }
                    }
                }
            }
            TokenKind::Duration => {
                let t = self.bump();
                match literal::parse_duration(self.text(&t)) {
                    Ok(d) => self.literal(LiteralKind::Duration(d), None, t.span),
                    Err(e) => {
                        self.diags.push(Diagnostic::syntax(e, t.span).with_clause("2.2.3.1"));
                        Expr { kind: ExprKind::Error, span: t.span }
                    }
                }
            }
            TokenKind::TimeOfDay => {
                let t = self.bump();
                match literal::parse_time_of_day(self.text(&t)) {
                    Ok(v) => self.literal(LiteralKind::TimeOfDay(v), None, t.span),
                    Err(e) => {
                        self.diags.push(Diagnostic::syntax(e, t.span).with_clause("2.2.3.2"));
                        Expr { kind: ExprKind::Error, span: t.span }
                    }
                }
            }
            TokenKind::Date => {
                let t = self.bump();
                match literal::parse_date(self.text(&t)) {
                    Ok(v) => self.literal(LiteralKind::Date(v), None, t.span),
                    Err(e) => {
                        self.diags.push(Diagnostic::syntax(e, t.span).with_clause("2.2.3.2"));
                        Expr { kind: ExprKind::Error, span: t.span }
                    }
                }
            }
            TokenKind::DateAndTime => {
                let t = self.bump();
                match literal::parse_date_and_time(self.text(&t)) {
                    Ok((d, tod)) => self.literal(LiteralKind::DateAndTime(d, tod), None, t.span),
                    Err(e) => {
                        self.diags.push(Diagnostic::syntax(e, t.span).with_clause("2.2.3.2"));
                        Expr { kind: ExprKind::Error, span: t.span }
                    }
                }
            }
            TokenKind::Kw(Keyword::True) => {
                let t = self.bump();
                self.literal(LiteralKind::Bool(true), None, t.span)
            }
            TokenKind::Kw(Keyword::False) => {
                let t = self.bump();
                self.literal(LiteralKind::Bool(false), None, t.span)
            }
            TokenKind::TypedPrefix => self.typed_literal(),
            TokenKind::DirectVar => {
                let d = self.direct_var_token().unwrap();
                let span = d.span;
                Expr { kind: ExprKind::DirectVar(d), span }
            }
            TokenKind::LParen => {
                self.bump();
                let inner = self.expr();
                let end = self.span();
                self.expect(TokenKind::RParen);
                Expr { kind: ExprKind::Paren(Box::new(inner)), span: start.to(end) }
            }
            TokenKind::Ident => self.variable_expr(),
            TokenKind::Kw(kw @ (Keyword::And | Keyword::Or | Keyword::Xor | Keyword::Mod | Keyword::Not))
                if self.kind(1) == TokenKind::LParen =>
            {
                // Standard functions whose names are operator keywords, e.g. OR(a, b, c).
                let t = self.bump();
                let name = Ident::new(kw.as_str(), t.span);
                let callee = Expr { kind: ExprKind::Name(name), span: t.span };
                self.call_suffix(callee)
            }
            _ => {
                self.expected("an expression");
                let span = self.span();
                if !self.at_eof() && !matches!(self.kind(0), TokenKind::Semi | TokenKind::RParen) {
                    self.bump();
                }
                Expr { kind: ExprKind::Error, span }
            }
        }
    }

    fn typed_literal(&mut self) -> Expr {
        let t = self.bump();
        let full = self.text(&t);
        let name_text = &full[..full.len() - full.chars().last().unwrap().len_utf8()];
        let prefix = Ident::new(name_text.to_string(), t.span);
        let start = t.span;
        match self.kind(0) {
            TokenKind::Int | TokenKind::BasedInt | TokenKind::Real => self.numeric_literal(false, Some(prefix), start),
            TokenKind::Minus | TokenKind::Plus
                if matches!(self.kind(1), TokenKind::Int | TokenKind::Real | TokenKind::BasedInt) =>
            {
                let neg = self.kind(0) == TokenKind::Minus;
                self.bump();
                self.numeric_literal(neg, Some(prefix), start)
            }
            TokenKind::Kw(Keyword::True) | TokenKind::Kw(Keyword::False) => {
                let v = self.bump();
                let b = v.kind == TokenKind::Kw(Keyword::True);
                self.literal(LiteralKind::Bool(b), Some(prefix), start.to(v.span))
            }
            TokenKind::SString | TokenKind::DString => {
                let v = self.bump();
                let quote_wide = v.kind == TokenKind::DString;
                let wide = prefix.is("WSTRING") || (quote_wide && !prefix.is("STRING"));
                match literal::parse_string(self.text(&v), quote_wide) {
                    Ok(chars) => {
                        if !wide && chars.iter().any(|c| *c > 0xFF) {
                            self.diags.push(Diagnostic::syntax("character not representable in STRING", v.span));
                        }
                        self.literal(LiteralKind::String { wide, chars }, Some(prefix), start.to(v.span))
                    }
                    Err(e) => {
                        self.diags.push(Diagnostic::syntax(e, v.span).with_clause("2.2.2"));
                        Expr { kind: ExprKind::Error, span: start.to(v.span) }
                    }
                }
            }
            TokenKind::Ident => {
                let value = self.ident().unwrap();
                let span = start.to(value.span);
                Expr { kind: ExprKind::TypedEnum { ty: prefix, value }, span }
            }
            _ => {
                self.expected("a literal or enumerated value after the type prefix");
                Expr { kind: ExprKind::Error, span: start }
            }
        }
    }

    /// A symbolic variable with selectors, or a function/FB call.
    pub(crate) fn variable_expr(&mut self) -> Expr {
        let Some(name) = self.ident() else {
            if self.at(TokenKind::DirectVar) {
                let d = self.direct_var_token().unwrap();
                let span = d.span;
                return Expr { kind: ExprKind::DirectVar(d), span };
            }
            self.expected("a variable");
            return Expr { kind: ExprKind::Error, span: self.span() };
        };
        let span = name.span;
        let mut e = Expr { kind: ExprKind::Name(name), span };
        loop {
            match self.kind(0) {
                TokenKind::Dot => {
                    if self.kind(1) == TokenKind::Ident {
                        self.bump();
                        let field = self.ident().unwrap();
                        let span = e.span.to(field.span);
                        e = Expr { kind: ExprKind::Field { base: Box::new(e), field }, span };
                    } else {
                        break;
                    }
                }
                TokenKind::LBracket => {
                    self.bump();
                    let mut indices = Vec::new();
                    loop {
                        indices.push(self.expr());
                        if !self.eat(TokenKind::Comma) {
                            break;
                        }
                    }
                    let end = self.span();
                    self.expect(TokenKind::RBracket);
                    let span = e.span.to(end);
                    e = Expr { kind: ExprKind::Index { base: Box::new(e), indices }, span };
                }
                TokenKind::LParen if matches!(e.kind, ExprKind::Name(_)) => {
                    e = self.call_suffix(e);
                }
                _ => break,
            }
        }
        e
    }

    fn call_suffix(&mut self, callee: Expr) -> Expr {
        let start = callee.span;
        self.expect(TokenKind::LParen);
        let args = self.args();
        let end = self.span();
        self.expect(TokenKind::RParen);
        let span = start.to(end);
        Expr { kind: ExprKind::Call(CallExpr { callee: Box::new(callee), args, span }), span }
    }

    /// `param_assignment {',' param_assignment}`
    pub(crate) fn args(&mut self) -> Vec<Arg> {
        let mut args = Vec::new();
        if self.at(TokenKind::RParen) {
            return args;
        }
        loop {
            args.push(self.arg());
            if !self.eat(TokenKind::Comma) {
                break;
            }
        }
        args
    }

    fn arg(&mut self) -> Arg {
        let start = self.span();
        if self.at_kw(Keyword::Not) && self.kind(1) == TokenKind::Ident && self.kind(2) == TokenKind::Arrow {
            self.bump();
            let name = self.ident().unwrap();
            self.bump();
            let target = self.output_target();
            return Arg::Output { name, target, negated: true, span: start.to(self.prev_span()) };
        }
        if self.at(TokenKind::Ident) && self.kind(1) == TokenKind::Assign {
            let name = self.ident().unwrap();
            self.bump();
            let value = self.expr();
            return Arg::Input { name, value };
        }
        if self.at(TokenKind::Ident) && self.kind(1) == TokenKind::Arrow {
            let name = self.ident().unwrap();
            self.bump();
            let target = self.output_target();
            return Arg::Output { name, target, negated: false, span: start.to(self.prev_span()) };
        }
        Arg::Positional(self.expr())
    }

    fn output_target(&mut self) -> Option<Expr> {
        if matches!(self.kind(0), TokenKind::Comma | TokenKind::RParen) {
            None
        } else {
            Some(self.variable_expr())
        }
    }

    // ------------------------------------------------------------ statements

    /// Parses statements until one of `stops` or end of input.
    pub(crate) fn stmt_list(&mut self, stops: &[TokenKind]) -> Vec<Stmt> {
        let mut stmts = Vec::new();
        while !self.at_eof() && !stops.contains(&self.kind(0)) {
            let before = self.pos;
            if let Some(s) = self.statement(stops) {
                stmts.push(s);
            }
            if self.pos == before {
                self.bump();
            }
        }
        stmts
    }

    fn end_statement(&mut self, compound: bool) {
        if self.eat(TokenKind::Semi) {
            return;
        }
        if compound {
            let span = self.prev_span();
            self.diags.push(
                Diagnostic::extension("missing ';' after compound statement (accepted)", span).with_clause("B.3.2"),
            );
        } else {
            self.expected("';'");
        }
    }

    fn statement(&mut self, stops: &[TokenKind]) -> Option<Stmt> {
        let start = self.span();
        let kind = match self.kind(0) {
            TokenKind::Semi => {
                self.bump();
                return Some(Stmt { kind: StmtKind::Empty, span: start });
            }
            TokenKind::Kw(Keyword::If) => {
                let k = self.if_stmt();
                self.end_statement(true);
                k
            }
            TokenKind::Kw(Keyword::Case) => {
                let k = self.case_stmt();
                self.end_statement(true);
                k
            }
            TokenKind::Kw(Keyword::For) => {
                let k = self.for_stmt();
                self.end_statement(true);
                k
            }
            TokenKind::Kw(Keyword::While) => {
                self.bump();
                let cond = self.expr();
                self.expect_kw(Keyword::Do);
                let body = self.stmt_list(&[TokenKind::Kw(Keyword::EndWhile)]);
                self.expect_kw(Keyword::EndWhile);
                self.end_statement(true);
                StmtKind::While { cond, body }
            }
            TokenKind::Kw(Keyword::Repeat) => {
                self.bump();
                let body = self.stmt_list(&[TokenKind::Kw(Keyword::Until)]);
                self.expect_kw(Keyword::Until);
                let until = self.expr();
                self.expect_kw(Keyword::EndRepeat);
                self.end_statement(true);
                StmtKind::Repeat { body, until }
            }
            TokenKind::Kw(Keyword::Exit) => {
                self.bump();
                self.end_statement(false);
                StmtKind::Exit
            }
            TokenKind::Kw(Keyword::Return) => {
                self.bump();
                self.end_statement(false);
                StmtKind::Return
            }
            TokenKind::Ident | TokenKind::DirectVar => {
                let target = self.variable_expr();
                if self.eat(TokenKind::Assign) {
                    let value = self.expr();
                    if !matches!(self.kind(0), TokenKind::Semi) {
                        self.expected("';'");
                        self.recover(stops);
                    } else {
                        self.bump();
                    }
                    StmtKind::Assign { target, value }
                } else if let ExprKind::Call(call) = target.kind {
                    self.end_statement(false);
                    StmtKind::Call(call)
                } else {
                    self.expected("':=' or a function block invocation");
                    self.recover(stops);
                    return None;
                }
            }
            _ => {
                self.expected("a statement");
                self.recover(stops);
                return None;
            }
        };
        Some(Stmt { kind, span: start.to(self.prev_span()) })
    }

    fn if_stmt(&mut self) -> StmtKind {
        self.bump();
        let mut branches = Vec::new();
        let mut else_body = None;
        let cond = self.expr();
        self.expect_kw(Keyword::Then);
        let stops = [TokenKind::Kw(Keyword::Elsif), TokenKind::Kw(Keyword::Else), TokenKind::Kw(Keyword::EndIf)];
        let body = self.stmt_list(&stops);
        branches.push((cond, body));
        loop {
            if self.eat_kw(Keyword::Elsif) {
                let cond = self.expr();
                self.expect_kw(Keyword::Then);
                let body = self.stmt_list(&stops);
                branches.push((cond, body));
            } else if self.eat_kw(Keyword::Else) {
                else_body = Some(self.stmt_list(&[TokenKind::Kw(Keyword::EndIf)]));
            } else {
                break;
            }
        }
        self.expect_kw(Keyword::EndIf);
        StmtKind::If { branches, else_body }
    }

    fn for_stmt(&mut self) -> StmtKind {
        self.bump();
        let var = self.expect_ident("a control variable").unwrap_or_else(|| Ident::new("?", self.span()));
        self.expect(TokenKind::Assign);
        let from = self.expr();
        self.expect_kw(Keyword::To);
        let to = self.expr();
        let by = if self.eat_kw(Keyword::By) { Some(self.expr()) } else { None };
        self.expect_kw(Keyword::Do);
        let body = self.stmt_list(&[TokenKind::Kw(Keyword::EndFor)]);
        self.expect_kw(Keyword::EndFor);
        StmtKind::For { var, from, to, by, body }
    }

    /// Looks ahead for `case_list ':'`.
    fn at_case_label(&self) -> bool {
        let mut n = 0;
        loop {
            match self.kind(n) {
                TokenKind::Colon => return n > 0,
                TokenKind::Int
                | TokenKind::BasedInt
                | TokenKind::Minus
                | TokenKind::Plus
                | TokenKind::Ident
                | TokenKind::TypedPrefix
                | TokenKind::DotDot
                | TokenKind::Comma => n += 1,
                _ => return false,
            }
        }
    }

    fn case_stmt(&mut self) -> StmtKind {
        self.bump();
        let selector = self.expr();
        self.expect_kw(Keyword::Of);
        let mut arms = Vec::new();
        let mut else_body = None;
        loop {
            if self.at_kw(Keyword::EndCase) || self.at_eof() {
                break;
            }
            if self.eat_kw(Keyword::Else) {
                else_body = Some(self.stmt_list(&[TokenKind::Kw(Keyword::EndCase)]));
                break;
            }
            if !self.at_case_label() {
                self.expected("a case label");
                self.recover(&[TokenKind::Kw(Keyword::EndCase)]);
                continue;
            }
            let start = self.span();
            let mut labels = Vec::new();
            loop {
                let lo = self.expr();
                if self.eat(TokenKind::DotDot) {
                    let hi = self.expr();
                    labels.push(CaseLabel::Range(lo, hi));
                } else {
                    labels.push(CaseLabel::Value(lo));
                }
                if !self.eat(TokenKind::Comma) {
                    break;
                }
            }
            self.expect(TokenKind::Colon);
            let mut body = Vec::new();
            while !self.at_eof() && !self.at_kw(Keyword::EndCase) && !self.at_kw(Keyword::Else) && !self.at_case_label() {
                let before = self.pos;
                if let Some(s) = self.statement(&[TokenKind::Kw(Keyword::EndCase), TokenKind::Kw(Keyword::Else)]) {
                    body.push(s);
                }
                if self.pos == before {
                    self.bump();
                }
            }
            arms.push(CaseArm { labels, body, span: start.to(self.prev_span()) });
        }
        self.expect_kw(Keyword::EndCase);
        StmtKind::Case { selector, arms, else_body }
    }
}
