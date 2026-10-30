//! Expressions (3.3.1): typing, coercion, operators, variables.

use super::*;
use crate::consteval;
use structure_runtime::error::TrapCode;
use structure_runtime::image::{CmpOp, Native};
use structure_runtime::ty::{self as mty, MICROS_PER_DAY};
use structure_syntax::ast::{self, BinaryOp, ExprKind, UnaryOp};
use structure_syntax::diag::Diagnostic;
use structure_syntax::Ident;

/// An assignable or readable memory location with access information.
#[derive(Clone, Debug)]
pub struct Place {
    pub loc: Loc,
    pub ty: TypeId,
    /// Why the place cannot be written, with the clause and whether it is
    /// an annex E error.
    pub read_only: Option<(String, &'static str, bool)>,
    /// Root variable of the current POU, if any.
    pub var: Option<usize>,
    /// The place is (part of) the function result.
    pub is_result: bool,
    /// The place is an output of another function block instance.
    pub fb_output: bool,
}

/// Elementary implicit conversion allowed as an extension.
pub fn widens(from: Elem, to: Elem) -> bool {
    if from == to {
        return true;
    }
    let (fw, tw) = (from.width(), to.width());
    match (from, to) {
        (f, t) if f.is_signed_int() && t.is_signed_int() => tw > fw,
        (f, t) if f.is_unsigned_int() && t.is_unsigned_int() => tw > fw,
        (f, t) if f.is_unsigned_int() && t.is_signed_int() => tw > fw,
        (f, t) if f.is_bit() && t.is_bit() && f != Elem::Bool && t != Elem::Bool => tw > fw,
        (f, Elem::Real) if f.is_int() => fw <= 16,
        (f, Elem::LReal) if f.is_int() => fw <= 32,
        (Elem::Real, Elem::LReal) => true,
        _ => false,
    }
}

impl<'c, 's> Lowerer<'c, 's> {
    // ------------------------------------------------------------ helpers

    pub fn base_elem_ty(&self, ty: TypeId) -> TypeId {
        match self.cx.types.elem_of(ty) {
            Some(e) => self.cx.types.elem(e),
            None => self.cx.types.resolve(ty),
        }
    }

    fn default_lit_type(&self, v: &VK) -> TypeId {
        match v {
            VK::IntLit(i) => {
                if *i >= i32::MIN as i128 && *i <= i32::MAX as i128 {
                    self.elem_ty(Elem::DInt)
                } else if *i >= i64::MIN as i128 && *i <= i64::MAX as i128 {
                    self.elem_ty(Elem::LInt)
                } else {
                    self.elem_ty(Elem::ULInt)
                }
            }
            _ => self.elem_ty(Elem::LReal),
        }
    }

    /// Gives an untyped literal its default type; leaves other values alone.
    pub fn settle(&mut self, v: V, span: Span) -> V {
        match &v.kind {
            VK::IntLit(_) | VK::RealLit(_) => {
                let t = self.default_lit_type(&v.kind);
                let ex = self.coerce(v, t, span);
                V::val(ex, t)
            }
            _ => v,
        }
    }

    /// Wraps pre-statements around a scalar expression.
    fn with_pre(pre: Vec<St>, ex: Ex) -> Ex {
        if pre.is_empty() {
            ex
        } else {
            Ex::Seq(pre, Box::new(ex))
        }
    }

    /// The scalar value of `v` in its own type.
    pub fn scalar(&mut self, v: V, span: Span) -> (Ex, TypeId) {
        let v = self.settle(v, span);
        let ty = v.ty;
        match v.kind {
            VK::Val(ex) => (Self::with_pre(v.pre, ex), ty),
            VK::Mem(loc) => match self.cx.types.scalar(ty) {
                Some(mt) => (Self::with_pre(v.pre, Ex::Load(mt, loc)), ty),
                None => {
                    if !self.cx.types.is_error(ty) {
                        self.cx.error(format!("a value of type {} cannot be used here", self.cx.types.name(ty)), span);
                    }
                    (Ex::Const(Ty::U64, 0), self.cx.types.error)
                }
            },
            _ => (Ex::Const(Ty::U64, 0), self.cx.types.error),
        }
    }

    /// Converts `v` to a scalar of type `target`, checking assignment
    /// compatibility (2.3.3.3, 2.5.1.4).
    pub fn coerce(&mut self, v: V, target: TypeId, span: Span) -> Ex {
        let tmt = self.cx.types.scalar(target).unwrap_or(Ty::U64);
        if self.cx.types.is_error(target) {
            return Ex::Const(tmt, 0);
        }
        match &v.kind {
            VK::Err => return Ex::Const(tmt, 0),
            VK::IntLit(i) => {
                return self.const_to(ConstValue::Int(*i), target, span);
            }
            VK::RealLit(f) => {
                return self.const_to(ConstValue::Real(*f), target, span);
            }
            _ => {}
        }
        let src = v.ty;
        if self.cx.types.is_error(src) {
            return Ex::Const(tmt, 0);
        }
        let (ex, _) = self.scalar(v, span);
        let types = &self.cx.types;
        if types.compatible(src, target) {
            return self.range_checked(ex, src, target);
        }
        let (se, te) = (types.elem_of(src), types.elem_of(target));
        if let (Some(se), Some(te)) = (se, te) {
            if widens(se, te) {
                self.cx.extension(
                    format!("implicit conversion from {} to {} is an extension (use {}_TO_{})", se.name(), te.name(), se.short(), te.short()),
                    span,
                );
                let conv = Ex::Conv(se.ty(), te.ty(), Box::new(ex));
                return self.range_checked(conv, self.cx.types.elem(te), target);
            }
        }
        self.cx.annex_e(
            "3.3.1",
            format!("type mismatch: expected {}, found {}", self.cx.types.name(target), self.cx.types.name(src)),
            span,
        );
        Ex::Const(tmt, 0)
    }

