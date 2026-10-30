//! Initialisation of variables (2.4.2, 2.3.3.2). Memory is always zeroed
//! before initialisation, so only non-zero values are written.

use super::*;
use crate::consteval;
use structure_runtime::image::Native;

/// Whether initialising a value of this type (with its defaults) writes
/// anything.
pub fn needs_init(cx: &Ctx, ty: TypeId) -> bool {
    let t = cx.types.get(ty);
    if let Some(i) = &t.init {
        return init_nonzero(cx, ty, i);
    }
    match &t.kind {
        TypeKind::Alias { base, .. } => needs_init(cx, *base),
        TypeKind::Subrange { lo, .. } => *lo != 0,
        TypeKind::Elem(_) | TypeKind::Enum { .. } | TypeKind::Error => false,
        TypeKind::String { .. } => true,
        TypeKind::Array { elem, .. } => needs_init(cx, *elem),
        TypeKind::Struct { fields, .. } => fields.iter().any(|f| f.init.as_ref().map(|i| init_nonzero(cx, f.ty, i)).unwrap_or(false) || needs_init(cx, f.ty)),
        TypeKind::Instance { pou } => instance_needs_init(cx, *pou),
    }
}

pub fn instance_needs_init(cx: &Ctx, pou: usize) -> bool {
    let p = &cx.pous[pou];
    if p.sfc.as_ref().map(|s| !s.steps.is_empty()).unwrap_or(false) {
        return true;
    }
    p.vars.iter().any(|v| {
        if matches!(v.kind, VKind::Temp | VKind::External | VKind::InOut) {
            return false;
        }
        if v.is_ref() && v.location.is_none() {
            return false;
        }
        match &v.init {
            Some(i) => init_nonzero(cx, v.ty, i) || needs_init(cx, v.ty),
            None => needs_init(cx, v.ty),
        }
    })
}

fn init_nonzero(cx: &Ctx, ty: TypeId, i: &InitValue) -> bool {
    match i {
        InitValue::Const(c) => match c {
            ConstValue::Str { .. } => true,
            c => consteval::encode(&cx.types, c, ty).map(|v| v != 0).unwrap_or(true),
        },
        _ => true,
    }
}

