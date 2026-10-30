//! Compilation context: declared types, program organisation units and
//! their variables, and generated routines.

use crate::codegen::RoutineCode;
use crate::consteval::{self, Const};
use crate::ir::{Loc, St};
use crate::types::*;
use std::collections::{HashMap, HashSet};
use structure_runtime::strings::{DEFAULT_STRING_LENGTH, MAX_STRING_LENGTH};
use structure_syntax::ast::{self, Edge, Initializer, TypeSpec, VarKind, VarQualifier};
use structure_syntax::diag::{Category, Diagnostic, Diagnostics, Severity};
use structure_syntax::{Ident, SourceMap, Span};

/// Start of the constant data pool (bytes). Address 0 is never a valid
/// reference, so it is used to mark "no value".
pub const POOL_BASE: u64 = 8;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PouKind {
    Function,
    FunctionBlock,
    Program,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VKind {
    Input,
    Output,
    InOut,
    Local,
    Temp,
    External,
    /// VAR_GLOBAL inside a program.
    Global,
    /// Function result.
    Result,
    /// Compiler-generated variable.
    Hidden,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Storage {
    /// Field of the instance (FB/program).
    Inst(u64),
    /// Instance field holding a reference.
    InstRef(u64),
    /// Field of the call frame.
    Frame(u64),
    /// Frame field holding a reference.
    FrameRef(u64),
}

#[derive(Clone, Debug)]
pub struct VarInfo {
    pub name: String,
    pub kind: VKind,
    pub ty: TypeId,
    pub storage: Storage,
    pub constant: bool,
    pub retain: Option<bool>,
    pub edge: Option<Edge>,
    /// Hidden variable holding the edge-detected value of an R_EDGE/F_EDGE input.
    pub edge_result: Option<usize>,
    pub init: Option<InitValue>,
    /// Full location (`AT %IX1`) or incomplete location (`AT %Q*`).
    pub location: Option<ast::DirectVar>,
    pub span: Span,
    /// Implicitly declared (EN, ENO).
    pub implicit: bool,
}

impl VarInfo {
    pub fn is_ref(&self) -> bool {
        matches!(self.storage, Storage::InstRef(_) | Storage::FrameRef(_))
    }

    /// Location of the variable's storage relative to the instance/frame.
    /// References that are set by invocations (in-out variables and
    /// function block instances passed as inputs) are checked at run time
    /// (2.5.2.2 rule 11: "no value specified").
    pub fn loc(&self) -> Loc {
        let checked = |slot: Loc| -> Loc {
            Loc::Ind(Box::new(crate::ir::Ex::Check {
                ty: structure_runtime::ty::Ty::U64,
                lo: 1,
                hi: i64::MAX,
                code: structure_runtime::error::TrapCode::NoValueSpecified,
                value: Box::new(crate::ir::Ex::Load(structure_runtime::ty::Ty::U64, slot)),
            }))
        };
        let needs_check = matches!(self.kind, VKind::InOut | VKind::Input);
        match self.storage {
            Storage::Inst(o) => Loc::Ip(o as u32),
            Storage::Frame(o) => Loc::Fp(o as u32),
            Storage::InstRef(o) if needs_check => checked(Loc::Ip(o as u32)),
            Storage::FrameRef(o) if needs_check => checked(Loc::Fp(o as u32)),
            Storage::InstRef(o) => Loc::Ip(o as u32).deref(),
            Storage::FrameRef(o) => Loc::Fp(o as u32).deref(),
        }
    }

    /// Location of the reference slot itself (for references).
    pub fn slot(&self) -> Loc {
        match self.storage {
            Storage::Inst(o) | Storage::InstRef(o) => Loc::Ip(o as u32),
            Storage::Frame(o) | Storage::FrameRef(o) => Loc::Fp(o as u32),
        }
    }

    pub fn offset(&self) -> u64 {
        match self.storage {
            Storage::Inst(o) | Storage::InstRef(o) | Storage::Frame(o) | Storage::FrameRef(o) => o,
        }
    }
}

/// SFC state allocated in the instance of a POU with an SFC body.
#[derive(Clone, Debug, Default)]
pub struct SfcLayout {
    pub steps: Vec<SfcStepLayout>,
    pub actions: Vec<SfcActionLayout>,
}

#[derive(Clone, Debug)]
pub struct SfcStepLayout {
    pub name: String,
    pub initial: bool,
    /// The step belongs to a chart included in an action (table 42
    /// feature 2s); its initial step is activated with the action.
    pub nested: bool,
    /// Offsets of X (BOOL), T (TIME) and the activation time (TIME).
    pub x: u64,
    pub t: u64,
    pub start: u64,
}

#[derive(Clone, Debug)]
pub struct SfcActionLayout {
    pub name: String,
    /// Offset of the ACTION_CONTROL instance.
    pub control: u64,
}

#[derive(Clone, Debug)]
pub struct PouInfo {
    pub name: String,
    pub kind: PouKind,
    pub span: Span,
    pub ast: ast::Pou,
    pub std: bool,
    pub result: Option<TypeId>,
    pub vars: Vec<VarInfo>,
    pub var_map: HashMap<String, usize>,
    /// Instance type (function blocks and programs).
    pub inst_type: Option<TypeId>,
    /// Instance size in bits.
    pub inst_size: u64,
    /// First bit of the region written back after execution of a
    /// task-associated instance (outputs and internal variables).
    pub outputs_start: u64,
    /// Frame size in bytes before temporaries.
    pub frame_base: u64,
    pub routine: Option<usize>,
    pub init_routine: Option<usize>,
    pub laid_out: bool,
    pub sfc: Option<SfcLayout>,
}

impl PouInfo {
    pub fn var(&self, name: &str) -> Option<&VarInfo> {
        self.var_map.get(name).map(|i| &self.vars[*i])
    }

    pub fn has_en(&self) -> Option<&VarInfo> {
        self.var("EN")
    }

    pub fn has_eno(&self) -> Option<&VarInfo> {
        self.var("ENO")
    }

    pub fn inputs(&self) -> impl Iterator<Item = (usize, &VarInfo)> {
        self.vars.iter().enumerate().filter(|(_, v)| v.kind == VKind::Input && v.name != "EN")
    }
}

#[derive(Clone, Debug)]
pub struct RoutineDef {
    pub name: String,
    pub body: Vec<St>,
    pub frame_bytes: u64,
    pub code: Option<RoutineCode>,
}

#[derive(Clone, Debug, Default)]
pub struct CompileOptions {
    /// Name of the configuration to build when several are present.
    pub configuration: Option<String>,
}

enum TypeState {
    Pending(ast::TypeDecl),
    InProgress,
    Done(TypeId),
}

pub struct Ctx<'s> {
    pub sources: &'s SourceMap,
    pub types: Types,
    pub pous: Vec<PouInfo>,
    pub pou_by_name: HashMap<String, usize>,
    pub routines: Vec<RoutineDef>,
    pub pool: Vec<u8>,
    pool_map: HashMap<(bool, Vec<u32>), u64>,
    pub diags: Diagnostics,
    type_states: HashMap<String, TypeState>,
    pub configs: Vec<ast::Configuration>,
    pub options: CompileOptions,
    /// Names of standard functions (reserved).
    pub std_function_names: crate::stdlib::StdNames,
    /// Whether code for the standard library is being processed.
    pub in_std: bool,
}

impl<'s> Ctx<'s> {
    pub fn new(sources: &'s SourceMap, options: CompileOptions) -> Ctx<'s> {
        Ctx {
            sources,
            types: Types::new(),
            pous: Vec::new(),
            pou_by_name: HashMap::new(),
            routines: Vec::new(),
            pool: Vec::new(),
            pool_map: HashMap::new(),
            diags: Diagnostics::new(),
            type_states: HashMap::new(),
            configs: Vec::new(),
            options,
            std_function_names: crate::stdlib::standard_function_names(),
            in_std: false,
        }
    }

    // ------------------------------------------------------------ diagnostics

    pub fn error(&mut self, msg: impl Into<String>, span: Span) {
        self.diags.push(Diagnostic::error(msg, span));
    }

    pub fn error_clause(&mut self, clause: &'static str, msg: impl Into<String>, span: Span) {
        self.diags.push(Diagnostic::error(msg, span).with_clause(clause));
    }

    pub fn annex_e(&mut self, clause: &'static str, msg: impl Into<String>, span: Span) {
        self.diags.push(Diagnostic::annex_e(clause, msg, span));
    }

    pub fn extension(&mut self, msg: impl Into<String>, span: Span) {
        if !self.in_std {
            self.diags.push(Diagnostic::extension(msg, span));
        }
    }

    pub fn warning(&mut self, msg: impl Into<String>, span: Span) {
        if !self.in_std {
            self.diags.push(Diagnostic::warning(msg, span));
        }
    }

    pub fn impl_dependent(&mut self, msg: impl Into<String>, span: Span) {
        if !self.in_std {
            self.diags.push(Diagnostic::new(Severity::Note, Category::ImplementationDependent, msg, Some(span)));
        }
    }

    // ------------------------------------------------------------ data pool

    /// Places a string constant in the data pool; returns its bit address.
    pub fn string_literal(&mut self, wide: bool, chars: &[u32]) -> u64 {
        let key = (wide, chars.to_vec());
        if let Some(a) = self.pool_map.get(&key) {
            return *a;
        }
        let bytes = structure_runtime::strings::encode_literal(chars, wide);
        // keep 2-byte alignment for readability
        if self.pool.len() % 2 == 1 {
            self.pool.push(0);
        }
        let addr = (POOL_BASE + self.pool.len() as u64) * 8;
        self.pool.extend_from_slice(&bytes);
        self.pool_map.insert(key, addr);
        addr
    }

    // ------------------------------------------------------------ routines

    pub fn new_routine(&mut self, name: impl Into<String>) -> usize {
        self.routines.push(RoutineDef { name: name.into(), body: Vec::new(), frame_bytes: 0, code: None });
        self.routines.len() - 1
    }

    // ------------------------------------------------------------ registration

    pub fn is_reserved_name(&self, key: &str) -> bool {
        Elem::from_name(key).is_some()
            || Generic::from_name(key).is_some()
            || key == "STRING"
            || key == "WSTRING"
            || (self.std_function_names.contains(key) && !self.in_std)
            || (!self.in_std && self.pou_by_name.get(key).map(|p| self.pous[*p].std).unwrap_or(false))
    }

    /// Registers the library elements of a parsed source unit.
    pub fn register(&mut self, unit: &ast::SourceUnit, std: bool) {
        for el in &unit.elements {
            match el {
                ast::LibraryElement::DataTypes(d) => {
                    for t in &d.types {
                        let key = t.name.key();
                        if self.is_reserved_name(&key) || self.type_states.contains_key(&key) || self.pou_by_name.contains_key(&key) {
                            self.error_clause("2.3.3.1", format!("duplicate or reserved type name {}", t.name), t.name.span);
                            continue;
                        }
                        self.type_states.insert(key, TypeState::Pending(t.clone()));
                    }
                }
                ast::LibraryElement::Pou(p) => {
                    let key = p.name.key();
                    if (!std && self.is_reserved_name(&key)) || self.type_states.contains_key(&key) || self.pou_by_name.contains_key(&key) {
                        let msg = if self.pou_by_name.get(&key).map(|i| self.pous[*i].std).unwrap_or(false) {
                            format!("{} is the name of a standard function block and cannot be redeclared (1.5.1 h)", p.name)
                        } else {
                            format!("duplicate or reserved name {}", p.name)
                        };
                        self.error_clause("1.5.1", msg, p.name.span);
                        continue;
                    }
                    let kind = match p.kind {
                        ast::PouKind::Function { .. } => PouKind::Function,
                        ast::PouKind::FunctionBlock => PouKind::FunctionBlock,
                        ast::PouKind::Program => PouKind::Program,
                    };
                    let idx = self.pous.len();
                    let inst_type = if kind != PouKind::Function {
                        Some(self.types.add(TypeInfo { kind: TypeKind::Instance { pou: idx }, size: 0, init: None }))
                    } else {
                        None
                    };
                    self.pous.push(PouInfo {
                        name: key.clone(),
                        kind,
                        span: p.name.span,
                        ast: p.clone(),
                        std,
                        result: None,
                        vars: Vec::new(),
                        var_map: HashMap::new(),
                        inst_type,
                        inst_size: 0,
                        outputs_start: 0,
                        frame_base: 0,
                        routine: None,
                        init_routine: None,
                        laid_out: false,
                        sfc: None,
                    });
                    self.pou_by_name.insert(key, idx);
                }
                ast::LibraryElement::Configuration(c) => self.configs.push(c.clone()),
            }
        }
    }

    // ------------------------------------------------------------ types

    pub fn resolve_all_types(&mut self) {
        let names: Vec<String> = self.type_states.keys().cloned().collect();
        let mut names = names;
        names.sort();
        for n in names {
            self.named_type(&n, Span::default());
        }
    }

    /// Looks up a type by (upper-case) name, resolving a pending
    /// declaration if necessary.
    pub fn named_type(&mut self, key: &str, span: Span) -> Option<TypeId> {
        if let Some(id) = self.types.by_name.get(key) {
            return Some(*id);
        }
        match self.type_states.get(key) {
            Some(TypeState::Done(id)) => return Some(*id),
            Some(TypeState::InProgress) => {
                self.error_clause("2.3.3.1", format!("type {} is defined in terms of itself", key), span);
                return Some(self.types.error);
            }
            Some(TypeState::Pending(_)) => {}
            None => {
                if let Some(p) = self.pou_by_name.get(key) {
                    return self.pous[*p].inst_type;
                }
                return None;
            }
        }
        let Some(TypeState::Pending(decl)) = self.type_states.insert(key.to_string(), TypeState::InProgress) else { unreachable!() };
        let id = self.declare_type(&decl);
        self.type_states.insert(key.to_string(), TypeState::Done(id));
        self.types.by_name.insert(key.to_string(), id);
        Some(id)
    }

    fn declare_type(&mut self, decl: &ast::TypeDecl) -> TypeId {
        let name = decl.name.key();
        let id = match &decl.spec {
            TypeSpec::Named(base) => {
                let b = self.resolve_spec(&decl.spec, false);
                let _ = base;
                let size = self.types.size(b);
                self.types.add(TypeInfo { kind: TypeKind::Alias { name: name.clone(), base: b }, size, init: None })
            }
            TypeSpec::Subrange { base, lo, hi, span } => {
                let (be, lo, hi) = self.subrange_bounds(base, lo, hi, *span);
                let size = if be == Elem::Bool { 8 } else { be.ty().bits() };
                self.types.add(TypeInfo { kind: TypeKind::Subrange { name: Some(name.clone()), base: be, lo, hi }, size, init: None })
            }
            TypeSpec::Enum { values, .. } => self.enum_type(name.clone(), values),
            TypeSpec::Struct { fields, .. } => self.struct_type(name.clone(), fields),
            TypeSpec::String { wide, len, span } => {
                let l = self.string_len(len.as_deref(), *span);
                let s = self.types.string(*wide, l);
                let size = self.types.size(s);
                self.types.add(TypeInfo { kind: TypeKind::Alias { name: name.clone(), base: s }, size, init: None })
            }
            TypeSpec::Array { .. } => {
                let a = self.resolve_spec(&decl.spec, false);
                let size = self.types.size(a);
                self.types.add(TypeInfo { kind: TypeKind::Alias { name: name.clone(), base: a }, size, init: None })
            }
        };
        if let Some(init) = &decl.init {
            let v = self.initializer(init, id);
            self.types.list[id].init = v;
        }
        if self.types.instance_pou(id).is_some() {
            self.error_clause("2.3.3.1", "a data type cannot be derived from a function block type", decl.span);
        }
        id
    }

    fn enum_type(&mut self, name: String, values: &[Ident]) -> TypeId {
        let mut seen = HashSet::new();
        let mut vals = Vec::new();
        for v in values {
            let k = v.key();
            if !seen.insert(k.clone()) {
                self.error_clause("2.3.3.1", format!("duplicate enumerated value {}", v), v.span);
            }
            vals.push(k);
        }
        self.types.add(TypeInfo { kind: TypeKind::Enum { name, values: vals }, size: 32, init: None })
    }

    fn struct_type(&mut self, name: String, fields: &[ast::StructField]) -> TypeId {
        let mut out: Vec<Field> = Vec::new();
        let mut off = 0u64;
        for f in fields {
            let key = f.name.key();
            if out.iter().any(|x| x.name == key) {
                self.error_clause("2.3.3.1", format!("duplicate structure element {}", f.name), f.name.span);
                continue;
            }
            if matches!(f.spec, TypeSpec::Struct { .. }) {
                self.extension("nested anonymous structure declarations are an extension", f.spec.span());
            }
            let ty = self.resolve_spec(&f.spec, true);
            if self.types.instance_pou(ty).is_some() {
                self.error_clause("2.3.3.1", "structure elements cannot be function block instances", f.span);
            }
            let init = f.init.as_ref().and_then(|i| self.initializer(i, ty));
            off = round8(off);
            out.push(Field { name: key, ty, offset: off, init });
            off += self.types.size(ty);
        }
        let size = round8(off).max(8);
        self.types.add(TypeInfo { kind: TypeKind::Struct { name, fields: out }, size, init: None })
    }

    fn subrange_bounds(&mut self, base: &Ident, lo: &ast::Expr, hi: &ast::Expr, span: Span) -> (Elem, i128, i128) {
        let be = match self.named_type(&base.key(), base.span).and_then(|t| self.types.elem_of(t)) {
            Some(e) if e.is_int() => e,
            _ => {
                self.error_clause("2.3.3.1", format!("the base type of a subrange shall be an integer type, not {}", base), base.span);
                Elem::DInt
            }
        };
        let bt = self.types.elem(be);
        let l = self.const_int(lo, Some(bt)).unwrap_or(0);
        let h = self.const_int(hi, Some(bt)).unwrap_or(0);
        let (rlo, rhi) = be.ty().int_range();
        if l < rlo || h > rhi {
            self.error_clause("2.3.3.1", format!("subrange limits are outside the range of {}", be.name()), span);
        }
        if l > h {
            self.error_clause("2.3.3.1", "the lower limit of a subrange shall not exceed the upper limit", span);
        }
        (be, l, h)
    }

    fn string_len(&mut self, len: Option<&ast::Expr>, span: Span) -> u32 {
        match len {
            None => DEFAULT_STRING_LENGTH,
            Some(e) => match self.const_int(e, None) {
                Some(n) if n >= 1 && n <= MAX_STRING_LENGTH as i128 => n as u32,
                Some(n) => {
                    self.error_clause("2.3.3.2", format!("string length {} is outside 1..{} (annex D)", n, MAX_STRING_LENGTH), span);
                    DEFAULT_STRING_LENGTH
                }
                None => DEFAULT_STRING_LENGTH,
            },
        }
    }

    /// Evaluates a constant integer expression.
    pub fn const_int(&mut self, e: &ast::Expr, ty: Option<TypeId>) -> Option<i128> {
        match self.const_eval(e, ty) {
            Some(Const { value: ConstValue::Int(i), .. }) => Some(i),
            Some(_) => {
                self.error("an integer constant is required here", e.span);
                None
            }
            None => None,
        }
    }

    pub fn const_eval(&mut self, e: &ast::Expr, expected: Option<TypeId>) -> Option<Const> {
        let types = &self.types;
        let lookup = |name: &str, exp: Option<TypeId>| -> Result<ConstValue, String> { enum_lookup(types, name, exp) };
        match consteval::eval(&self.types, e, expected, &lookup) {
            Ok(c) => {
                if !consteval::is_literal(e) && !self.in_std {
                    let is_paren_lit = matches!(&e.kind, ast::ExprKind::Paren(_));
                    if !is_paren_lit {
                        self.extension("constant expressions in declarations are an extension (B.1.2 requires a literal)", e.span);
                    }
                }
                Some(c)
            }
            Err(m) => {
                self.error(m, e.span);
                None
            }
        }
    }

    /// Resolves a type specification to a type.
    pub fn resolve_spec(&mut self, spec: &TypeSpec, allow_struct: bool) -> TypeId {
        match spec {
            TypeSpec::Named(id) => {
                let key = id.key();
                if key == "STRING" {
                    return self.types.string(false, DEFAULT_STRING_LENGTH);
                }
                if key == "WSTRING" {
                    return self.types.string(true, DEFAULT_STRING_LENGTH);
                }
                if let Some(g) = Generic::from_name(&key) {
                    if !self.in_std {
                        self.error_clause(
                            "2.3.2",
                            format!("generic data type {} shall not be used in user-declared program organization units", g.name()),
                            id.span,
                        );
                    }
                    return self.types.error;
                }
                match self.named_type(&key, id.span) {
                    Some(t) => t,
                    None => {
                        self.error(format!("unknown data type {}", id), id.span);
                        self.types.error
                    }
                }
            }
            TypeSpec::String { wide, len, span } => {
                let l = self.string_len(len.as_deref(), *span);
                self.types.string(*wide, l)
            }
            TypeSpec::Subrange { base, lo, hi, span } => {
                let (be, lo, hi) = self.subrange_bounds(base, lo, hi, *span);
                let size = be.ty().bits();
                self.types.add(TypeInfo { kind: TypeKind::Subrange { name: None, base: be, lo, hi }, size, init: None })
            }
            TypeSpec::Enum { values, .. } => self.enum_type("<enumeration>".into(), values),
            TypeSpec::Array { dims, elem, span } => {
                let mut ds = Vec::new();
                for (lo, hi) in dims {
                    let l = self.const_int(lo, None).unwrap_or(0);
                    let h = self.const_int(hi, None).unwrap_or(0);
                    if l > h {
                        self.error_clause("2.3.3.1", "the lower bound of an array subrange shall not exceed the upper bound", lo.span.to(hi.span));
                    }
                    if l < i32::MIN as i128 || h > i32::MAX as i128 {
                        self.error_clause("2.3.3.1", "array subscript range exceeds the implementation limit (annex D)", lo.span.to(hi.span));
                    }
                    ds.push((l as i64, h.max(l) as i64));
                }
                let et = self.resolve_spec(elem, false);
                if self.types.instance_pou(et).is_some() {
                    self.extension("arrays of function block instances are an extension", *span);
                }
                let count: i128 = ds.iter().map(|(l, h)| (h - l + 1) as i128).product();
                if count * self.types.size(et) as i128 > (1i128 << 32) * 8 {
                    self.error_clause("2.3.3.1", "array exceeds the maximum array size (annex D)", *span);
                }
                self.types.array(ds, et)
            }
            TypeSpec::Struct { fields, span } => {
                if !allow_struct {
                    self.error_clause("2.3.3.1", "anonymous structures can only be declared in TYPE declarations", *span);
                }
                self.struct_type("<structure>".into(), fields)
            }
        }
    }

    // ------------------------------------------------------------ initial values

    /// Resolves an initializer for a value of type `ty`.
    pub fn initializer(&mut self, init: &Initializer, ty: TypeId) -> Option<InitValue> {
        let r = self.types.resolve(ty);
        match (init, self.types.kind(r).clone()) {
            (Initializer::Array(elems, span), TypeKind::Array { dims, elem }) => {
                let count: i64 = dims.iter().map(|(l, h)| h - l + 1).product();
                let mut out = Vec::new();
                for e in elems {
                    match e {
                        ast::ArrayInitElem::Single(i) => {
                            if let Some(v) = self.initializer(i, elem) {
                                out.push(v);
                            }
                        }
                        ast::ArrayInitElem::Repeat { count: c, value, span } => {
                            let n = self.const_int(c, None).unwrap_or(0);
                            if n < 0 {
                                self.error("repetition factor shall not be negative", *span);
                            }
                            let v = match value {
                                Some(i) => self.initializer(i, elem),
                                None => None,
                            };
                            for _ in 0..n.max(0).min(count as i128 + 1) {
                                match &v {
                                    Some(v) => out.push(v.clone()),
                                    None => out.push(self.default_init(elem)),
                                }
                            }
                        }
                    }
                }
                if out.len() as i64 > count {
                    self.diags.push(
                        Diagnostic::warning(
                            format!("{} initial values given for an array of {} elements; the excess values are ignored", out.len(), count),
                            *span,
                        )
                        .with_clause("2.4.3.2"),
                    );
                    out.truncate(count as usize);
                } else if (out.len() as i64) < count && !self.in_std {
                    self.diags.push(
                        Diagnostic::warning(
                            format!(
                                "{} initial values given for an array of {} elements; the remaining elements take their default values",
                                out.len(),
                                count
                            ),
                            *span,
                        )
                        .with_clause("2.4.3.2"),
                    );
                }
                Some(InitValue::Array(out))
            }
            (Initializer::Struct(elems, span), TypeKind::Struct { fields, .. }) => {
                let mut out = Vec::new();
                for (name, v) in elems {
                    let key = name.key();
                    match fields.iter().find(|f| f.name == key) {
                        Some(f) => {
                            if let Some(iv) = self.initializer(v, f.ty) {
                                out.push((key, iv));
                            }
                        }
                        None => self.error(format!("{} has no element named {}", self.types.name(ty), name), name.span),
                    }
                }
                let _ = span;
                Some(InitValue::Struct(out))
            }
            (Initializer::Struct(elems, _), TypeKind::Instance { pou }) => {
                // FB instance initialisation: inputs and accessible variables.
                let mut out = Vec::new();
                for (name, v) in elems {
                    let key = name.key();
                    let var = self.pous[pou].var(&key).cloned();
                    match var {
                        Some(var) if matches!(var.kind, VKind::Input | VKind::Output | VKind::Local) && !var.is_ref() => {
                            if let Some(iv) = self.initializer(v, var.ty) {
                                out.push((key, iv));
                            }
                        }
                        Some(_) => self.error(format!("variable {} cannot be initialized here", name), name.span),
                        None => self.error(format!("function block {} has no variable named {}", self.pous[pou].name, name), name.span),
                    }
                }
                Some(InitValue::Struct(out))
            }
            (Initializer::Expr(e), _) => {
                if self.types.is_aggregate(r) && self.types.is_string(r).is_none() {
                    self.error(format!("a structured initial value is required for type {}", self.types.name(ty)), e.span);
                    return None;
                }
                let c = self.const_eval(e, Some(ty))?;
                if let Some(t) = c.ty {
                    if !self.types.compatible(t, ty) && !(self.types.is_string(t).is_some() && self.types.is_string(ty).is_some()) {
                        self.error(format!("initial value of type {} is not compatible with {}", self.types.name(t), self.types.name(ty)), e.span);
                        return None;
                    }
                }
                match consteval::coerce(&self.types, &c.value, ty) {
                    Ok((v, ext)) => {
                        if ext {
                            self.extension("implicit conversion of an integer literal to a real type is an extension", e.span);
                        }
                        Some(InitValue::Const(v))
                    }
                    Err(m) => {
                        let clause = if self.types.subrange(ty).is_some() { "2.3.3.1" } else { "2.4.2" };
                        self.diags.push(Diagnostic::annex_e(clause, m, e.span));
                        None
                    }
                }
            }
            (i, _) => {
                self.error(format!("this initial value does not match type {}", self.types.name(ty)), i.span());
                None
            }
        }
    }

    /// The default initial value of a type (2.3.3.2, table 13).
    pub fn default_init(&self, ty: TypeId) -> InitValue {
        if let Some(i) = &self.types.get(ty).init {
            return i.clone();
        }
        match self.types.kind(ty) {
            TypeKind::Alias { base, .. } => self.default_init(*base),
            TypeKind::Subrange { lo, .. } => InitValue::Const(ConstValue::Int(*lo)),
            TypeKind::Enum { .. } => InitValue::Const(ConstValue::Enum { ty, index: 0 }),
            TypeKind::Elem(e) => InitValue::Const(match e {
                Elem::Bool => ConstValue::Bool(false),
                Elem::Real | Elem::LReal => ConstValue::Real(0.0),
                Elem::Time => ConstValue::Time(0),
                Elem::Date => ConstValue::Date(0),
                Elem::Tod => ConstValue::Tod(0),
                Elem::Dt => ConstValue::Dt(0),
                _ => ConstValue::Int(0),
            }),
            TypeKind::String { wide, .. } => InitValue::Const(ConstValue::Str { wide: *wide, chars: vec![] }),
            _ => InitValue::Struct(vec![]),
        }
    }
}

/// Resolves an enumerated value name, given an optional expected type.
pub fn enum_lookup(types: &Types, name: &str, expected: Option<TypeId>) -> Result<ConstValue, String> {
    if let Some(t) = expected {
        if let Some(values) = types.enum_values(t) {
            if let Some(i) = values.iter().position(|v| v == name) {
                return Ok(ConstValue::Enum { ty: types.resolve(t), index: i });
            }
        }
    }
    let mut found = Vec::new();
    for (id, t) in types.list.iter().enumerate() {
        if let TypeKind::Enum { values, .. } = &t.kind {
            if let Some(i) = values.iter().position(|v| v == name) {
                found.push((id, i));
            }
        }
    }
    match found.len() {
        0 => Err(format!("unknown identifier {}", name)),
        1 => Ok(ConstValue::Enum { ty: found[0].0, index: found[0].1 }),
        _ => Err(format!("ambiguous enumerated value {}; qualify it with its type name (2.3.3.1)", name)),
    }
}

pub fn var_kind_name(k: VarKind) -> &'static str {
    k.keyword()
}

pub fn qualifier_retain(q: Option<VarQualifier>) -> Option<bool> {
    match q {
        Some(VarQualifier::Retain) => Some(true),
        Some(VarQualifier::NonRetain) => Some(false),
        _ => None,
    }
}
