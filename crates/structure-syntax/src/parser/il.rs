//! Instruction List (B.2).

use super::Parser;
use crate::ast::*;
use crate::diag::Diagnostic;
use crate::ident::Ident;
use crate::token::{Keyword, TokenKind};

impl<'a> Parser<'a> {
    fn at_line_end(&self) -> bool {
        self.at_eof() || self.tok(0).nl_before
    }

    /// `instruction_list ::= il_instruction {il_instruction}`
    pub(crate) fn il_list(&mut self) -> Vec<IlInstr> {
        let mut list = Vec::new();
        while !self.at_eof() {
            if !self.tok(0).nl_before && !list.is_empty() {
                self.error_here("each IL instruction shall begin on a new line");
            }
            let before = self.pos;
            if let Some(i) = self.il_instruction(true) {
                list.push(i);
            }
            if self.pos == before {
                self.bump();
            }
            // Anything left on the line is an error.
            if !self.at_line_end() {
                self.error_here("unexpected text after IL instruction");
                while !self.at_line_end() {
                    self.bump();
                }
            }
        }
        list
    }

    /// Text of the current token as an IL operator mnemonic.
    fn il_mnemonic(&self) -> Option<String> {
        match self.kind(0) {
            TokenKind::Ident => Some(self.cur_upper()),
            TokenKind::Kw(k @ (Keyword::And | Keyword::Or | Keyword::Xor | Keyword::Not | Keyword::Mod)) => {
                Some(k.as_str().to_string())
            }
            TokenKind::Amp => {
                if self.kind(1) == TokenKind::Ident && !self.tok(1).ws_before && self.text(self.tok(1)).eq_ignore_ascii_case("N") {
                    Some("&N".to_string())
                } else {
                    Some("&".to_string())
                }
            }
            _ => None,
        }
    }

    fn il_instruction(&mut self, allow_label: bool) -> Option<IlInstr> {
        let start = self.span();
        let mut label = None;
        if allow_label && self.at(TokenKind::Ident) && self.kind(1) == TokenKind::Colon && !self.tok(1).nl_before {
            label = self.ident();
            self.bump();
        }
        if self.at_eof() || (label.is_some() && self.at_line_end()) {
            return Some(IlInstr { label, op: IlOp::None, span: start });
        }
        let Some(mnemonic) = self.il_mnemonic() else {
            self.expected("an IL operator");
            while !self.at_eof() && !self.at_line_end() {
                self.bump();
            }
            if !self.at_eof() && self.pos < self.limit {
                // consume the offending first token of the line
            }
            return None;
        };
        let op_tok = self.bump();
        if mnemonic == "&N" {
            self.bump();
        }
        let op = match mnemonic.as_str() {
            "JMP" | "JMPC" | "JMPCN" => {
                let cond = cond_of(&mnemonic, "JMP");
                if self.at_line_end() {
                    self.expected("a label");
                    IlOp::None
                } else {
                    match self.expect_ident("a label") {
                        Some(l) => IlOp::Jump { cond, label: l },
                        None => IlOp::None,
                    }
                }
            }
            "RET" | "RETC" | "RETCN" => IlOp::Return { cond: cond_of(&mnemonic, "RET") },
            "CAL" | "CALC" | "CALCN" => {
                let cond = cond_of(&mnemonic, "CAL");
                let instance = self.il_instance();
                let args = if self.at(TokenKind::LParen) && !self.tok(0).nl_before {
                    self.bump();
                    if self.il_formal_follows() {
                        IlCallArgs::Formal(self.il_param_list())
                    } else {
                        let mut ops = Vec::new();
                        if !self.at(TokenKind::RParen) {
                            loop {
                                ops.push(self.il_operand());
                                if !self.eat(TokenKind::Comma) {
                                    break;
                                }
                            }
                        }
                        self.expect(TokenKind::RParen);
                        IlCallArgs::NonFormal(ops)
                    }
                } else {
                    IlCallArgs::None
                };
                IlOp::Call { cond, instance, args }
            }
            m => {
                if let Some(op) = IlOperator::from_upper(m) {
                    if op.is_expr_operator() && self.at(TokenKind::LParen) && !self.tok(0).nl_before && !self.il_paren_is_formal_call() {
                        self.bump();
                        let operand = if self.at_line_end() { None } else { Some(self.il_operand()) };
                        let body = self.il_nested_list();
                        IlOp::Deferred { op, operand, body }
                    } else if op.is_expr_operator() && self.at(TokenKind::LParen) && !self.tok(0).nl_before {
                        let name = Ident::new(self.text(&op_tok).to_string(), op_tok.span);
                        self.bump();
                        let params = self.il_param_list();
                        IlOp::FormalFunction { name, params }
                    } else {
                        let operand = if self.at_line_end() { None } else { Some(self.il_operand()) };
                        IlOp::Simple { op, operand }
                    }
                } else {
                    let name = Ident::new(self.text(&op_tok).to_string(), op_tok.span);
                    if op_tok.kind != TokenKind::Ident && !matches!(op_tok.kind, TokenKind::Kw(_)) {
                        self.diags.push(Diagnostic::syntax(format!("'{}' is not an IL operator", m), op_tok.span));
                    }
                    if self.at(TokenKind::LParen) && !self.tok(0).nl_before {
                        self.bump();
                        let params = self.il_param_list();
                        IlOp::FormalFunction { name, params }
                    } else {
                        let mut operands = Vec::new();
                        if !self.at_line_end() {
                            loop {
                                operands.push(self.il_operand());
                                if !self.eat(TokenKind::Comma) {
                                    break;
                                }
                            }
                        }
                        IlOp::Function { name, operands }
                    }
                }
            }
        };
        Some(IlInstr { label, op, span: start.to(self.prev_span()) })
    }

