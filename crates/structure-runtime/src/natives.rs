//! Natively implemented operations: character string functions
//! (table 29), conversions to and from strings and BCD conversions
//! (table 22).

use crate::error::TrapCode;
use crate::image::Native;
use crate::memory::Memory;
use crate::strings;
use crate::text;
use crate::ty::*;

type R<T> = Result<T, TrapCode>;

fn pop(stack: &mut Vec<u64>) -> u64 {
    stack.pop().unwrap_or(0)
}

fn pop_count(stack: &mut Vec<u64>) -> R<usize> {
    let v = pop(stack) as i64;
    if v < 0 {
        return Err(TrapCode::NegativeStringArgument);
    }
    Ok(v as usize)
}

pub fn exec(n: Native, stack: &mut Vec<u64>, mem: &mut Memory) -> R<()> {
    match n {
        Native::StrLen { wide } => {
            let s = pop(stack);
            let v = strings::read(mem, s, wide)?;
            stack.push(v.len() as u64);
        }
        Native::StrLeft { wide } => {
            let l = pop_count(stack)?;
            let s = pop(stack);
            let dst = pop(stack);
            let v = strings::read(mem, s, wide)?;
            if l > v.len() {
                return Err(TrapCode::InvalidCharacterPosition);
            }
            strings::write(mem, dst, wide, &v[..l], false)?;
        }
        Native::StrRight { wide } => {
            let l = pop_count(stack)?;
            let s = pop(stack);
            let dst = pop(stack);
            let v = strings::read(mem, s, wide)?;
            if l > v.len() {
                return Err(TrapCode::InvalidCharacterPosition);
            }
            strings::write(mem, dst, wide, &v[v.len() - l..], false)?;
        }
        Native::StrMid { wide } => {
            let p = pop_count(stack)?;
            let l = pop_count(stack)?;
            let s = pop(stack);
            let dst = pop(stack);
            let v = strings::read(mem, s, wide)?;
            // characters P .. P+L-1 (1-based)
            if p < 1 || p - 1 + l > v.len() {
                return Err(TrapCode::InvalidCharacterPosition);
            }
            strings::write(mem, dst, wide, &v[p - 1..p - 1 + l], false)?;
        }
        Native::StrConcat { wide, n } => {
            let mut parts = Vec::with_capacity(n as usize);
            for _ in 0..n {
                parts.push(pop(stack));
            }
            let dst = pop(stack);
            let mut out = Vec::new();
            for s in parts.iter().rev() {
                out.extend(strings::read(mem, *s, wide)?);
            }
            if out.len() > strings::MAX_STRING_LENGTH as usize {
                return Err(TrapCode::StringTooLong);
            }
            strings::write(mem, dst, wide, &out, false)?;
        }
        Native::StrInsert { wide } => {
            let p = pop_count(stack)?;
            let s2 = pop(stack);
            let s1 = pop(stack);
            let dst = pop(stack);
            let a = strings::read(mem, s1, wide)?;
            let b = strings::read(mem, s2, wide)?;
            // insert IN2 after the P-th character position of IN1
            if p > a.len() {
                return Err(TrapCode::InvalidCharacterPosition);
            }
            let mut out = a[..p].to_vec();
            out.extend_from_slice(&b);
            out.extend_from_slice(&a[p..]);
            if out.len() > strings::MAX_STRING_LENGTH as usize {
                return Err(TrapCode::StringTooLong);
            }
            strings::write(mem, dst, wide, &out, false)?;
        }
        Native::StrDelete { wide } => {
            let p = pop_count(stack)?;
            let l = pop_count(stack)?;
            let s = pop(stack);
            let dst = pop(stack);
            let a = strings::read(mem, s, wide)?;
            if l > 0 && (p < 1 || p - 1 + l > a.len()) || (l == 0 && p > a.len() + 1) {
                return Err(TrapCode::InvalidCharacterPosition);
            }
            let start = p.saturating_sub(1);
            let mut out = a[..start].to_vec();
            out.extend_from_slice(&a[(start + l).min(a.len())..]);
            strings::write(mem, dst, wide, &out, false)?;
        }
        Native::StrReplace { wide } => {
            let p = pop_count(stack)?;
            let l = pop_count(stack)?;
            let s2 = pop(stack);
            let s1 = pop(stack);
            let dst = pop(stack);
            let a = strings::read(mem, s1, wide)?;
            let b = strings::read(mem, s2, wide)?;
            if p < 1 || p - 1 > a.len() || p - 1 + l > a.len() {
                return Err(TrapCode::InvalidCharacterPosition);
            }
            let mut out = a[..p - 1].to_vec();
            out.extend_from_slice(&b);
            out.extend_from_slice(&a[p - 1 + l..]);
            if out.len() > strings::MAX_STRING_LENGTH as usize {
                return Err(TrapCode::StringTooLong);
            }
            strings::write(mem, dst, wide, &out, false)?;
        }
        Native::StrFind { wide } => {
            let s2 = pop(stack);
            let s1 = pop(stack);
            let a = strings::read(mem, s1, wide)?;
            let b = strings::read(mem, s2, wide)?;
            let pos = if b.is_empty() {
                0
            } else {
                a.windows(b.len()).position(|w| w == b.as_slice()).map(|i| i + 1).unwrap_or(0)
            };
            stack.push(pos as u64);
        }
        Native::StrCmp { wide } => {
            let b = pop(stack);
            let a = pop(stack);
            let x = strings::read(mem, a, wide)?;
            let y = strings::read(mem, b, wide)?;
            // The shorter string is extended with characters of value zero (2.5.1.5.5).
            let n = x.len().max(y.len());
            let mut r = 0i64;
            for i in 0..n {
                let c1 = x.get(i).copied().unwrap_or(0);
                let c2 = y.get(i).copied().unwrap_or(0);
                if c1 != c2 {
                    r = if c1 < c2 { -1 } else { 1 };
                    break;
                }
            }
            stack.push(r as u64);
        }
        Native::StrAssign { wide } => {
            let src = pop(stack);
            let dst = pop(stack);
            let v = strings::read(mem, src, wide)?;
            strings::write(mem, dst, wide, &v, true)?;
        }
        Native::StrInit { cap, .. } => {
            let dst = pop(stack);
            strings::init(mem, dst, cap)?;
        }
        Native::ToText { kind, wide } => {
            let v = pop(stack);
            let dst = pop(stack);
            let s = text::format(kind, v);
            let chars = strings::from_rust(&s, wide);
            strings::write(mem, dst, wide, &chars, false)?;
        }
        Native::FromText { kind, wide } => {
            let src = pop(stack);
            let v = strings::read(mem, src, wide)?;
            let s = strings::to_rust(&v);
            let r = text::parse_text(kind, &s).ok_or(TrapCode::ConversionError)?;
            stack.push(r);
        }
        Native::StrToWstr => {
            let src = pop(stack);
            let dst = pop(stack);
            let v = strings::read(mem, src, false)?;
            strings::write(mem, dst, true, &v, false)?;
        }
        Native::WstrToStr => {
            let src = pop(stack);
            let dst = pop(stack);
            let v = strings::read(mem, src, true)?;
            if v.iter().any(|c| *c > 0xFF) {
                return Err(TrapCode::ConversionError);
            }
            strings::write(mem, dst, false, &v, false)?;
        }
        Native::BcdToInt { from, to } => {
            let v = pop(stack) & from.mask();
            let mut r: i128 = 0;
            let digits = from.bits() / 4;
            for i in (0..digits).rev() {
                let d = (v >> (i * 4)) & 0xF;
                if d > 9 {
                    return Err(TrapCode::ConversionError);
                }
                r = r * 10 + d as i128;
            }
            stack.push(from_i128(to, r).ok_or(TrapCode::ConversionError)?);
        }
        Native::IntToBcd { from, to } => {
            let x = to_i128(from, pop(stack));
            if x < 0 {
                return Err(TrapCode::ConversionError);
            }
            let digits = to.bits() / 4;
            let mut r: u64 = 0;
            let mut rest = x;
            for i in 0..digits {
                r |= ((rest % 10) as u64) << (i * 4);
                rest /= 10;
            }
            if rest != 0 {
                return Err(TrapCode::ConversionError);
            }
            stack.push(r);
        }
    }
    Ok(())
}
