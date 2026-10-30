//! Instruction List (3.2).
//!
//! Each instruction is translated into the equivalent ST construct over the
//! "current result" (CR), which is held in a typed frame slot and referred
//! to from syntax trees through the synthetic variable `$CR`; untyped
//! literals stay symbolic until their type is known from their use. A dry
//! run determines the type of the current result at labels reached by
//! jumps.

use super::*;
use std::collections::{HashMap, HashSet};
use structure_syntax::ast::*;
use structure_syntax::diag::Diagnostic;
use structure_syntax::Ident;

#[derive(Clone, Debug)]
pub enum Cr {
    Undef,
    /// An untyped literal (or folded constant) not yet materialised.
    Lit(Expr),
    /// A value of the given type in the CR slot of that type.
    Val(TypeId),
}

#[derive(Default)]
struct IlCtx {
    labels: HashMap<String, u32>,
    label_types: HashMap<String, Cr>,
    live: HashSet<String>,
    slots: HashMap<TypeId, Loc>,
    final_pass: bool,
}

fn uses_cr(op: &IlOp) -> Option<bool> {
    Some(match op {
        IlOp::None => return None,
        IlOp::Simple { op, .. } => !matches!(op, IlOperator::Ld | IlOperator::Ldn),
        IlOp::Deferred { .. } | IlOp::Function { .. } => true,
        IlOp::FormalFunction { .. } => false,
        IlOp::Jump { cond, .. } | IlOp::Call { cond, .. } | IlOp::Return { cond } => *cond != IlCond::Always,
    })
}

fn is_untyped_literal(e: &Expr) -> bool {
    match &e.kind {
        ExprKind::Literal(l) => l.type_prefix.is_none() && matches!(l.kind, LiteralKind::Int { .. } | LiteralKind::Real { .. }),
        ExprKind::Unary { op: UnaryOp::Neg, operand } => is_untyped_literal(operand),
        ExprKind::Paren(e) => is_untyped_literal(e),
        _ => false,
    }
}

fn lit_expr(v: &VK, span: Span) -> Expr {
    let kind = match v {
        VK::IntLit(i) => {
            let lit = Literal { kind: LiteralKind::Int { value: i.unsigned_abs(), negative: false, based: false }, type_prefix: None };
            let e = Expr { kind: ExprKind::Literal(lit), span };
            if *i < 0 {
                return Expr { kind: ExprKind::Unary { op: UnaryOp::Neg, operand: Box::new(e) }, span };
            }
            return e;
        }
        VK::RealLit(f) => {
            let lit = Literal { kind: LiteralKind::Real { text: format!("{:?}", f.abs()), negative: false }, type_prefix: None };
            let e = Expr { kind: ExprKind::Literal(lit), span };
            if *f < 0.0 {
                return Expr { kind: ExprKind::Unary { op: UnaryOp::Neg, operand: Box::new(e) }, span };
            }
            return e;
        }
        _ => ExprKind::Error,
    };
    Expr { kind, span }
}

/// Standard function block input operators of table 54.
const STD_INPUT_OPERATORS: &[(&str, &[&str])] = &[
    ("SR", &["S1", "R"]),
    ("RS", &["S", "R1"]),
    ("R_TRIG", &["CLK"]),
    ("F_TRIG", &["CLK"]),
    ("CTU", &["CU", "R", "PV"]),
    ("CTD", &["CD", "PV"]),
    ("CTUD", &["CU", "CD", "R", "PV"]),
    ("TP", &["IN", "PT"]),
    ("TON", &["IN", "PT"]),
    ("TOF", &["IN", "PT"]),
];

impl<'c, 's> Lowerer<'c, 's> {
    pub fn il_body(&mut self, out: &mut Vec<St>, list: &[IlInstr]) {
        self.il_run(out, list);
    }

    /// Lowers an IL list and returns the final current result as a BOOL
    /// expression (for transition conditions, 2.6.3).
    pub fn il_condition(&mut self, out: &mut Vec<St>, list: &[IlInstr], span: Span) -> Ex {
        let cr = self.il_run(out, list);
        match self.cr_expr(&cr, span) {
            Some(e) => {
                let b = self.bool_ty();
                let v = self.expr(&e, Some(b));
                self.coerce(v, b, span)
            }
            None => Ex::bool(false),
        }
    }