    fn range_checked(&mut self, ex: Ex, src: TypeId, target: TypeId) -> Ex {
        if let Some((lo, hi)) = self.cx.types.subrange(target) {
            if let Some((slo, shi)) = self.cx.types.subrange(src) {
                if slo >= lo && shi <= hi {
                    return ex;
                }
            }
            let mt = self.mty(target);
            return Ex::Check {
                ty: mt,
                lo: lo.clamp(i64::MIN as i128, i64::MAX as i128) as i64,
                hi: hi.clamp(i64::MIN as i128, i64::MAX as i128) as i64,
                code: TrapCode::SubrangeViolation,
                value: Box::new(ex),
            };
        }
        ex
    }

    /// A typed constant of `target`.
    pub fn const_to(&mut self, cv: ConstValue, target: TypeId, span: Span) -> Ex {
        let mt = self.cx.types.scalar(target).unwrap_or(Ty::U64);
        match consteval::coerce(&self.cx.types, &cv, target) {
            Ok((v, ext)) => {
                if ext {
                    self.cx.extension("implicit conversion of an integer literal to a real type is an extension", span);
                }
                Ex::Const(mt, consteval::encode(&self.cx.types, &v, target).unwrap_or(0))
            }
            Err(m) => {
                if self.cx.types.subrange(target).is_some() {
                    self.cx.annex_e("2.3.3.1", m, span);
                } else {
                    self.cx.annex_e("3.3.1", m, span);
                }
                Ex::Const(mt, 0)
            }
        }
    }

    /// Emits an assignment of `v` to a location of type `ty`.
    pub fn assign(&mut self, out: &mut Vec<St>, loc: Loc, ty: TypeId, v: V, span: Span) {
        if v.is_err() || self.cx.types.is_error(ty) {
            return;
        }
        if let Some((wide, _)) = self.cx.types.is_string(ty) {
            match v.kind {
                VK::Mem(src) if self.cx.types.is_string(v.ty).map(|(w, _)| w) == Some(wide) => {
                    out.extend(v.pre);
                    out.push(St::Native(Native::StrAssign { wide }, vec![Ex::Addr(loc), Ex::Addr(src)]));
                }
                _ => {
                    self.cx.annex_e(
                        "3.3.1",
                        format!("type mismatch: expected {}, found {}", self.cx.types.name(ty), self.cx.types.name(v.ty)),
                        span,
                    );
                }
            }
            return;
        }
        if self.cx.types.scalar(ty).is_some() {
            let mt = self.mty(ty);
            let ex = self.coerce(v, ty, span);
            out.push(St::Store(mt, loc, ex));
            return;
        }
        if self.cx.types.instance_pou(ty).is_some() {
            self.cx.error("function block instances cannot be assigned", span);
            return;
        }
        // arrays and structures
        if !self.cx.types.compatible(v.ty, ty) {
            self.cx.annex_e(
                "3.3.1",
                format!("type mismatch: expected {}, found {}", self.cx.types.name(ty), self.cx.types.name(v.ty)),
                span,
            );
            return;
        }
        match v.kind {
            VK::Mem(src) => {
                out.extend(v.pre);
                let bits = self.cx.types.size(ty);
                out.push(St::Copy { dst: loc, src, bits });
            }
            _ => {
                self.cx.error("invalid value for structured assignment", span);
            }
        }
    }

    // ------------------------------------------------------------ expressions

    pub fn expr(&mut self, e: &ast::Expr, expected: Option<TypeId>) -> V {
        match &e.kind {
            ExprKind::Error => V::err(&self.cx.types),
            ExprKind::Paren(inner) => self.expr(inner, expected),
            ExprKind::Literal(l) => self.literal(l, e.span),
            ExprKind::TypedEnum { ty, value } => self.typed_enum(ty, value, e.span),
            ExprKind::Name(_) | ExprKind::Field { .. } | ExprKind::Index { .. } | ExprKind::DirectVar(_) => {
                // A bare name may be an enumerated value.
                if let ExprKind::Name(id) = &e.kind {
                    if !self.is_variable_name(id) {
                        return self.enum_value(id, expected, e.span);
                    }
                }
                match self.place(e, false) {
                    Some(p) => {
                        self.check_readable(&p, e.span);
                        V::mem(p.loc, p.ty)
                    }
                    None => V::err(&self.cx.types),
                }
            }
            ExprKind::Call(c) => self.call_expr(c, expected, e.span),
            ExprKind::Unary { op, operand } => {
                let v = self.expr(operand, expected);
                self.unary(*op, v, e.span)
            }
            ExprKind::Binary { op, lhs, rhs } => {
                let hint = match op {
                    BinaryOp::Eq | BinaryOp::Ne | BinaryOp::Lt | BinaryOp::Gt | BinaryOp::Le | BinaryOp::Ge => None,
                    BinaryOp::Pow => None,
                    _ => expected,
                };
                let a = self.expr(lhs, hint);
                // The left operand's type is the expected type of the right one
                // (needed for enumerated values in comparisons).
                let hint_b = if a.is_literal() || a.is_err() { hint } else { Some(a.ty) };
                let b = self.expr(rhs, hint_b);
                let a = if a.is_literal() || matches!(a.kind, VK::Err) {
                    a
                } else {
                    a
                };
                self.binary(*op, a, b, e.span, lhs.span, rhs.span)
            }
        }
    }

