//! The bytecode interpreter.

use crate::error::TrapCode;
use crate::image::{Addr, Area, CmpOp, Image, MathFn, Op};
use crate::memory::Memory;
use crate::natives;
use crate::ty::*;

pub const RETURN_TO_HOST: u32 = u32::MAX;

#[derive(Clone, Debug)]
struct CallRecord {
    ret_pc: u32,
    fp: u64,
    ip: u64,
    /// Bytes of automatically allocated frame to release on return.
    auto_frame: u64,
    routine: u32,
}

#[derive(Clone, Debug)]
struct TryRecord {
    handler: u32,
    calls: usize,
    stack: usize,
    pending: usize,
    sp: u64,
}

/// Execution state of one job (or initialisation routine).
#[derive(Clone, Debug)]
pub struct Context {
    pub pc: u32,
    pub fp: u64,
    pub ip: u64,
    pub stack: Vec<u64>,
    calls: Vec<CallRecord>,
    pending: Vec<(u64, u64)>,
    /// Data stack pointer (byte address).
    pub sp: u64,
    pub area: Area,
    tries: Vec<TryRecord>,
    pub routine: u32,
    /// Current time reported by `Now` (microseconds).
    pub now: i64,
    /// Instructions executed since the context was started.
    pub steps: u64,
    pub done: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Exit {
    /// The entry routine returned.
    Done,
    /// The instruction budget was exhausted; execution can be resumed.
    Yield,
    /// A trap that was not handled.
    Trap(TrapCode, u32),
}

type R<T> = Result<T, TrapCode>;

impl Context {
    /// Creates a context that will run `routine` with the given instance.
    pub fn new(image: &Image, routine: u32, instance: u64, area: Area, now: i64) -> Context {
        let mut ctx = Context {
            pc: RETURN_TO_HOST,
            fp: 0,
            ip: instance,
            stack: Vec::with_capacity(64),
            calls: Vec::new(),
            pending: Vec::new(),
            sp: area.base,
            area,
            tries: Vec::new(),
            routine,
            now,
            steps: 0,
            done: false,
        };
        // Simulate CallFb from the host.
        let r = &image.routines[routine as usize];
        ctx.calls.push(CallRecord { ret_pc: RETURN_TO_HOST, fp: 0, ip: instance, auto_frame: r.frame_size as u64, routine });
        ctx.fp = ctx.sp * 8;
        ctx.sp += r.frame_size as u64;
        ctx.pc = r.entry;
        ctx
    }

    #[inline]
    fn pop(&mut self) -> u64 {
        self.stack.pop().unwrap_or(0)
    }

    #[inline]
    fn push(&mut self, v: u64) {
        self.stack.push(v);
    }

    fn addr(&mut self, a: Addr) -> R<u64> {
        Ok(match a {
            Addr::Abs(x) => x,
            Addr::Fp(o) => self.fp + o as u64,
            Addr::Ip(o) => self.ip + o as u64,
            Addr::Nf(o) => self.pending.last().ok_or(TrapCode::InvalidOperation)?.0 + o as u64,
            Addr::Ind => self.pop(),
        })
    }

    fn alloc(&mut self, mem: &mut Memory, bytes: u64) -> R<u64> {
        let base = self.sp;
        let end = base + bytes;
        if end > self.area.base + self.area.size {
            return Err(TrapCode::StackOverflow);
        }
        mem.zero(base * 8, bytes * 8)?;
        self.sp = end;
        Ok(base)
    }

    /// Runs until the entry routine returns, a trap occurs or `budget`
    /// instructions have been executed.
    pub fn run(&mut self, image: &Image, mem: &mut Memory, budget: u64) -> Exit {
        let mut remaining = budget;
        while remaining > 0 {
            if self.done {
                return Exit::Done;
            }
            remaining -= 1;
            self.steps += 1;
            let pc = self.pc;
            let op = match image.code.get(pc as usize) {
                Some(op) => *op,
                None => return Exit::Trap(TrapCode::InvalidOperation, pc),
            };
            match self.step(image, mem, op) {
                Ok(()) => {}
                Err(code) => {
                    // Find a handler in the current routine.
                    let depth = self.calls.len();
                    if let Some(t) = self.tries.pop_if(|t| t.calls == depth) {
                        self.stack.truncate(t.stack);
                        self.pending.truncate(t.pending);
                        self.sp = t.sp;
                        self.pc = t.handler;
                        continue;
                    }
                    return Exit::Trap(code, pc);
                }
            }
        }
        if self.done {
            Exit::Done
        } else {
            Exit::Yield
        }
    }