    fn il_run(&mut self, out: &mut Vec<St>, list: &[IlInstr]) -> Cr {
        let mut cx = IlCtx::default();
        // labels and their liveness
        for (i, ins) in list.iter().enumerate() {
            if let Some(l) = &ins.label {
                let key = l.key();
                if cx.labels.contains_key(&key) {
                    self.cx.error_clause("3.2.1", format!("duplicate label {}", l), l.span);
                    continue;
                }
                let id = self.label();
                cx.labels.insert(key.clone(), id);
                let live = list[i..].iter().find_map(|x| uses_cr(&x.op)).unwrap_or(false);
                if live {
                    cx.live.insert(key);
                }
            }
        }
        // dry run to infer the current result type at labels
        let saved_diags = self.cx.diags.items.len();
        let saved_frame = self.frame_top;
        let saved_synth = self.synth.clone();
        let mut scratch = Vec::new();
        let _ = self.il_seq(&mut scratch, list, &mut cx, Cr::Undef);
        self.cx.diags.items.truncate(saved_diags);
        self.frame_top = saved_frame;
        self.synth = saved_synth;
        cx.slots.clear();
        cx.final_pass = true;
        self.il_seq(out, list, &mut cx, Cr::Undef)
    }

    fn cr_slot(&mut self, cx: &mut IlCtx, t: TypeId) -> Loc {
        if let Some(l) = cx.slots.get(&t) {
            return l.clone();
        }
        let l = self.temp_typed(t);
        cx.slots.insert(t, l.clone());
        l
    }

    fn cr_expr(&mut self, cr: &Cr, span: Span) -> Option<Expr> {
        match cr {
            Cr::Undef => {
                self.cx.error_clause("3.2.2", "the current result is undefined here (an LD instruction is missing)", span);
                None
            }
            Cr::Lit(e) => Some(e.clone()),
            Cr::Val(_) => Some(Self::synth_expr("$CR", span)),
        }
    }

    /// Makes `$CR` refer to the slot of the current result.
    fn bind_cr(&mut self, cx: &mut IlCtx, cr: &Cr) {
        if let Cr::Val(t) = cr {
            let slot = self.cr_slot(cx, *t);
            self.synth.insert("$CR".into(), (slot, *t));
        }
    }

    /// Evaluates `e` as the new current result.
    fn set_cr(&mut self, out: &mut Vec<St>, cx: &mut IlCtx, e: &Expr, expected: Option<TypeId>) -> Cr {
        if is_untyped_literal(e) {
            return Cr::Lit(e.clone());
        }
        let v = self.expr(e, expected);
        if v.is_err() {
            return Cr::Undef;
        }
        if v.is_literal() {
            return Cr::Lit(lit_expr(&v.kind, e.span));
        }
        let t = v.ty;
        let slot = self.cr_slot(cx, t);
        if let Some((wide, len)) = self.cx.types.is_string(t) {
            // keep the slot initialised for the string natives
            out.push(St::Native(structure_runtime::image::Native::StrInit { wide, cap: len }, vec![Ex::Addr(slot.clone())]));
        }
        self.assign(out, slot, t, v, e.span);
        Cr::Val(t)
    }

    fn merge(&mut self, a: &Cr, b: &Cr, at: Span, report: bool) -> Cr {
        match (a, b) {
            (Cr::Undef, x) | (x, Cr::Undef) => x.clone(),
            (Cr::Lit(_), Cr::Val(t)) | (Cr::Val(t), Cr::Lit(_)) => Cr::Val(*t),
            (Cr::Lit(e), Cr::Lit(_)) => Cr::Lit(e.clone()),
            (Cr::Val(t), Cr::Val(u)) => {
                if self.cx.types.compatible(*t, *u) {
                    Cr::Val(*t)
                } else {
                    if report {
                        self.cx.annex_e(
                            "3.2.2",
                            format!(
                                "the current result has different data types ({} and {}) on the paths reaching this label",
                                self.cx.types.name(*t),
                                self.cx.types.name(*u)
                            ),
                            at,
                        );
                    }
                    Cr::Val(*t)
                }
            }
        }
    }

