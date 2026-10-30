//! Data types (2.3): elementary, generic and derived types, their storage
//! layout and their run-time descriptions.

use std::collections::HashMap;
use structure_runtime::image::{FieldDesc, TypeDesc};
use structure_runtime::strings;
use structure_runtime::ty::Ty;

pub type TypeId = usize;

/// Elementary data types (table 10).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Elem {
    Bool,
    SInt,
    Int,
    DInt,
    LInt,
    USInt,
    UInt,
    UDInt,
    ULInt,
    Real,
    LReal,
    Time,
    Date,
    Tod,
    Dt,
    Byte,
    Word,
    DWord,
    LWord,
}

pub const ALL_ELEMS: [Elem; 19] = [
    Elem::Bool,
    Elem::SInt,
    Elem::Int,
    Elem::DInt,
    Elem::LInt,
    Elem::USInt,
    Elem::UInt,
    Elem::UDInt,
    Elem::ULInt,
    Elem::Real,
    Elem::LReal,
    Elem::Time,
    Elem::Date,
    Elem::Tod,
    Elem::Dt,
    Elem::Byte,
    Elem::Word,
    Elem::DWord,
    Elem::LWord,
];

impl Elem {
    pub fn name(self) -> &'static str {
        match self {
            Elem::Bool => "BOOL",
            Elem::SInt => "SINT",
            Elem::Int => "INT",
            Elem::DInt => "DINT",
            Elem::LInt => "LINT",
            Elem::USInt => "USINT",
            Elem::UInt => "UINT",
            Elem::UDInt => "UDINT",
            Elem::ULInt => "ULINT",
            Elem::Real => "REAL",
            Elem::LReal => "LREAL",
            Elem::Time => "TIME",
            Elem::Date => "DATE",
            Elem::Tod => "TIME_OF_DAY",
            Elem::Dt => "DATE_AND_TIME",
            Elem::Byte => "BYTE",
            Elem::Word => "WORD",
            Elem::DWord => "DWORD",
            Elem::LWord => "LWORD",
        }
    }

    /// Short name used to form typed function names (`ADD_INT`, `TOD_TO_DT`).
    pub fn short(self) -> &'static str {
        match self {
            Elem::Tod => "TOD",
            Elem::Dt => "DT",
            e => e.name(),
        }
    }

    pub fn from_name(s: &str) -> Option<Elem> {
        Some(match s {
            "BOOL" => Elem::Bool,
            "SINT" => Elem::SInt,
            "INT" => Elem::Int,
            "DINT" => Elem::DInt,
            "LINT" => Elem::LInt,
            "USINT" => Elem::USInt,
            "UINT" => Elem::UInt,
            "UDINT" => Elem::UDInt,
            "ULINT" => Elem::ULInt,
            "REAL" => Elem::Real,
            "LREAL" => Elem::LReal,
            "TIME" => Elem::Time,
            "DATE" => Elem::Date,
            "TIME_OF_DAY" | "TOD" => Elem::Tod,
            "DATE_AND_TIME" | "DT" => Elem::Dt,
            "BYTE" => Elem::Byte,
            "WORD" => Elem::Word,
            "DWORD" => Elem::DWord,
            "LWORD" => Elem::LWord,
            _ => return None,
        })
    }

    pub fn ty(self) -> Ty {
        match self {
            Elem::Bool => Ty::Bool,
            Elem::SInt => Ty::I8,
            Elem::Int => Ty::I16,
            Elem::DInt => Ty::I32,
            Elem::LInt => Ty::I64,
            Elem::USInt | Elem::Byte => Ty::U8,
            Elem::UInt | Elem::Word => Ty::U16,
            Elem::UDInt | Elem::DWord => Ty::U32,
            Elem::ULInt | Elem::LWord => Ty::U64,
            Elem::Real => Ty::F32,
            Elem::LReal => Ty::F64,
            Elem::Time | Elem::Date | Elem::Tod | Elem::Dt => Ty::I64,
        }
    }

    pub fn is_signed_int(self) -> bool {
        matches!(self, Elem::SInt | Elem::Int | Elem::DInt | Elem::LInt)
    }
    pub fn is_unsigned_int(self) -> bool {
        matches!(self, Elem::USInt | Elem::UInt | Elem::UDInt | Elem::ULInt)
    }
    pub fn is_int(self) -> bool {
        self.is_signed_int() || self.is_unsigned_int()
    }
    pub fn is_real(self) -> bool {
        matches!(self, Elem::Real | Elem::LReal)
    }
    pub fn is_num(self) -> bool {
        self.is_int() || self.is_real()
    }
    pub fn is_bit(self) -> bool {
        matches!(self, Elem::Bool | Elem::Byte | Elem::Word | Elem::DWord | Elem::LWord)
    }
    pub fn is_magnitude(self) -> bool {
        self.is_num() || self == Elem::Time
    }
    pub fn is_date(self) -> bool {
        matches!(self, Elem::Date | Elem::Tod | Elem::Dt)
    }

    /// Bit width of the value (BOOL: 1).
    pub fn width(self) -> u32 {
        self.ty().bits() as u32
    }
}

