//! Lowering of POU bodies to the IR, with type checking.

pub mod call;
pub mod expr;
pub mod il;
pub mod init;
pub mod pou;
pub mod sfc;
pub mod stmt;
pub mod graphic;

use crate::ctx::*;
use crate::ir::*;
use crate::types::*;
use structure_runtime::ty::Ty;
use structure_syntax::Span;

/// A lowered expression value.
#[derive(Clone, Debug)]
pub struct V {
    pub kind: VK,
    pub ty: TypeId,
    /// Statements that must execute before the value is used.
    pub pre: Vec<St>,
}

#[derive(Clone, Debug)]
pub enum VK {
    /// A scalar value.
    Val(Ex),
    /// A value stored in memory (any type).
    Mem(Loc),
    /// Untyped integer literal.
    IntLit(i128),
    /// Untyped real literal.
    RealLit(f64),
    /// Error already reported.
    Err,
}

impl V {
    pub fn err(types: &Types) -> V {
        V { kind: VK::Err, ty: types.error, pre: vec![] }
    }
    pub fn val(ex: Ex, ty: TypeId) -> V {
        V { kind: VK::Val(ex), ty, pre: vec![] }
    }
    pub fn mem(loc: Loc, ty: TypeId) -> V {
        V { kind: VK::Mem(loc), ty, pre: vec![] }
    }
    pub fn is_err(&self) -> bool {
        matches!(self.kind, VK::Err)
    }
    pub fn is_literal(&self) -> bool {
        matches!(self.kind, VK::IntLit(_) | VK::RealLit(_))
    }
}

/// Context of the code being lowered (affects name resolution and checks).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Region {
    Body,
    /// Transition condition: no side effects allowed (2.6.3).
    Transition,
    /// Action body.
    Action,
}

pub struct Lowerer<'c, 's> {
    pub cx: &'c mut Ctx<'s>,
    pub pou: usize,
    pub kind: PouKind,
    /// Next free byte of the frame (temporaries).
    pub frame_top: u64,
    pub next_label: u32,
    /// Exit labels of enclosing loops.
    pub loops: Vec<u32>,
    pub return_label: u32,
    /// Control variables of enclosing FOR loops.
    pub for_vars: Vec<String>,
    pub region: Region,
    /// Language of the code being lowered (for language-specific rules).
    pub language: Lang,
    /// Type to give to operations whose operands are all untyped literals.
    pub lit_hint: Option<TypeId>,
    /// Compiler-generated variables (names starting with `$`), used by the
    /// IL and graphical lowering to refer to temporaries from syntax trees.
    pub synth: std::collections::HashMap<String, (Loc, TypeId)>,
    pub synth_count: u32,
    /// IL: type expected by the instruction consuming the current result.
    pub il_hint: Option<TypeId>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Lang {
    St,
    Il,
    Graphic,
}

impl<'c, 's> Lowerer<'c, 's> {
    pub fn new(cx: &'c mut Ctx<'s>, pou: usize) -> Self {
        let kind = cx.pous[pou].kind;
        let frame_top = cx.pous[pou].frame_base;
        Lowerer { cx, pou, kind, frame_top, next_label: 1, loops: Vec::new(), return_label: 0, for_vars: Vec::new(), region: Region::Body, language: Lang::St, lit_hint: None, synth: Default::default(), synth_count: 0, il_hint: None }
    }

    pub fn label(&mut self) -> u32 {
        self.next_label += 1;
        self.next_label
    }

    /// Allocates a temporary of `bits` bits in the frame.
    pub fn temp(&mut self, bits: u64) -> Loc {
        let off = self.frame_top;
        self.frame_top += round8(bits.max(8)) / 8;
        Loc::Fp((off * 8) as u32)
    }

    pub fn temp_typed(&mut self, ty: TypeId) -> Loc {
        let bits = self.cx.types.size(ty);
        self.temp(bits)
    }

    pub fn types(&self) -> &Types {
        &self.cx.types
    }

    pub fn pou_info(&self) -> &PouInfo {
        &self.cx.pous[self.pou]
    }

    pub fn bool_ty(&self) -> TypeId {
        self.cx.types.elem(Elem::Bool)
    }

    pub fn elem_ty(&self, e: Elem) -> TypeId {
        self.cx.types.elem(e)
    }

    pub fn mty(&self, ty: TypeId) -> Ty {
        self.cx.types.scalar(ty).unwrap_or(Ty::U64)
    }

    pub fn err(&mut self, msg: impl Into<String>, span: Span) -> V {
        self.cx.error(msg, span);
        V::err(&self.cx.types)
    }

    /// Registers a synthetic variable for a location; returns its name.
    pub fn synth_var(&mut self, prefix: &str, loc: Loc, ty: TypeId) -> String {
        self.synth_count += 1;
        let name = format!("${}{}", prefix, self.synth_count);
        self.synth.insert(name.clone(), (loc, ty));
        name
    }

    /// A syntax-tree reference to a synthetic variable.
    pub fn synth_expr(name: &str, span: Span) -> structure_syntax::ast::Expr {
        structure_syntax::ast::Expr {
            kind: structure_syntax::ast::ExprKind::Name(structure_syntax::Ident::new(name, span)),
            span,
        }
    }

    /// Initialises a temporary string so that natives know its capacity.
    pub fn string_temp(&mut self, wide: bool, len: u32, out: &mut Vec<St>) -> (Loc, TypeId) {
        let ty = self.cx.types.string(wide, len);
        let loc = self.temp_typed(ty);
        out.push(St::Native(structure_runtime::image::Native::StrInit { wide, cap: len }, vec![Ex::Addr(loc.clone())]));
        (loc, ty)
    }
}