    /// After `OP(`: is this a formal call (`name :=` on the next line)?
    fn il_paren_is_formal_call(&self) -> bool {
        // self.kind(0) is '('
        let t1 = self.tok(1);
        if t1.nl_before {
            return (self.kind(1) == TokenKind::Ident && matches!(self.kind(2), TokenKind::Assign | TokenKind::Arrow))
                || (self.kind(1) == TokenKind::Kw(Keyword::Not) && self.kind(3) == TokenKind::Arrow);
        }
        false
    }

    /// After `CAL fb(`: decides between formal and non-formal argument lists.
    fn il_formal_follows(&self) -> bool {
        if self.at(TokenKind::RParen) {
            return false;
        }
        if self.tok(0).nl_before {
            return true;
        }
        (self.at(TokenKind::Ident) && matches!(self.kind(1), TokenKind::Assign | TokenKind::Arrow))
            || (self.at_kw(Keyword::Not) && self.kind(2) == TokenKind::Arrow)
    }

    /// Function block instance name for CAL (may be a structured path).
    fn il_instance(&mut self) -> Expr {
        let Some(name) = self.expect_ident("a function block instance name") else {
            return Expr { kind: ExprKind::Error, span: self.span() };
        };
        let span = name.span;
        let mut e = Expr { kind: ExprKind::Name(name), span };
        loop {
            if self.at(TokenKind::Dot) && self.kind(1) == TokenKind::Ident {
                self.bump();
                let field = self.ident().unwrap();
                let span = e.span.to(field.span);
                e = Expr { kind: ExprKind::Field { base: Box::new(e), field }, span };
            } else if self.at(TokenKind::LBracket) {
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
            } else {
                break;
            }
        }
        e
    }

