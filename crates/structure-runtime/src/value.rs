//! Typed values for reading and writing variables from the host.

use crate::error::TrapCode;
use crate::image::{FieldDesc, TypeDesc};
use crate::memory::Memory;
use crate::strings;
use crate::text;
use crate::ty::*;
use std::fmt;

#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    Bool(bool),
    Int(i128),
    Real(f64),
    /// TIME in microseconds.
    Time(i64),
    /// DATE in microseconds since 0001-01-01.
    Date(i64),
    /// TIME_OF_DAY in microseconds since midnight.
    TimeOfDay(i64),
    /// DATE_AND_TIME in microseconds since 0001-01-01.
    DateAndTime(i64),
    String(String),
    WString(String),
    Enum(String),
    Array(Vec<Value>),
    Struct(Vec<(String, Value)>),
}

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Value::Bool(b) => write!(f, "{}", if *b { "TRUE" } else { "FALSE" }),
            Value::Int(i) => write!(f, "{}", i),
            Value::Real(r) => write!(f, "{}", text::format_real(*r, false)),
            Value::Time(t) => write!(f, "{}", text::format_time(*t)),
            Value::Date(t) => write!(f, "{}", text::format_date(*t)),
            Value::TimeOfDay(t) => write!(f, "{}", text::format_tod(*t)),
            Value::DateAndTime(t) => write!(f, "{}", text::format_dt(*t)),
            Value::String(s) => write!(f, "'{}'", escape(s, '\'')),
            Value::WString(s) => write!(f, "\"{}\"", escape(s, '"')),
            Value::Enum(s) => write!(f, "{}", s),
            Value::Array(v) => {
                write!(f, "[")?;
                for (i, x) in v.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{}", x)?;
                }
                write!(f, "]")
            }
            Value::Struct(v) => {
                write!(f, "(")?;
                for (i, (n, x)) in v.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{} := {}", n, x)?;
                }
                write!(f, ")")
            }
        }
    }
}

fn escape(s: &str, quote: char) -> String {
    let mut out = String::new();
    for c in s.chars() {
        match c {
            '$' => out.push_str("$$"),
            '\n' => out.push_str("$N"),
            '\r' => out.push_str("$R"),
            '\t' => out.push_str("$T"),
            '\x0c' => out.push_str("$P"),
            c if c == quote => {
                out.push('$');
                out.push(c);
            }
            c => out.push(c),
        }
    }
    out
}

type R<T> = Result<T, TrapCode>;

/// Reads a value of the described type at a bit address.
pub fn read_value(mem: &Memory, addr: u64, ty: &TypeDesc) -> R<Value> {
    Ok(match ty {
        TypeDesc::Bool => Value::Bool(mem.load(Ty::Bool, addr)? != 0),
        TypeDesc::Real => Value::Real(f32_of(mem.load(Ty::F32, addr)?) as f64),
        TypeDesc::LReal => Value::Real(f64_of(mem.load(Ty::F64, addr)?)),
        TypeDesc::Time => Value::Time(mem.load(Ty::I64, addr)? as i64),
        TypeDesc::Date => Value::Date(mem.load(Ty::I64, addr)? as i64),
        TypeDesc::TimeOfDay => Value::TimeOfDay(mem.load(Ty::I64, addr)? as i64),
        TypeDesc::DateAndTime => Value::DateAndTime(mem.load(Ty::I64, addr)? as i64),
        TypeDesc::String { .. } => Value::String(strings::to_rust(&strings::read(mem, addr, false)?)),
        TypeDesc::WString { .. } => Value::WString(strings::to_rust(&strings::read(mem, addr, true)?)),
        TypeDesc::Enum { values, .. } => {
            let i = mem.load(Ty::I32, addr)? as i64;
            Value::Enum(values.get(i as usize).cloned().unwrap_or_else(|| format!("#{}", i)))
        }
        TypeDesc::Derived { base, .. } => read_value(mem, addr, base)?,
        TypeDesc::Array { dims, elem, stride } => {
            let count: i64 = dims.iter().map(|(l, h)| h - l + 1).product();
            let mut v = Vec::with_capacity(count as usize);
            for i in 0..count {
                v.push(read_value(mem, addr + i as u64 * stride, elem)?);
            }
            Value::Array(v)
        }
        TypeDesc::Struct { fields, .. } | TypeDesc::Instance { fields, .. } => {
            let mut v = Vec::new();
            for f in fields {
                if f.name.starts_with("__") {
                    continue;
                }
                v.push((f.name.clone(), read_value(mem, addr + f.offset, &f.ty)?));
            }
            Value::Struct(v)
        }
        TypeDesc::Reference(inner) => {
            let p = mem.load(Ty::U64, addr)?;
            if p == 0 {
                return Err(TrapCode::NoValueSpecified);
            }
            read_value(mem, p, inner)?
        }
        t => {
            let s = t.scalar().ok_or(TrapCode::InvalidOperation)?;
            Value::Int(to_i128(s, mem.load(s, addr)?))
        }
    })
}