    fn is_variable_name(&self, id: &Ident) -> bool {
        let key = id.key();
        if self.synth.contains_key(&key) {
            return true;
        }
        let p = &self.cx.pous[self.pou];
        p.var_map.contains_key(&key) || p.sfc.as_ref().map(|s| s.steps.iter().any(|x| x.name == key) || s.actions.iter().any(|a| a.name == key)).unwrap_or(false)
    }

    fn literal(&mut self, l: &ast::Literal, span: Span) -> V {
        let cv = match consteval::literal_value(&l.kind) {
            Ok(v) => v,
            Err(m) => return self.err(m, span),
        };
        if let Some(p) = &l.type_prefix {
            let key = p.key();
            let Some(t) = self.cx.named_type(&key, p.span) else {
                return self.err(format!("unknown type {} in typed literal", p), p.span);
            };
            if let ConstValue::Str { wide, chars } = &cv {
                let tw = self.cx.types.is_string(t).map(|(w, _)| w);
                if tw.is_none() {
                    return self.err(format!("{} is not a string type", p), p.span);
                }
                let wide = tw.unwrap_or(*wide);
                let addr = self.cx.string_literal(wide, chars);
                let ty = self.cx.types.string(wide, (chars.len() as u32).max(1));
                return V::mem(Loc::Abs(addr), ty);
            }
            if self.cx.types.elem_of(t).is_none() {
                return self.err(format!("{} cannot be used as a literal type prefix", p), p.span);
            }
            let ex = self.const_to(cv, t, span);
            return V::val(ex, t);
        }
        match cv {
            ConstValue::Int(i) => V { kind: VK::IntLit(i), ty: self.cx.types.error, pre: vec![] },
            ConstValue::Real(f) => V { kind: VK::RealLit(f), ty: self.cx.types.error, pre: vec![] },
            ConstValue::Bool(b) => V::val(Ex::bool(b), self.bool_ty()),
            ConstValue::Str { wide, chars } => {
                let addr = self.cx.string_literal(wide, &chars);
                let ty = self.cx.types.string(wide, (chars.len() as u32).max(1));
                V::mem(Loc::Abs(addr), ty)
            }
            ConstValue::Time(t) => V::val(Ex::Const(Ty::I64, t as u64), self.elem_ty(Elem::Time)),
            ConstValue::Date(t) => V::val(Ex::Const(Ty::I64, t as u64), self.elem_ty(Elem::Date)),
            ConstValue::Tod(t) => V::val(Ex::Const(Ty::I64, t as u64), self.elem_ty(Elem::Tod)),
            ConstValue::Dt(t) => V::val(Ex::Const(Ty::I64, t as u64), self.elem_ty(Elem::Dt)),
            ConstValue::Enum { ty, index } => V::val(Ex::Const(Ty::I32, index as u64), ty),
        }
    }

    fn typed_enum(&mut self, ty: &Ident, value: &Ident, span: Span) -> V {
        let Some(t) = self.cx.named_type(&ty.key(), ty.span) else {
            return self.err(format!("unknown type {}", ty), ty.span);
        };
        let Some(values) = self.cx.types.enum_values(t) else {
            return self.err(format!("{} is not an enumerated type", ty), ty.span);
        };
        match values.iter().position(|v| *v == value.key()) {
            Some(i) => V::val(Ex::Const(Ty::I32, i as u64), self.cx.types.resolve(t)),
            None => self.err(format!("{} is not a value of {}", value, ty), span),
        }
    }

    fn enum_value(&mut self, id: &Ident, expected: Option<TypeId>, span: Span) -> V {
        match crate::ctx::enum_lookup(&self.cx.types, &id.key(), expected) {
            Ok(ConstValue::Enum { ty, index }) => V::val(Ex::Const(Ty::I32, index as u64), ty),
            Ok(_) => unreachable!(),
            Err(m) => {
                if m.starts_with("ambiguous") {
                    self.cx.annex_e("2.3.3.1", m, span);
                } else {
                    self.cx.error(format!("unknown variable or value {}", id), span);
                }
                V::err(&self.cx.types)
            }
        }
    }

    // ------------------------------------------------------------ places

    pub fn check_readable(&mut self, p: &Place, span: Span) {
        let _ = (p, span);
    }