    /// `il_operand ::= constant | variable | enumerated_value`
    pub(crate) fn il_operand(&mut self) -> Expr {
        let start = self.span();
        if matches!(self.kind(0), TokenKind::Minus | TokenKind::Plus) && matches!(self.kind(1), TokenKind::Int | TokenKind::Real) {
            let neg = self.at(TokenKind::Minus);
            self.bump();
            let e = self.primary();
            let span = start.to(e.span);
            if neg {
                return Expr { kind: ExprKind::Unary { op: UnaryOp::Neg, operand: Box::new(e) }, span };
            }
            return Expr { kind: e.kind, span };
        }
        match self.kind(0) {
            TokenKind::Ident => {
                // a variable, possibly with selectors, but never a call
                let name = self.ident().unwrap();
                let span = name.span;
                let mut e = Expr { kind: ExprKind::Name(name), span };
                loop {
                    if self.at(TokenKind::Dot) && self.kind(1) == TokenKind::Ident {
                        self.bump();
                        let field = self.ident().unwrap();
                        let span = e.span.to(field.span);
                        e = Expr { kind: ExprKind::Field { base: Box::new(e), field }, span };
                    } else if self.at(TokenKind::LBracket) {
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
                    } else {
                        break;
                    }
                }
                e
            }
            _ => self.primary(),
        }
    }

    /// Instructions of a deferred operation up to the closing `)`.
    fn il_nested_list(&mut self) -> Vec<IlInstr> {
        let mut list = Vec::new();
        loop {
            if self.at_eof() {
                self.expected("')' closing the deferred IL operation");
                break;
            }
            if self.at(TokenKind::RParen) {
                if !self.tok(0).nl_before {
                    self.error_here("')' shall begin a new line");
                }
                self.bump();
                break;
            }
            if !self.tok(0).nl_before {
                self.error_here("each IL instruction shall begin on a new line");
            }
            let before = self.pos;
            if let Some(i) = self.il_instruction(false) {
                if matches!(i.op, IlOp::Jump { .. } | IlOp::Call { .. } | IlOp::Return { .. }) {
                    self.diags.push(Diagnostic::syntax("only simple instructions are allowed inside a deferred operation", i.span).with_clause("B.2.1"));
                }
                list.push(i);
            }
            if self.pos == before {
                self.bump();
            }
            if !self.at_line_end() && !self.at(TokenKind::RParen) {
                self.error_here("unexpected text after IL instruction");
                while !self.at_line_end() {
                    self.bump();
                }
            }
        }
        list
    }

    /// Formal parameter list after `(`, including the closing `)`.
    fn il_param_list(&mut self) -> Vec<IlParam> {
        let mut params = Vec::new();
        loop {
            if self.eat(TokenKind::RParen) {
                break;
            }
            if self.at_eof() {
                self.expected("')'");
                break;
            }
            let start = self.span();
            let negated = if self.at_kw(Keyword::Not) && self.kind(2) == TokenKind::Arrow {
                self.bump();
                true
            } else {
                false
            };
            let Some(name) = self.expect_ident("a formal parameter name") else {
                while !self.at_eof() && !self.at(TokenKind::RParen) && !self.at(TokenKind::Comma) {
                    self.bump();
                }
                self.eat(TokenKind::Comma);
                continue;
            };
            if self.eat(TokenKind::Arrow) {
                let target = self.il_operand();
                params.push(IlParam::Output { name, target, negated, span: start.to(self.prev_span()) });
            } else if self.eat(TokenKind::Assign) {
                if negated {
                    self.diags.push(Diagnostic::syntax("NOT is only allowed with output assignments", start));
                }
                let value = if self.at(TokenKind::LParen) && self.tok(1).nl_before {
                    self.bump();
                    IlParamValue::Nested(self.il_nested_list())
                } else {
                    IlParamValue::Operand(self.il_operand())
                };
                params.push(IlParam::Input { name, value, span: start.to(self.prev_span()) });
            } else {
                self.expected("':=' or '=>'");
            }
            if !self.eat(TokenKind::Comma) && !self.at(TokenKind::RParen) {
                self.expected("',' or ')'");
            }
        }
        params
    }
}

fn cond_of(m: &str, base: &str) -> IlCond {
    match &m[base.len()..] {
        "C" => IlCond::IfTrue,
        "CN" => IlCond::IfFalse,
        _ => IlCond::Always,
    }
}