    /// Converts the current result to the given state (type).
    fn cr_convert(&mut self, out: &mut Vec<St>, cx: &mut IlCtx, cr: Cr, target: &Cr, span: Span) -> Cr {
        match (&cr, target) {
            (Cr::Val(a), Cr::Val(b)) if a == b => cr,
            (Cr::Undef, _) => cr,
            (_, Cr::Val(t)) => {
                let t = *t;
                self.bind_cr(cx, &cr);
                let Some(e) = self.cr_expr(&cr, span) else { return Cr::Undef };
                let v = self.expr(&e, Some(t));
                let slot = self.cr_slot(cx, t);
                self.assign(out, slot, t, v, span);
                Cr::Val(t)
            }
            _ => cr,
        }
    }

    /// Type expected for the current result by an instruction (its operand
    /// type), used to type literal-only computations.
    fn consumer_hint(&mut self, ins: Option<&IlInstr>) -> Option<TypeId> {
        let ins = ins?;
        let IlOp::Simple { op, operand: Some(o) } = &ins.op else { return None };
        if matches!(op, IlOperator::Ld | IlOperator::Ldn) || is_untyped_literal(o) {
            return None;
        }
        let saved = self.cx.diags.items.len();
        let p = self.place(o, false);
        self.cx.diags.items.truncate(saved);
        let t = p?.ty;
        if matches!(op, IlOperator::Gt | IlOperator::Ge | IlOperator::Eq | IlOperator::Lt | IlOperator::Le | IlOperator::Ne | IlOperator::St | IlOperator::Stn)
            || self.cx.types.scalar(t).is_some()
        {
            Some(t)
        } else {
            None
        }
    }

    fn il_seq(&mut self, out: &mut Vec<St>, list: &[IlInstr], cx: &mut IlCtx, mut cr: Cr) -> Cr {
        for (idx, ins) in list.iter().enumerate() {
            self.il_hint = self.consumer_hint(list.get(idx + 1));
            out.push(St::At(ins.span));
            if let Some(l) = &ins.label {
                let key = l.key();
                if let Some(&id) = cx.labels.get(&key) {
                    if cx.live.contains(&key) {
                        let known = cx.label_types.get(&key).cloned().unwrap_or(Cr::Undef);
                        let t = self.merge(&cr, &known, l.span, cx.final_pass);
                        let _ = self.cr_convert(out, cx, cr, &t, l.span);
                        cx.label_types.insert(key, t.clone());
                        out.push(St::Label(id));
                        cr = t;
                    } else {
                        out.push(St::Label(id));
                        cr = Cr::Undef;
                    }
                }
            }
            cr = self.il_instr(out, ins, cx, cr);
        }
        cr
    }