    /// Resolves a variable reference to a memory location.
    pub fn place(&mut self, e: &ast::Expr, write: bool) -> Option<Place> {
        match &e.kind {
            ExprKind::Paren(inner) => self.place(inner, write),
            ExprKind::Name(id) => self.name_place(id, write),
            ExprKind::DirectVar(d) => {
                if d.incomplete {
                    self.cx.error("an incompletely specified location cannot be used in an expression", e.span);
                    return None;
                }
                let key = d.to_text();
                if self.kind == PouKind::Function {
                    self.cx.annex_e("2.5.1", "directly represented variables cannot be used in functions", e.span);
                    return None;
                }
                let p = &self.cx.pous[self.pou];
                let Some(vi) = p.var_map.get(&key).copied() else {
                    self.cx.error(format!("unknown directly represented variable {}", key), e.span);
                    return None;
                };
                let v = &p.vars[vi];
                Some(Place { loc: v.loc(), ty: v.ty, read_only: None, var: Some(vi), is_result: false, fb_output: false })
            }
            ExprKind::Field { base, field } => self.field_place(base, field, write, e.span),
            ExprKind::Index { base, indices } => {
                let bp = self.place(base, write)?;
                let r = self.cx.types.resolve(bp.ty);
                let TypeKind::Array { dims, elem } = self.cx.types.kind(r).clone() else {
                    self.cx.error(format!("{} is not an array", self.cx.types.name(bp.ty)), base.span);
                    return None;
                };
                if dims.len() != indices.len() {
                    self.cx.error(format!("expected {} subscripts, found {}", dims.len(), indices.len()), e.span);
                    return None;
                }
                if self.language != Lang::St {
                    for i in indices {
                        if !matches!(i.kind, ExprKind::Name(_) | ExprKind::Literal(_) | ExprKind::DirectVar(_) | ExprKind::Field { .. }) {
                            self.cx.error_clause("2.4.1.2", "subscripts in IL and graphic languages are restricted to single-element variables or integer literals", i.span);
                        }
                    }
                }
                let stride = self.cx.types.stride(elem);
                let mut strides = vec![stride; dims.len()];
                for k in (0..dims.len().saturating_sub(1)).rev() {
                    let n = (dims[k + 1].1 - dims[k + 1].0 + 1) as u64;
                    strides[k] = strides[k + 1] * n;
                }
                let mut loc = bp.loc;
                for (k, ix) in indices.iter().enumerate() {
                    let i64t = self.elem_ty(Elem::LInt);
                    let v = self.expr(ix, Some(i64t));
                    let iv = match &v.kind {
                        VK::IntLit(n) => {
                            if *n < dims[k].0 as i128 || *n > dims[k].1 as i128 {
                                self.cx.error_clause("2.4.1.2", format!("subscript {} is outside the range {}..{}", n, dims[k].0, dims[k].1), ix.span);
                            }
                            Ex::Const(Ty::I64, *n as i64 as u64)
                        }
                        _ => {
                            let (ex, t) = self.scalar(v, ix.span);
                            match self.cx.types.elem_of(t) {
                                Some(el) if el.is_int() => Ex::Conv(el.ty(), Ty::I64, Box::new(ex)),
                                _ => {
                                    if !self.cx.types.is_error(t) {
                                        self.cx.error_clause("2.4.1.2", "a subscript shall be an expression of type ANY_INT", ix.span);
                                    }
                                    Ex::Const(Ty::I64, dims[k].0 as u64)
                                }
                            }
                        }
                    };
                    loc = Loc::Elem { base: Box::new(loc), index: Box::new(iv), lo: dims[k].0, hi: dims[k].1, stride: strides[k] };
                }
                Some(Place { loc, ty: elem, read_only: bp.read_only, var: bp.var, is_result: bp.is_result, fb_output: bp.fb_output })
            }
            _ => {
                self.cx.error("a variable is required here", e.span);
                None
            }
        }
    }

    fn name_place(&mut self, id: &Ident, write: bool) -> Option<Place> {
        let key = id.key();
        if let Some((loc, ty)) = self.synth.get(&key) {
            return Some(Place { loc: loc.clone(), ty: *ty, read_only: None, var: None, is_result: false, fb_output: false });
        }
        let p = &self.cx.pous[self.pou];
        if let Some(&vi) = p.var_map.get(&key) {
            let v = p.vars[vi].clone();
            if v.kind == VKind::Hidden && key.starts_with("__") && !self.cx.in_std {
                self.cx.error(format!("unknown variable {}", id), id.span);
                return None;
            }
            // Edge-triggered inputs are read through their detector.
            let (loc, ty) = match v.edge_result {
                Some(q) if !write => (p.vars[q].loc(), p.vars[q].ty),
                _ => (v.loc(), v.ty),
            };
            let mut read_only = None;
            if v.constant {
                read_only = Some((format!("{} is declared CONSTANT and cannot be modified", v.name), "2.4.3", true));
            } else if v.kind == VKind::Input && !self.cx.in_std {
                read_only = Some((format!("input variable {} cannot be modified within the program organization unit", v.name), "2.4.3", false));
            } else if self.for_vars.contains(&key) {
                read_only = Some((format!("the control variable {} of a FOR loop shall not be altered by the repeated statements", v.name), "3.3.2.4", false));
            }
            if v.kind == VKind::Hidden && v.location.is_some() && v.location.as_ref().unwrap().location == ast::LocationPrefix::Input {
                // writing to inputs of the programmable controller is allowed
            }
            return Some(Place { loc, ty, read_only, var: Some(vi), is_result: v.kind == VKind::Result, fb_output: false });
        }
        if let Some(sfc) = &p.sfc {
            if sfc.steps.iter().any(|s| s.name == key) || sfc.actions.iter().any(|a| a.name == key) {
                self.cx.error(format!("{} is a step or action name; use {}.X, {}.T or {}.Q", id, id, id, id), id.span);
                return None;
            }
        }
        if self.cx.pou_by_name.contains_key(&key) || self.cx.std_function_names.contains(&key) {
            self.cx.error(format!("{} is not a variable", id), id.span);
            return None;
        }
        self.cx.error(format!("unknown variable {}", id), id.span);
        None
    }