/// Generic data types (table 11).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Generic {
    Any,
    AnyDerived,
    AnyElementary,
    AnyMagnitude,
    AnyNum,
    AnyReal,
    AnyInt,
    AnyBit,
    AnyString,
    AnyDate,
}

impl Generic {
    pub fn from_name(s: &str) -> Option<Generic> {
        Some(match s {
            "ANY" => Generic::Any,
            "ANY_DERIVED" => Generic::AnyDerived,
            "ANY_ELEMENTARY" => Generic::AnyElementary,
            "ANY_MAGNITUDE" => Generic::AnyMagnitude,
            "ANY_NUM" => Generic::AnyNum,
            "ANY_REAL" => Generic::AnyReal,
            "ANY_INT" => Generic::AnyInt,
            "ANY_BIT" => Generic::AnyBit,
            "ANY_STRING" => Generic::AnyString,
            "ANY_DATE" => Generic::AnyDate,
            _ => return None,
        })
    }

    pub fn name(self) -> &'static str {
        match self {
            Generic::Any => "ANY",
            Generic::AnyDerived => "ANY_DERIVED",
            Generic::AnyElementary => "ANY_ELEMENTARY",
            Generic::AnyMagnitude => "ANY_MAGNITUDE",
            Generic::AnyNum => "ANY_NUM",
            Generic::AnyReal => "ANY_REAL",
            Generic::AnyInt => "ANY_INT",
            Generic::AnyBit => "ANY_BIT",
            Generic::AnyString => "ANY_STRING",
            Generic::AnyDate => "ANY_DATE",
        }
    }
}

/// A compile-time constant value.
#[derive(Clone, Debug, PartialEq)]
pub enum ConstValue {
    Bool(bool),
    Int(i128),
    Real(f64),
    /// TIME, DATE, TIME_OF_DAY and DATE_AND_TIME in microseconds.
    Time(i64),
    Date(i64),
    Tod(i64),
    Dt(i64),
    Str { wide: bool, chars: Vec<u32> },
    Enum { ty: TypeId, index: usize },
}

/// Resolved initial value of a variable or type.
#[derive(Clone, Debug, PartialEq)]
pub enum InitValue {
    Const(ConstValue),
    /// Array elements in storage order (rightmost subscript varies fastest).
    Array(Vec<InitValue>),
    /// Structure elements or function block variables by (upper-case) name.
    Struct(Vec<(String, InitValue)>),
}

#[derive(Clone, Debug)]
pub struct Field {
    pub name: String,
    pub ty: TypeId,
    /// Bit offset.
    pub offset: u64,
    pub init: Option<InitValue>,
}

#[derive(Clone, Debug)]
pub enum TypeKind {
    Elem(Elem),
    String { wide: bool, len: u32 },
    /// Directly derived type (table 12 feature 1) or derived structure
    /// with new initial values (table 14 feature 6).
    Alias { name: String, base: TypeId },
    Subrange { name: Option<String>, base: Elem, lo: i128, hi: i128 },
    Enum { name: String, values: Vec<String> },
    Array { dims: Vec<(i64, i64)>, elem: TypeId },
    Struct { name: String, fields: Vec<Field> },
    /// Function block or program instance.
    Instance { pou: usize },
    /// Placeholder after errors.
    Error,
}

