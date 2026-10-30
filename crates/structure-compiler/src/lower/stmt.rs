//! Structured Text statements (3.3.2).

use super::*;
use structure_runtime::image::CmpOp;
use structure_syntax::ast::{self, CaseLabel, ExprKind, Stmt, StmtKind};

impl<'c, 's> Lowerer<'c, 's> {
    pub fn stmts(&mut self, out: &mut Vec<St>, list: &[Stmt]) {
        for s in list {
            self.stmt(out, s);
        }
    }

    fn stmt(&mut self, out: &mut Vec<St>, s: &Stmt) {
        out.push(St::At(s.span));
        match &s.kind {
            StmtKind::Empty => {}
            StmtKind::Assign { target, value } => self.assignment(out, target, value),
            StmtKind::Call(c) => self.fb_call(out, c, s.span),
            StmtKind::Return => {
                if self.region == Region::Transition {
                    self.cx.error("RETURN is not allowed in a transition condition", s.span);
                }
                out.push(St::Jump(self.return_label));
            }
            StmtKind::Exit => match self.loops.last() {
                Some(l) => out.push(St::Jump(*l)),
                None => self.cx.error_clause("3.3.2.4", "EXIT shall be located within an iteration statement", s.span),
            },
            StmtKind::If { branches, else_body } => {
                let end = self.label();
                for (cond, body) in branches {
                    let next = self.label();
                    let c = self.bool_expr(cond);
                    out.push(St::JumpIfNot(c, next));
                    self.stmts(out, body);
                    out.push(St::Jump(end));
                    out.push(St::Label(next));
                }
                if let Some(b) = else_body {
                    self.stmts(out, b);
                }
                out.push(St::Label(end));
            }
            StmtKind::Case { selector, arms, else_body } => self.case(out, selector, arms, else_body.as_deref(), s.span),
            StmtKind::For { var, from, to, by, body } => self.for_loop(out, var, from, to, by.as_ref(), body, s.span),
            StmtKind::While { cond, body } => {
                let top = self.label();
                let end = self.label();
                out.push(St::Label(top));
                let c = self.bool_expr(cond);
                out.push(St::JumpIfNot(c, end));
                self.loops.push(end);
                self.stmts(out, body);
                self.loops.pop();
                out.push(St::Jump(top));
                out.push(St::Label(end));
            }
            StmtKind::Repeat { body, until } => {
                let top = self.label();
                let end = self.label();
                out.push(St::Label(top));
                self.loops.push(end);
                self.stmts(out, body);
                self.loops.pop();
                let c = self.bool_expr(until);
                out.push(St::JumpIfNot(c, top));
                out.push(St::Label(end));
            }
        }
    }

    pub fn assignment(&mut self, out: &mut Vec<St>, target: &ast::Expr, value: &ast::Expr) {
        let Some(p) = self.target(target) else {
            // still check the value
            let _ = self.expr(value, None);
            return;
        };
        let v = self.expr(value, Some(p.ty));
        if p.is_result {
            self.mark_assigned(out);
        }
        self.assign(out, p.loc, p.ty, v, value.span);
    }

    /// Records that the function result has been assigned (3.3.2.1).
    pub fn mark_assigned(&mut self, out: &mut Vec<St>) {
        if let Some(a) = self.cx.pous[self.pou].var("__ASSIGNED") {
            out.push(St::Store(Ty::Bool, a.loc(), Ex::bool(true)));
        }
    }