    fn field_place(&mut self, base: &ast::Expr, field: &Ident, write: bool, span: Span) -> Option<Place> {
        let fkey = field.key();
        // Step flags and times, action flags.
        if let ExprKind::Name(bid) = &base.kind {
            let bkey = bid.key();
            let p = &self.cx.pous[self.pou];
            if !p.var_map.contains_key(&bkey) {
                if let Some(sfc) = &p.sfc {
                    if let Some(s) = sfc.steps.iter().find(|s| s.name == bkey) {
                        let (off, ty) = match fkey.as_str() {
                            "X" => (s.x, self.bool_ty()),
                            "T" => (s.t, self.elem_ty(Elem::Time)),
                            _ => {
                                self.cx.error(format!("a step has only the elements X and T, not {}", field), field.span);
                                return None;
                            }
                        };
                        return Some(Place {
                            loc: Loc::Ip(off as u32),
                            ty,
                            read_only: Some((format!("the step {} {}.{} cannot be assigned by the user program", if fkey == "X" { "flag" } else { "time" }, bid, field), "2.6.2", true)),
                            var: None,
                            is_result: false,
                            fb_output: false,
                        });
                    }
                    if let Some(a) = sfc.actions.iter().find(|a| a.name == bkey) {
                        let ac = self.cx.pou_by_name["__ACTION_CONTROL"];
                        let Some(v) = self.cx.pous[ac].var(&fkey).cloned() else {
                            self.cx.error(format!("an action has only the elements Q and A, not {}", field), field.span);
                            return None;
                        };
                        if fkey != "Q" && fkey != "A" {
                            self.cx.error(format!("an action has only the elements Q and A, not {}", field), field.span);
                            return None;
                        }
                        return Some(Place {
                            loc: Loc::Ip((a.control + v.offset()) as u32),
                            ty: v.ty,
                            read_only: Some(("action flags are read-only".into(), "2.6.4.5", false)),
                            var: None,
                            is_result: false,
                            fb_output: false,
                        });
                    }
                }
            }
        }
        let bp = self.place(base, write)?;
        let r = self.cx.types.resolve(bp.ty);
        match self.cx.types.kind(r).clone() {
            TypeKind::Struct { fields, .. } => match fields.iter().find(|f| f.name == fkey) {
                Some(f) => Some(Place { loc: bp.loc.offset(f.offset), ty: f.ty, ..bp }),
                None => {
                    self.cx.error(format!("{} has no element named {}", self.cx.types.name(bp.ty), field), field.span);
                    None
                }
            },
            TypeKind::Instance { pou } => {
                let Some(vi) = self.cx.pous[pou].var_map.get(&fkey).copied() else {
                    self.cx.error(format!("{} has no variable named {}", self.cx.pous[pou].name, field), field.span);
                    return None;
                };
                let v = self.cx.pous[pou].vars[vi].clone();
                let inside_std = self.cx.in_std;
                let mut read_only = bp.read_only.clone();
                let mut fb_output = false;
                match v.kind {
                    VKind::Output => {
                        fb_output = true;
                        read_only = Some((
                            format!("the output {} of a function block can only be assigned from within the function block", field),
                            "2.5.2.1",
                            false,
                        ));
                    }
                    VKind::Input => {
                        if !write && !inside_std {
                            self.cx.error_clause("2.5.2.1", format!("the input {} of a function block cannot be read from outside the function block (table 32)", field), span);
                        } else if write && self.language != Lang::Il && !inside_std {
                            read_only = Some((
                                format!("the input {} of a function block can only be assigned as part of its invocation", field),
                                "2.5.2.1",
                                false,
                            ));
                        }
                    }
                    VKind::InOut => {}
                    _ => {
                        if !inside_std {
                            self.cx.error_clause("2.5.2", format!("the internal variable {} of a function block is not accessible from outside", field), field.span);
                            return None;
                        }
                    }
                }
                let loc = match v.storage {
                    Storage::Inst(o) => bp.loc.offset(o),
                    Storage::InstRef(o) => bp.loc.offset(o).deref(),
                    _ => unreachable!(),
                };
                Some(Place { loc, ty: v.ty, read_only, var: bp.var, is_result: false, fb_output })
            }
            _ => {
                if !self.cx.types.is_error(r) {
                    self.cx.error(format!("{} has no elements", self.cx.types.name(bp.ty)), base.span);
                }
                None
            }
        }
    }

    /// Resolves an assignment target and checks that it is writable.
    pub fn target(&mut self, e: &ast::Expr) -> Option<Place> {
        let p = self.place(e, true)?;
        if let Some((msg, clause, annex)) = &p.read_only {
            if *annex {
                self.cx.annex_e(clause, msg.clone(), e.span);
            } else {
                self.cx.error_clause(clause, msg.clone(), e.span);
            }
        }
        if self.region == Region::Transition {
            self.cx.annex_e("2.6.3", "side effects are not allowed in the evaluation of a transition condition", e.span);
        }
        Some(p)
    }

    // ------------------------------------------------------------ operators

    fn unary(&mut self, op: UnaryOp, v: V, span: Span) -> V {
        match (op, &v.kind) {
            (_, VK::Err) => v,
            (UnaryOp::Neg, VK::IntLit(i)) => V { kind: VK::IntLit(-i), ..v },
            (UnaryOp::Neg, VK::RealLit(f)) => V { kind: VK::RealLit(-f), ..v },
            (UnaryOp::Neg, _) => {
                let (ex, ty) = self.scalar(v, span);
                let bt = self.base_elem_ty(ty);
                match self.cx.types.elem_of(ty) {
                    Some(e) if e.is_signed_int() || e.is_real() || e == Elem::Time => V::val(Ex::Un(UnOp::Neg, e.ty(), Box::new(ex)), bt),
                    _ => {
                        self.cx.annex_e("3.3.1", format!("negation is not defined for {}", self.cx.types.name(ty)), span);
                        V::err(&self.cx.types)
                    }
                }
            }
            (UnaryOp::Not, VK::IntLit(_)) | (UnaryOp::Not, VK::RealLit(_)) => {
                let b = self.bool_ty();
                let ex = self.coerce(v, b, span);
                V::val(Ex::not(ex), b)
            }
            (UnaryOp::Not, _) => {
                let (ex, ty) = self.scalar(v, span);
                match self.cx.types.elem_of(ty) {
                    Some(e) if e.is_bit() => V::val(Ex::Un(UnOp::Not, e.ty(), Box::new(ex)), self.base_elem_ty(ty)),
                    _ => {
                        self.cx.annex_e("3.3.1", format!("NOT requires an operand of type ANY_BIT, found {}", self.cx.types.name(ty)), span);
                        V::err(&self.cx.types)
                    }
                }
            }
        }
    }