#[derive(Clone, Debug)]
pub struct TypeInfo {
    pub kind: TypeKind,
    /// Storage size in bits (multiple of 8 except never; BOOL is 8).
    pub size: u64,
    /// Default initial value declared with the type.
    pub init: Option<InitValue>,
}

pub struct Types {
    pub list: Vec<TypeInfo>,
    pub by_name: HashMap<String, TypeId>,
    strings: HashMap<(bool, u32), TypeId>,
    arrays: HashMap<(Vec<(i64, i64)>, TypeId), TypeId>,
    elems: HashMap<Elem, TypeId>,
    pub error: TypeId,
}

pub fn round8(bits: u64) -> u64 {
    bits.div_ceil(8) * 8
}

impl Types {
    pub fn new() -> Types {
        let mut t = Types {
            list: Vec::new(),
            by_name: HashMap::new(),
            strings: HashMap::new(),
            arrays: HashMap::new(),
            elems: HashMap::new(),
            error: 0,
        };
        t.error = t.add(TypeInfo { kind: TypeKind::Error, size: 0, init: None });
        for e in ALL_ELEMS {
            let size = if e == Elem::Bool { 8 } else { e.ty().bits() };
            let id = t.add(TypeInfo { kind: TypeKind::Elem(e), size, init: None });
            t.elems.insert(e, id);
            t.by_name.insert(e.name().to_string(), id);
        }
        t.by_name.insert("TOD".into(), t.elem(Elem::Tod));
        t.by_name.insert("DT".into(), t.elem(Elem::Dt));
        t
    }

    pub fn add(&mut self, info: TypeInfo) -> TypeId {
        self.list.push(info);
        self.list.len() - 1
    }

    pub fn get(&self, id: TypeId) -> &TypeInfo {
        &self.list[id]
    }

    pub fn kind(&self, id: TypeId) -> &TypeKind {
        &self.list[id].kind
    }

    pub fn elem(&self, e: Elem) -> TypeId {
        self.elems[&e]
    }

    pub fn string(&mut self, wide: bool, len: u32) -> TypeId {
        if let Some(id) = self.strings.get(&(wide, len)) {
            return *id;
        }
        let size = strings::storage_bytes(len, wide) * 8;
        let id = self.add(TypeInfo { kind: TypeKind::String { wide, len }, size, init: None });
        self.strings.insert((wide, len), id);
        id
    }

    pub fn array(&mut self, dims: Vec<(i64, i64)>, elem: TypeId) -> TypeId {
        if let Some(id) = self.arrays.get(&(dims.clone(), elem)) {
            return *id;
        }
        let stride = self.stride(elem);
        let count: u64 = dims.iter().map(|(l, h)| (h - l + 1).max(0) as u64).product();
        let size = round8(stride * count);
        let id = self.add(TypeInfo { kind: TypeKind::Array { dims: dims.clone(), elem }, size, init: None });
        self.arrays.insert((dims, elem), id);
        id
    }

    /// Distance between consecutive array elements (BOOL arrays are packed).
    pub fn stride(&self, elem: TypeId) -> u64 {
        if self.elem_of(elem) == Some(Elem::Bool) && self.scalar(elem) == Some(Ty::Bool) {
            1
        } else {
            self.size(elem)
        }
    }

    pub fn size(&self, id: TypeId) -> u64 {
        self.list[id].size
    }

    /// Follows aliases to the defining type.
    pub fn resolve(&self, mut id: TypeId) -> TypeId {
        while let TypeKind::Alias { base, .. } = &self.list[id].kind {
            id = *base;
        }
        id
    }

    /// The elementary type underlying a single-element type.
    pub fn elem_of(&self, id: TypeId) -> Option<Elem> {
        match &self.list[self.resolve(id)].kind {
            TypeKind::Elem(e) => Some(*e),
            TypeKind::Subrange { base, .. } => Some(*base),
            _ => None,
        }
    }

    /// Machine type of a scalar (single-element) type.
    pub fn scalar(&self, id: TypeId) -> Option<Ty> {
        match &self.list[self.resolve(id)].kind {
            TypeKind::Elem(e) => Some(e.ty()),
            TypeKind::Subrange { base, .. } => Some(base.ty()),
            TypeKind::Enum { .. } => Some(Ty::I32),
            _ => None,
        }
    }

