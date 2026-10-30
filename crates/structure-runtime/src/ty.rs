//! Machine-level scalar types and their encoding in 64-bit stack slots.
//!
//! Every value on the operand stack is a `u64`:
//! * `Bool`: 0 or 1;
//! * signed integers: the two's complement value sign-extended to 64 bits;
//! * unsigned integers and bit strings: zero-extended;
//! * `F32`: the IEEE 754 single bit pattern in the low 32 bits;
//! * `F64`: the IEEE 754 double bit pattern.
//!
//! Time types are 64-bit signed integers counting microseconds (TIME: a
//! duration; TIME_OF_DAY: since midnight; DATE and DATE_AND_TIME: since
//! 0001-01-01T00:00:00).

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Ty {
    Bool,
    I8,
    I16,
    I32,
    I64,
    U8,
    U16,
    U32,
    U64,
    F32,
    F64,
}

impl Ty {
    /// Storage size in bits (BOOL occupies one bit of storage).
    pub fn bits(self) -> u64 {
        match self {
            Ty::Bool => 1,
            Ty::I8 | Ty::U8 => 8,
            Ty::I16 | Ty::U16 => 16,
            Ty::I32 | Ty::U32 | Ty::F32 => 32,
            Ty::I64 | Ty::U64 | Ty::F64 => 64,
        }
    }

    /// Storage size in bytes when stored on its own.
    pub fn bytes(self) -> u64 {
        match self {
            Ty::Bool => 1,
            t => t.bits() / 8,
        }
    }

    pub fn is_signed(self) -> bool {
        matches!(self, Ty::I8 | Ty::I16 | Ty::I32 | Ty::I64)
    }

    pub fn is_unsigned(self) -> bool {
        matches!(self, Ty::U8 | Ty::U16 | Ty::U32 | Ty::U64)
    }

    pub fn is_int(self) -> bool {
        self.is_signed() || self.is_unsigned()
    }

    pub fn is_float(self) -> bool {
        matches!(self, Ty::F32 | Ty::F64)
    }

    /// Inclusive range of an integer type.
    pub fn int_range(self) -> (i128, i128) {
        match self {
            Ty::Bool => (0, 1),
            Ty::I8 => (i8::MIN as i128, i8::MAX as i128),
            Ty::I16 => (i16::MIN as i128, i16::MAX as i128),
            Ty::I32 => (i32::MIN as i128, i32::MAX as i128),
            Ty::I64 => (i64::MIN as i128, i64::MAX as i128),
            Ty::U8 => (0, u8::MAX as i128),
            Ty::U16 => (0, u16::MAX as i128),
            Ty::U32 => (0, u32::MAX as i128),
            Ty::U64 => (0, u64::MAX as i128),
            Ty::F32 | Ty::F64 => (i128::MIN, i128::MAX),
        }
    }

    pub fn mask(self) -> u64 {
        match self.bits() {
            64 => u64::MAX,
            b => (1u64 << b) - 1,
        }
    }
}

/// Decodes an integer slot to `i128` according to its type.
#[inline]
pub fn to_i128(ty: Ty, v: u64) -> i128 {
    if ty.is_signed() {
        v as i64 as i128
    } else {
        v as i128
    }
}

/// Encodes an integer if it is within the range of `ty`.
#[inline]
pub fn from_i128(ty: Ty, v: i128) -> Option<u64> {
    let (lo, hi) = ty.int_range();
    if v < lo || v > hi {
        None
    } else if ty.is_signed() {
        Some(v as i64 as u64)
    } else {
        Some(v as u64)
    }
}

#[inline]
pub fn f32_of(v: u64) -> f32 {
    f32::from_bits(v as u32)
}

#[inline]
pub fn f64_of(v: u64) -> f64 {
    f64::from_bits(v)
}

#[inline]
pub fn of_f32(f: f32) -> u64 {
    f.to_bits() as u64
}

#[inline]
pub fn of_f64(f: f64) -> u64 {
    f.to_bits()
}

/// Decodes a numeric slot as `f64`.
pub fn to_f64(ty: Ty, v: u64) -> f64 {
    match ty {
        Ty::F32 => f32_of(v) as f64,
        Ty::F64 => f64_of(v),
        t => to_i128(t, v) as f64,
    }
}

pub const MICROS_PER_DAY: i64 = 86_400_000_000;