    /// Brings two operands to a common type.
    pub fn unify(&mut self, a: V, b: V, span: Span) -> Option<(V, V, TypeId)> {
        if a.is_err() || b.is_err() {
            return None;
        }
        match (a.is_literal(), b.is_literal()) {
            (true, true) => {
                if let Some(h) = self.lit_hint {
                    let is_real = matches!(a.kind, VK::RealLit(_)) || matches!(b.kind, VK::RealLit(_));
                    let ok = match self.cx.types.elem_of(h) {
                        Some(e) if is_real => e.is_real(),
                        Some(e) => e.is_num() || e.is_bit(),
                        None => false,
                    };
                    if ok {
                        let t = self.base_elem_ty(h);
                        return Some((a, b, t));
                    }
                }
                let t = if matches!(a.kind, VK::RealLit(_)) || matches!(b.kind, VK::RealLit(_)) {
                    self.elem_ty(Elem::LReal)
                } else {
                    let ta = self.default_lit_type(&a.kind);
                    let tb = self.default_lit_type(&b.kind);
                    if self.cx.types.size(ta) >= self.cx.types.size(tb) {
                        ta
                    } else {
                        tb
                    }
                };
                Some((a, b, t))
            }
            (true, false) => {
                let t = b.ty;
                Some((a, b, t))
            }
            (false, true) => {
                let t = a.ty;
                Some((a, b, t))
            }
            (false, false) => {
                let (ta, tb) = (a.ty, b.ty);
                if self.cx.types.compatible(ta, tb) {
                    return Some((a, b, ta));
                }
                if let (Some(ea), Some(eb)) = (self.cx.types.elem_of(ta), self.cx.types.elem_of(tb)) {
                    if widens(ea, eb) {
                        return Some((a, b, tb));
                    }
                    if widens(eb, ea) {
                        return Some((a, b, ta));
                    }
                }
                if let (Some((w1, _)), Some((w2, _))) = (self.cx.types.is_string(ta), self.cx.types.is_string(tb)) {
                    if w1 == w2 {
                        return Some((a, b, ta));
                    }
                }
                self.cx.annex_e(
                    "3.3.1",
                    format!("operands are not of the same data type ({} and {})", self.cx.types.name(ta), self.cx.types.name(tb)),
                    span,
                );
                None
            }
        }
    }

    fn fold_int(op: BinaryOp, x: i128, y: i128) -> Option<VK> {
        Some(match op {
            BinaryOp::Add => VK::IntLit(x.checked_add(y)?),
            BinaryOp::Sub => VK::IntLit(x.checked_sub(y)?),
            BinaryOp::Mul => VK::IntLit(x.checked_mul(y)?),
            BinaryOp::Div if y != 0 => VK::IntLit(x / y),
            BinaryOp::Mod => VK::IntLit(if y == 0 { 0 } else { x % y }),
            _ => return None,
        })
    }

    pub fn binary(&mut self, op: BinaryOp, a: V, b: V, span: Span, sa: Span, sb: Span) -> V {
        if a.is_err() || b.is_err() {
            return V::err(&self.cx.types);
        }
        // constant folding of literals
        if let (VK::IntLit(x), VK::IntLit(y)) = (&a.kind, &b.kind) {
            if op == BinaryOp::Div && *y == 0 {
                self.cx.annex_e("3.3.1", "division by zero", span);
                return V::err(&self.cx.types);
            }
            if let Some(k) = Self::fold_int(op, *x, *y) {
                return V { kind: k, ty: self.cx.types.error, pre: vec![] };
            }
        }
        if let (VK::RealLit(x), VK::RealLit(y)) = (&a.kind, &b.kind) {
            let r = match op {
                BinaryOp::Add => Some(x + y),
                BinaryOp::Sub => Some(x - y),
                BinaryOp::Mul => Some(x * y),
                BinaryOp::Div if *y != 0.0 => Some(x / y),
                _ => None,
            };
            if let Some(r) = r {
                return V { kind: VK::RealLit(r), ty: self.cx.types.error, pre: vec![] };
            }
        }
        match op {
            BinaryOp::Add | BinaryOp::Sub | BinaryOp::Mul | BinaryOp::Div => {
                if let Some(v) = self.time_arith(op, &a, &b, span) {
                    return v;
                }
                let (a, b, t) = match self.unify(a, b, span) {
                    Some(x) => x,
                    None => return V::err(&self.cx.types),
                };
                let bt = self.base_elem_ty(t);
                match self.cx.types.elem_of(t) {
                    Some(e) if e.is_num() => {}
                    Some(Elem::Time) if matches!(op, BinaryOp::Add | BinaryOp::Sub) => {}
                    _ => {
                        self.cx.annex_e("3.3.1", format!("operator {} is not defined for {}", op.symbol(), self.cx.types.name(t)), span);
                        return V::err(&self.cx.types);
                    }
                }
                let ea = self.coerce(a, bt, sa);
                let eb = self.coerce(b, bt, sb);
                let bop = match op {
                    BinaryOp::Add => BinOp::Add,
                    BinaryOp::Sub => BinOp::Sub,
                    BinaryOp::Mul => BinOp::Mul,
                    _ => BinOp::Div,
                };
                V::val(Ex::Bin(bop, self.mty(bt), Box::new(ea), Box::new(eb)), bt)
            }
            BinaryOp::Mod => {
                let (a, b, t) = match self.unify(a, b, span) {
                    Some(x) => x,
                    None => return V::err(&self.cx.types),
                };
                let bt = self.base_elem_ty(t);
                if !self.cx.types.elem_of(t).map(|e| e.is_int()).unwrap_or(false) {
                    self.cx.annex_e("3.3.1", format!("MOD requires operands of type ANY_INT, found {}", self.cx.types.name(t)), span);
                    return V::err(&self.cx.types);
                }
                let ea = self.coerce(a, bt, sa);
                let eb = self.coerce(b, bt, sb);
                V::val(Ex::Bin(BinOp::Mod, self.mty(bt), Box::new(ea), Box::new(eb)), bt)
            }
            BinaryOp::Pow => self.expt(a, b, span, sa, sb),
            BinaryOp::And | BinaryOp::Or | BinaryOp::Xor => {
                let (a, b, t) = match self.unify(a, b, span) {
                    Some(x) => x,
                    None => return V::err(&self.cx.types),
                };
                let t = if a.is_literal() && b.is_literal() { self.bool_ty() } else { t };
                let bt = self.base_elem_ty(t);
                if !self.cx.types.elem_of(t).map(|e| e.is_bit()).unwrap_or(false) {
                    self.cx.annex_e("3.3.1", format!("{} requires operands of type ANY_BIT, found {}", op.symbol(), self.cx.types.name(t)), span);
                    return V::err(&self.cx.types);
                }
                let ea = self.coerce(a, bt, sa);
                let eb = self.coerce(b, bt, sb);
                let bop = match op {
                    BinaryOp::And => BinOp::And,
                    BinaryOp::Or => BinOp::Or,
                    _ => BinOp::Xor,
                };
                V::val(Ex::Bin(bop, self.mty(bt), Box::new(ea), Box::new(eb)), bt)
            }
            BinaryOp::Eq | BinaryOp::Ne | BinaryOp::Lt | BinaryOp::Gt | BinaryOp::Le | BinaryOp::Ge => {
                let c = match op {
                    BinaryOp::Eq => CmpOp::Eq,
                    BinaryOp::Ne => CmpOp::Ne,
                    BinaryOp::Lt => CmpOp::Lt,
                    BinaryOp::Gt => CmpOp::Gt,
                    BinaryOp::Le => CmpOp::Le,
                    _ => CmpOp::Ge,
                };
                self.compare(c, a, b, span, sa, sb)
            }
        }
    }