    pub fn is_string(&self, id: TypeId) -> Option<(bool, u32)> {
        match &self.list[self.resolve(id)].kind {
            TypeKind::String { wide, len } => Some((*wide, *len)),
            _ => None,
        }
    }

    pub fn is_enum(&self, id: TypeId) -> bool {
        matches!(self.list[self.resolve(id)].kind, TypeKind::Enum { .. })
    }

    pub fn enum_values(&self, id: TypeId) -> Option<&Vec<String>> {
        match &self.list[self.resolve(id)].kind {
            TypeKind::Enum { values, .. } => Some(values),
            _ => None,
        }
    }

    pub fn instance_pou(&self, id: TypeId) -> Option<usize> {
        match &self.list[self.resolve(id)].kind {
            TypeKind::Instance { pou } => Some(*pou),
            _ => None,
        }
    }

    pub fn subrange(&self, id: TypeId) -> Option<(i128, i128)> {
        let mut id = id;
        loop {
            match &self.list[id].kind {
                TypeKind::Alias { base, .. } => id = *base,
                TypeKind::Subrange { lo, hi, .. } => return Some((*lo, *hi)),
                _ => return None,
            }
        }
    }

    pub fn is_bool(&self, id: TypeId) -> bool {
        self.elem_of(id) == Some(Elem::Bool)
    }

    pub fn is_error(&self, id: TypeId) -> bool {
        matches!(self.list[id].kind, TypeKind::Error)
    }

    pub fn is_aggregate(&self, id: TypeId) -> bool {
        self.scalar(id).is_none()
    }

    /// Membership in a generic type (table 11 and the rules of 2.3.2).
    pub fn in_generic(&self, id: TypeId, g: Generic) -> bool {
        // Directly derived types share the generic type of their base.
        let r = self.resolve(id);
        let derived = matches!(self.list[r].kind, TypeKind::Enum { .. } | TypeKind::Array { .. } | TypeKind::Struct { .. });
        if let TypeKind::Subrange { .. } = self.list[r].kind {
            return matches!(g, Generic::Any | Generic::AnyElementary | Generic::AnyMagnitude | Generic::AnyNum | Generic::AnyInt);
        }
        if derived {
            return matches!(g, Generic::Any | Generic::AnyDerived);
        }
        if let TypeKind::String { .. } = self.list[r].kind {
            return matches!(g, Generic::Any | Generic::AnyElementary | Generic::AnyString);
        }
        let Some(e) = self.elem_of(r) else { return g == Generic::Any };
        match g {
            Generic::Any | Generic::AnyElementary => true,
            Generic::AnyDerived => false,
            Generic::AnyMagnitude => e.is_magnitude(),
            Generic::AnyNum => e.is_num(),
            Generic::AnyReal => e.is_real(),
            Generic::AnyInt => e.is_int(),
            Generic::AnyBit => e.is_bit(),
            Generic::AnyString => false,
            Generic::AnyDate => e.is_date(),
        }
    }

    pub fn name(&self, id: TypeId) -> String {
        match &self.list[id].kind {
            TypeKind::Elem(e) => e.name().to_string(),
            TypeKind::String { wide, len } => {
                format!("{}[{}]", if *wide { "WSTRING" } else { "STRING" }, len)
            }
            TypeKind::Alias { name, .. } => name.clone(),
            TypeKind::Subrange { name: Some(n), .. } => n.clone(),
            TypeKind::Subrange { name: None, base, lo, hi } => format!("{} ({}..{})", base.name(), lo, hi),
            TypeKind::Enum { name, .. } => name.clone(),
            TypeKind::Array { dims, elem } => {
                let d: Vec<String> = dims.iter().map(|(l, h)| format!("{}..{}", l, h)).collect();
                format!("ARRAY [{}] OF {}", d.join(","), self.name(*elem))
            }
            TypeKind::Struct { name, .. } => name.clone(),
            TypeKind::Instance { pou } => format!("<instance of POU #{}>", pou),
            TypeKind::Error => "<error>".into(),
        }
    }

