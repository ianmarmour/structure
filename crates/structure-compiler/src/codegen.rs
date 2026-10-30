//! Translation of the IR into bytecode.

use crate::ir::*;
use structure_runtime::image::{Addr, Op};
use structure_runtime::ty::Ty;
use structure_syntax::Span;
use std::collections::HashMap;

#[derive(Clone, Copy, Debug)]
pub enum CallKind {
    Func,
    Fb,
}

/// A call made by a routine, used to compute data stack requirements.
#[derive(Clone, Debug)]
pub struct CallSite {
    pub routine: usize,
    /// Bytes of pending frames at the call (for functions, including the
    /// callee's own frame).
    pub pending: u64,
    pub kind: CallKind,
}

#[derive(Clone, Debug, Default)]
pub struct RoutineCode {
    /// Code with routine-local jump targets.
    pub ops: Vec<Op>,
    pub lines: Vec<(u32, Span)>,
    pub calls: Vec<CallSite>,
}

struct Gen<'f> {
    frame_of: &'f dyn Fn(usize) -> u64,
    ops: Vec<Op>,
    labels: HashMap<u32, u32>,
    fixups: Vec<(usize, u32)>,
    next_label: u32,
    lines: Vec<(u32, Span)>,
    calls: Vec<CallSite>,
    pending: u64,
}

const INTERNAL_LABELS: u32 = 1 << 31;

pub fn generate(body: &[St], frame_of: &dyn Fn(usize) -> u64) -> RoutineCode {
    let mut g = Gen {
        frame_of,
        ops: Vec::new(),
        labels: HashMap::new(),
        fixups: Vec::new(),
        next_label: INTERNAL_LABELS,
        lines: Vec::new(),
        calls: Vec::new(),
        pending: 0,
    };
    for s in body {
        g.st(s);
    }
    g.ops.push(Op::Ret);
    for (at, label) in std::mem::take(&mut g.fixups) {
        let target = *g.labels.get(&label).expect("undefined label");
        g.ops[at] = match g.ops[at] {
            Op::Jmp(_) => Op::Jmp(target),
            Op::Jz(_) => Op::Jz(target),
            Op::Jnz(_) => Op::Jnz(target),
            Op::TryBegin(_) => Op::TryBegin(target),
            o => o,
        };
    }
    RoutineCode { ops: g.ops, lines: g.lines, calls: g.calls }
}

/// Relocates routine-local jump targets by `base`.
pub fn relocate(ops: &mut [Op], base: u32) {
    for op in ops {
        *op = match *op {
            Op::Jmp(t) => Op::Jmp(t + base),
            Op::Jz(t) => Op::Jz(t + base),
            Op::Jnz(t) => Op::Jnz(t + base),
            Op::TryBegin(t) => Op::TryBegin(t + base),
            o => o,
        };
    }
}

impl<'f> Gen<'f> {
    fn label(&mut self) -> u32 {
        self.next_label += 1;
        self.next_label
    }

    fn place(&mut self, l: u32) {
        self.labels.insert(l, self.ops.len() as u32);
    }

    fn jump(&mut self, op: Op, l: u32) {
        self.fixups.push((self.ops.len(), l));
        self.ops.push(op);
    }

    fn emit(&mut self, op: Op) {
        self.ops.push(op);
    }

    fn simple(l: &Loc) -> Option<Addr> {
        Some(match l {
            Loc::Abs(a) => Addr::Abs(*a),
            Loc::Fp(o) => Addr::Fp(*o),
            Loc::Ip(o) => Addr::Ip(*o),
            Loc::Nf(o) => Addr::Nf(*o),
            _ => return None,
        })
    }

    fn addr(&mut self, l: &Loc) {
        if let Some(a) = Self::simple(l) {
            self.emit(Op::Address(a));
            return;
        }
        match l {
            Loc::Ind(e) => self.ex(e),
            Loc::Elem { base, index, lo, hi, stride } => {
                self.addr(base);
                self.ex(index);
                self.emit(Op::Index { lo: *lo, hi: *hi, stride: *stride });
            }
            _ => unreachable!(),
        }
    }