    fn case(&mut self, out: &mut Vec<St>, selector: &ast::Expr, arms: &[ast::CaseArm], else_body: Option<&[Stmt]>, span: Span) {
        let v = self.expr(selector, None);
        let v = self.settle(v, selector.span);
        let ty = v.ty;
        let is_enum = self.cx.types.is_enum(ty);
        let is_int = self.cx.types.elem_of(ty).map(|e| e.is_int()).unwrap_or(false);
        if !is_enum && !is_int && !self.cx.types.is_error(ty) {
            self.cx.error_clause("3.3.2.3", "the selector of a CASE statement shall be of type ANY_INT or of an enumerated type", selector.span);
        }
        let mt = self.mty(ty);
        let (ex, _) = self.scalar(v, selector.span);
        let sel = self.temp(64);
        out.push(St::Store(mt, sel.clone(), ex));
        let end = self.label();
        let mut seen: Vec<(i128, i128)> = Vec::new();
        for arm in arms {
            let body_label = self.label();
            let next = self.label();
            for l in &arm.labels {
                let (lo, hi) = match l {
                    CaseLabel::Value(e) => {
                        let c = self.case_label(e, ty);
                        (c, c)
                    }
                    CaseLabel::Range(a, b) => {
                        if is_enum {
                            self.cx.error_clause("3.3.2.3", "ranges are only allowed for integer selectors", a.span.to(b.span));
                        }
                        (self.case_label(a, ty), self.case_label(b, ty))
                    }
                };
                if seen.iter().any(|(a, b)| lo <= *b && hi >= *a) {
                    self.cx.warning("this CASE label overlaps an earlier one; the first matching group is executed", arm.span);
                }
                seen.push((lo, hi));
                let load = || Ex::Load(mt, sel.clone());
                let enc = |v: i128| structure_runtime::ty::from_i128(mt, v).unwrap_or(0);
                if lo == hi {
                    let c = Ex::Cmp(CmpOp::Eq, mt, Box::new(load()), Box::new(Ex::Const(mt, enc(lo))));
                    out.push(St::JumpIf(c, body_label));
                } else {
                    let c1 = Ex::Cmp(CmpOp::Ge, mt, Box::new(load()), Box::new(Ex::Const(mt, enc(lo))));
                    let c2 = Ex::Cmp(CmpOp::Le, mt, Box::new(load()), Box::new(Ex::Const(mt, enc(hi))));
                    out.push(St::JumpIf(Ex::and(c1, c2), body_label));
                }
            }
            out.push(St::Jump(next));
            out.push(St::Label(body_label));
            self.stmts(out, &arm.body);
            out.push(St::Jump(end));
            out.push(St::Label(next));
        }
        if let Some(b) = else_body {
            self.stmts(out, b);
        }
        out.push(St::Label(end));
        let _ = span;
    }