    /// Two types are the same for assignment purposes (2.3.3.3): identical,
    /// related by direct derivation/subrange to the same elementary type,
    /// structurally identical arrays, or strings of the same kind.
    pub fn compatible(&self, a: TypeId, b: TypeId) -> bool {
        if a == b || self.is_error(a) || self.is_error(b) {
            return true;
        }
        let (ra, rb) = (self.resolve(a), self.resolve(b));
        if ra == rb {
            return true;
        }
        match (&self.list[ra].kind, &self.list[rb].kind) {
            (TypeKind::Elem(x), TypeKind::Elem(y)) => x == y,
            (TypeKind::Subrange { base, .. }, TypeKind::Elem(y)) | (TypeKind::Elem(y), TypeKind::Subrange { base, .. }) => base == y,
            (TypeKind::Subrange { base: x, .. }, TypeKind::Subrange { base: y, .. }) => x == y,
            (TypeKind::String { wide: x, .. }, TypeKind::String { wide: y, .. }) => x == y,
            (TypeKind::Array { dims: d1, elem: e1 }, TypeKind::Array { dims: d2, elem: e2 }) => {
                d1.len() == d2.len()
                    && d1.iter().zip(d2).all(|((l1, h1), (l2, h2))| h1 - l1 == h2 - l2)
                    && self.compatible(*e1, *e2)
                    && self.size(*e1) == self.size(*e2)
            }
            _ => false,
        }
    }

    /// Run-time description of a type.
    pub fn desc(&self, id: TypeId, pou_fields: &dyn Fn(usize) -> (String, Vec<FieldDesc>)) -> TypeDesc {
        match &self.list[id].kind {
            TypeKind::Elem(e) => match e {
                Elem::Bool => TypeDesc::Bool,
                Elem::SInt => TypeDesc::SInt,
                Elem::Int => TypeDesc::Int,
                Elem::DInt => TypeDesc::DInt,
                Elem::LInt => TypeDesc::LInt,
                Elem::USInt => TypeDesc::USInt,
                Elem::UInt => TypeDesc::UInt,
                Elem::UDInt => TypeDesc::UDInt,
                Elem::ULInt => TypeDesc::ULInt,
                Elem::Real => TypeDesc::Real,
                Elem::LReal => TypeDesc::LReal,
                Elem::Time => TypeDesc::Time,
                Elem::Date => TypeDesc::Date,
                Elem::Tod => TypeDesc::TimeOfDay,
                Elem::Dt => TypeDesc::DateAndTime,
                Elem::Byte => TypeDesc::Byte,
                Elem::Word => TypeDesc::Word,
                Elem::DWord => TypeDesc::DWord,
                Elem::LWord => TypeDesc::LWord,
            },
            TypeKind::String { wide: false, len } => TypeDesc::String { cap: *len },
            TypeKind::String { wide: true, len } => TypeDesc::WString { cap: *len },
            TypeKind::Alias { name, base } => match self.desc(*base, pou_fields) {
                TypeDesc::Struct { fields, .. } => TypeDesc::Struct { name: name.clone(), fields },
                d => TypeDesc::Derived { name: name.clone(), base: Box::new(d) },
            },
            TypeKind::Subrange { name, base, .. } => TypeDesc::Derived {
                name: name.clone().unwrap_or_else(|| self.name(id)),
                base: Box::new(self.desc(self.elem(*base), pou_fields)),
            },
            TypeKind::Enum { name, values } => TypeDesc::Enum { name: name.clone(), values: values.clone() },
            TypeKind::Array { dims, elem } => {
                TypeDesc::Array { dims: dims.clone(), elem: Box::new(self.desc(*elem, pou_fields)), stride: self.stride(*elem) }
            }
            TypeKind::Struct { name, fields } => TypeDesc::Struct {
                name: name.clone(),
                fields: fields.iter().map(|f| FieldDesc { name: f.name.clone(), offset: f.offset, ty: self.desc(f.ty, pou_fields) }).collect(),
            },
            TypeKind::Instance { pou } => {
                let (name, fields) = pou_fields(*pou);
                TypeDesc::Instance { name, fields }
            }
            TypeKind::Error => TypeDesc::Bool,
        }
    }
}

impl Default for Types {
    fn default() -> Self {
        Types::new()
    }
}
