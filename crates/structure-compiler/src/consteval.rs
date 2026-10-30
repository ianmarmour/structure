//! Evaluation of constant expressions and typing of constants.

use crate::types::*;
use structure_runtime::calendar::days_from_civil;
use structure_runtime::ty::{self as mty, Ty, MICROS_PER_DAY};
use structure_syntax::ast::{BinaryOp, Expr, ExprKind, LiteralKind, UnaryOp};
use structure_syntax::literal::Duration;

/// A constant with an optional explicit type (from a type prefix).
#[derive(Clone, Debug)]
pub struct Const {
    pub value: ConstValue,
    pub ty: Option<TypeId>,
}

pub fn duration_micros(d: &Duration) -> Result<i64, String> {
    let us = (d.nanos + 500) / 1000;
    let v = i64::try_from(us).map_err(|_| "duration literal exceeds the range of TIME".to_string())?;
    Ok(if d.negative { -v } else { v })
}

pub fn date_micros(d: &structure_syntax::literal::Date) -> i64 {
    days_from_civil(d.year as i64, d.month, d.day) * MICROS_PER_DAY
}

/// Converts a literal to a constant (without context typing).
pub fn literal_value(kind: &LiteralKind) -> Result<ConstValue, String> {
    Ok(match kind {
        LiteralKind::Int { value, negative, .. } => {
            let v = *value as i128;
            ConstValue::Int(if *negative { -v } else { v })
        }
        LiteralKind::Real { text, negative } => {
            let f: f64 = text.parse().map_err(|_| format!("invalid real literal {}", text))?;
            ConstValue::Real(if *negative { -f } else { f })
        }
        LiteralKind::Bool(b) => ConstValue::Bool(*b),
        LiteralKind::String { wide, chars } => ConstValue::Str { wide: *wide, chars: chars.clone() },
        LiteralKind::Duration(d) => ConstValue::Time(duration_micros(d)?),
        LiteralKind::TimeOfDay(ns) => ConstValue::Tod(((*ns + 500) / 1000) as i64),
        LiteralKind::Date(d) => ConstValue::Date(date_micros(d)),
        LiteralKind::DateAndTime(d, ns) => ConstValue::Dt(date_micros(d) + ((*ns + 500) / 1000) as i64),
    })
}

/// Encodes a constant as a stack slot for the machine type of `ty`.
pub fn encode(types: &Types, cv: &ConstValue, ty: TypeId) -> Option<u64> {
    let mt = types.scalar(ty)?;
    Some(match cv {
        ConstValue::Bool(b) => *b as u64,
        ConstValue::Int(i) => match mt {
            Ty::F32 => mty::of_f32(*i as f32),
            Ty::F64 => mty::of_f64(*i as f64),
            Ty::Bool => (*i != 0) as u64,
            t => mty::from_i128(t, *i)?,
        },
        ConstValue::Real(f) => match mt {
            Ty::F32 => mty::of_f32(*f as f32),
            _ => mty::of_f64(*f),
        },
        ConstValue::Time(t) | ConstValue::Date(t) | ConstValue::Tod(t) | ConstValue::Dt(t) => *t as u64,
        ConstValue::Enum { index, .. } => *index as u64,
        ConstValue::Str { .. } => return None,
    })
}

/// Checks that a constant can be a value of type `ty` and converts it.
/// Returns an error message or (value, uses_extension).
pub fn coerce(types: &Types, cv: &ConstValue, ty: TypeId) -> Result<(ConstValue, bool), String> {
    let r = types.resolve(ty);
    if types.is_error(r) {
        return Ok((cv.clone(), false));
    }
    if let Some((wide, len)) = types.is_string(r) {
        return match cv {
            ConstValue::Str { wide: w, chars } if *w == wide || (!*w && wide) => {
                if chars.len() > len as usize {
                    Err(format!("character string of length {} does not fit in {}", chars.len(), types.name(ty)))
                } else {
                    Ok((ConstValue::Str { wide, chars: chars.clone() }, *w != wide))
                }
            }
            _ => Err(format!("expected a {} value", types.name(ty))),
        };
    }
    if let Some(values) = types.enum_values(r) {
        let _ = values;
        return match cv {
            ConstValue::Enum { ty: t, index } if types.resolve(*t) == r => Ok((ConstValue::Enum { ty: r, index: *index }, false)),
            _ => Err(format!("expected a value of enumerated type {}", types.name(ty))),
        };
    }
    let Some(e) = types.elem_of(r) else {
        return Err(format!("a constant cannot be a value of type {}", types.name(ty)));
    };
    let range_check = |v: i128| -> Result<(), String> {
        if let Some((lo, hi)) = types.subrange(ty) {
            if v < lo || v > hi {
                return Err(format!("value {} is outside the subrange {}..{}", v, lo, hi));
            }
        }
        Ok(())
    };
    match (cv, e) {
        (ConstValue::Bool(b), Elem::Bool) => Ok((ConstValue::Bool(*b), false)),
        (ConstValue::Int(i), Elem::Bool) if *i == 0 || *i == 1 => Ok((ConstValue::Bool(*i == 1), false)),
        (ConstValue::Int(i), e) if e.is_int() || (e.is_bit() && e != Elem::Bool) => {
            let (lo, hi) = e.ty().int_range();
            if *i < lo || *i > hi {
                return Err(format!("value {} is out of the range of {}", i, e.name()));
            }
            range_check(*i)?;
            Ok((ConstValue::Int(*i), false))
        }
        (ConstValue::Int(i), e) if e.is_real() => Ok((ConstValue::Real(*i as f64), true)),
        (ConstValue::Real(f), Elem::Real) => {
            if f.is_finite() && !(*f as f32).is_finite() {
                return Err("real literal exceeds the range of REAL".into());
            }
            Ok((ConstValue::Real(*f), false))
        }
        (ConstValue::Real(f), Elem::LReal) => Ok((ConstValue::Real(*f), false)),
        (ConstValue::Time(t), Elem::Time) => Ok((ConstValue::Time(*t), false)),
        (ConstValue::Date(t), Elem::Date) => Ok((ConstValue::Date(*t), false)),
        (ConstValue::Tod(t), Elem::Tod) => Ok((ConstValue::Tod(*t), false)),
        (ConstValue::Dt(t), Elem::Dt) => Ok((ConstValue::Dt(*t), false)),
        (cv, e) => Err(format!("{} is not a valid value of type {}", describe(cv), e.name())),
    }
}

