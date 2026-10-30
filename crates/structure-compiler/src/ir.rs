//! Typed intermediate representation shared by all languages. Every
//! language (ST, IL, LD, FBD, SFC) is lowered to this form, which is then
//! translated to bytecode by [`crate::codegen`].

use structure_runtime::error::TrapCode;
use structure_runtime::image::{CmpOp, MathFn, Native};
use structure_runtime::ty::Ty;
use structure_syntax::Span;

/// A memory location. Offsets and addresses are in bits.
#[derive(Clone, Debug)]
pub enum Loc {
    Abs(u64),
    Fp(u32),
    Ip(u32),
    /// Pending callee frame.
    Nf(u32),
    /// Address given by an expression.
    Ind(Box<Ex>),
    /// Array element with bounds checking.
    Elem { base: Box<Loc>, index: Box<Ex>, lo: i64, hi: i64, stride: u64 },
}

impl Loc {
    /// Adds a constant bit offset.
    pub fn offset(self, bits: u64) -> Loc {
        if bits == 0 {
            return self;
        }
        match self {
            Loc::Abs(a) => Loc::Abs(a + bits),
            Loc::Fp(o) => Loc::Fp(o + bits as u32),
            Loc::Ip(o) => Loc::Ip(o + bits as u32),
            Loc::Nf(o) => Loc::Nf(o + bits as u32),
            Loc::Ind(e) => match *e {
                Ex::Offset(a, o) => Loc::Ind(Box::new(Ex::Offset(a, o + bits as i64))),
                e => Loc::Ind(Box::new(Ex::Offset(Box::new(e), bits as i64))),
            },
            l @ Loc::Elem { .. } => Loc::Ind(Box::new(Ex::Offset(Box::new(Ex::Addr(l)), bits as i64))),
        }
    }

    /// Location whose address is stored at this location (a reference).
    pub fn deref(self) -> Loc {
        Loc::Ind(Box::new(Ex::Load(Ty::U64, self)))
    }

    pub fn is_simple(&self) -> bool {
        matches!(self, Loc::Abs(_) | Loc::Fp(_) | Loc::Ip(_) | Loc::Nf(_))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UnOp {
    Neg,
    Abs,
    Not,
    Math(MathFn),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BinOp {
    Add,
    Sub,
    Mul,
    Div,
    Mod,
    And,
    Or,
    Xor,
    Shl,
    Shr,
    Rol,
    Ror,
    /// Exponentiation; the operand gives the exponent's machine type.
    Expt(Ty),
}

#[derive(Clone, Debug)]
pub enum Ex {
    Const(Ty, u64),
    Load(Ty, Loc),
    /// Address of a location (as a U64 slot).
    Addr(Loc),
    /// Address plus constant bit offset.
    Offset(Box<Ex>, i64),
    Un(UnOp, Ty, Box<Ex>),
    Bin(BinOp, Ty, Box<Ex>, Box<Ex>),
    Cmp(CmpOp, Ty, Box<Ex>, Box<Ex>),
    Conv(Ty, Ty, Box<Ex>),
    Trunc(Ty, Ty, Box<Ex>),
    /// Value with a run-time range check.
    Check { ty: Ty, lo: i64, hi: i64, code: TrapCode, value: Box<Ex> },
    /// Native operation producing one value.
    Native(Native, Vec<Ex>),
    /// Statements followed by a value.
    Seq(Vec<St>, Box<Ex>),
    /// Function call producing a scalar value.
    Call(Box<CallSeq>),
    /// `if cond then a else b` (both of the same machine type).
    Select(Box<Ex>, Box<Ex>, Box<Ex>),
    Now,
}

/// The calling sequence of a function: push a frame, store the arguments,
/// call, read the result and outputs, pop the frame.
#[derive(Clone, Debug)]
pub struct CallSeq {
    pub frame_bytes: u32,
    pub setup: Vec<St>,
    pub routine: usize,
    /// Scalar result to leave on the stack: type and offset in the frame.
    pub result: Option<(Ty, u32)>,
    /// Statements executed after the call (output assignments, copies of
    /// aggregate results). They must not leave values on the stack.
    pub post: Vec<St>,
}

#[derive(Clone, Debug)]
pub enum St {
    Store(Ty, Loc, Ex),
    Copy { dst: Loc, src: Loc, bits: u64 },
    Zero(Loc, u64),
    /// Native operation without a result.
    Native(Native, Vec<Ex>),
    /// Evaluate and discard.
    Eval(Ex),
    If(Ex, Vec<St>, Vec<St>),
    Label(u32),
    Jump(u32),
    JumpIf(Ex, u32),
    JumpIfNot(Ex, u32),
    Call(Box<CallSeq>),
    CallFb { routine: usize, inst: Ex },
    Return,
    /// Traps inside `body` transfer control to `handler`.
    Try(Vec<St>, Vec<St>),
    Trap(TrapCode),
    Assert(Ex, TrapCode),
    /// Source location of the following code.
    At(Span),
}

impl Ex {
    pub fn konst(ty: Ty, v: u64) -> Ex {
        Ex::Const(ty, v)
    }
    pub fn bool(b: bool) -> Ex {
        Ex::Const(Ty::Bool, b as u64)
    }
    pub fn not(e: Ex) -> Ex {
        match e {
            Ex::Const(Ty::Bool, v) => Ex::Const(Ty::Bool, v ^ 1),
            e => Ex::Un(UnOp::Not, Ty::Bool, Box::new(e)),
        }
    }
    pub fn and(a: Ex, b: Ex) -> Ex {
        Ex::Bin(BinOp::And, Ty::Bool, Box::new(a), Box::new(b))
    }
    pub fn or(a: Ex, b: Ex) -> Ex {
        Ex::Bin(BinOp::Or, Ty::Bool, Box::new(a), Box::new(b))
    }
    pub fn load(ty: Ty, l: Loc) -> Ex {
        Ex::Load(ty, l)
    }
}
