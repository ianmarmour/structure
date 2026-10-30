//! Configuration elements (B.1.7).

use super::Parser;
use crate::ast::*;
use crate::diag::Diagnostic;
use crate::ident::Ident;
use crate::token::{Keyword, TokenKind};

impl<'a> Parser<'a> {
    pub(crate) fn configuration(&mut self) -> Configuration {
        let start = self.span();
        self.bump();
        let name = self.expect_ident("a configuration name").unwrap_or_else(|| Ident::new("?", start));
        let mut var_blocks = Vec::new();
        let mut resources: Vec<Resource> = Vec::new();
        let mut single: Option<Resource> = None;
        loop {
            match self.kind(0) {
                TokenKind::Kw(Keyword::EndConfiguration) | TokenKind::Eof => break,
                TokenKind::Kw(
                    Keyword::VarGlobal
                    | Keyword::VarAccess
                    | Keyword::VarConfig
                    | Keyword::Var
                    | Keyword::VarExternal
                    | Keyword::VarInput
                    | Keyword::VarOutput
                    | Keyword::VarInOut
                    | Keyword::VarTemp,
                ) => {
                    let b = self.var_block();
                    if !matches!(b.kind, VarKind::Global | VarKind::Access | VarKind::Config) {
                        self.diags.push(Diagnostic::syntax(
                            format!("{} is not allowed in a configuration", b.kind.keyword()),
                            b.span,
                        ));
                    }
                    var_blocks.push(b);
                }
                TokenKind::Kw(Keyword::Resource) => {
                    resources.push(self.resource());
                }
                TokenKind::Kw(Keyword::Task) | TokenKind::Kw(Keyword::Program) => {
                    let r = single.get_or_insert_with(|| Resource {
                        name: None,
                        on: None,
                        var_blocks: Vec::new(),
                        tasks: Vec::new(),
                        programs: Vec::new(),
                        span: self.toks[self.pos].span,
                    });
                    if self.at_kw(Keyword::Task) {
                        if let Some(t) = self.task_decl() {
                            r.tasks.push(t);
                        }
                    } else if let Some(p) = self.program_config() {
                        r.programs.push(p);
                    }
                }
                _ => {
                    if self.at_library_start() {
                        break;
                    }
                    self.expected("RESOURCE, TASK, PROGRAM, a variable block or END_CONFIGURATION");
                    self.bump();
                }
            }
        }
        if let Some(mut r) = single {
            r.span = r.span.to(self.prev_span());
            if !resources.is_empty() {
                self.diags.push(Diagnostic::syntax(
                    "a configuration shall contain either RESOURCE declarations or a single resource declaration",
                    r.span,
                ));
            }
            resources.insert(0, r);
        }
        if resources.is_empty() {
            self.diags.push(Diagnostic::syntax("a configuration shall contain at least one resource", start).with_clause("B.1.7"));
        }
        let end = self.span();
        self.expect_kw(Keyword::EndConfiguration);
        Configuration { name, var_blocks, resources, span: start.to(end) }
    }

    fn resource(&mut self) -> Resource {
        let start = self.span();
        self.bump();
        let name = self.expect_ident("a resource name");
        let on = if self.eat_kw(Keyword::On) {
            self.expect_ident("a resource type name")
        } else {
            self.expected("ON");
            None
        };
        let mut var_blocks = Vec::new();
        let mut tasks = Vec::new();
        let mut programs = Vec::new();
        loop {
            match self.kind(0) {
                TokenKind::Kw(Keyword::EndResource) | TokenKind::Eof | TokenKind::Kw(Keyword::EndConfiguration) => break,
                TokenKind::Kw(Keyword::VarGlobal | Keyword::VarExternal | Keyword::Var | Keyword::VarAccess | Keyword::VarConfig) => {
                    let b = self.var_block();
                    if !matches!(b.kind, VarKind::Global | VarKind::External) {
                        self.diags.push(Diagnostic::syntax(format!("{} is not allowed in a resource", b.kind.keyword()), b.span));
                    }
                    var_blocks.push(b);
                }
                TokenKind::Kw(Keyword::Task) => {
                    if let Some(t) = self.task_decl() {
                        tasks.push(t);
                    }
                }
                TokenKind::Kw(Keyword::Program) => {
                    if let Some(p) = self.program_config() {
                        programs.push(p);
                    }
                }
                _ => {
                    self.expected("TASK, PROGRAM, VAR_GLOBAL or END_RESOURCE");
                    self.bump();
                }
            }
        }
        let end = self.span();
        self.expect_kw(Keyword::EndResource);
        if programs.is_empty() {
            self.diags.push(Diagnostic::syntax("a resource shall contain at least one program configuration", start).with_clause("B.1.7"));
        }
        Resource { name, on, var_blocks, tasks, programs, span: start.to(end) }
    }