    /// Comparison of two values (table 28, 2.5.1.5.5 for strings).
    pub fn compare(&mut self, c: CmpOp, a: V, b: V, span: Span, sa: Span, sb: Span) -> V {
        let bool_ty = self.bool_ty();
        let (a, b, t) = match self.unify(a, b, span) {
            Some(x) => x,
            None => return V::err(&self.cx.types),
        };
        if let Some((wide, _)) = self.cx.types.is_string(t) {
            let mut pre = Vec::new();
            pre.extend(a.pre.clone());
            pre.extend(b.pre.clone());
            let (VK::Mem(la), VK::Mem(lb)) = (a.kind, b.kind) else {
                return self.err("invalid string operand", span);
            };
            let cmp = Ex::Native(Native::StrCmp { wide }, vec![Ex::Addr(la), Ex::Addr(lb)]);
            let ex = Ex::Cmp(c, Ty::I64, Box::new(cmp), Box::new(Ex::Const(Ty::I64, 0)));
            return V { kind: VK::Val(ex), ty: bool_ty, pre };
        }
        if self.cx.types.is_enum(t) {
            if !matches!(c, CmpOp::Eq | CmpOp::Ne) {
                self.cx.annex_e("2.5.1.5.7", "only EQ and NE comparisons are defined for enumerated types (table 31)", span);
                return V::err(&self.cx.types);
            }
        } else if self.cx.types.elem_of(t).is_none() {
            self.cx.annex_e("3.3.1", format!("comparison is not defined for {}", self.cx.types.name(t)), span);
            return V::err(&self.cx.types);
        }
        let bt = if self.cx.types.is_enum(t) { self.cx.types.resolve(t) } else { self.base_elem_ty(t) };
        let ea = self.coerce(a, bt, sa);
        let eb = self.coerce(b, bt, sb);
        V::val(Ex::Cmp(c, self.mty(bt), Box::new(ea), Box::new(eb)), bool_ty)
    }

    fn expt(&mut self, a: V, b: V, span: Span, sa: Span, sb: Span) -> V {
        let a = if let VK::IntLit(i) = a.kind { V { kind: VK::RealLit(i as f64), ..a } } else { a };
        let a = self.settle(a, sa);
        let at = a.ty;
        if !self.cx.types.elem_of(at).map(|e| e.is_real()).unwrap_or(false) {
            self.cx.annex_e("2.5.1.5.2", format!("the base of ** (EXPT) shall be of type ANY_REAL, found {}", self.cx.types.name(at)), span);
            return V::err(&self.cx.types);
        }
        let b = self.settle(b, sb);
        let bt = b.ty;
        let Some(be) = self.cx.types.elem_of(bt).filter(|e| e.is_num()) else {
            self.cx.annex_e("2.5.1.5.2", format!("the exponent of ** (EXPT) shall be of type ANY_NUM, found {}", self.cx.types.name(bt)), span);
            return V::err(&self.cx.types);
        };
        let rt = self.base_elem_ty(at);
        let (ea, _) = self.scalar(a, sa);
        let (eb, _) = self.scalar(b, sb);
        V::val(Ex::Bin(BinOp::Expt(be.ty()), self.mty(rt), Box::new(ea), Box::new(eb)), rt)
    }