    fn il_instr(&mut self, out: &mut Vec<St>, ins: &IlInstr, cx: &mut IlCtx, cr: Cr) -> Cr {
        let span = ins.span;
        self.bind_cr(cx, &cr);
        match &ins.op {
            IlOp::None => cr,
            IlOp::Simple { op, operand } => self.il_simple(out, cx, *op, operand.as_ref(), cr, span),
            IlOp::Deferred { op, operand, body } => {
                // save the current result in its own temporary
                let saved = match &cr {
                    Cr::Lit(e) => e.clone(),
                    Cr::Val(t) => {
                        let t = *t;
                        let tmp = self.temp_typed(t);
                        let name = self.synth_var("S", tmp.clone(), t);
                        let v = V::mem(self.cr_slot(cx, t), t);
                        self.assign(out, tmp, t, v, span);
                        Self::synth_expr(&name, span)
                    }
                    Cr::Undef => {
                        self.cx.error_clause("3.2.2", "the current result is undefined here", span);
                        return Cr::Undef;
                    }
                };
                let mut inner = match operand {
                    Some(o) => self.set_cr(out, cx, o, None),
                    None => Cr::Undef,
                };
                inner = self.il_seq(out, body, cx, inner);
                self.bind_cr(cx, &inner);
                let Some(rhs) = self.cr_expr(&inner, span) else { return Cr::Undef };
                let e = self.il_binary(*op, saved, rhs, span);
                self.set_cr(out, cx, &e, None)
            }
            IlOp::Function { name, operands } => {
                let Some(first) = self.cr_expr(&cr, span) else { return Cr::Undef };
                let mut args = vec![Arg::Positional(first)];
                args.extend(operands.iter().map(|o| Arg::Positional(o.clone())));
                let call = Expr { kind: ExprKind::Call(CallExpr { callee: Box::new(Expr { kind: ExprKind::Name(name.clone()), span: name.span }), args, span }), span };
                let hint = self.il_hint;
                self.set_cr(out, cx, &call, hint)
            }
            IlOp::FormalFunction { name, params } => {
                let hint = self.il_hint;
                let args = self.il_params(out, cx, params);
                let call = Expr { kind: ExprKind::Call(CallExpr { callee: Box::new(Expr { kind: ExprKind::Name(name.clone()), span: name.span }), args, span }), span };
                self.set_cr(out, cx, &call, hint)
            }
            IlOp::Jump { cond, label } => {
                if self.region == Region::Transition {
                    self.cx.annex_e("2.6.3", "jumps are not allowed in the evaluation of a transition condition", span);
                }
                let key = label.key();
                let Some(&id) = cx.labels.get(&key) else {
                    self.cx.error_clause("3.2.1", format!("undefined label {}", label), label.span);
                    return cr;
                };
                let mut cr = cr;
                let cond_ex = match cond {
                    IlCond::Always => None,
                    c => {
                        let Some(e) = self.cr_expr(&cr, span) else { return cr };
                        let b = self.bool_expr(&e);
                        Some(if *c == IlCond::IfTrue { b } else { Ex::not(b) })
                    }
                };
                if cx.live.contains(&key) {
                    let known = cx.label_types.get(&key).cloned().unwrap_or(Cr::Undef);
                    let t = self.merge(&cr, &known, label.span, cx.final_pass);
                    cx.label_types.insert(key, t.clone());
                    if cond_ex.is_some() && !matches!((&cr, &t), (Cr::Val(a), Cr::Val(b)) if a == b) && matches!(t, Cr::Val(_)) {
                        // conditional jump with a conversion only on the taken path
                        let mut taken = Vec::new();
                        let c = cond_ex.clone().unwrap();
                        let _ = self.cr_convert(&mut taken, cx, cr.clone(), &t, span);
                        taken.push(St::Jump(id));
                        out.push(St::If(c, taken, vec![]));
                        return cr;
                    }
                    cr = self.cr_convert(out, cx, cr, &t, span);
                }
                match cond_ex {
                    None => {
                        out.push(St::Jump(id));
                        Cr::Undef
                    }
                    Some(c) => {
                        out.push(St::JumpIf(c, id));
                        cr
                    }
                }
            }
            IlOp::Return { cond } => {
                match cond {
                    IlCond::Always => out.push(St::Jump(self.return_label)),
                    c => {
                        let Some(e) = self.cr_expr(&cr, span) else { return cr };
                        let b = self.bool_expr(&e);
                        out.push(St::JumpIf(if *c == IlCond::IfTrue { b } else { Ex::not(b) }, self.return_label));
                    }
                }
                if self.region == Region::Transition {
                    self.cx.error("RET is not allowed in a transition condition", span);
                }
                cr
            }
            IlOp::Call { cond, instance, args } => {
                // The condition is evaluated before the argument list.
                let cond_ex = match cond {
                    IlCond::Always => None,
                    c => {
                        let Some(e) = self.cr_expr(&cr, span) else { return cr };
                        let b = self.bool_expr(&e);
                        let t = self.temp(8);
                        out.push(St::Store(Ty::Bool, t.clone(), if *c == IlCond::IfTrue { b } else { Ex::not(b) }));
                        Some(Ex::Load(Ty::Bool, t))
                    }
                };
                let args = match args {
                    IlCallArgs::None => vec![],
                    IlCallArgs::Formal(p) => self.il_params(out, cx, p),
                    IlCallArgs::NonFormal(ops) => ops.iter().map(|o| Arg::Positional(o.clone())).collect(),
                };
                let call = CallExpr { callee: Box::new(instance.clone()), args, span };
                match cond_ex {
                    None => self.fb_call(out, &call, span),
                    Some(c) => {
                        let mut body = Vec::new();
                        self.fb_call(&mut body, &call, span);
                        out.push(St::If(c, body, vec![]));
                    }
                }
                self.bind_cr(cx, &cr);
                cr
            }
        }
    }