    /// `TASK name '(' [SINGLE := src ,] [INTERVAL := src ,] PRIORITY := int ')' ';'`
    fn task_decl(&mut self) -> Option<TaskDecl> {
        let start = self.span();
        self.bump();
        let name = self.expect_ident("a task name")?;
        let mut single = None;
        let mut interval = None;
        let mut priority = None;
        if self.expect(TokenKind::LParen) {
            loop {
                if self.at(TokenKind::RParen) {
                    break;
                }
                let key = self.cur_upper();
                let kspan = self.span();
                if !self.at(TokenKind::Ident) {
                    self.expected("SINGLE, INTERVAL or PRIORITY");
                    break;
                }
                self.bump();
                self.expect(TokenKind::Assign);
                let v = self.expr();
                match key.as_str() {
                    "SINGLE" => single = Some(v),
                    "INTERVAL" => interval = Some(v),
                    "PRIORITY" => priority = Some(v),
                    _ => self.diags.push(Diagnostic::syntax(format!("unknown task parameter '{}'", key), kspan)),
                }
                if !self.eat(TokenKind::Comma) {
                    break;
                }
            }
            self.expect(TokenKind::RParen);
        }
        if priority.is_none() {
            self.diags.push(Diagnostic::syntax("a task declaration shall specify PRIORITY", start).with_clause("B.1.7"));
        }
        let end = self.prev_span();
        self.expect(TokenKind::Semi);
        Some(TaskDecl { name, single, interval, priority, span: start.to(end) })
    }

    fn program_config(&mut self) -> Option<ProgramConfig> {
        let start = self.span();
        self.bump();
        let retain = if self.eat_kw(Keyword::Retain) {
            Some(VarQualifier::Retain)
        } else if self.eat_kw(Keyword::NonRetain) {
            Some(VarQualifier::NonRetain)
        } else {
            None
        };
        let name = self.expect_ident("a program instance name")?;
        let task = if self.eat_kw(Keyword::With) { self.expect_ident("a task name") } else { None };
        self.expect(TokenKind::Colon);
        let type_name = self.expect_ident("a program type name")?;
        let mut elements = Vec::new();
        if self.eat(TokenKind::LParen) {
            loop {
                if self.at(TokenKind::RParen) {
                    break;
                }
                let es = self.span();
                let target = self.variable_expr();
                if self.eat_kw(Keyword::With) {
                    if let Some(t) = self.expect_ident("a task name") {
                        elements.push(ProgConfElement::FbTask { fb: target, task: t, span: es.to(self.prev_span()) });
                    }
                } else if self.eat(TokenKind::Assign) {
                    let source = self.expr();
                    match target.kind {
                        ExprKind::Name(var) => elements.push(ProgConfElement::Input { var, source, span: es.to(self.prev_span()) }),
                        _ => self.diags.push(Diagnostic::syntax("expected a program input name", target.span)),
                    }
                } else if self.eat(TokenKind::Arrow) {
                    let sink = self.expr();
                    match target.kind {
                        ExprKind::Name(var) => elements.push(ProgConfElement::Output { var, sink, span: es.to(self.prev_span()) }),
                        _ => self.diags.push(Diagnostic::syntax("expected a program output name", target.span)),
                    }
                } else {
                    self.expected("WITH, ':=' or '=>'");
                    while !self.at_eof() && !self.at(TokenKind::Comma) && !self.at(TokenKind::RParen) {
                        self.bump();
                    }
                }
                if !self.eat(TokenKind::Comma) {
                    break;
                }
            }
            self.expect(TokenKind::RParen);
        }
        let end = self.prev_span();
        self.expect(TokenKind::Semi);
        Some(ProgramConfig { name, retain, task, type_name, elements, span: start.to(end) })
    }
}