    fn ex(&mut self, e: &Ex) {
        match e {
            Ex::Const(_, v) => self.emit(Op::Const(*v)),
            Ex::Load(ty, l) => match Self::simple(l) {
                Some(a) => self.emit(Op::Load(*ty, a)),
                None => {
                    self.addr(l);
                    self.emit(Op::Load(*ty, Addr::Ind));
                }
            },
            Ex::Addr(l) => self.addr(l),
            Ex::Offset(a, o) => {
                self.ex(a);
                if *o != 0 {
                    self.emit(Op::Offset(*o));
                }
            }
            Ex::Un(op, ty, a) => {
                self.ex(a);
                self.emit(match op {
                    UnOp::Neg => Op::Neg(*ty),
                    UnOp::Abs => Op::Abs(*ty),
                    UnOp::Not => Op::Not(*ty),
                    UnOp::Math(f) => Op::Math(*f, *ty),
                });
            }
            Ex::Bin(op, ty, a, b) => {
                self.ex(a);
                self.ex(b);
                self.emit(match op {
                    BinOp::Add => Op::Add(*ty),
                    BinOp::Sub => Op::Sub(*ty),
                    BinOp::Mul => Op::Mul(*ty),
                    BinOp::Div => Op::Div(*ty),
                    BinOp::Mod => Op::Mod(*ty),
                    BinOp::And => Op::And(*ty),
                    BinOp::Or => Op::Or(*ty),
                    BinOp::Xor => Op::Xor(*ty),
                    BinOp::Shl => Op::Shl(*ty),
                    BinOp::Shr => Op::Shr(*ty),
                    BinOp::Rol => Op::Rol(*ty),
                    BinOp::Ror => Op::Ror(*ty),
                    BinOp::Expt(et) => Op::Expt(*ty, *et),
                });
            }
            Ex::Cmp(c, ty, a, b) => {
                self.ex(a);
                self.ex(b);
                self.emit(Op::Cmp(*c, *ty));
            }
            Ex::Conv(f, t, a) => {
                self.ex(a);
                if f != t {
                    self.emit(Op::Conv(*f, *t));
                }
            }
            Ex::Trunc(f, t, a) => {
                self.ex(a);
                self.emit(Op::Trunc(*f, *t));
            }
            Ex::Check { ty, lo, hi, code, value } => {
                self.ex(value);
                self.emit(Op::CheckRange { ty: *ty, lo: *lo, hi: *hi, code: *code });
            }
            Ex::Native(n, args) => {
                for a in args {
                    self.ex(a);
                }
                self.emit(Op::Native(*n));
            }
            Ex::Seq(stmts, v) => {
                for s in stmts {
                    self.st(s);
                }
                self.ex(v);
            }
            Ex::Call(c) => self.call(c),
            Ex::Select(c, a, b) => {
                let le = self.label();
                let lend = self.label();
                self.ex(c);
                self.jump(Op::Jz(0), le);
                self.ex(a);
                self.jump(Op::Jmp(0), lend);
                self.place(le);
                self.ex(b);
                self.place(lend);
            }
            Ex::Now => self.emit(Op::Now),
        }
    }

    fn call(&mut self, c: &CallSeq) {
        let bytes = (self.frame_of)(c.routine);
        self.emit(Op::PushFrame(bytes as u32));
        self.pending += bytes;
        for s in &c.setup {
            self.st(s);
        }
        self.calls.push(CallSite { routine: c.routine, pending: self.pending, kind: CallKind::Func });
        self.emit(Op::CallFunc(c.routine as u32));
        if let Some((ty, off)) = c.result {
            self.emit(Op::Load(ty, Addr::Nf(off)));
        }
        for s in &c.post {
            self.st(s);
        }
        self.emit(Op::PopFrame);
        self.pending -= bytes;
    }

    fn st(&mut self, s: &St) {
        match s {
            St::Store(ty, l, v) => match Self::simple(l) {
                Some(a) => {
                    self.ex(v);
                    self.emit(Op::Store(*ty, a));
                }
                None => {
                    self.addr(l);
                    self.ex(v);
                    self.emit(Op::Store(*ty, Addr::Ind));
                }
            },
            St::Copy { dst, src, bits } => {
                self.addr(dst);
                self.addr(src);
                self.emit(Op::Copy(*bits));
            }
            St::Zero(l, bits) => {
                self.addr(l);
                self.emit(Op::Zero(*bits));
            }
            St::Native(n, args) => {
                for a in args {
                    self.ex(a);
                }
                self.emit(Op::Native(*n));
            }
            St::Eval(e) => {
                self.ex(e);
                self.emit(Op::Pop);
            }
            St::If(c, t, e) => {
                let le = self.label();
                let lend = self.label();
                self.ex(c);
                self.jump(Op::Jz(0), le);
                for s in t {
                    self.st(s);
                }
                if e.is_empty() {
                    self.place(le);
                    self.place(lend);
                } else {
                    self.jump(Op::Jmp(0), lend);
                    self.place(le);
                    for s in e {
                        self.st(s);
                    }
                    self.place(lend);
                }
            }
            St::Label(l) => self.place(*l),
            St::Jump(l) => self.jump(Op::Jmp(0), *l),
            St::JumpIf(c, l) => {
                self.ex(c);
                self.jump(Op::Jnz(0), *l);
            }
            St::JumpIfNot(c, l) => {
                self.ex(c);
                self.jump(Op::Jz(0), *l);
            }
            St::Call(c) => {
                self.call(c);
                if c.result.is_some() {
                    self.emit(Op::Pop);
                }
            }
            St::CallFb { routine, inst } => {
                self.ex(inst);
                self.calls.push(CallSite { routine: *routine, pending: self.pending, kind: CallKind::Fb });
                self.emit(Op::CallFb(*routine as u32));
            }
            St::Return => self.emit(Op::Ret),
            St::Try(body, handler) => {
                let lh = self.label();
                let lend = self.label();
                self.jump(Op::TryBegin(0), lh);
                for s in body {
                    self.st(s);
                }
                self.emit(Op::TryEnd);
                self.jump(Op::Jmp(0), lend);
                self.place(lh);
                for s in handler {
                    self.st(s);
                }
                self.place(lend);
            }
            St::Trap(c) => self.emit(Op::Trap(*c)),
            St::Assert(e, c) => {
                self.ex(e);
                self.emit(Op::Assert(*c));
            }
            St::At(span) => {
                let pc = self.ops.len() as u32;
                if self.lines.last().map(|(p, _)| *p == pc).unwrap_or(false) {
                    self.lines.pop();
                }
                self.lines.push((pc, *span));
            }
        }
    }
}

#[allow(dead_code)]
fn _ty(_: Ty) {}
