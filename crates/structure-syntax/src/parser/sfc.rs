//! Textual Sequential Function Chart elements (B.1.6).

use super::Parser;
use crate::ast::*;
use crate::diag::Diagnostic;
use crate::ident::Ident;
use crate::token::{Keyword, TokenKind};

impl<'a> Parser<'a> {
    pub(crate) fn sfc_text(&mut self) -> Sfc {
        let start = self.span();
        let mut sfc = Sfc::default();
        while !self.at_eof() {
            let before = self.pos;
            match self.kind(0) {
                TokenKind::Kw(Keyword::InitialStep) | TokenKind::Kw(Keyword::Step) => {
                    let s = self.step();
                    sfc.steps.push(s);
                }
                TokenKind::Kw(Keyword::Transition) => {
                    if let Some(t) = self.transition() {
                        sfc.transitions.push(t);
                    }
                }
                TokenKind::Kw(Keyword::Action) => {
                    let a = self.action();
                    sfc.actions.push(a);
                }
                _ => {
                    self.expected("INITIAL_STEP, STEP, TRANSITION or ACTION");
                    self.bump();
                    while !self.at_eof()
                        && !matches!(
                            self.kind(0),
                            TokenKind::Kw(Keyword::InitialStep)
                                | TokenKind::Kw(Keyword::Step)
                                | TokenKind::Kw(Keyword::Transition)
                                | TokenKind::Kw(Keyword::Action)
                        )
                    {
                        self.bump();
                    }
                }
            }
            if self.pos == before {
                self.bump();
            }
        }
        sfc.span = start.to(self.prev_span());
        sfc
    }

    fn step(&mut self) -> Step {
        let start = self.span();
        let initial = self.at_kw(Keyword::InitialStep);
        self.bump();
        let name = self.expect_ident("a step name").unwrap_or_else(|| Ident::new("?", start));
        self.expect(TokenKind::Colon);
        let mut associations = Vec::new();
        while !self.at_kw(Keyword::EndStep) && !self.at_eof() {
            if matches!(
                self.kind(0),
                TokenKind::Kw(Keyword::InitialStep) | TokenKind::Kw(Keyword::Step) | TokenKind::Kw(Keyword::Transition) | TokenKind::Kw(Keyword::Action)
            ) {
                break;
            }
            let before = self.pos;
            if let Some(a) = self.action_association() {
                associations.push(a);
            }
            if !self.expect(TokenKind::Semi) {
                self.recover(&[TokenKind::Kw(Keyword::EndStep)]);
            }
            if self.pos == before {
                self.bump();
            }
        }
        let end = self.span();
        self.expect_kw(Keyword::EndStep);
        Step { name, initial, associations, span: start.to(end) }
    }

    /// `action_name '(' [action_qualifier] {',' indicator_name} ')'`
    fn action_association(&mut self) -> Option<ActionAssociation> {
        let start = self.span();
        let action = self.expect_ident("an action name")?;
        let mut qualifier = None;
        let mut time = None;
        let mut indicators = Vec::new();
        if self.eat(TokenKind::LParen) {
            let mut first = true;
            loop {
                if self.at(TokenKind::RParen) {
                    break;
                }
                if !first && !self.expect(TokenKind::Comma) {
                    break;
                }
                if first && self.at(TokenKind::Comma) {
                    // qualifier omitted
                    first = false;
                    continue;
                }
                if first {
                    first = false;
                    if self.at(TokenKind::Ident) {
                        if let Some(q) = Qualifier::from_upper(&self.cur_upper()) {
                            let qt = self.bump();
                            qualifier = Some(q);
                            if q.is_timed() {
                                if self.expect(TokenKind::Comma) {
                                    time = Some(self.primary());
                                }
                            } else if matches!(q, Qualifier::P1 | Qualifier::P0) {
                                let _ = qt;
                            }
                            continue;
                        }
                    }
                }
                match self.expect_ident("an indicator variable") {
                    Some(i) => indicators.push(i),
                    None => break,
                }
            }
            self.expect(TokenKind::RParen);
        } else {
            self.diags.push(Diagnostic::syntax("expected '(' after action name in step body", self.span()).with_clause("B.1.6"));
        }
        Some(ActionAssociation { action, qualifier, time, indicators, span: start.to(self.prev_span()) })
    }

    fn step_list(&mut self) -> Vec<Ident> {
        let mut v = Vec::new();
        if self.eat(TokenKind::LParen) {
            while let Some(i) = self.expect_ident("a step name") {
                v.push(i);
                if !self.eat(TokenKind::Comma) {
                    break;
                }
            }
            self.expect(TokenKind::RParen);
            if v.len() < 2 {
                self.diags.push(Diagnostic::syntax("a parenthesized step list shall contain at least two steps", self.prev_span()).with_clause("B.1.6"));
            }
        } else if let Some(i) = self.expect_ident("a step name") {
            v.push(i);
        }
        v
    }

    fn transition(&mut self) -> Option<Transition> {
        let start = self.span();
        self.bump();
        let name = self.ident();
        let mut priority = None;
        if self.at(TokenKind::LParen) {
            self.bump();
            if self.at(TokenKind::Ident) && self.cur_upper() == "PRIORITY" {
                self.bump();
                self.expect(TokenKind::Assign);
                priority = Some(self.expr());
            } else {
                self.expected("PRIORITY");
            }
            self.expect(TokenKind::RParen);
        }
        self.expect_kw(Keyword::From);
        let from = self.step_list();
        self.expect_kw(Keyword::To);
        let to = self.step_list();
        let condition = if self.eat(TokenKind::Assign) {
            let e = self.expr();
            self.expect(TokenKind::Semi);
            TransitionCondition::St(e)
        } else if self.eat(TokenKind::Colon) {
            let end = self.find_end_transition();
            let end_off = if end < self.toks.len() { self.toks[end].span.start } else { self.file.text.len() as u32 };
            let start_tok = self.pos;
            let lang = self.classify(start_tok, end);
            match lang {
                super::BodyLang::Graphic => {
                    let first = self.toks[start_tok].span.start;
                    let g = self.graphic_region(first, end_off);
                    self.pos = end;
                    TransitionCondition::Graphic(g)
                }
                _ => {
                    let saved = self.limit;
                    self.limit = end;
                    let list = self.il_list();
                    self.limit = saved;
                    self.pos = end;
                    TransitionCondition::Il(list)
                }
            }
        } else {
            self.expected("':=' or ':' introducing the transition condition");
            self.recover(&[TokenKind::Kw(Keyword::EndTransition)]);
            TransitionCondition::St(Expr { kind: ExprKind::Error, span: self.span() })
        };
        let end = self.span();
        self.expect_kw(Keyword::EndTransition);
        Some(Transition { name, priority, from, to, condition, branch_priority: None, span: start.to(end) })
    }

    fn find_end_transition(&self) -> usize {
        let mut i = self.pos;
        while i < self.limit {
            if self.toks[i].kind == TokenKind::Kw(Keyword::EndTransition) {
                return i;
            }
            i += 1;
        }
        self.limit
    }

    fn action(&mut self) -> Action {
        let start = self.span();
        self.bump();
        let name = self.expect_ident("an action name").unwrap_or_else(|| Ident::new("?", start));
        self.expect(TokenKind::Colon);
        let body = self.body(Keyword::EndAction);
        let end = self.span();
        self.expect_kw(Keyword::EndAction);
        Action { name, body, span: start.to(end) }
    }
}