    /// Arithmetic on time data types (table 30). Returns None if the operands
    /// are not time types.
    fn time_arith(&mut self, op: BinaryOp, a: &V, b: &V, span: Span) -> Option<V> {
        let ea = if a.is_literal() { None } else { self.cx.types.elem_of(a.ty) };
        let eb = if b.is_literal() { None } else { self.cx.types.elem_of(b.ty) };
        use Elem::*;
        let is_time = |e: Option<Elem>| matches!(e, Some(Time | Date | Tod | Dt));
        if !is_time(ea) && !is_time(eb) {
            return None;
        }
        let a = a.clone();
        let b = b.clone();
        let t = |s: &Self, e: Elem| s.elem_ty(e);
        let deprecated = |s: &mut Self| {
            s.cx.diags.push(
                Diagnostic::warning("this overloaded use of a time arithmetic operator is deprecated; use the typed function (table 30 footnote b)", span)
                    .with_clause("2.5.1.5.6"),
            );
        };
        let i64_ = Ty::I64;
        let day = MICROS_PER_DAY;
        match (op, ea, eb) {
            (BinaryOp::Add | BinaryOp::Sub, Some(Time), Some(Time)) | (BinaryOp::Add | BinaryOp::Sub, Some(Time), None) | (BinaryOp::Add | BinaryOp::Sub, None, Some(Time)) => {
                None // ordinary ANY_MAGNITUDE arithmetic
            }
            (BinaryOp::Add, Some(Tod), Some(Time)) | (BinaryOp::Sub, Some(Tod), Some(Time)) => {
                deprecated(self);
                let (x, _) = self.scalar(a, span);
                let (y, _) = self.scalar(b, span);
                let bop = if op == BinaryOp::Add { BinOp::Add } else { BinOp::Sub };
                let r = Ex::Bin(bop, i64_, Box::new(x), Box::new(y));
                Some(V::val(Ex::Check { ty: i64_, lo: 0, hi: day - 1, code: TrapCode::TimeRange, value: Box::new(r) }, t(self, Tod)))
            }
            (BinaryOp::Add, Some(Dt), Some(Time)) | (BinaryOp::Sub, Some(Dt), Some(Time)) => {
                deprecated(self);
                let (x, _) = self.scalar(a, span);
                let (y, _) = self.scalar(b, span);
                let bop = if op == BinaryOp::Add { BinOp::Add } else { BinOp::Sub };
                let r = Ex::Bin(bop, i64_, Box::new(x), Box::new(y));
                let max = structure_runtime::calendar::dt_max();
                Some(V::val(Ex::Check { ty: i64_, lo: 0, hi: max, code: TrapCode::TimeRange, value: Box::new(r) }, t(self, Dt)))
            }
            (BinaryOp::Sub, Some(Date), Some(Date)) | (BinaryOp::Sub, Some(Tod), Some(Tod)) | (BinaryOp::Sub, Some(Dt), Some(Dt)) => {
                deprecated(self);
                let (x, _) = self.scalar(a, span);
                let (y, _) = self.scalar(b, span);
                Some(V::val(Ex::Bin(BinOp::Sub, i64_, Box::new(x), Box::new(y)), t(self, Time)))
            }
            (BinaryOp::Mul | BinaryOp::Div, Some(Time), _) => {
                let bop = if op == BinaryOp::Mul { BinOp::Mul } else { BinOp::Div };
                let (x, _) = self.scalar(a, span);
                Some(self.time_scale(bop, x, b, span))
            }
            (BinaryOp::Mul, _, Some(Time)) => {
                let (y, _) = self.scalar(b, span);
                Some(self.time_scale(BinOp::Mul, y, a, span))
            }
            _ => {
                self.cx.annex_e("2.5.1.5.6", format!("operator {} is not defined for these time data types (table 30)", op.symbol()), span);
                Some(V::err(&self.cx.types))
            }
        }
    }

    /// TIME multiplied or divided by an ANY_NUM value (MULTIME, DIVTIME).
    pub fn time_scale(&mut self, op: BinOp, time: Ex, factor: V, span: Span) -> V {
        let time_ty = self.elem_ty(Elem::Time);
        let factor = self.settle(factor, span);
        let (f, ft) = self.scalar(factor, span);
        match self.cx.types.elem_of(ft) {
            Some(e) if e.is_int() => {
                let fx = Ex::Conv(e.ty(), Ty::I64, Box::new(f));
                V::val(Ex::Bin(op, Ty::I64, Box::new(time), Box::new(fx)), time_ty)
            }
            Some(e) if e.is_real() => {
                let tf = Ex::Conv(Ty::I64, Ty::F64, Box::new(time));
                let ff = Ex::Conv(e.ty(), Ty::F64, Box::new(f));
                let r = Ex::Bin(op, Ty::F64, Box::new(tf), Box::new(ff));
                V::val(Ex::Conv(Ty::F64, Ty::I64, Box::new(r)), time_ty)
            }
            _ => {
                if !self.cx.types.is_error(ft) {
                    self.cx.annex_e("2.5.1.5.6", "a TIME value can only be multiplied or divided by an ANY_NUM value", span);
                }
                V::err(&self.cx.types)
            }
        }
    }

    /// Lowers an expression that must be BOOL.
    pub fn bool_expr(&mut self, e: &ast::Expr) -> Ex {
        let b = self.bool_ty();
        let v = self.expr(e, Some(b));
        self.coerce(v, b, e.span)
    }

    /// Makes an aggregate value available at a memory location.
    pub fn materialize(&mut self, v: V, out: &mut Vec<St>, span: Span) -> Option<(Loc, TypeId)> {
        let v = self.settle(v, span);
        match v.kind {
            VK::Mem(l) => {
                out.extend(v.pre);
                Some((l, v.ty))
            }
            VK::Val(ex) => {
                let mt = self.mty(v.ty);
                let loc = self.temp_typed(v.ty);
                out.push(St::Store(mt, loc.clone(), Self::with_pre(v.pre, ex)));
                Some((loc, v.ty))
            }
            _ => None,
        }
    }

    pub fn unused(&self) -> u64 {
        mty::to_i128(Ty::I8, 0) as u64
    }
}