    fn case_label(&mut self, e: &ast::Expr, ty: TypeId) -> i128 {
        if self.cx.types.is_enum(ty) {
            let v = self.expr(e, Some(ty));
            if !v.is_err() && !self.cx.types.compatible(v.ty, ty) {
                self.cx.error_clause("3.3.2.3", "the case label is not a value of the selector's type", e.span);
            }
            return match v.kind {
                VK::Val(Ex::Const(_, x)) => x as i128,
                _ => 0,
            };
        }
        match self.cx.const_eval(e, Some(ty)) {
            Some(c) => match c.value {
                ConstValue::Int(i) => {
                    if let Some(el) = self.cx.types.elem_of(ty) {
                        let (lo, hi) = el.ty().int_range();
                        if i < lo || i > hi {
                            self.cx.error_clause("3.3.2.3", format!("case label {} is outside the range of {}", i, el.name()), e.span);
                        }
                    }
                    i
                }
                _ => {
                    self.cx.error_clause("3.3.2.3", "case labels shall be integer literals or enumerated values", e.span);
                    0
                }
            },
            None => 0,
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn for_loop(&mut self, out: &mut Vec<St>, var: &structure_syntax::Ident, from: &ast::Expr, to: &ast::Expr, by: Option<&ast::Expr>, body: &[Stmt], span: Span) {
        let ve = ast::Expr { kind: ExprKind::Name(var.clone()), span: var.span };
        let Some(p) = self.target(&ve) else { return };
        let Some(el) = self.cx.types.elem_of(p.ty).filter(|e| e.is_int()) else {
            self.cx.error_clause("3.3.2.4", "the control variable of a FOR loop shall be of an integer type", var.span);
            return;
        };
        if p.var.map(|vi| self.cx.pous[self.pou].vars[vi].is_ref()).unwrap_or(false) {
            // allowed, nothing special
        }
        let ty = p.ty;
        let base = self.elem_ty(el);
        let mt = el.ty();
        let fv = self.expr(from, Some(ty));
        let tv = self.expr(to, Some(ty));
        for (v, e) in [(&fv, from), (&tv, to)] {
            if !v.is_literal() && !v.is_err() && !self.cx.types.compatible(v.ty, ty) {
                self.cx.error_clause("3.3.2.4", "the control variable, initial value and final value of a FOR loop shall be of the same integer type", e.span);
            }
        }
        let init = self.coerce(fv, ty, from.span);
        let fin = self.coerce(tv, base, to.span);
        let step = match by {
            Some(b) => {
                let v = self.expr(b, Some(base));
                self.coerce(v, base, b.span)
            }
            None => self.const_to(ConstValue::Int(1), base, span),
        };
        // Evaluate the final value and the increment once, in 64 bits.
        let fin_t = self.temp(64);
        let step_t = self.temp(64);
        let wide = if el.is_unsigned_int() && el.width() == 64 { Ty::U64 } else { Ty::I64 };
        out.push(St::Store(mt, p.loc.clone(), init));
        out.push(St::Store(wide, fin_t.clone(), Ex::Conv(mt, wide, Box::new(fin))));
        out.push(St::Store(Ty::I64, step_t.clone(), Ex::Conv(mt, Ty::I64, Box::new(step))));
        let top = self.label();
        let end = self.label();
        let body_l = self.label();
        let cur = || Ex::Conv(mt, wide, Box::new(Ex::Load(mt, p.loc.clone())));
        out.push(St::Label(top));
        // termination test at the beginning of each iteration
        let pos = Ex::Cmp(CmpOp::Ge, Ty::I64, Box::new(Ex::Load(Ty::I64, step_t.clone())), Box::new(Ex::Const(Ty::I64, 0)));
        let up_ok = Ex::Cmp(CmpOp::Le, wide, Box::new(cur()), Box::new(Ex::Load(wide, fin_t.clone())));
        let down_ok = Ex::Cmp(CmpOp::Ge, wide, Box::new(cur()), Box::new(Ex::Load(wide, fin_t.clone())));
        out.push(St::JumpIfNot(Ex::Select(Box::new(pos), Box::new(up_ok), Box::new(down_ok)), end));
        out.push(St::Label(body_l));
        self.loops.push(end);
        self.for_vars.push(var.key());
        self.stmts(out, body);
        self.for_vars.pop();
        self.loops.pop();
        // increment without spurious overflow: stop when the next value
        // would pass the final value
        let next_t = self.temp(64);
        if wide == Ty::I64 && el.width() < 64 {
            out.push(St::Store(
                Ty::I64,
                next_t.clone(),
                Ex::Bin(BinOp::Add, Ty::I64, Box::new(cur()), Box::new(Ex::Load(Ty::I64, step_t.clone()))),
            ));
            let pos = Ex::Cmp(CmpOp::Ge, Ty::I64, Box::new(Ex::Load(Ty::I64, step_t.clone())), Box::new(Ex::Const(Ty::I64, 0)));
            let up_ok = Ex::Cmp(CmpOp::Le, Ty::I64, Box::new(Ex::Load(Ty::I64, next_t.clone())), Box::new(Ex::Load(Ty::I64, fin_t.clone())));
            let down_ok = Ex::Cmp(CmpOp::Ge, Ty::I64, Box::new(Ex::Load(Ty::I64, next_t.clone())), Box::new(Ex::Load(Ty::I64, fin_t.clone())));
            out.push(St::JumpIfNot(Ex::Select(Box::new(pos), Box::new(up_ok), Box::new(down_ok)), end));
            out.push(St::Store(mt, p.loc.clone(), Ex::Conv(Ty::I64, mt, Box::new(Ex::Load(Ty::I64, next_t)))));
        } else {
            let step = Ex::Conv(Ty::I64, mt, Box::new(Ex::Load(Ty::I64, step_t)));
            out.push(St::Store(mt, p.loc.clone(), Ex::Bin(BinOp::Add, mt, Box::new(Ex::Load(mt, p.loc.clone())), Box::new(step))));
        }
        out.push(St::Jump(top));
        out.push(St::Label(end));
        // check that the body does not assign the control variable
        let _ = body_l;
    }
}