pub fn describe(cv: &ConstValue) -> String {
    match cv {
        ConstValue::Bool(b) => (if *b { "TRUE" } else { "FALSE" }).to_string(),
        ConstValue::Int(i) => i.to_string(),
        ConstValue::Real(f) => f.to_string(),
        ConstValue::Time(_) => "a TIME literal".into(),
        ConstValue::Date(_) => "a DATE literal".into(),
        ConstValue::Tod(_) => "a TIME_OF_DAY literal".into(),
        ConstValue::Dt(_) => "a DATE_AND_TIME literal".into(),
        ConstValue::Str { .. } => "a character string".into(),
        ConstValue::Enum { .. } => "an enumerated value".into(),
    }
}

/// Evaluates a constant expression. `enum_lookup` resolves bare
/// identifiers given an optional expected type.
/// Resolves an enumerated value written without its type.
pub type EnumLookup<'a> = dyn Fn(&str, Option<TypeId>) -> Result<ConstValue, String> + 'a;

pub fn eval(types: &Types, e: &Expr, expected: Option<TypeId>, enum_lookup: &EnumLookup) -> Result<Const, String> {
    match &e.kind {
        ExprKind::Literal(l) => {
            let v = literal_value(&l.kind)?;
            let ty = match &l.type_prefix {
                Some(p) => Some(*types.by_name.get(&p.key()).ok_or_else(|| format!("unknown type prefix {}", p))?),
                None => None,
            };
            if let Some(t) = ty {
                let (v, _) = coerce(types, &v, t)?;
                return Ok(Const { value: v, ty: Some(t) });
            }
            Ok(Const { value: v, ty: None })
        }
        ExprKind::Paren(inner) => eval(types, inner, expected, enum_lookup),
        ExprKind::Name(id) => Ok(Const { value: enum_lookup(&id.key(), expected)?, ty: expected }),
        ExprKind::TypedEnum { ty, value } => {
            let t = *types.by_name.get(&ty.key()).ok_or_else(|| format!("unknown type {}", ty))?;
            let values = types.enum_values(t).ok_or_else(|| format!("{} is not an enumerated type", ty))?;
            let index = values.iter().position(|v| *v == value.key()).ok_or_else(|| format!("{} is not a value of {}", value, ty))?;
            Ok(Const { value: ConstValue::Enum { ty: types.resolve(t), index }, ty: Some(t) })
        }
        ExprKind::Unary { op, operand } => {
            let c = eval(types, operand, expected, enum_lookup)?;
            let v = match (op, &c.value) {
                (UnaryOp::Neg, ConstValue::Int(i)) => ConstValue::Int(-i),
                (UnaryOp::Neg, ConstValue::Real(f)) => ConstValue::Real(-f),
                (UnaryOp::Neg, ConstValue::Time(t)) => ConstValue::Time(-t),
                (UnaryOp::Not, ConstValue::Bool(b)) => ConstValue::Bool(!b),
                _ => return Err("invalid operand in constant expression".into()),
            };
            Ok(Const { value: v, ty: c.ty })
        }
        ExprKind::Binary { op, lhs, rhs } => {
            let a = eval(types, lhs, expected, enum_lookup)?;
            let b = eval(types, rhs, expected, enum_lookup)?;
            let v = match (&a.value, &b.value) {
                (ConstValue::Int(x), ConstValue::Int(y)) => ConstValue::Int(match op {
                    BinaryOp::Add => x + y,
                    BinaryOp::Sub => x - y,
                    BinaryOp::Mul => x * y,
                    BinaryOp::Div => {
                        if *y == 0 {
                            return Err("division by zero in constant expression".into());
                        }
                        x / y
                    }
                    BinaryOp::Mod => {
                        if *y == 0 {
                            0
                        } else {
                            x % y
                        }
                    }
                    _ => return Err("operator not allowed in constant expression".into()),
                }),
                (ConstValue::Real(x), ConstValue::Real(y)) => ConstValue::Real(match op {
                    BinaryOp::Add => x + y,
                    BinaryOp::Sub => x - y,
                    BinaryOp::Mul => x * y,
                    BinaryOp::Div => x / y,
                    _ => return Err("operator not allowed in constant expression".into()),
                }),
                _ => return Err("operands of constant expression have different types".into()),
            };
            Ok(Const { value: v, ty: a.ty.or(b.ty) })
        }
        _ => Err("a constant is required here".into()),
    }
}

/// True if the expression is a literal (possibly with a sign), as required
/// for initial values by the grammar of annex B.
pub fn is_literal(e: &Expr) -> bool {
    match &e.kind {
        ExprKind::Literal(_) | ExprKind::Name(_) | ExprKind::TypedEnum { .. } => true,
        ExprKind::Unary { op: UnaryOp::Neg, operand } => matches!(operand.kind, ExprKind::Literal(_)),
        _ => false,
    }
}