impl<'c, 's> Lowerer<'c, 's> {
    /// Emits code initialising `loc` (of type `ty`) with `init`, or with
    /// the default initial value of the type.
    pub fn emit_init(&mut self, out: &mut Vec<St>, loc: Loc, ty: TypeId, init: Option<&InitValue>) {
        let default;
        let init = match init {
            Some(i) => i,
            None => {
                if !needs_init(self.cx, ty) {
                    return;
                }
                default = self.cx.default_init(ty);
                &default
            }
        };
        let r = self.cx.types.resolve(ty);
        match self.cx.types.kind(r).clone() {
            TypeKind::Elem(_) | TypeKind::Subrange { .. } | TypeKind::Enum { .. } => {
                if let InitValue::Const(c) = init {
                    let mt = self.mty(ty);
                    if let Some(v) = consteval::encode(&self.cx.types, c, ty) {
                        if v != 0 {
                            out.push(St::Store(mt, loc, Ex::Const(mt, v)));
                        }
                    }
                }
            }
            TypeKind::String { wide, len } => {
                out.push(St::Native(Native::StrInit { wide, cap: len }, vec![Ex::Addr(loc.clone())]));
                if let InitValue::Const(ConstValue::Str { chars, .. }) = init {
                    if !chars.is_empty() {
                        let addr = self.cx.string_literal(wide, chars);
                        out.push(St::Native(Native::StrAssign { wide }, vec![Ex::Addr(loc), Ex::Addr(Loc::Abs(addr))]));
                    }
                }
            }
            TypeKind::Array { dims, elem } => {
                let count: i64 = dims.iter().map(|(l, h)| h - l + 1).product();
                let stride = self.cx.types.stride(elem);
                let given: Vec<InitValue> = match init {
                    InitValue::Array(v) => v.clone(),
                    _ => Vec::new(),
                };
                for (i, v) in given.iter().enumerate() {
                    let el = loc.clone().offset_elem(i as u64 * stride);
                    self.emit_init(out, el, elem, Some(v));
                }
                let rest = count - given.len() as i64;
                if rest > 0 && needs_init(self.cx, elem) {
                    if rest <= 8 {
                        for i in given.len() as i64..count {
                            let el = loc.clone().offset_elem(i as u64 * stride);
                            self.emit_init(out, el, elem, None);
                        }
                    } else {
                        // loop over the remaining elements
                        let idx = self.temp(64);
                        let top = self.label();
                        let end = self.label();
                        out.push(St::Store(Ty::I64, idx.clone(), Ex::Const(Ty::I64, given.len() as u64)));
                        out.push(St::Label(top));
                        out.push(St::JumpIfNot(
                            Ex::Cmp(
                                structure_runtime::image::CmpOp::Lt,
                                Ty::I64,
                                Box::new(Ex::Load(Ty::I64, idx.clone())),
                                Box::new(Ex::Const(Ty::I64, count as u64)),
                            ),
                            end,
                        ));
                        let el = Loc::Elem { base: Box::new(loc.clone()), index: Box::new(Ex::Load(Ty::I64, idx.clone())), lo: 0, hi: count - 1, stride };
                        self.emit_init(out, el, elem, None);
                        out.push(St::Store(
                            Ty::I64,
                            idx.clone(),
                            Ex::Bin(BinOp::Add, Ty::I64, Box::new(Ex::Load(Ty::I64, idx)), Box::new(Ex::Const(Ty::I64, 1))),
                        ));
                        out.push(St::Jump(top));
                        out.push(St::Label(end));
                    }
                }
            }
            TypeKind::Struct { fields, .. } => {
                let overrides: Vec<(String, InitValue)> = match init {
                    InitValue::Struct(v) => v.clone(),
                    _ => Vec::new(),
                };
                // A derived structure type may carry its own initial values.
                let type_init: Vec<(String, InitValue)> = match &self.cx.types.get(ty).init {
                    Some(InitValue::Struct(v)) => v.clone(),
                    _ => Vec::new(),
                };
                for f in fields {
                    let v = overrides
                        .iter()
                        .find(|(n, _)| *n == f.name)
                        .or_else(|| type_init.iter().find(|(n, _)| *n == f.name))
                        .map(|(_, v)| v.clone())
                        .or_else(|| f.init.clone());
                    self.emit_init(out, loc.clone().offset(f.offset), f.ty, v.as_ref());
                }
            }
            TypeKind::Instance { pou } => {
                if instance_needs_init(self.cx, pou) {
                    let r = self.cx.pous[pou].init_routine.unwrap_or(usize::MAX);
                    out.push(St::CallFb { routine: r, inst: Ex::Addr(loc.clone()) });
                }
                if let InitValue::Struct(ov) = init {
                    for (name, v) in ov {
                        if let Some(var) = self.cx.pous[pou].var(name).cloned() {
                            if let Storage::Inst(o) = var.storage {
                                self.emit_init(out, loc.clone().offset(o), var.ty, Some(v));
                            }
                        }
                    }
                }
            }
            TypeKind::Alias { .. } | TypeKind::Error => {}
        }
    }

    /// Statements initialising all variables of the instance (`Ip`) or the
    /// frame of the current POU that belong to the given kinds.
    pub fn init_vars(&mut self, out: &mut Vec<St>, kinds: &[VKind]) {
        let vars = self.cx.pous[self.pou].vars.clone();
        for v in vars {
            if !kinds.contains(&v.kind) {
                continue;
            }
            if v.is_ref() && v.location.is_none() {
                continue;
            }
            let init = v.init.clone();
            self.emit_init(out, v.loc(), v.ty, init.as_ref());
        }
    }
}

impl Loc {
    /// Offset used for array elements (handles bit offsets of BOOL arrays).
    pub fn offset_elem(self, bits: u64) -> Loc {
        self.offset(bits)
    }
}