    #[inline]
    fn step(&mut self, image: &Image, mem: &mut Memory, op: Op) -> R<()> {
        let mut next = self.pc.wrapping_add(1);
        match op {
            Op::Nop => {}
            Op::Const(v) => self.push(v),
            Op::Dup => {
                let v = *self.stack.last().ok_or(TrapCode::InvalidOperation)?;
                self.push(v);
            }
            Op::Over => {
                let n = self.stack.len();
                if n < 2 {
                    return Err(TrapCode::InvalidOperation);
                }
                let v = self.stack[n - 2];
                self.push(v);
            }
            Op::Pop => {
                self.pop();
            }
            Op::Swap => {
                let n = self.stack.len();
                if n < 2 {
                    return Err(TrapCode::InvalidOperation);
                }
                self.stack.swap(n - 1, n - 2);
            }
            Op::Rot => {
                let n = self.stack.len();
                if n < 3 {
                    return Err(TrapCode::InvalidOperation);
                }
                let a = self.stack.remove(n - 3);
                self.stack.push(a);
            }
            Op::Load(ty, a) => {
                let addr = self.addr(a)?;
                let v = mem.load(ty, addr)?;
                self.push(v);
            }
            Op::Store(ty, a) => {
                let v = self.pop();
                let addr = self.addr(a)?;
                mem.store(ty, addr, v)?;
            }
            Op::Address(a) => {
                let addr = self.addr(a)?;
                self.push(addr);
            }
            Op::Offset(o) => {
                let a = self.pop();
                self.push((a as i64).wrapping_add(o) as u64);
            }
            Op::Index { lo, hi, stride } => {
                let i = self.pop() as i64;
                let a = self.pop();
                if i < lo || i > hi {
                    return Err(TrapCode::IndexOutOfRange);
                }
                self.push(a + (i - lo) as u64 * stride);
            }
            Op::Copy(bits) => {
                let src = self.pop();
                let dst = self.pop();
                mem.copy(dst, src, bits)?;
            }
            Op::Zero(bits) => {
                let dst = self.pop();
                mem.zero(dst, bits)?;
            }
            Op::Add(t) | Op::Sub(t) | Op::Mul(t) | Op::Div(t) | Op::Mod(t) => {
                let b = self.pop();
                let a = self.pop();
                let r = arith(op, t, a, b)?;
                self.push(r);
            }
            Op::Neg(t) => {
                let a = self.pop();
                let r = match t {
                    Ty::F32 => of_f32(-f32_of(a)),
                    Ty::F64 => of_f64(-f64_of(a)),
                    _ => from_i128(t, -to_i128(t, a)).ok_or(TrapCode::Overflow)?,
                };
                self.push(r);
            }
            Op::Abs(t) => {
                let a = self.pop();
                let r = match t {
                    Ty::F32 => of_f32(f32_of(a).abs()),
                    Ty::F64 => of_f64(f64_of(a).abs()),
                    _ => from_i128(t, to_i128(t, a).abs()).ok_or(TrapCode::Overflow)?,
                };
                self.push(r);
            }
            Op::Expt(bt, et) => {
                let e = self.pop();
                let b = self.pop();
                let r = expt(bt, et, b, e)?;
                self.push(r);
            }
            Op::And(t) => {
                let b = self.pop();
                let a = self.pop();
                self.push((a & b) & t.mask());
            }
            Op::Or(t) => {
                let b = self.pop();
                let a = self.pop();
                self.push((a | b) & t.mask());
            }
            Op::Xor(t) => {
                let b = self.pop();
                let a = self.pop();
                self.push((a ^ b) & t.mask());
            }
            Op::Not(t) => {
                let a = self.pop();
                self.push(!a & t.mask());
            }
            Op::Shl(t) | Op::Shr(t) | Op::Rol(t) | Op::Ror(t) => {
                let n = self.pop() as i64;
                let v = self.pop() & t.mask();
                if n < 0 {
                    return Err(TrapCode::NegativeShift);
                }
                let bits = t.bits();
                let r = match op {
                    Op::Shl(_) => {
                        if n as u64 >= bits {
                            0
                        } else {
                            (v << n) & t.mask()
                        }
                    }
                    Op::Shr(_) => {
                        if n as u64 >= bits {
                            0
                        } else {
                            v >> n
                        }
                    }
                    Op::Rol(_) => {
                        let n = (n as u64) % bits;
                        if n == 0 {
                            v
                        } else {
                            ((v << n) | (v >> (bits - n))) & t.mask()
                        }
                    }
                    _ => {
                        let n = (n as u64) % bits;
                        if n == 0 {
                            v
                        } else {
                            ((v >> n) | (v << (bits - n))) & t.mask()
                        }
                    }
                };
                self.push(r);
            }
            Op::Cmp(c, t) => {
                let b = self.pop();
                let a = self.pop();
                let ord = match t {
                    Ty::F32 => f32_of(a).partial_cmp(&f32_of(b)),
                    Ty::F64 => f64_of(a).partial_cmp(&f64_of(b)),
                    t if t.is_signed() => Some((a as i64).cmp(&(b as i64))),
                    _ => Some(a.cmp(&b)),
                };
                use std::cmp::Ordering::*;
                let r = match (c, ord) {
                    (CmpOp::Ne, None) => true,
                    (_, None) => false,
                    (CmpOp::Eq, Some(o)) => o == Equal,
                    (CmpOp::Ne, Some(o)) => o != Equal,
                    (CmpOp::Lt, Some(o)) => o == Less,
                    (CmpOp::Le, Some(o)) => o != Greater,
                    (CmpOp::Gt, Some(o)) => o == Greater,
                    (CmpOp::Ge, Some(o)) => o != Less,
                };
                self.push(r as u64);
            }
            Op::Conv(from, to) => {
                let a = self.pop();
                let r = convert(from, to, a, false)?;
                self.push(r);
            }
            Op::Trunc(from, to) => {
                let a = self.pop();
                let r = convert(from, to, a, true)?;
                self.push(r);
            }
            Op::Math(f, t) => {
                let a = self.pop();
                let x = to_f64(t, a);
                let r = match f {
                    MathFn::Sqrt => x.sqrt(),
                    MathFn::Ln => x.ln(),
                    MathFn::Log => x.log10(),
                    MathFn::Exp => x.exp(),
                    MathFn::Sin => x.sin(),
                    MathFn::Cos => x.cos(),
                    MathFn::Tan => x.tan(),
                    MathFn::Asin => x.asin(),
                    MathFn::Acos => x.acos(),
                    MathFn::Atan => x.atan(),
                };
                let v = float_result(t, r, x.is_finite())?;
                self.push(v);
            }
            Op::CheckRange { ty, lo, hi, code } => {
                let v = *self.stack.last().ok_or(TrapCode::InvalidOperation)?;
                let x = to_i128(ty, v);
                if x < lo as i128 || x > hi as i128 {
                    return Err(code);
                }
            }
            Op::Assert(code) => {
                if self.pop() == 0 {
                    return Err(code);
                }
            }
            Op::Jmp(t) => next = t,
            Op::Jz(t) => {
                if self.pop() == 0 {
                    next = t;
                }
            }
            Op::Jnz(t) => {
                if self.pop() != 0 {
                    next = t;
                }
            }
            Op::PushFrame(size) => {
                let base = self.alloc(mem, size as u64)?;
                self.pending.push((base * 8, size as u64));
            }
            Op::PopFrame => {
                let (_, size) = self.pending.pop().ok_or(TrapCode::InvalidOperation)?;
                self.sp -= size;
            }
            Op::CallFunc(r) => {
                let routine = image.routines.get(r as usize).ok_or(TrapCode::InvalidOperation)?;
                let (base, _) = *self.pending.last().ok_or(TrapCode::InvalidOperation)?;
                self.calls.push(CallRecord { ret_pc: next, fp: self.fp, ip: self.ip, auto_frame: 0, routine: self.routine });
                self.fp = base;
                self.routine = r;
                next = routine.entry;
            }
            Op::CallFb(r) => {
                let inst = self.pop();
                if inst == 0 {
                    return Err(TrapCode::NoValueSpecified);
                }
                let routine = image.routines.get(r as usize).ok_or(TrapCode::InvalidOperation)?;
                let size = routine.frame_size as u64;
                let base = self.alloc(mem, size)?;
                self.calls.push(CallRecord { ret_pc: next, fp: self.fp, ip: self.ip, auto_frame: size, routine: self.routine });
                self.fp = base * 8;
                self.ip = inst;
                self.routine = r;
                next = routine.entry;
            }
            Op::Ret => {
                let rec = self.calls.pop().ok_or(TrapCode::InvalidOperation)?;
                self.sp -= rec.auto_frame;
                self.fp = rec.fp;
                self.ip = rec.ip;
                self.routine = rec.routine;
                // Drop try regions of the returning routine.
                while self.tries.last().map(|t| t.calls > self.calls.len()).unwrap_or(false) {
                    self.tries.pop();
                }
                if rec.ret_pc == RETURN_TO_HOST {
                    self.done = true;
                    next = RETURN_TO_HOST;
                } else {
                    next = rec.ret_pc;
                }
            }
            Op::TryBegin(h) => {
                self.tries.push(TryRecord {
                    handler: h,
                    calls: self.calls.len(),
                    stack: self.stack.len(),
                    pending: self.pending.len(),
                    sp: self.sp,
                });
            }
            Op::TryEnd => {
                self.tries.pop();
            }
            Op::Trap(c) => return Err(c),
            Op::Native(n) => natives::exec(n, &mut self.stack, mem)?,
            Op::Now => {
                let t = self.now as u64;
                self.push(t);
            }
        }
        self.pc = next;
        Ok(())
    }
}

/// Checks a floating point result: a non-finite result from finite
/// operands is an overflow (2.5.1.5.2, 3.3.1).
fn float_result(t: Ty, r: f64, inputs_finite: bool) -> R<u64> {
    match t {
        Ty::F32 => {
            let f = r as f32;
            if !f.is_finite() && inputs_finite {
                return Err(TrapCode::Overflow);
            }
            Ok(of_f32(f))
        }
        _ => {
            if !r.is_finite() && inputs_finite {
                return Err(TrapCode::Overflow);
            }
            Ok(of_f64(r))
        }
    }
}

fn arith(op: Op, t: Ty, a: u64, b: u64) -> R<u64> {
    match t {
        Ty::F32 => {
            let (x, y) = (f32_of(a), f32_of(b));
            let r = match op {
                Op::Add(_) => x + y,
                Op::Sub(_) => x - y,
                Op::Mul(_) => x * y,
                Op::Div(_) => {
                    if y == 0.0 {
                        return Err(TrapCode::DivisionByZero);
                    }
                    x / y
                }
                _ => return Err(TrapCode::InvalidOperation),
            };
            if !r.is_finite() && x.is_finite() && y.is_finite() {
                return Err(TrapCode::Overflow);
            }
            Ok(of_f32(r))
        }
        Ty::F64 => {
            let (x, y) = (f64_of(a), f64_of(b));
            let r = match op {
                Op::Add(_) => x + y,
                Op::Sub(_) => x - y,
                Op::Mul(_) => x * y,
                Op::Div(_) => {
                    if y == 0.0 {
                        return Err(TrapCode::DivisionByZero);
                    }
                    x / y
                }
                _ => return Err(TrapCode::InvalidOperation),
            };
            if !r.is_finite() && x.is_finite() && y.is_finite() {
                return Err(TrapCode::Overflow);
            }
            Ok(of_f64(r))
        }
        _ => {
            let (x, y) = (to_i128(t, a), to_i128(t, b));
            let r = match op {
                Op::Add(_) => x.checked_add(y),
                Op::Sub(_) => x.checked_sub(y),
                Op::Mul(_) => x.checked_mul(y),
                Op::Div(_) => {
                    if y == 0 {
                        return Err(TrapCode::DivisionByZero);
                    }
                    Some(x / y)
                }
                Op::Mod(_) => {
                    if y == 0 {
                        Some(0)
                    } else {
                        Some(x % y)
                    }
                }
                _ => return Err(TrapCode::InvalidOperation),
            };
            r.and_then(|r| from_i128(t, r)).ok_or(TrapCode::Overflow)
        }
    }
}

fn expt(bt: Ty, et: Ty, b: u64, e: u64) -> R<u64> {
    let x = to_f64(bt, b);
    let r = if et.is_float() {
        x.powf(to_f64(et, e))
    } else {
        let n = to_i128(et, e);
        if n.abs() <= i32::MAX as i128 {
            x.powi(n as i32)
        } else {
            x.powf(n as f64)
        }
    };
    let finite = x.is_finite();
    if r.is_nan() && finite {
        return Err(TrapCode::Overflow);
    }
    float_result(bt, r, finite)
}

/// Value conversion between scalar types (table 22).
pub fn convert(from: Ty, to: Ty, v: u64, truncate: bool) -> R<u64> {
    if from == to {
        return Ok(v);
    }
    if from.is_float() {
        let x = to_f64(from, v);
        return match to {
            Ty::F32 => {
                let f = x as f32;
                if x.is_finite() && !f.is_finite() {
                    return Err(TrapCode::ConversionError);
                }
                Ok(of_f32(f))
            }
            Ty::F64 => Ok(of_f64(x)),
            Ty::Bool => Ok((x != 0.0) as u64),
            _ => {
                if !x.is_finite() {
                    return Err(TrapCode::ConversionError);
                }
                let r = if truncate { x.trunc() } else { x.round_ties_even() };
                if !(-1.8e38..=1.8e38).contains(&r) {
                    return Err(TrapCode::ConversionError);
                }
                from_i128(to, r as i128).ok_or(TrapCode::ConversionError)
            }
        };
    }
    let x = if from == Ty::Bool { (v & 1) as i128 } else { to_i128(from, v) };
    match to {
        Ty::F32 => Ok(of_f32(x as f32)),
        Ty::F64 => Ok(of_f64(x as f64)),
        Ty::Bool => Ok((x != 0) as u64),
        _ => from_i128(to, x).ok_or(TrapCode::ConversionError),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rounding_per_table_22() {
        let c = |f: f32| convert(Ty::F32, Ty::I16, of_f32(f), false).unwrap() as i64;
        assert_eq!(c(1.6), 2);
        assert_eq!(c(-1.6), -2);
        assert_eq!(c(1.5), 2);
        assert_eq!(c(-1.5), -2);
        assert_eq!(c(1.4), 1);
        assert_eq!(c(2.5), 2);
        assert_eq!(c(-2.5), -2);
        let t = |f: f32| convert(Ty::F32, Ty::I16, of_f32(f), true).unwrap() as i64;
        assert_eq!(t(1.6), 1);
        assert_eq!(t(-1.6), -1);
        assert!(convert(Ty::I32, Ty::I16, 70000, false).is_err());
    }

    #[test]
    fn integer_division() {
        assert_eq!(arith(Op::Div(Ty::I16), Ty::I16, 7, 3).unwrap(), 2);
        assert_eq!(arith(Op::Div(Ty::I16), Ty::I16, (-7i64) as u64, 3).unwrap() as i64, -2);
        assert_eq!(arith(Op::Mod(Ty::I16), Ty::I16, 7, 0).unwrap(), 0);
        assert!(arith(Op::Div(Ty::I16), Ty::I16, 7, 0).is_err());
        assert!(arith(Op::Add(Ty::I8), Ty::I8, 127, 1).is_err());
    }
}