/// Writes a value of the described type at a bit address.
pub fn write_value(mem: &mut Memory, addr: u64, ty: &TypeDesc, v: &Value) -> Result<(), String> {
    let err = |e: TrapCode| e.message().to_string();
    match (ty, v) {
        (TypeDesc::Bool, Value::Bool(b)) => mem.store(Ty::Bool, addr, *b as u64).map_err(err),
        (TypeDesc::Bool, Value::Int(i)) if *i == 0 || *i == 1 => mem.store(Ty::Bool, addr, *i as u64).map_err(err),
        (TypeDesc::Real, Value::Real(r)) => mem.store(Ty::F32, addr, of_f32(*r as f32)).map_err(err),
        (TypeDesc::Real, Value::Int(i)) => mem.store(Ty::F32, addr, of_f32(*i as f32)).map_err(err),
        (TypeDesc::LReal, Value::Real(r)) => mem.store(Ty::F64, addr, of_f64(*r)).map_err(err),
        (TypeDesc::LReal, Value::Int(i)) => mem.store(Ty::F64, addr, of_f64(*i as f64)).map_err(err),
        (TypeDesc::Time, Value::Time(t))
        | (TypeDesc::Date, Value::Date(t))
        | (TypeDesc::TimeOfDay, Value::TimeOfDay(t))
        | (TypeDesc::DateAndTime, Value::DateAndTime(t)) => mem.store(Ty::I64, addr, *t as u64).map_err(err),
        (TypeDesc::String { .. }, Value::String(s)) => {
            strings::write(mem, addr, false, &strings::from_rust(s, false), true).map_err(err)
        }
        (TypeDesc::WString { .. }, Value::WString(s) | Value::String(s)) => {
            strings::write(mem, addr, true, &strings::from_rust(s, true), true).map_err(err)
        }
        (TypeDesc::Enum { values, name }, Value::Enum(s)) => {
            let key = s.rsplit('#').next().unwrap_or(s);
            let i = values
                .iter()
                .position(|v| v.eq_ignore_ascii_case(key))
                .ok_or_else(|| format!("'{}' is not a value of enumerated type {}", s, name))?;
            mem.store(Ty::I32, addr, i as u64).map_err(err)
        }
        (TypeDesc::Derived { base, .. }, v) => write_value(mem, addr, base, v),
        (TypeDesc::Array { dims, elem, stride }, Value::Array(vs)) => {
            let count: i64 = dims.iter().map(|(l, h)| h - l + 1).product();
            if vs.len() as i64 != count {
                return Err(format!("expected {} array elements, got {}", count, vs.len()));
            }
            for (i, x) in vs.iter().enumerate() {
                write_value(mem, addr + i as u64 * stride, elem, x)?;
            }
            Ok(())
        }
        (TypeDesc::Struct { fields, .. } | TypeDesc::Instance { fields, .. }, Value::Struct(vs)) => {
            for (n, x) in vs {
                let f: &FieldDesc = fields
                    .iter()
                    .find(|f| f.name.eq_ignore_ascii_case(n))
                    .ok_or_else(|| format!("no element named {}", n))?;
                write_value(mem, addr + f.offset, &f.ty, x)?;
            }
            Ok(())
        }
        (TypeDesc::Reference(inner), v) => {
            let p = mem.load(Ty::U64, addr).map_err(err)?;
            if p == 0 {
                return Err("reference is not bound".into());
            }
            write_value(mem, p, inner, v)
        }
        (t, Value::Int(i)) if t.scalar().map(|s| s.is_int()).unwrap_or(false) => {
            let s = t.scalar().unwrap();
            let raw = from_i128(s, *i).ok_or_else(|| format!("value {} is out of range", i))?;
            mem.store(s, addr, raw).map_err(err)
        }
        (t, v) => Err(format!("cannot assign {} to a variable of type {:?}", v, t)),
    }
}

/// Parses a textual value (IEC literal syntax) for a given type.
pub fn parse_value(ty: &TypeDesc, s: &str) -> Result<Value, String> {
    use crate::image::TextKind;
    let s = s.trim();
    let bad = || format!("invalid value '{}'", s);
    Ok(match ty {
        TypeDesc::Bool => Value::Bool(text::parse_text(TextKind::Bool, s).ok_or_else(bad)? != 0),
        TypeDesc::Real | TypeDesc::LReal => {
            let t = if s.contains(['.', 'e', 'E']) { s.to_string() } else { format!("{}.0", s) };
            Value::Real(f64_of(text::parse_text(TextKind::Real(Ty::F64), &t).ok_or_else(bad)?))
        }
        TypeDesc::Time => Value::Time(text::parse_text(TextKind::Time, s).ok_or_else(bad)? as i64),
        TypeDesc::Date => Value::Date(text::parse_text(TextKind::Date, s).ok_or_else(bad)? as i64),
        TypeDesc::TimeOfDay => Value::TimeOfDay(text::parse_text(TextKind::TimeOfDay, s).ok_or_else(bad)? as i64),
        TypeDesc::DateAndTime => Value::DateAndTime(text::parse_text(TextKind::DateAndTime, s).ok_or_else(bad)? as i64),
        TypeDesc::String { .. } => Value::String(s.trim_matches('\'').to_string()),
        TypeDesc::WString { .. } => Value::WString(s.trim_matches('"').to_string()),
        TypeDesc::Enum { .. } => Value::Enum(s.to_string()),
        TypeDesc::Derived { base, .. } => parse_value(base, s)?,
        t if t.scalar().map(|x| x.is_int()).unwrap_or(false) => {
            Value::Int(to_i128(Ty::I64, text::parse_text(TextKind::Int(Ty::I64), s).ok_or_else(bad)?).max(i64::MIN as i128))
        }
        _ => return Err("values of this type cannot be parsed from text".into()),
    })
}