    fn il_binary(&mut self, op: IlOperator, lhs: Expr, rhs: Expr, span: Span) -> Expr {
        use IlOperator::*;
        let (bop, negate) = match op {
            And => (BinaryOp::And, false),
            Andn => (BinaryOp::And, true),
            Or => (BinaryOp::Or, false),
            Orn => (BinaryOp::Or, true),
            Xor => (BinaryOp::Xor, false),
            Xorn => (BinaryOp::Xor, true),
            Add => (BinaryOp::Add, false),
            Sub => (BinaryOp::Sub, false),
            Mul => (BinaryOp::Mul, false),
            Div => (BinaryOp::Div, false),
            Mod => (BinaryOp::Mod, false),
            Gt => (BinaryOp::Gt, false),
            Ge => (BinaryOp::Ge, false),
            Eq => (BinaryOp::Eq, false),
            Lt => (BinaryOp::Lt, false),
            Le => (BinaryOp::Le, false),
            Ne => (BinaryOp::Ne, false),
            _ => unreachable!(),
        };
        let rhs = if negate { Expr { kind: ExprKind::Unary { op: UnaryOp::Not, operand: Box::new(rhs) }, span } } else { rhs };
        Expr { kind: ExprKind::Binary { op: bop, lhs: Box::new(lhs), rhs: Box::new(rhs) }, span }
    }

