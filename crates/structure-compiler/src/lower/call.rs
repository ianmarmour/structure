//! Invocation of functions and function blocks (2.5.1.1, 2.5.2.1, 3.3.2.2)
//! and lowering of the standard functions (2.5.1.5).

use super::*;
use crate::stdlib::{self, Arith, Bitwise, Compare, ConvTy, Num1, Shift, StdFn, StrFn, Text, TimeFn};
use structure_runtime::error::TrapCode;
use structure_runtime::image::{CmpOp, MathFn, Native, TextKind};
use structure_runtime::strings::MAX_STRING_LENGTH;
use structure_runtime::ty::MICROS_PER_DAY;
use structure_syntax::ast::{self, Arg, BinaryOp, CallExpr, Expr, ExprKind};
use structure_syntax::Ident;

/// Arguments of an invocation bound to the formal parameters.
pub struct Bound<'a> {
    pub inputs: Vec<(usize, &'a Expr)>,
    pub inouts: Vec<(usize, &'a Expr)>,
    pub outputs: Vec<(usize, Option<&'a Expr>, bool)>,
    pub en: Option<&'a Expr>,
    pub eno: Option<(Option<&'a Expr>, bool)>,
}

impl<'c, 's> Lowerer<'c, 's> {
    // ------------------------------------------------------------ binding

    /// Binds actual arguments to the parameters of a user-declared POU
    /// (table 19a features 1 and 2).
    pub fn bind<'a>(&mut self, pi: usize, args: &'a [Arg], span: Span) -> Option<Bound<'a>> {
        let pou = self.cx.pous[pi].clone();
        let mut b = Bound { inputs: vec![], inouts: vec![], outputs: vec![], en: None, eno: None };
        let positional = args.iter().filter(|a| matches!(a, Arg::Positional(_))).count();
        if positional > 0 && positional != args.len() {
            self.cx.error_clause("2.5.1.1", "formal and non-formal arguments cannot be mixed in one invocation (table 19a)", span);
            return None;
        }
        if positional > 0 {
            // Parameters in declaration order, except EN and ENO.
            let params: Vec<usize> = pou
                .vars
                .iter()
                .enumerate()
                .filter(|(_, v)| matches!(v.kind, VKind::Input | VKind::InOut | VKind::Output) && !v.implicit && v.name != "EN" && v.name != "ENO")
                .map(|(i, _)| i)
                .collect();
            let n_in = params.iter().filter(|i| pou.vars[**i].kind != VKind::Output).count();
            if positional != n_in && positional != params.len() {
                self.cx.error_clause(
                    "2.5.1.1",
                    format!("{} expects {} arguments in a non-formal invocation, found {}", pou.name, n_in, positional),
                    span,
                );
                return None;
            }
            for (a, &vi) in args.iter().zip(&params) {
                let Arg::Positional(e) = a else { unreachable!() };
                match pou.vars[vi].kind {
                    VKind::Input => b.inputs.push((vi, e)),
                    VKind::InOut => b.inouts.push((vi, e)),
                    _ => b.outputs.push((vi, Some(e), false)),
                }
            }
        } else {
            for a in args {
                match a {
                    Arg::Input { name, value } => {
                        let key = name.key();
                        if key == "EN" {
                            b.en = Some(value);
                            continue;
                        }
                        match pou.var_map.get(&key).map(|i| (*i, &pou.vars[*i])) {
                            Some((vi, v)) if v.kind == VKind::Input => {
                                if b.inputs.iter().any(|(x, _)| *x == vi) {
                                    self.cx.error(format!("parameter {} is assigned more than once", name), name.span);
                                }
                                b.inputs.push((vi, value))
                            }
                            Some((vi, v)) if v.kind == VKind::InOut => b.inouts.push((vi, value)),
                            Some((_, v)) if v.kind == VKind::Output => {
                                self.cx.error_clause("2.5.1.1", format!("{} is an output; use '=>' to assign it to a variable", name), name.span);
                            }
                            _ => self.cx.error(format!("{} has no input parameter named {}", pou.name, name), name.span),
                        }
                    }
                    Arg::Output { name, target, negated, span } => {
                        let key = name.key();
                        if key == "ENO" {
                            b.eno = Some((target.as_ref(), *negated));
                            continue;
                        }
                        match pou.var_map.get(&key).map(|i| (*i, &pou.vars[*i])) {
                            Some((vi, v)) if v.kind == VKind::Output => {
                                if *negated && !self.cx.types.is_bool(v.ty) {
                                    self.cx.error("NOT can only be applied to BOOL outputs", *span);
                                }
                                b.outputs.push((vi, target.as_ref(), *negated))
                            }
                            _ => self.cx.error(format!("{} has no output parameter named {}", pou.name, name), name.span),
                        }
                    }
                    Arg::Positional(_) => unreachable!(),
                }
            }
        }
        Some(b)
    }

    /// Checks that an in-out actual argument is "properly mapped" (2.5.1.1
    /// rule 16) and returns its place.
    fn inout_place(&mut self, e: &Expr) -> Option<expr::Place> {
        let p = match &e.kind {
            ExprKind::Name(_) | ExprKind::Field { .. } | ExprKind::Index { .. } | ExprKind::DirectVar(_) | ExprKind::Paren(_) => self.place(e, true)?,
            _ => {
                self.cx.annex_e("2.5.1.1", "a VAR_IN_OUT argument shall be a variable (it is not properly mapped)", e.span);
                return None;
            }
        };
        let mut ok = p.read_only.is_none() && !p.fb_output;
        if let Some(vi) = p.var {
            let v = &self.cx.pous[self.pou].vars[vi];
            if v.constant || v.kind == VKind::Input {
                ok = false;
            }
        }
        if !ok {
            self.cx.annex_e(
                "2.5.1.1",
                "a VAR_IN_OUT argument shall be a non-constant variable of the invoking POU or a properly mapped in-out variable (it is not properly mapped)",
                e.span,
            );
        }
        Some(p)
    }

    fn output_target(&mut self, out: &mut Vec<St>, target: &Expr, src: Loc, ty: TypeId, negated: bool, span: Span) {
        let Some(tp) = self.target(target) else { return };
        let mt = self.mty(ty);
        let v = if negated { V::val(Ex::not(Ex::Load(mt, src)), ty) } else { V::mem(src, ty) };
        self.assign(out, tp.loc, tp.ty, v, span);
    }

    // ------------------------------------------------------------ user functions

    pub fn call_expr(&mut self, c: &CallExpr, expected: Option<TypeId>, span: Span) -> V {
        let Some(name) = c.callee_name().cloned() else {
            return self.err("invalid function invocation", span);
        };
        let key = name.key();
        if let Some(&pi) = self.cx.pou_by_name.get(&key) {
            if self.cx.pous[pi].kind == PouKind::Function {
                return self.call_function(pi, &c.args, span);
            }
            return self.err(format!("{} is not a function", name), name.span);
        }
        if self.cx.pous[self.pou].var_map.contains_key(&key) {
            return self.err("function block invocations are statements and cannot be used in expressions (3.3.2.2)", span);
        }
        if key.starts_with("__") && self.cx.in_std {
            return self.intrinsic(&key, &c.args, span);
        }
        match stdlib::parse(&key) {
            Some(f) => self.std_call(&name, f, &c.args, expected, span),
            None => self.err(format!("unknown function {}", name), name.span),
        }
    }

    fn intrinsic(&mut self, key: &str, _args: &[Arg], span: Span) -> V {
        match key {
            "__NOW" => V::val(Ex::Now, self.elem_ty(Elem::Time)),
            "__TRAP_CONTENTION" => V { kind: VK::Val(Ex::Const(Ty::Bool, 0)), ty: self.bool_ty(), pre: vec![St::Trap(TrapCode::ActionControlContention)] },
            _ => self.err(format!("unknown intrinsic {}", key), span),
        }
    }

    pub fn call_function(&mut self, pi: usize, args: &[Arg], span: Span) -> V {
        if self.region == Region::Transition {
            let has_out = self.cx.pous[pi].vars.iter().any(|v| matches!(v.kind, VKind::Output | VKind::InOut) && !v.implicit);
            if has_out && args.iter().any(|a| matches!(a, Arg::Output { .. })) {
                self.cx.annex_e("2.6.3", "side effects are not allowed in the evaluation of a transition condition", span);
            }
        }
        let Some(b) = self.bind(pi, args, span) else { return V::err(&self.cx.types) };
        let pou = self.cx.pous[pi].clone();
        let mut setup = Vec::new();
        // Inputs: provided values or default values (2.5.1.1).
        for (vi, v) in pou.vars.iter().enumerate() {
            if v.kind != VKind::Input || v.name == "EN" {
                continue;
            }
            let loc = Loc::Nf(v.offset() as u32);
            if v.is_ref() {
                // function block instance passed as input
                match b.inputs.iter().find(|(x, _)| *x == vi) {
                    Some((_, e)) => {
                        if let Some(p) = self.place(e, false) {
                            if !self.cx.types.compatible(p.ty, v.ty) {
                                self.cx.error(format!("expected an instance of {}", self.cx.types.name(v.ty)), e.span);
                            }
                            setup.push(St::Store(Ty::U64, loc, Ex::Addr(p.loc)));
                        }
                    }
                    None => self.cx.annex_e("2.5.2.2", format!("no value specified for the function block instance input {}", v.name), span),
                }
                continue;
            }
            match b.inputs.iter().find(|(x, _)| *x == vi) {
                Some((_, e)) => {
                    if self.cx.types.is_aggregate(v.ty) {
                        let init = self.cx.default_init(v.ty);
                        self.emit_init(&mut setup, loc.clone(), v.ty, Some(&init));
                    }
                    let val = self.expr(e, Some(v.ty));
                    self.assign(&mut setup, loc, v.ty, val, e.span);
                }
                None => {
                    let init = v.init.clone().unwrap_or_else(|| self.cx.default_init(v.ty));
                    self.emit_init(&mut setup, loc, v.ty, Some(&init));
                }
            }
        }
        for (vi, v) in pou.vars.iter().enumerate() {
            if v.kind != VKind::InOut {
                continue;
            }
            match b.inouts.iter().find(|(x, _)| *x == vi) {
                Some((_, e)) => {
                    if let Some(p) = self.inout_place(e) {
                        if !self.cx.types.compatible(p.ty, v.ty) {
                            self.cx.annex_e("3.3.1", format!("type mismatch for in-out {}: expected {}, found {}", v.name, self.cx.types.name(v.ty), self.cx.types.name(p.ty)), e.span);
                        }
                        setup.push(St::Store(Ty::U64, Loc::Nf(v.offset() as u32), Ex::Addr(p.loc)));
                    }
                }
                None => self.cx.annex_e("2.5.1.1", format!("the in-out variable {} of {} is not properly mapped (no value specified)", v.name, pou.name), span),
            }
        }
        let en_var = pou.var("EN").unwrap().clone();
        let en = match b.en {
            Some(e) => self.bool_expr(e),
            None => Ex::bool(true),
        };
        setup.push(St::Store(Ty::Bool, Loc::Nf(en_var.offset() as u32), en));
        let rt = pou.result.unwrap_or(self.cx.types.error);
        let res_var = pou.var(&pou.name).unwrap().clone();
        let mut post = Vec::new();
        let mut result_slot = None;
        let mut agg_temp = None;
        if self.cx.types.scalar(rt).is_some() {
            result_slot = Some((self.mty(rt), res_var.offset() as u32));
        } else {
            let t = self.temp_typed(rt);
            let bits = self.cx.types.size(rt);
            post.push(St::Copy { dst: t.clone(), src: Loc::Nf(res_var.offset() as u32), bits });
            agg_temp = Some(t);
        }
        let mut outs = Vec::new();
        for (vi, target, negated) in &b.outputs {
            let v = pou.vars[*vi].clone();
            if let Some(t) = target {
                if self.region == Region::Transition {
                    self.cx.annex_e("2.6.3", "side effects are not allowed in the evaluation of a transition condition", t.span);
                }
                self.output_target(&mut outs, t, Loc::Nf(v.offset() as u32), v.ty, *negated, span);
            }
        }
        // Outputs are not assigned when the function was not executed (EN = FALSE).
        if b.en.is_some() && !outs.is_empty() {
            let eno = pou.var("ENO").unwrap().offset();
            post.push(St::If(Ex::Load(Ty::Bool, Loc::Nf(eno as u32)), outs, vec![]));
        } else {
            post.extend(outs);
        }
        if let Some((Some(t), negated)) = b.eno {
            let eno = pou.var("ENO").unwrap().clone();
            self.output_target(&mut post, t, Loc::Nf(eno.offset() as u32), eno.ty, negated, span);
        }
        let routine = pou.routine.unwrap_or(usize::MAX);
        let seq = CallSeq { frame_bytes: 0, setup, routine, result: result_slot, post };
        match agg_temp {
            None => V::val(Ex::Call(Box::new(seq)), rt),
            Some(t) => V { kind: VK::Mem(t), ty: rt, pre: vec![St::Call(Box::new(seq))] },
        }
    }

    // ------------------------------------------------------------ function blocks

    /// Function block invocation statement.
    pub fn fb_call(&mut self, out: &mut Vec<St>, c: &CallExpr, span: Span) {
        if let Some(name) = c.callee_name() {
            let key = name.key();
            let is_var = self.cx.pous[self.pou].var_map.contains_key(&key);
            if !is_var && key.starts_with("__") && self.cx.in_std {
                let v = self.call_expr(c, None, span);
                self.discard(out, v);
                return;
            }
            if !is_var {
                if let Some(&pi) = self.cx.pou_by_name.get(&key) {
                    if self.cx.pous[pi].kind == PouKind::Function {
                        self.cx.extension("invoking a function as a statement is an extension (B.3.2.2)", span);
                        let v = self.call_function(pi, &c.args, span);
                        self.discard(out, v);
                        return;
                    }
                }
                if stdlib::parse(&key).is_some() {
                    self.cx.extension("invoking a function as a statement is an extension (B.3.2.2)", span);
                    let v = self.call_expr(c, None, span);
                    self.discard(out, v);
                    return;
                }
            }
        }
        let Some(p) = self.place(&c.callee, false) else { return };
        let Some(fb) = self.cx.types.instance_pou(p.ty) else {
            self.cx.error(format!("{} is not a function block instance", self.cx.types.name(p.ty)), c.callee.span);
            return;
        };
        if let Some(vi) = p.var {
            let v = &self.cx.pous[self.pou].vars[vi];
            if v.kind == VKind::Input && self.cx.types.instance_pou(v.ty).is_some() {
                self.cx.error_clause("2.5.2.2", "a function block instance passed as VAR_INPUT cannot be invoked (rules 4 and 5 of 2.5.2.2)", span);
            }
        }
        if self.region == Region::Transition {
            self.cx.annex_e("2.6.3", "function block invocations in a transition condition have side effects", span);
        }
        let Some(b) = self.bind(fb, &c.args, span) else { return };
        self.fb_invoke(out, p, fb, b, span);
    }

    pub fn fb_invoke(&mut self, out: &mut Vec<St>, p: expr::Place, fb: usize, b: Bound, span: Span) {
        let pou = self.cx.pous[fb].clone();
        // Keep the instance address in a temporary if it is computed.
        let inst_loc = if p.loc.is_simple() {
            p.loc.clone()
        } else {
            let t = self.temp(64);
            out.push(St::Store(Ty::U64, t.clone(), Ex::Addr(p.loc.clone())));
            t.deref()
        };
        for (vi, e) in &b.inputs {
            let v = pou.vars[*vi].clone();
            if v.is_ref() {
                if let Some(ap) = self.place(e, false) {
                    if !self.cx.types.compatible(ap.ty, v.ty) {
                        self.cx.error(format!("expected an instance of {}", self.cx.types.name(v.ty)), e.span);
                    }
                    out.push(St::Store(Ty::U64, inst_loc.clone().offset(v.offset()), Ex::Addr(ap.loc)));
                }
                continue;
            }
            let val = self.expr(e, Some(v.ty));
            self.assign(out, inst_loc.clone().offset(v.offset()), v.ty, val, e.span);
        }
        for (vi, v) in pou.vars.iter().enumerate() {
            if v.kind != VKind::InOut {
                continue;
            }
            match b.inouts.iter().find(|(x, _)| *x == vi) {
                Some((_, e)) => {
                    if let Some(ap) = self.inout_place(e) {
                        if !self.cx.types.compatible(ap.ty, v.ty) {
                            self.cx.annex_e("3.3.1", format!("type mismatch for in-out {}", v.name), e.span);
                        }
                        out.push(St::Store(Ty::U64, inst_loc.clone().offset(v.offset()), Ex::Addr(ap.loc)));
                    }
                }
                None => {
                    if self.language != Lang::Il {
                        self.cx.annex_e("2.5.2.2", format!("no value specified for the in-out variable {} of {}", v.name, pou.name), span);
                    }
                }
            }
        }
        if let Some(e) = b.en {
            let en = pou.var("EN").unwrap().clone();
            let ex = self.bool_expr(e);
            out.push(St::Store(Ty::Bool, inst_loc.clone().offset(en.offset()), ex));
        }
        let call = St::CallFb { routine: pou.routine.unwrap_or(usize::MAX), inst: Ex::Addr(inst_loc.clone()) };
        // Instances declared directly in a program can be associated with a
        // task; their execution is then under control of that task (2.7.2 rule 5).
        let tasked = self.kind == PouKind::Program && p.var.map(|vi| self.cx.pous[self.pou].vars[vi].kind == VKind::Local).unwrap_or(false);
        if tasked {
            out.push(St::If(Ex::not(Ex::Load(Ty::Bool, inst_loc.clone())), vec![call], vec![]));
        } else {
            out.push(call);
        }
        for (vi, target, negated) in &b.outputs {
            let v = pou.vars[*vi].clone();
            if let Some(t) = target {
                let src = match v.storage {
                    Storage::Inst(o) => inst_loc.clone().offset(o),
                    _ => inst_loc.clone().offset(v.offset()).deref(),
                };
                self.output_target(out, t, src, v.ty, *negated, span);
            }
        }
        if let Some((Some(t), negated)) = b.eno {
            let eno = pou.var("ENO").unwrap().clone();
            self.output_target(out, t, inst_loc.offset(eno.offset()), eno.ty, negated, span);
        }
    }

    fn discard(&mut self, out: &mut Vec<St>, v: V) {
        out.extend(v.pre.clone());
        if let VK::Val(ex) = v.kind {
            out.push(St::Eval(ex));
        }
    }

    // ------------------------------------------------------------ standard functions

    /// Binds the arguments of a standard function to its parameters.
    fn std_bind<'a>(&mut self, name: &Ident, f: &StdFn, args: &'a [Arg], span: Span) -> Option<(Vec<Option<&'a Expr>>, Option<&'a Expr>, Option<(Option<&'a Expr>, bool)>)> {
        let ps = stdlib::params(f);
        let mut slots: Vec<Option<&Expr>> = vec![None; ps.fixed.len()];
        let mut en = None;
        let mut eno = None;
        let positional = args.iter().filter(|a| matches!(a, Arg::Positional(_))).count();
        let formal = args.iter().filter(|a| matches!(a, Arg::Input { name, .. } if !name.is("EN"))).count();
        if positional > 0 && formal > 0 {
            self.cx.error_clause("2.5.1.1", "formal and non-formal arguments cannot be mixed (table 19a)", span);
            return None;
        }
        let mut pos = 0;
        for a in args {
            match a {
                Arg::Positional(e) => {
                    if pos >= slots.len() {
                        if ps.ext.is_some() {
                            slots.push(None);
                        } else {
                            self.cx.error(format!("too many arguments for {}", name), e.span);
                            return None;
                        }
                    }
                    slots[pos] = Some(e);
                    pos += 1;
                }
                Arg::Input { name: pn, value } => {
                    let key = pn.key();
                    if key == "EN" {
                        en = Some(value);
                        continue;
                    }
                    let idx = if let Some(i) = ps.fixed.iter().position(|n| *n == key) {
                        Some(i)
                    } else if let Some((prefix, start, _)) = ps.ext {
                        key.strip_prefix(prefix)
                            .and_then(|n| n.parse::<usize>().ok())
                            .filter(|n| *n >= start && *n < start + stdlib::MAX_EXTENSIBLE_INPUTS)
                            .map(|n| ps.fixed.len() + n - start)
                    } else {
                        None
                    };
                    match idx {
                        Some(i) => {
                            while slots.len() <= i {
                                slots.push(None);
                            }
                            if slots[i].is_some() {
                                self.cx.error(format!("parameter {} is assigned more than once", pn), pn.span);
                            }
                            slots[i] = Some(value);
                        }
                        None => {
                            self.cx.error(format!("{} has no input parameter named {}", name, pn), pn.span);
                            return None;
                        }
                    }
                }
                Arg::Output { name: pn, target, negated, .. } => {
                    if pn.is("ENO") {
                        eno = Some((target.as_ref(), *negated));
                    } else {
                        self.cx.error(format!("{} has no output parameter named {}", name, pn), pn.span);
                    }
                }
            }
        }
        if let Some((_, _, min)) = ps.ext {
            if slots.len() < ps.fixed.len() + min {
                if positional > 0 || slots.len() < ps.fixed.len() + 1 {
                    self.cx.error(format!("{} requires at least {} inputs", name, min + ps.fixed.len()), span);
                    return None;
                }
            }
        } else if positional > 0 && positional != ps.fixed.len() {
            self.cx.error_clause("2.5.1.1", format!("{} expects {} arguments, found {}", name, ps.fixed.len(), positional), span);
            return None;
        }
        Some((slots, en, eno))
    }

    fn std_call(&mut self, name: &Ident, f: StdFn, args: &[Arg], expected: Option<TypeId>, span: Span) -> V {
        let Some((slots, en, eno)) = self.std_bind(name, &f, args, span) else { return V::err(&self.cx.types) };
        // Missing arguments take the default value of their type; for
        // generic parameters this is determined from the other arguments.
        let mut vals: Vec<Option<(V, Span)>> = Vec::new();
        let hint = self.std_hint(&f, expected);
        for (i, s) in slots.iter().enumerate() {
            vals.push(s.map(|e| {
                let h = self.std_param_hint(&f, i, hint);
                (self.expr(e, h), e.span)
            }));
        }
        let mut pre = Vec::new();
        let saved_hint = self.lit_hint;
        self.lit_hint = hint.filter(|t| self.cx.types.scalar(*t).is_some());
        let value = self.std_body(name, f, vals, expected, span, &mut pre);
        self.lit_hint = saved_hint;
        if en.is_none() && eno.is_none() {
            let mut v = value;
            let mut p = pre;
            p.extend(v.pre);
            v.pre = p;
            return v;
        }
        // EN/ENO (2.5.1.2): errors reset ENO.
        if value.is_err() {
            return value;
        }
        let bool_ty = self.bool_ty();
        let eno_t = self.temp(8);
        let ty = value.ty;
        let mut body = pre;
        let (res_loc, rt) = if self.cx.types.scalar(ty).is_some() || value.is_literal() {
            let value = match (value.is_literal(), expected) {
                (true, Some(t)) if self.cx.types.elem_of(t).map(|e| e.is_num() || e.is_bit()).unwrap_or(false) => {
                    let ex = self.coerce(value, t, span);
                    V::val(ex, self.base_elem_ty(t))
                }
                _ => self.settle(value, span),
            };
            let rt = value.ty;
            let mt = self.mty(rt);
            let loc = self.temp_typed(rt);
            let ex = self.coerce(value, rt, span);
            body.push(St::Store(mt, loc.clone(), ex));
            (loc, rt)
        } else {
            let loc = self.temp_typed(ty);
            if let Some((wide, len)) = self.cx.types.is_string(ty) {
                body.push(St::Native(Native::StrInit { wide, cap: len }, vec![Ex::Addr(loc.clone())]));
            }
            self.assign(&mut body, loc.clone(), ty, value, span);
            (loc, ty)
        };
        body.push(St::Store(Ty::Bool, eno_t.clone(), Ex::bool(true)));
        let guarded = St::Try(body, vec![St::Store(Ty::Bool, eno_t.clone(), Ex::bool(false))]);
        let mut stmts = vec![];
        match en {
            Some(e) => {
                let c = self.bool_expr(e);
                stmts.push(St::If(c, vec![guarded], vec![St::Store(Ty::Bool, eno_t.clone(), Ex::bool(false))]));
            }
            None => stmts.push(guarded),
        }
        if let Some((Some(t), negated)) = eno {
            self.output_target(&mut stmts, t, eno_t.clone(), bool_ty, negated, span);
        }
        V { kind: VK::Mem(res_loc), ty: rt, pre: stmts }
    }

    /// Expected type for generic arguments, from typed function names or
    /// from the context.
    fn std_hint(&self, f: &StdFn, expected: Option<TypeId>) -> Option<TypeId> {
        let typed = match f {
            StdFn::Num1 { ty, .. }
            | StdFn::Arith { ty, .. }
            | StdFn::Shift { ty, .. }
            | StdFn::Bitwise { ty, .. }
            | StdFn::Sel { ty }
            | StdFn::Max { ty }
            | StdFn::Min { ty }
            | StdFn::Limit { ty }
            | StdFn::Mux { ty, .. }
            | StdFn::Compare { ty, .. } => *ty,
            _ => None,
        };
        match typed {
            Some(e) => Some(self.elem_ty(e)),
            None => match f {
                StdFn::Compare { .. } => None,
                _ => expected,
            },
        }
    }

    fn std_param_hint(&self, f: &StdFn, i: usize, hint: Option<TypeId>) -> Option<TypeId> {
        match f {
            StdFn::Conv { from: ConvTy::Elem(e), .. } => Some(self.elem_ty(*e)),
            StdFn::BcdToInt { from: Some(e), .. } | StdFn::IntToBcd { from: Some(e), .. } => Some(self.elem_ty(*e)),
            StdFn::Shift { .. } if i == 1 => None,
            StdFn::Sel { .. } if i == 0 => Some(self.bool_ty()),
            StdFn::Mux { k, .. } if i == 0 => k.map(|k| self.elem_ty(k)),
            StdFn::Str(_) => None,
            StdFn::Arith { op: Arith::Expt, .. } if i == 1 => None,
            StdFn::Time { .. } => None,
            _ => hint,
        }
    }

    /// Takes the value of a parameter or reports it as missing.
    fn arg(&mut self, vals: &mut [Option<(V, Span)>], i: usize, span: Span) -> Option<(V, Span)> {
        let r = vals.get_mut(i).and_then(|x| x.take());
        if r.is_none() {
            self.cx.error("missing argument", span);
        }
        r
    }

    /// Typed function: coerce an argument to the type in the name.
    fn typed_arg(&mut self, v: V, ty: Option<Elem>, span: Span) -> V {
        match ty {
            Some(e) => {
                let t = self.elem_ty(e);
                let ex = self.coerce(v, t, span);
                V::val(ex, t)
            }
            None => v,
        }
    }

    /// Fills missing extensible inputs with zero literals and returns all
    /// values.
    fn all_args(&mut self, vals: Vec<Option<(V, Span)>>, span: Span) -> Vec<(V, Span)> {
        vals.into_iter()
            .map(|v| v.unwrap_or_else(|| (V { kind: VK::IntLit(0), ty: self.cx.types.error, pre: vec![] }, span)))
            .collect()
    }

    /// Evaluates a value once into a temporary when it is not already a
    /// simple memory reference or constant.
    fn once(&mut self, v: V, out: &mut Vec<St>, span: Span) -> V {
        match &v.kind {
            VK::Mem(l) if l.is_simple() && v.pre.is_empty() => v,
            VK::IntLit(_) | VK::RealLit(_) | VK::Err => v,
            VK::Val(Ex::Const(..)) if v.pre.is_empty() => v,
            _ => {
                if self.cx.types.scalar(v.ty).is_some() {
                    let ty = v.ty;
                    let (ex, _) = self.scalar(v, span);
                    let t = self.temp_typed(ty);
                    out.push(St::Store(self.mty(ty), t.clone(), ex));
                    V::mem(t, ty)
                } else {
                    match self.materialize(v, out, span) {
                        Some((l, t)) => V::mem(l, t),
                        None => V::err(&self.cx.types),
                    }
                }
            }
        }
    }

    fn std_body(&mut self, name: &Ident, f: StdFn, mut vals: Vec<Option<(V, Span)>>, expected: Option<TypeId>, span: Span, pre: &mut Vec<St>) -> V {
        match f {
            StdFn::Conv { from, to } => {
                let Some((v, s)) = self.arg(&mut vals, 0, span) else { return V::err(&self.cx.types) };
                self.conversion(from, to, v, s, span, pre)
            }
            StdFn::Trunc => {
                let Some((v, s)) = self.arg(&mut vals, 0, span) else { return V::err(&self.cx.types) };
                let v = if let VK::IntLit(i) = v.kind { V { kind: VK::RealLit(i as f64), ..v } } else { v };
                let (ex, t) = self.scalar(v, s);
                let Some(e) = self.cx.types.elem_of(t).filter(|e| e.is_real()) else {
                    return self.err("TRUNC requires an input of type ANY_REAL", s);
                };
                let rt = match expected.and_then(|t| self.cx.types.elem_of(t)) {
                    Some(r) if r.is_int() => self.elem_ty(r),
                    _ => self.elem_ty(Elem::DInt),
                };
                V::val(Ex::Trunc(e.ty(), self.mty(rt), Box::new(ex)), rt)
            }
            StdFn::BcdToInt { from, to } => {
                if from.is_none() {
                    self.cx.extension(format!("the generic BCD conversion {} is an extension; use *_BCD_TO_** (table 22 feature 3)", name), span);
                } else if to.map(|t| t.is_signed_int()).unwrap_or(false) {
                    self.cx.extension(format!("{}: BCD conversion to a signed integer type is an extension (table 22 footnote d)", name), span);
                }
                let Some((v, s)) = self.arg(&mut vals, 0, span) else { return V::err(&self.cx.types) };
                let v = match from {
                    Some(e) => self.typed_arg(v, Some(e), s),
                    None => self.settle(v, s),
                };
                let (ex, t) = self.scalar(v, s);
                let Some(fe) = self.cx.types.elem_of(t).filter(|e| e.is_bit() && *e != Elem::Bool) else {
                    return self.err("BCD conversion requires a BYTE, WORD, DWORD or LWORD input", s);
                };
                let te = to.unwrap_or(Elem::Int);
                V::val(Ex::Native(Native::BcdToInt { from: fe.ty(), to: te.ty() }, vec![ex]), self.elem_ty(te))
            }
            StdFn::IntToBcd { from, to } => {
                if to.is_none() {
                    self.cx.extension(format!("the generic BCD conversion {} is an extension; use **_TO_BCD_* (table 22 feature 4)", name), span);
                } else if from.map(|t| t.is_signed_int()).unwrap_or(false) {
                    self.cx.extension(format!("{}: BCD conversion from a signed integer type is an extension (table 22 footnote d)", name), span);
                }
                let Some((v, s)) = self.arg(&mut vals, 0, span) else { return V::err(&self.cx.types) };
                let v = match from {
                    Some(e) => self.typed_arg(v, Some(e), s),
                    None => self.settle(v, s),
                };
                let (ex, t) = self.scalar(v, s);
                let Some(fe) = self.cx.types.elem_of(t).filter(|e| e.is_int()) else {
                    return self.err("BCD conversion requires an integer input", s);
                };
                let te = to.unwrap_or(match fe.width() {
                    8 => Elem::Byte,
                    16 => Elem::Word,
                    32 => Elem::DWord,
                    _ => Elem::LWord,
                });
                V::val(Ex::Native(Native::IntToBcd { from: fe.ty(), to: te.ty() }, vec![ex]), self.elem_ty(te))
            }
            StdFn::Num1 { op, ty } => {
                let Some((v, s)) = self.arg(&mut vals, 0, span) else { return V::err(&self.cx.types) };
                let v = self.typed_arg(v, ty, s);
                let v = if op != Num1::Abs {
                    if let VK::IntLit(i) = v.kind { V { kind: VK::RealLit(i as f64), ..v } } else { v }
                } else {
                    v
                };
                let v = match (&v.kind, expected) {
                    (VK::IntLit(_) | VK::RealLit(_), Some(t)) if self.cx.types.elem_of(t).map(|e| e.is_num()).unwrap_or(false) => {
                        let ex = self.coerce(v, t, s);
                        V::val(ex, self.base_elem_ty(t))
                    }
                    _ => v,
                };
                let (ex, t) = self.scalar(v, s);
                let e = self.cx.types.elem_of(t);
                let ok = match op {
                    Num1::Abs => e.map(|e| e.is_num()).unwrap_or(false),
                    _ => e.map(|e| e.is_real()).unwrap_or(false),
                };
                if !ok {
                    if !self.cx.types.is_error(t) {
                        self.cx.annex_e(
                            "2.5.1.5.2",
                            format!("{} requires an input of type {}, found {}", name, if op == Num1::Abs { "ANY_NUM" } else { "ANY_REAL" }, self.cx.types.name(t)),
                            s,
                        );
                    }
                    return V::err(&self.cx.types);
                }
                let e = e.unwrap();
                let u = match op {
                    Num1::Abs => UnOp::Abs,
                    Num1::Sqrt => UnOp::Math(MathFn::Sqrt),
                    Num1::Ln => UnOp::Math(MathFn::Ln),
                    Num1::Log => UnOp::Math(MathFn::Log),
                    Num1::Exp => UnOp::Math(MathFn::Exp),
                    Num1::Sin => UnOp::Math(MathFn::Sin),
                    Num1::Cos => UnOp::Math(MathFn::Cos),
                    Num1::Tan => UnOp::Math(MathFn::Tan),
                    Num1::Asin => UnOp::Math(MathFn::Asin),
                    Num1::Acos => UnOp::Math(MathFn::Acos),
                    Num1::Atan => UnOp::Math(MathFn::Atan),
                };
                if op == Num1::Abs && e.is_unsigned_int() {
                    return V::val(ex, self.elem_ty(e));
                }
                V::val(Ex::Un(u, e.ty(), Box::new(ex)), self.elem_ty(e))
            }
            StdFn::Arith { op, ty } => {
                if op == Arith::Move {
                    let Some((v, s)) = self.arg(&mut vals, 0, span) else { return V::err(&self.cx.types) };
                    return self.typed_arg(v, ty, s);
                }
                let args = self.all_args(vals, span);
                let bop = match op {
                    Arith::Add => BinaryOp::Add,
                    Arith::Mul => BinaryOp::Mul,
                    Arith::Sub => BinaryOp::Sub,
                    Arith::Div => BinaryOp::Div,
                    Arith::Mod => BinaryOp::Mod,
                    Arith::Expt => BinaryOp::Pow,
                    Arith::Move => unreachable!(),
                };
                let mut it = args.into_iter();
                let (first, s0) = it.next().unwrap();
                let mut acc = if op == Arith::Expt { first } else { self.typed_arg(first, ty, s0) };
                let mut acc_span = s0;
                for (v, s) in it {
                    let v = if op == Arith::Expt { v } else { self.typed_arg(v, ty, s) };
                    acc = self.binary(bop, acc, v, span, acc_span, s);
                    acc_span = span;
                }
                acc
            }
            StdFn::Shift { op, ty } => {
                let Some((v, s)) = self.arg(&mut vals, 0, span) else { return V::err(&self.cx.types) };
                let Some((n, sn)) = self.arg(&mut vals, 1, span) else { return V::err(&self.cx.types) };
                let v = self.typed_arg(v, ty, s);
                let v = match (&v.kind, expected) {
                    (VK::IntLit(_), Some(t)) => {
                        let ex = self.coerce(v, t, s);
                        V::val(ex, self.base_elem_ty(t))
                    }
                    _ => v,
                };
                let (ex, t) = self.scalar(v, s);
                let Some(e) = self.cx.types.elem_of(t).filter(|e| e.is_bit()) else {
                    return self.err(format!("{} requires an input of type ANY_BIT", name), s);
                };
                let n = self.settle(n, sn);
                let (nex, nt) = self.scalar(n, sn);
                let Some(ne) = self.cx.types.elem_of(nt).filter(|e| e.is_int()) else {
                    return self.err("the N input of a shift function shall be of type ANY_INT", sn);
                };
                let bop = match op {
                    Shift::Shl => BinOp::Shl,
                    Shift::Shr => BinOp::Shr,
                    Shift::Rol => BinOp::Rol,
                    Shift::Ror => BinOp::Ror,
                };
                V::val(Ex::Bin(bop, e.ty(), Box::new(ex), Box::new(Ex::Conv(ne.ty(), Ty::I64, Box::new(nex)))), self.elem_ty(e))
            }
            StdFn::Bitwise { op, ty } => {
                let args = self.all_args(vals, span);
                if op == Bitwise::Not {
                    let (v, s) = args.into_iter().next().unwrap();
                    let v = self.typed_arg(v, ty, s);
                    let v = match (&v.kind, expected) {
                        (VK::IntLit(_), Some(t)) => {
                            let ex = self.coerce(v, t, s);
                            V::val(ex, self.base_elem_ty(t))
                        }
                        _ => v,
                    };
                    let (ex, t) = self.scalar(v, s);
                    let Some(e) = self.cx.types.elem_of(t).filter(|e| e.is_bit()) else {
                        return self.err("NOT requires an input of type ANY_BIT", s);
                    };
                    return V::val(Ex::Un(UnOp::Not, e.ty(), Box::new(ex)), self.elem_ty(e));
                }
                let bop = match op {
                    Bitwise::And => BinaryOp::And,
                    Bitwise::Or => BinaryOp::Or,
                    _ => BinaryOp::Xor,
                };
                let mut it = args.into_iter();
                let (first, s0) = it.next().unwrap();
                let mut acc = self.typed_arg(first, ty, s0);
                for (v, s) in it {
                    let v = self.typed_arg(v, ty, s);
                    acc = self.binary(bop, acc, v, span, span, s);
                }
                acc
            }
            StdFn::Compare { op, ty } => {
                let args = self.all_args(vals, span);
                let c = match op {
                    Compare::Gt => CmpOp::Gt,
                    Compare::Ge => CmpOp::Ge,
                    Compare::Eq => CmpOp::Eq,
                    Compare::Le => CmpOp::Le,
                    Compare::Lt => CmpOp::Lt,
                    Compare::Ne => CmpOp::Ne,
                };
                let n = args.len();
                let mut vs = Vec::new();
                for (i, (v, s)) in args.into_iter().enumerate() {
                    let v = self.typed_arg(v, ty, s);
                    // middle operands are used twice
                    let v = if i > 0 && i + 1 < n { self.once(v, pre, s) } else { v };
                    vs.push((v, s));
                }
                let mut acc: Option<V> = None;
                for i in 0..n - 1 {
                    let (a, sa) = vs[i].clone();
                    let (b, sb) = vs[i + 1].clone();
                    let a = V { pre: if i == 0 { a.pre } else { vec![] }, ..a };
                    let r = self.compare(c, a, b, span, sa, sb);
                    acc = Some(match acc {
                        None => r,
                        Some(prev) => self.binary(BinaryOp::And, prev, r, span, span, span),
                    });
                }
                acc.unwrap()
            }
            StdFn::Sel { ty } => {
                let Some((g, sg)) = self.arg(&mut vals, 0, span) else { return V::err(&self.cx.types) };
                let b = self.bool_ty();
                let gex = self.coerce(g, b, sg);
                let a0 = vals[1].take();
                let a1 = vals[2].take();
                let (Some((v0, s0)), Some((v1, s1))) = (a0, a1) else {
                    return self.err("SEL requires the inputs IN0 and IN1", span);
                };
                let v0 = self.typed_arg(v0, ty, s0);
                let v1 = self.typed_arg(v1, ty, s1);
                self.select(gex, v1, v0, span, pre)
            }
            StdFn::Max { ty } | StdFn::Min { ty } => {
                let is_max = matches!(f, StdFn::Max { .. });
                let args = self.all_args(vals, span);
                let mut it = args.into_iter();
                let (first, s0) = it.next().unwrap();
                let first = self.typed_arg(first, ty, s0);
                let mut acc = self.once(first, pre, s0);
                for (v, s) in it {
                    let v = self.typed_arg(v, ty, s);
                    let v = self.once(v, pre, s);
                    let c = self.compare(if is_max { CmpOp::Gt } else { CmpOp::Lt }, v.clone(), acc.clone(), span, s, span);
                    if c.is_err() {
                        return c;
                    }
                    let (cex, _) = self.scalar(c, span);
                    let sel = self.select(cex, v, acc, span, pre);
                    acc = self.once(sel, pre, span);
                }
                self.check_elementary(&acc, name, span);
                acc
            }
            StdFn::Limit { ty } => {
                let mn = vals[0].take();
                let inp = vals[1].take();
                let mx = vals[2].take();
                let (Some((mn, smn)), Some((iv, si)), Some((mx, smx))) = (mn, inp, mx) else {
                    return self.err("LIMIT requires the inputs MN, IN and MX", span);
                };
                let mn = self.typed_arg(mn, ty, smn);
                let iv = self.typed_arg(iv, ty, si);
                let mx = self.typed_arg(mx, ty, smx);
                // OUT := MIN(MAX(IN, MN), MX)
                let iv = self.once(iv, pre, si);
                let mn = self.once(mn, pre, smn);
                let mx = self.once(mx, pre, smx);
                let c1 = self.compare(CmpOp::Lt, iv.clone(), mn.clone(), span, si, smn);
                if c1.is_err() {
                    return c1;
                }
                let (c1, _) = self.scalar(c1, span);
                let t = self.select(c1, mn, iv, span, pre);
                let t = self.once(t, pre, span);
                let c2 = self.compare(CmpOp::Gt, t.clone(), mx.clone(), span, span, smx);
                if c2.is_err() {
                    return c2;
                }
                let (c2, _) = self.scalar(c2, span);
                let r = self.select(c2, mx, t, span, pre);
                self.check_elementary(&r, name, span);
                r
            }
            StdFn::Mux { k, ty } => {
                let Some((kv, sk)) = self.arg(&mut vals, 0, span) else { return V::err(&self.cx.types) };
                let kv = match k {
                    Some(e) => self.typed_arg(kv, Some(e), sk),
                    None => self.settle(kv, sk),
                };
                let (kex, kt) = self.scalar(kv, sk);
                let Some(ke) = self.cx.types.elem_of(kt).filter(|e| e.is_int()) else {
                    return self.err("the K input of MUX shall be of type ANY_INT", sk);
                };
                let kt = self.temp(64);
                pre.push(St::Store(Ty::I64, kt.clone(), Ex::Conv(ke.ty(), Ty::I64, Box::new(kex))));
                let ins: Vec<(V, Span)> = self.all_args(vals.split_off(1), span);
                let n = ins.len();
                // Determine the common type of the inputs.
                let mut common: Option<V> = None;
                let mut typed_ins = Vec::new();
                for (v, s) in ins {
                    let v = self.typed_arg(v, ty, s);
                    if let Some(c) = &common {
                        if let Some((_, b, t)) = self.unify(c.clone(), v.clone(), s) {
                            common = Some(V { ty: t, ..b });
                        }
                    } else if !v.is_literal() {
                        common = Some(v.clone());
                    }
                    typed_ins.push((v, s));
                }
                let rt = match &common {
                    Some(c) if !c.is_literal() => c.ty,
                    _ => match expected {
                        Some(t) => t,
                        None => self.elem_ty(Elem::DInt),
                    },
                };
                // result
                if self.cx.types.scalar(rt).is_some() {
                    let mt = self.mty(rt);
                    let mut chain = Ex::Seq(vec![St::Trap(TrapCode::MuxSelectorOutOfRange)], Box::new(Ex::Const(mt, 0)));
                    for (i, (v, s)) in typed_ins.into_iter().enumerate().rev() {
                        let ex = self.coerce(v, rt, s);
                        let cond = Ex::Cmp(CmpOp::Eq, Ty::I64, Box::new(Ex::Load(Ty::I64, kt.clone())), Box::new(Ex::Const(Ty::I64, i as u64)));
                        chain = Ex::Select(Box::new(cond), Box::new(ex), Box::new(chain));
                    }
                    let _ = n;
                    V::val(chain, self.base_elem_ty(rt))
                } else {
                    let res = self.temp_typed(rt);
                    if let Some((wide, len)) = self.cx.types.is_string(rt) {
                        let len = typed_ins.iter().map(|(v, _)| self.cx.types.is_string(v.ty).map(|x| x.1).unwrap_or(len)).max().unwrap_or(len);
                        let (l, t) = self.string_temp(wide, len, pre);
                        let mut chain = vec![St::Trap(TrapCode::MuxSelectorOutOfRange)];
                        for (i, (v, s)) in typed_ins.into_iter().enumerate().rev() {
                            let mut body = Vec::new();
                            self.assign(&mut body, l.clone(), t, v, s);
                            let cond = Ex::Cmp(CmpOp::Eq, Ty::I64, Box::new(Ex::Load(Ty::I64, kt.clone())), Box::new(Ex::Const(Ty::I64, i as u64)));
                            chain = vec![St::If(cond, body, chain)];
                        }
                        pre.extend(chain);
                        return V::mem(l, t);
                    }
                    let mut chain = vec![St::Trap(TrapCode::MuxSelectorOutOfRange)];
                    for (i, (v, s)) in typed_ins.into_iter().enumerate().rev() {
                        let mut body = Vec::new();
                        self.assign(&mut body, res.clone(), rt, v, s);
                        let cond = Ex::Cmp(CmpOp::Eq, Ty::I64, Box::new(Ex::Load(Ty::I64, kt.clone())), Box::new(Ex::Const(Ty::I64, i as u64)));
                        chain = vec![St::If(cond, body, chain)];
                    }
                    pre.extend(chain);
                    V::mem(res, rt)
                }
            }
            StdFn::Str(sf) => self.string_fn(name, sf, vals, expected, span, pre),
            StdFn::Time { f: tf, ty } => {
                let Some((a, sa)) = self.arg(&mut vals, 0, span) else { return V::err(&self.cx.types) };
                let Some((b, sb)) = self.arg(&mut vals, 1, span) else { return V::err(&self.cx.types) };
                use Elem::*;
                let (ta, tb, tr) = match tf {
                    TimeFn::AddTime | TimeFn::SubTime => (Time, Time, Time),
                    TimeFn::AddTodTime | TimeFn::SubTodTime => (Tod, Time, Tod),
                    TimeFn::AddDtTime | TimeFn::SubDtTime => (Dt, Time, Dt),
                    TimeFn::SubDateDate => (Date, Date, Time),
                    TimeFn::SubTodTod => (Tod, Tod, Time),
                    TimeFn::SubDtDt => (Dt, Dt, Time),
                    TimeFn::ConcatDateTod => (Date, Tod, Dt),
                    TimeFn::MulTime | TimeFn::DivTime => {
                        let t = self.elem_ty(Time);
                        let x = self.coerce(a, t, sa);
                        let b = self.typed_arg(b, ty, sb);
                        let op = if tf == TimeFn::MulTime { BinOp::Mul } else { BinOp::Div };
                        return self.time_scale(op, x, b, sb);
                    }
                };
                let x = {
                    let t = self.elem_ty(ta);
                    self.coerce(a, t, sa)
                };
                let y = {
                    let t = self.elem_ty(tb);
                    self.coerce(b, t, sb)
                };
                let op = match tf {
                    TimeFn::AddTime | TimeFn::AddTodTime | TimeFn::AddDtTime | TimeFn::ConcatDateTod => BinOp::Add,
                    _ => BinOp::Sub,
                };
                let r = Ex::Bin(op, Ty::I64, Box::new(x), Box::new(y));
                let r = match tr {
                    Tod => Ex::Check { ty: Ty::I64, lo: 0, hi: MICROS_PER_DAY - 1, code: TrapCode::TimeRange, value: Box::new(r) },
                    Dt => Ex::Check { ty: Ty::I64, lo: 0, hi: structure_runtime::calendar::dt_max(), code: TrapCode::TimeRange, value: Box::new(r) },
                    _ => r,
                };
                V::val(r, self.elem_ty(tr))
            }
        }
    }

    fn check_elementary(&mut self, v: &V, name: &Ident, span: Span) {
        if !v.is_err() && !v.is_literal() && self.cx.types.elem_of(v.ty).is_none() && self.cx.types.is_string(v.ty).is_none() && !self.cx.types.is_enum(v.ty) {
            self.cx.annex_e("2.5.1.5.4", format!("{} requires inputs of type ANY_ELEMENTARY", name), span);
        }
    }

    /// Selection between two values of the same type.
    fn select(&mut self, cond: Ex, a: V, b: V, span: Span, pre: &mut Vec<St>) -> V {
        let Some((a, b, t)) = self.unify(a, b, span) else { return V::err(&self.cx.types) };
        if self.cx.types.scalar(t).is_some() || (a.is_literal() && b.is_literal()) {
            let t = if self.cx.types.is_error(t) { self.elem_ty(Elem::DInt) } else { t };
            let bt = if self.cx.types.is_enum(t) { self.cx.types.resolve(t) } else { self.base_elem_ty(t) };
            let ea = self.coerce(a, bt, span);
            let eb = self.coerce(b, bt, span);
            return V::val(Ex::Select(Box::new(cond), Box::new(ea), Box::new(eb)), bt);
        }
        // aggregates: copy the chosen one into a temporary
        let (loc, rt) = if let Some((wide, len)) = self.cx.types.is_string(t) {
            let la = self.cx.types.is_string(a.ty).map(|x| x.1).unwrap_or(len);
            let lb = self.cx.types.is_string(b.ty).map(|x| x.1).unwrap_or(len);
            let (l, ty) = self.string_temp(wide, la.max(lb), pre);
            (l, ty)
        } else {
            (self.temp_typed(t), t)
        };
        let mut ta = Vec::new();
        self.assign(&mut ta, loc.clone(), rt, a, span);
        let mut tb = Vec::new();
        self.assign(&mut tb, loc.clone(), rt, b, span);
        pre.push(St::If(cond, ta, tb));
        V::mem(loc, rt)
    }

    // ------------------------------------------------------------ strings

    fn string_arg(&mut self, v: V, span: Span, pre: &mut Vec<St>) -> Option<(Loc, bool, u32)> {
        if v.is_err() {
            return None;
        }
        let Some((wide, len)) = self.cx.types.is_string(v.ty) else {
            self.cx.annex_e("2.5.1.5.5", format!("expected a character string, found {}", self.cx.types.name(v.ty)), span);
            return None;
        };
        let (l, _) = self.materialize(v, pre, span)?;
        Some((l, wide, len))
    }

    fn int_arg(&mut self, v: V, span: Span) -> Ex {
        let v = self.settle(v, span);
        let (ex, t) = self.scalar(v, span);
        match self.cx.types.elem_of(t) {
            Some(e) if e.is_int() => Ex::Conv(e.ty(), Ty::I64, Box::new(ex)),
            _ => {
                if !self.cx.types.is_error(t) {
                    self.cx.annex_e("2.5.1.5.5", "expected an argument of type ANY_INT", span);
                }
                Ex::Const(Ty::I64, 0)
            }
        }
    }

    fn string_fn(&mut self, name: &Ident, sf: StrFn, mut vals: Vec<Option<(V, Span)>>, expected: Option<TypeId>, span: Span, pre: &mut Vec<St>) -> V {
        let int_result = |s: &Self| -> TypeId {
            match expected.and_then(|t| s.cx.types.elem_of(t)) {
                Some(e) if e.is_int() => s.elem_ty(e),
                _ => s.elem_ty(Elem::Int),
            }
        };
        macro_rules! sarg {
            ($i:expr) => {{
                let Some((v, s)) = self.arg(&mut vals, $i, span) else { return V::err(&self.cx.types) };
                match self.string_arg(v, s, pre) {
                    Some(x) => x,
                    None => return V::err(&self.cx.types),
                }
            }};
        }
        macro_rules! iarg {
            ($i:expr) => {{
                let Some((v, s)) = self.arg(&mut vals, $i, span) else { return V::err(&self.cx.types) };
                self.int_arg(v, s)
            }};
        }
        match sf {
            StrFn::Len => {
                let (l, wide, _) = sarg!(0);
                let rt = int_result(self);
                let mt = self.mty(rt);
                let ex = Ex::Native(Native::StrLen { wide }, vec![Ex::Addr(l)]);
                V::val(Ex::Conv(Ty::I64, mt, Box::new(ex)), rt)
            }
            StrFn::Find => {
                let (a, wa, _) = sarg!(0);
                let (b, wb, _) = sarg!(1);
                if wa != wb {
                    return self.err("FIND requires inputs of the same string type", span);
                }
                let rt = int_result(self);
                let mt = self.mty(rt);
                let ex = Ex::Native(Native::StrFind { wide: wa }, vec![Ex::Addr(a), Ex::Addr(b)]);
                V::val(Ex::Conv(Ty::I64, mt, Box::new(ex)), rt)
            }
            StrFn::Left | StrFn::Right => {
                let (s, wide, len) = sarg!(0);
                let l = iarg!(1);
                let (dst, t) = self.string_temp(wide, len, pre);
                let n = if sf == StrFn::Left { Native::StrLeft { wide } } else { Native::StrRight { wide } };
                pre.push(St::Native(n, vec![Ex::Addr(dst.clone()), Ex::Addr(s), l]));
                V::mem(dst, t)
            }
            StrFn::Mid => {
                let (s, wide, len) = sarg!(0);
                let l = iarg!(1);
                let p = iarg!(2);
                let (dst, t) = self.string_temp(wide, len, pre);
                pre.push(St::Native(Native::StrMid { wide }, vec![Ex::Addr(dst.clone()), Ex::Addr(s), l, p]));
                V::mem(dst, t)
            }
            StrFn::Delete => {
                let (s, wide, len) = sarg!(0);
                let l = iarg!(1);
                let p = iarg!(2);
                let (dst, t) = self.string_temp(wide, len, pre);
                pre.push(St::Native(Native::StrDelete { wide }, vec![Ex::Addr(dst.clone()), Ex::Addr(s), l, p]));
                V::mem(dst, t)
            }
            StrFn::Insert | StrFn::Replace => {
                let (a, wa, la) = sarg!(0);
                let (b, wb, lb) = sarg!(1);
                if wa != wb {
                    return self.err(format!("{} requires inputs of the same string type", name), span);
                }
                let cap = (la + lb).min(MAX_STRING_LENGTH);
                let (dst, t) = self.string_temp(wa, cap, pre);
                if sf == StrFn::Insert {
                    let p = iarg!(2);
                    pre.push(St::Native(Native::StrInsert { wide: wa }, vec![Ex::Addr(dst.clone()), Ex::Addr(a), Ex::Addr(b), p]));
                } else {
                    let l = iarg!(2);
                    let p = iarg!(3);
                    pre.push(St::Native(Native::StrReplace { wide: wa }, vec![Ex::Addr(dst.clone()), Ex::Addr(a), Ex::Addr(b), l, p]));
                }
                V::mem(dst, t)
            }
            StrFn::Concat => {
                let n = vals.len();
                let mut parts = Vec::new();
                let mut cap: u32 = 0;
                let mut wide = None;
                for i in 0..n {
                    let (l, w, len) = sarg!(i);
                    if wide.is_some() && wide != Some(w) {
                        return self.err("CONCAT requires inputs of the same string type", span);
                    }
                    wide = Some(w);
                    cap = cap.saturating_add(len);
                    parts.push(Ex::Addr(l));
                }
                let wide = wide.unwrap_or(false);
                let cap = cap.min(MAX_STRING_LENGTH);
                let (dst, t) = self.string_temp(wide, cap, pre);
                let mut args = vec![Ex::Addr(dst.clone())];
                args.extend(parts);
                pre.push(St::Native(Native::StrConcat { wide, n: n as u8 }, args));
                V::mem(dst, t)
            }
        }
    }

    // ------------------------------------------------------------ conversions

    fn text_kind(e: Elem) -> TextKind {
        match e {
            Elem::Bool => TextKind::Bool,
            Elem::Real | Elem::LReal => TextKind::Real(e.ty()),
            Elem::Time => TextKind::Time,
            Elem::Date => TextKind::Date,
            Elem::Tod => TextKind::TimeOfDay,
            Elem::Dt => TextKind::DateAndTime,
            Elem::Byte | Elem::Word | Elem::DWord | Elem::LWord => TextKind::Bits(e.ty()),
            _ => TextKind::Int(e.ty()),
        }
    }

    /// Type conversion functions (table 22, table 30 features 13 and 14).
    fn conversion(&mut self, from: ConvTy, to: ConvTy, v: V, s: Span, span: Span, pre: &mut Vec<St>) -> V {
        match (from, to) {
            (ConvTy::Text(ft), ConvTy::Text(tt)) => {
                let Some((l, wide, len)) = self.string_arg(v, s, pre) else { return V::err(&self.cx.types) };
                if wide != (ft == Text::WString) {
                    return self.err("the input does not match the source type of the conversion", s);
                }
                if ft == tt {
                    return V::mem(l, self.cx.types.string(wide, len));
                }
                let (dst, t) = self.string_temp(tt == Text::WString, len, pre);
                let n = if tt == Text::WString { Native::StrToWstr } else { Native::WstrToStr };
                pre.push(St::Native(n, vec![Ex::Addr(dst.clone()), Ex::Addr(l)]));
                V::mem(dst, t)
            }
            (ConvTy::Text(ft), ConvTy::Elem(te)) => {
                let Some((l, wide, _)) = self.string_arg(v, s, pre) else { return V::err(&self.cx.types) };
                if wide != (ft == Text::WString) {
                    return self.err("the input does not match the source type of the conversion", s);
                }
                let ex = Ex::Native(Native::FromText { kind: Self::text_kind(te), wide }, vec![Ex::Addr(l)]);
                V::val(ex, self.elem_ty(te))
            }
            (ConvTy::Elem(fe), ConvTy::Text(tt)) => {
                let ft = self.elem_ty(fe);
                let ex = self.coerce(v, ft, s);
                let wide = tt == Text::WString;
                let cap = 64;
                let (dst, t) = self.string_temp(wide, cap, pre);
                pre.push(St::Native(Native::ToText { kind: Self::text_kind(fe), wide }, vec![Ex::Addr(dst.clone()), ex]));
                V::mem(dst, t)
            }
            (ConvTy::Elem(fe), ConvTy::Elem(te)) => {
                let time = |e: Elem| matches!(e, Elem::Time | Elem::Date | Elem::Tod | Elem::Dt);
                if (time(fe) || time(te)) && !matches!((fe, te), (Elem::Dt, Elem::Tod) | (Elem::Dt, Elem::Date)) && fe != te {
                    self.cx.impl_dependent(format!("the effect of conversion from {} to {} is implementation-dependent (table 30 note 5)", fe.name(), te.name()), span);
                }
                let ft = self.elem_ty(fe);
                let x = self.coerce(v, ft, s);
                match self.elem_conversion(fe, te, x) {
                    Some(ex) => V::val(ex, self.elem_ty(te)),
                    None => self.err(format!("conversion from {} to {} is not supported (see the implementation-dependent parameters)", fe.name(), te.name()), span),
                }
            }
        }
    }

    /// Conversion between elementary types. Time data types convert to and
    /// from numbers in milliseconds (TIME, TIME_OF_DAY) and seconds since
    /// 1970-01-01 (DATE, DATE_AND_TIME).
    pub fn elem_conversion(&mut self, fe: Elem, te: Elem, x: Ex) -> Option<Ex> {
        use Elem::*;
        let i64_ = Ty::I64;
        if fe == te {
            return Some(x);
        }
        let epoch = structure_runtime::calendar::unix_epoch_micros();
        let day = MICROS_PER_DAY;
        Some(match (fe, te) {
            (Dt, Tod) => Ex::Bin(BinOp::Mod, i64_, Box::new(x), Box::new(Ex::Const(i64_, day as u64))),
            (Dt, Date) => {
                let t = self.temp(64);
                Ex::Seq(
                    vec![St::Store(i64_, t.clone(), x)],
                    Box::new(Ex::Bin(
                        BinOp::Sub,
                        i64_,
                        Box::new(Ex::Load(i64_, t.clone())),
                        Box::new(Ex::Bin(BinOp::Mod, i64_, Box::new(Ex::Load(i64_, t)), Box::new(Ex::Const(i64_, day as u64)))),
                    )),
                )
            }
            (Date, Dt) => x,
            (Tod, Time) => x,
            (Time, Tod) => Ex::Check { ty: i64_, lo: 0, hi: day - 1, code: TrapCode::ConversionError, value: Box::new(x) },
            (Time | Tod, t) if t.is_int() => {
                let ms = Ex::Bin(BinOp::Div, i64_, Box::new(x), Box::new(Ex::Const(i64_, 1000)));
                Ex::Conv(i64_, t.ty(), Box::new(ms))
            }
            (Time | Tod, t) if t.is_real() => {
                let f = Ex::Conv(i64_, Ty::F64, Box::new(x));
                let ms = Ex::Bin(BinOp::Div, Ty::F64, Box::new(f), Box::new(Ex::Const(Ty::F64, 1000.0f64.to_bits())));
                Ex::Conv(Ty::F64, t.ty(), Box::new(ms))
            }
            (f, Time | Tod) if f.is_int() => {
                let v = Ex::Conv(f.ty(), i64_, Box::new(x));
                let us = Ex::Bin(BinOp::Mul, i64_, Box::new(v), Box::new(Ex::Const(i64_, 1000)));
                if te == Tod {
                    Ex::Check { ty: i64_, lo: 0, hi: day - 1, code: TrapCode::ConversionError, value: Box::new(us) }
                } else {
                    us
                }
            }
            (f, Time | Tod) if f.is_real() => {
                let v = Ex::Conv(f.ty(), Ty::F64, Box::new(x));
                let us = Ex::Bin(BinOp::Mul, Ty::F64, Box::new(v), Box::new(Ex::Const(Ty::F64, 1000.0f64.to_bits())));
                let r = Ex::Conv(Ty::F64, i64_, Box::new(us));
                if te == Tod {
                    Ex::Check { ty: i64_, lo: 0, hi: day - 1, code: TrapCode::ConversionError, value: Box::new(r) }
                } else {
                    r
                }
            }
            (Date | Dt, t) if t.is_int() => {
                let rel = Ex::Bin(BinOp::Sub, i64_, Box::new(x), Box::new(Ex::Const(i64_, epoch as u64)));
                let secs = Ex::Bin(BinOp::Div, i64_, Box::new(rel), Box::new(Ex::Const(i64_, 1_000_000)));
                Ex::Conv(i64_, t.ty(), Box::new(secs))
            }
            (f, Date | Dt) if f.is_int() => {
                let v = Ex::Conv(f.ty(), i64_, Box::new(x));
                let us = Ex::Bin(BinOp::Mul, i64_, Box::new(v), Box::new(Ex::Const(i64_, 1_000_000)));
                let mut r = Ex::Bin(BinOp::Add, i64_, Box::new(us), Box::new(Ex::Const(i64_, epoch as u64)));
                if te == Date {
                    let t = self.temp(64);
                    r = Ex::Seq(
                        vec![St::Store(i64_, t.clone(), r)],
                        Box::new(Ex::Bin(
                            BinOp::Sub,
                            i64_,
                            Box::new(Ex::Load(i64_, t.clone())),
                            Box::new(Ex::Bin(BinOp::Mod, i64_, Box::new(Ex::Load(i64_, t)), Box::new(Ex::Const(i64_, day as u64)))),
                        )),
                    );
                }
                Ex::Check { ty: i64_, lo: 0, hi: structure_runtime::calendar::dt_max(), code: TrapCode::ConversionError, value: Box::new(r) }
            }
            (f, t) if (f.is_num() || f.is_bit()) && (t.is_num() || t.is_bit()) => Ex::Conv(f.ty(), t.ty(), Box::new(x)),
            _ => return None,
        })
    }
}

#[allow(dead_code)]
fn _unused(_: &ast::Expr) {}
