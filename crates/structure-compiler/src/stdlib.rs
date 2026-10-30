//! Catalogue of the standard functions of 2.5.1.5 (tables 22 to 31),
//! including their typed forms (table 21, feature 2).

use crate::types::Elem;

/// Character string kinds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Text {
    String,
    WString,
}

/// Operand type of a conversion function.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConvTy {
    Elem(Elem),
    Text(Text),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Num1 {
    Abs,
    Sqrt,
    Ln,
    Log,
    Exp,
    Sin,
    Cos,
    Tan,
    Asin,
    Acos,
    Atan,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Arith {
    Add,
    Mul,
    Sub,
    Div,
    Mod,
    Expt,
    Move,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Shift {
    Shl,
    Shr,
    Ror,
    Rol,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Bitwise {
    And,
    Or,
    Xor,
    Not,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Compare {
    Gt,
    Ge,
    Eq,
    Le,
    Lt,
    Ne,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StrFn {
    Len,
    Left,
    Right,
    Mid,
    Concat,
    Insert,
    Delete,
    Replace,
    Find,
}

/// The time functions of table 30 with explicit names.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TimeFn {
    AddTime,
    AddTodTime,
    AddDtTime,
    SubTime,
    SubDateDate,
    SubTodTime,
    SubTodTod,
    SubDtTime,
    SubDtDt,
    MulTime,
    DivTime,
    ConcatDateTod,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StdFn {
    /// `*_TO_**` (table 22 feature 1) and table 30 features 13/14.
    Conv { from: ConvTy, to: ConvTy },
    /// TRUNC, optionally typed (`REAL_TRUNC_INT` is not standard; only
    /// `TRUNC` with the result type from context).
    Trunc,
    /// `*_BCD_TO_**` (feature 3); `from` absent for the generic `BCD_TO_**`.
    BcdToInt { from: Option<Elem>, to: Option<Elem> },
    /// `**_TO_BCD_*` (feature 4).
    IntToBcd { from: Option<Elem>, to: Option<Elem> },
    Num1 { op: Num1, ty: Option<Elem> },
    Arith { op: Arith, ty: Option<Elem> },
    Shift { op: Shift, ty: Option<Elem> },
    Bitwise { op: Bitwise, ty: Option<Elem> },
    Sel { ty: Option<Elem> },
    Max { ty: Option<Elem> },
    Min { ty: Option<Elem> },
    Limit { ty: Option<Elem> },
    Mux { k: Option<Elem>, ty: Option<Elem> },
    Compare { op: Compare, ty: Option<Elem> },
    Str(StrFn),
    Time { f: TimeFn, ty: Option<Elem> },
}

fn conv_ty(s: &str) -> Option<ConvTy> {
    match s {
        "STRING" => Some(ConvTy::Text(Text::String)),
        "WSTRING" => Some(ConvTy::Text(Text::WString)),
        _ => Elem::from_name(s).map(ConvTy::Elem),
    }
}

/// Splits `BASE_TYPE` into the base name and an elementary type suffix.
fn typed(name: &str, base: &str) -> Option<Option<Elem>> {
    if name == base {
        return Some(None);
    }
    let rest = name.strip_prefix(base)?.strip_prefix('_')?;
    Elem::from_name(rest).map(Some)
}

/// Recognises a standard function name (upper case).
pub fn parse(name: &str) -> Option<StdFn> {
    // Conversions: X_TO_Y, *_BCD_TO_**, **_TO_BCD_*.
    if let Some(idx) = name.find("_TO_") {
        let (a, b) = (&name[..idx], &name[idx + 4..]);
        if a == "BCD" {
            let to = Elem::from_name(b)?;
            return to.is_int().then_some(StdFn::BcdToInt { from: None, to: Some(to) });
        }
        if b == "BCD" {
            let from = Elem::from_name(a)?;
            return from.is_int().then_some(StdFn::IntToBcd { from: Some(from), to: None });
        }
        if let Some(src) = a.strip_suffix("_BCD") {
            let from = Elem::from_name(src)?;
            let to = Elem::from_name(b)?;
            return (from.is_bit() && from != Elem::Bool && to.is_int()).then_some(StdFn::BcdToInt { from: Some(from), to: Some(to) });
        }
        if let Some(dst) = b.strip_prefix("BCD_") {
            let from = Elem::from_name(a)?;
            let to = Elem::from_name(dst)?;
            return (from.is_int() && to.is_bit() && to != Elem::Bool).then_some(StdFn::IntToBcd { from: Some(from), to: Some(to) });
        }
        if let (Some(f), Some(t)) = (conv_ty(a), conv_ty(b)) {
            return Some(StdFn::Conv { from: f, to: t });
        }
        // Names like TIME_OF_DAY_TO_DATE_AND_TIME contain "_TO_" twice.
        for (i, _) in name.match_indices("_TO_") {
            let (a, b) = (&name[..i], &name[i + 4..]);
            if let (Some(f), Some(t)) = (conv_ty(a), conv_ty(b)) {
                return Some(StdFn::Conv { from: f, to: t });
            }
        }
    }
    if name == "TRUNC" {
        return Some(StdFn::Trunc);
    }
    let num1 = [
        ("ABS", Num1::Abs),
        ("SQRT", Num1::Sqrt),
        ("LN", Num1::Ln),
        ("LOG", Num1::Log),
        ("EXP", Num1::Exp),
        ("SIN", Num1::Sin),
        ("COS", Num1::Cos),
        ("TAN", Num1::Tan),
        ("ASIN", Num1::Asin),
        ("ACOS", Num1::Acos),
        ("ATAN", Num1::Atan),
    ];
    for (b, op) in num1 {
        if let Some(ty) = typed(name, b) {
            return Some(StdFn::Num1 { op, ty });
        }
    }
    let arith = [
        ("ADD", Arith::Add),
        ("MUL", Arith::Mul),
        ("SUB", Arith::Sub),
        ("DIV", Arith::Div),
        ("MOD", Arith::Mod),
        ("EXPT", Arith::Expt),
        ("MOVE", Arith::Move),
    ];
    let time = [
        ("ADD_TIME", TimeFn::AddTime),
        ("ADD_TOD_TIME", TimeFn::AddTodTime),
        ("ADD_DT_TIME", TimeFn::AddDtTime),
        ("SUB_TIME", TimeFn::SubTime),
        ("SUB_DATE_DATE", TimeFn::SubDateDate),
        ("SUB_TOD_TIME", TimeFn::SubTodTime),
        ("SUB_TOD_TOD", TimeFn::SubTodTod),
        ("SUB_DT_TIME", TimeFn::SubDtTime),
        ("SUB_DT_DT", TimeFn::SubDtDt),
        ("CONCAT_DATE_TOD", TimeFn::ConcatDateTod),
    ];
    for (b, f) in time {
        if name == b {
            return Some(StdFn::Time { f, ty: None });
        }
    }
    for (b, f) in [("MULTIME", TimeFn::MulTime), ("DIVTIME", TimeFn::DivTime)] {
        if let Some(ty) = typed(name, b) {
            return Some(StdFn::Time { f, ty });
        }
    }
    for (b, op) in arith {
        if let Some(ty) = typed(name, b) {
            return Some(StdFn::Arith { op, ty });
        }
    }
    for (b, op) in [("SHL", Shift::Shl), ("SHR", Shift::Shr), ("ROR", Shift::Ror), ("ROL", Shift::Rol)] {
        if let Some(ty) = typed(name, b) {
            return Some(StdFn::Shift { op, ty });
        }
    }
    for (b, op) in [("AND", Bitwise::And), ("OR", Bitwise::Or), ("XOR", Bitwise::Xor), ("NOT", Bitwise::Not)] {
        if let Some(ty) = typed(name, b) {
            return Some(StdFn::Bitwise { op, ty });
        }
    }
    if let Some(ty) = typed(name, "SEL") {
        return Some(StdFn::Sel { ty });
    }
    if let Some(ty) = typed(name, "MAX") {
        return Some(StdFn::Max { ty });
    }
    if let Some(ty) = typed(name, "MIN") {
        return Some(StdFn::Min { ty });
    }
    if let Some(ty) = typed(name, "LIMIT") {
        return Some(StdFn::Limit { ty });
    }
    if name == "MUX" {
        return Some(StdFn::Mux { k: None, ty: None });
    }
    if let Some(rest) = name.strip_prefix("MUX_") {
        // MUX_*_** : K type and input type
        for (i, _) in rest.match_indices('_') {
            if let (Some(k), Some(t)) = (Elem::from_name(&rest[..i]), Elem::from_name(&rest[i + 1..])) {
                return k.is_int().then_some(StdFn::Mux { k: Some(k), ty: Some(t) });
            }
        }
        return None;
    }
    for (b, op) in [("GT", Compare::Gt), ("GE", Compare::Ge), ("EQ", Compare::Eq), ("LE", Compare::Le), ("LT", Compare::Lt), ("NE", Compare::Ne)] {
        if let Some(ty) = typed(name, b) {
            return Some(StdFn::Compare { op, ty });
        }
    }
    let s = match name {
        "LEN" => StrFn::Len,
        "LEFT" => StrFn::Left,
        "RIGHT" => StrFn::Right,
        "MID" => StrFn::Mid,
        "CONCAT" => StrFn::Concat,
        "INSERT" => StrFn::Insert,
        "DELETE" => StrFn::Delete,
        "REPLACE" => StrFn::Replace,
        "FIND" => StrFn::Find,
        _ => return None,
    };
    Some(StdFn::Str(s))
}

/// Set of reserved standard function names.
#[derive(Clone, Copy, Default)]
pub struct StdNames;

impl StdNames {
    pub fn contains(&self, name: &str) -> bool {
        parse(name).is_some()
    }
}

pub fn standard_function_names() -> StdNames {
    StdNames
}

/// Formal parameter layout of a standard function.
pub struct Params {
    /// Fixed leading parameter names.
    pub fixed: &'static [&'static str],
    /// Extensible parameters: name prefix, first index, minimum count.
    pub ext: Option<(&'static str, usize, usize)>,
}

pub fn params(f: &StdFn) -> Params {
    const IN: &[&str] = &["IN"];
    const IN12: &[&str] = &["IN1", "IN2"];
    match f {
        StdFn::Conv { .. } | StdFn::Trunc | StdFn::BcdToInt { .. } | StdFn::IntToBcd { .. } | StdFn::Num1 { .. } => Params { fixed: IN, ext: None },
        StdFn::Arith { op: Arith::Add | Arith::Mul, .. } => Params { fixed: &[], ext: Some(("IN", 1, 2)) },
        StdFn::Arith { op: Arith::Move, .. } => Params { fixed: IN, ext: None },
        StdFn::Arith { .. } => Params { fixed: IN12, ext: None },
        StdFn::Shift { .. } => Params { fixed: &["IN", "N"], ext: None },
        StdFn::Bitwise { op: Bitwise::Not, .. } => Params { fixed: IN, ext: None },
        StdFn::Bitwise { .. } => Params { fixed: &[], ext: Some(("IN", 1, 2)) },
        StdFn::Sel { .. } => Params { fixed: &["G", "IN0", "IN1"], ext: None },
        StdFn::Max { .. } | StdFn::Min { .. } => Params { fixed: &[], ext: Some(("IN", 1, 2)) },
        StdFn::Limit { .. } => Params { fixed: &["MN", "IN", "MX"], ext: None },
        StdFn::Mux { .. } => Params { fixed: &["K"], ext: Some(("IN", 0, 1)) },
        StdFn::Compare { op: Compare::Ne, .. } => Params { fixed: IN12, ext: None },
        StdFn::Compare { .. } => Params { fixed: &[], ext: Some(("IN", 1, 2)) },
        StdFn::Str(s) => match s {
            StrFn::Len => Params { fixed: IN, ext: None },
            StrFn::Left | StrFn::Right => Params { fixed: &["IN", "L"], ext: None },
            StrFn::Mid => Params { fixed: &["IN", "L", "P"], ext: None },
            StrFn::Concat => Params { fixed: &[], ext: Some(("IN", 1, 2)) },
            StrFn::Insert => Params { fixed: &["IN1", "IN2", "P"], ext: None },
            StrFn::Delete => Params { fixed: &["IN", "L", "P"], ext: None },
            StrFn::Replace => Params { fixed: &["IN1", "IN2", "L", "P"], ext: None },
            StrFn::Find => Params { fixed: IN12, ext: None },
        },
        StdFn::Time { .. } => Params { fixed: IN12, ext: None },
    }
}

/// Maximum number of inputs of extensible functions (annex D).
pub const MAX_EXTENSIBLE_INPUTS: usize = 255;