    fn il_simple(&mut self, out: &mut Vec<St>, cx: &mut IlCtx, op: IlOperator, operand: Option<&Expr>, cr: Cr, span: Span) -> Cr {
        use IlOperator::*;
        let needs_operand = !matches!(op, Not);
        if needs_operand && operand.is_none() {
            self.cx.error_clause("3.2.2", format!("{} requires an operand", op.as_str()), span);
            return cr;
        }
        if !needs_operand && operand.is_some() {
            self.cx.error_clause("3.2.2", "NOT has no operand (table 52, footnote d)", span);
        }
        match op {
            Ld => self.set_cr(out, cx, operand.unwrap(), None),
            Ldn => {
                let o = operand.unwrap();
                let e = Expr { kind: ExprKind::Unary { op: UnaryOp::Not, operand: Box::new(o.clone()) }, span };
                self.set_cr(out, cx, &e, None)
            }
            St | Stn => {
                let target = operand.unwrap();
                let Some(v) = self.cr_expr(&cr, span) else { return cr };
                let v = if op == Stn { Expr { kind: ExprKind::Unary { op: UnaryOp::Not, operand: Box::new(v) }, span } } else { v };
                self.assignment(out, target, &v);
                cr
            }
            Not => {
                let Some(v) = self.cr_expr(&cr, span) else { return cr };
                let e = Expr { kind: ExprKind::Unary { op: UnaryOp::Not, operand: Box::new(v) }, span };
                self.set_cr(out, cx, &e, None)
            }
            S | R | S1 | R1 | Clk | Cu | Cd | Pv | In | Pt => {
                let o = operand.unwrap();
                // A function block instance operand: input operator (table 54).
                let is_fb = {
                    let saved = self.cx.diags.items.len();
                    let p = self.place(o, false);
                    self.cx.diags.items.truncate(saved);
                    p.and_then(|p| self.cx.types.instance_pou(p.ty))
                };
                if let Some(fb) = is_fb {
                    let input = op.as_str();
                    let fb_name = self.cx.pous[fb].name.clone();
                    let standard = STD_INPUT_OPERATORS.iter().any(|(n, ins)| (fb_name == *n || fb_name.starts_with(&format!("{}_", n))) && ins.contains(&input));
                    if !standard {
                        if self.cx.pous[fb].var(input).map(|v| v.kind == VKind::Input).unwrap_or(false) {
                            self.cx.extension(format!("use of {} as an input operator for {} is an extension (table 54)", input, fb_name), span);
                        } else {
                            self.cx.error_clause("3.2.3", format!("{} is not an input operator of {}", input, fb_name), span);
                            return cr;
                        }
                    }
                    let Some(v) = self.cr_expr(&cr, span) else { return cr };
                    let call = CallExpr { callee: Box::new(o.clone()), args: vec![Arg::Input { name: Ident::new(input, span), value: v }], span };
                    self.fb_call(out, &call, span);
                    return cr;
                }
                if !matches!(op, S | R) {
                    self.cx.error_clause("3.2.3", format!("the operand of {} shall be a function block instance (table 54)", op.as_str()), o.span);
                    return cr;
                }
                // S / R: set or reset a BOOL operand if the current result is 1.
                let Some(v) = self.cr_expr(&cr, span) else { return cr };
                let value = Expr {
                    kind: ExprKind::Literal(Literal { kind: LiteralKind::Bool(op == S), type_prefix: None }),
                    span,
                };
                let stmt = Stmt { kind: StmtKind::If { branches: vec![(v, vec![Stmt { kind: StmtKind::Assign { target: o.clone(), value }, span }])], else_body: None }, span };
                if let Some(p) = {
                    let saved = self.cx.diags.items.len();
                    let p = self.place(o, false);
                    self.cx.diags.items.truncate(saved);
                    p
                } {
                    if !self.cx.types.is_bool(p.ty) {
                        self.cx.diags.push(Diagnostic::annex_e("3.2.2", "the operand of S and R shall be of type BOOL (table 52, footnote e)", o.span));
                    }
                }
                self.stmts(out, std::slice::from_ref(&stmt));
                cr
            }
            And | Andn | Or | Orn | Xor | Xorn | Add | Sub | Mul | Div | Mod | Gt | Ge | Eq | Lt | Le | Ne => {
                let Some(lhs) = self.cr_expr(&cr, span) else { return cr };
                let e = self.il_binary(op, lhs, operand.unwrap().clone(), span);
                self.set_cr(out, cx, &e, None)
            }
        }
    }

    /// Converts IL formal parameters to call arguments; nested instruction
    /// lists are evaluated into temporaries.
    fn il_params(&mut self, out: &mut Vec<St>, cx: &mut IlCtx, params: &[IlParam]) -> Vec<Arg> {
        let mut args = Vec::new();
        for p in params {
            match p {
                IlParam::Input { name, value: IlParamValue::Operand(e), .. } => args.push(Arg::Input { name: name.clone(), value: e.clone() }),
                IlParam::Input { name, value: IlParamValue::Nested(list), span } => {
                    let saved_cr = self.synth.get("$CR").cloned();
                    let r = self.il_seq(out, list, cx, Cr::Undef);
                    self.bind_cr(cx, &r);
                    let e = match &r {
                        Cr::Lit(e) => e.clone(),
                        Cr::Val(t) => {
                            let t = *t;
                            let tmp = self.temp_typed(t);
                            let slot = self.cr_slot(cx, t);
                            self.assign(out, tmp.clone(), t, V::mem(slot, t), *span);
                            let n = self.synth_var("P", tmp, t);
                            Self::synth_expr(&n, *span)
                        }
                        Cr::Undef => Expr { kind: ExprKind::Error, span: *span },
                    };
                    if let Some(s) = saved_cr {
                        self.synth.insert("$CR".into(), s);
                    }
                    args.push(Arg::Input { name: name.clone(), value: e });
                }
                IlParam::Output { name, target, negated, span } => {
                    args.push(Arg::Output { name: name.clone(), target: Some(target.clone()), negated: *negated, span: *span })
                }
            }
        }
        args
    }
}
