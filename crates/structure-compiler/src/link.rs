//! Linking (2.7): elaboration of a configuration into resources, program
//! instances, tasks and jobs, memory layout, binding of external and
//! located variables, instance-specific initialisation, access paths,
//! retentive regions and final assembly of the executable image.

use crate::codegen::{self, CallKind, RoutineCode};
use crate::ctx::*;
use crate::ir::*;
use crate::lower::Lowerer;
use crate::types::*;
use std::collections::HashMap;
use structure_runtime::error::SourceLoc;
use structure_runtime::image::*;
use structure_runtime::ty::Ty;
use structure_syntax::ast::{self, DirectVar, ExprKind, LocationPrefix, SizePrefix, VarItems, VarKind, VarQualifier};
use structure_syntax::diag::Diagnostic;
use structure_syntax::Span;

#[derive(Clone, Debug)]
struct GVar {
    name: String,
    ty: TypeId,
    /// Bit address of the variable's storage.
    addr: u64,
    constant: bool,
    retain: Option<bool>,
    located: Option<DirectVar>,
    init: Option<InitValue>,
    span: Span,
}

#[derive(Default, Debug)]
struct IoExtent {
    /// (location, module path) -> bytes required
    sizes: HashMap<(char, Vec<u32>), u64>,
}

#[derive(Debug, Clone)]
struct ProgInst {
    name: String,
    pou: usize,
    addr: u64,
    cfg: ast::ProgramConfig,
}

struct ResLayout {
    name: String,
    type_name: String,
    globals: Vec<GVar>,
    programs: Vec<ProgInst>,
    /// (location, module path) -> area
    io: HashMap<(char, Vec<u32>), Area>,
}

/// Placement of a directly represented variable: (location, module path,
/// bit offset within the area) per the mapping documented in annex D.
fn io_place(dv: &DirectVar) -> (char, Vec<u32>, u64) {
    let loc = match dv.location {
        LocationPrefix::Input => 'I',
        LocationPrefix::Output => 'Q',
        LocationPrefix::Memory => 'M',
    };
    let a = &dv.address;
    match dv.size {
        SizePrefix::None | SizePrefix::X => {
            if a.len() == 1 {
                (loc, vec![], a[0] as u64)
            } else {
                let n = a.len();
                (loc, a[..n - 2].to_vec(), a[n - 2] as u64 * 8 + a[n - 1] as u64)
            }
        }
        s => {
            let bytes: u64 = match s {
                SizePrefix::B => 1,
                SizePrefix::W => 2,
                SizePrefix::D => 4,
                _ => 8,
            };
            let n = a.len();
            (loc, a[..n - 1].to_vec(), a[n - 1] as u64 * bytes * 8)
        }
    }
}

fn prefix_bits(s: SizePrefix) -> u64 {
    match s {
        SizePrefix::None | SizePrefix::X => 1,
        SizePrefix::B => 8,
        SizePrefix::W => 16,
        SizePrefix::D => 32,
        SizePrefix::L => 64,
    }
}

pub struct Linked {
    pub image: Image,
}

struct Linker<'c, 's> {
    cx: &'c mut Ctx<'s>,
    top: u64,
    cfg_pou: usize,
    cfg: ast::Configuration,
    cfg_globals: Vec<GVar>,
    res: Vec<ResLayout>,
    symbols: Vec<Symbol>,
    access: Vec<AccessPathInfo>,
    retain: Vec<(u64, u64)>,
    resources: Vec<ResourceInfo>,
    /// (resource, routine, instance address, stack placeholder)
    jobs_for_stack: Vec<(usize, usize, usize)>,
}

fn round_up(x: u64, a: u64) -> u64 {
    x.div_ceil(a) * a
}

impl<'c, 's> Linker<'c, 's> {
    fn alloc(&mut self, bits: u64) -> u64 {
        self.top = round_up(self.top, 8);
        let addr = self.top * 8;
        self.top += round_up(bits, 8) / 8;
        addr
    }

    fn named_resources(&self) -> bool {
        self.cfg.resources.iter().any(|r| r.name.is_some())
    }

    // ------------------------------------------------------------ globals

    fn declare_globals(&mut self, blocks: &[ast::VarBlock], ri: Option<usize>) -> Vec<GVar> {
        let mut out = Vec::new();
        for b in blocks {
            if b.kind != VarKind::Global {
                continue;
            }
            let VarItems::Vars(decls) = &b.items else { continue };
            for d in decls {
                let ty = self.cx.resolve_spec(&d.spec, false);
                if let Some(p) = self.cx.types.instance_pou(ty) {
                    if self.cx.pous[p].kind == PouKind::Program {
                        self.cx.error_clause("2.5.3", "programs can only be instantiated by program configurations", d.span);
                    }
                }
                let init = d.init.as_ref().and_then(|i| self.cx.initializer(i, ty));
                if d.edge.is_some() {
                    self.cx.error("edge qualifiers are not allowed in global variable declarations", d.span);
                }
                if let Some(l) = &d.location {
                    if l.incomplete {
                        self.cx.error_clause("2.4.3.1", "incompletely specified locations are not allowed in VAR_GLOBAL", d.span);
                    }
                }
                let names: Vec<(String, Span)> = if d.names.is_empty() {
                    match &d.location {
                        Some(l) => vec![(l.to_text(), l.span)],
                        None => vec![],
                    }
                } else {
                    d.names.iter().map(|n| (n.key(), n.span)).collect()
                };
                for (name, span) in names {
                    if out.iter().any(|g: &GVar| g.name == name) {
                        self.cx.error_clause("2.7.1", format!("duplicate global variable {}", name), span);
                        continue;
                    }
                    out.push(GVar {
                        name,
                        ty,
                        addr: 0,
                        constant: b.qualifier == Some(VarQualifier::Constant),
                        retain: qualifier_retain(b.qualifier),
                        located: d.location.clone().filter(|l| !l.incomplete),
                        init: init.clone(),
                        span,
                    });
                }
            }
        }
        let _ = ri;
        out
    }

    fn check_location_type(&mut self, dv: &DirectVar, ty: TypeId, span: Span) {
        let want = prefix_bits(dv.size);
        let r = self.cx.types.resolve(ty);
        let elem = match self.cx.types.kind(r) {
            TypeKind::Array { elem, .. } => *elem,
            _ => ty,
        };
        let ok = match self.cx.types.scalar(elem) {
            Some(Ty::Bool) => want == 1,
            Some(t) => t.bits() == want,
            None => false,
        };
        if !ok {
            self.cx.error_clause(
                "2.4.1.1",
                format!("the type {} does not match the size of the location {}", self.cx.types.name(ty), dv.to_text()),
                span,
            );
        }
    }

    // ------------------------------------------------------------ I/O scan

    fn note_io(ext: &mut IoExtent, dv: &DirectVar, bits: u64) {
        if dv.incomplete || dv.address.is_empty() {
            return;
        }
        let (loc, path, off) = io_place(dv);
        let need = (off + bits.max(1)).div_ceil(8);
        let e = ext.sizes.entry((loc, path)).or_insert(0);
        *e = (*e).max(need);
    }

    fn scan_pou_io(&self, pou: usize, ext: &mut IoExtent, depth: usize) {
        if depth > 64 {
            return;
        }
        for v in &self.cx.pous[pou].vars {
            if let Some(l) = &v.location {
                let bits = if v.kind == VKind::Hidden { self.cx.types.scalar(v.ty).map(|t| t.bits()).unwrap_or(8) } else { self.cx.types.size(v.ty) };
                let bits = match self.cx.types.kind(self.cx.types.resolve(v.ty)) {
                    TypeKind::Array { elem, dims } if self.cx.types.stride(*elem) == 1 => dims.iter().map(|(l, h)| (h - l + 1) as u64).product(),
                    _ => bits,
                };
                Self::note_io(ext, l, bits);
            }
            if v.is_ref() {
                continue;
            }
            if let Some(fp) = self.cx.types.instance_pou(v.ty) {
                self.scan_pou_io(fp, ext, depth + 1);
            }
        }
    }

    fn scan_expr_io(&self, e: &ast::Expr, ext: &mut IoExtent) {
        crate::visit::walk_expr(e, &mut |x| {
            if let ExprKind::DirectVar(d) = &x.kind {
                let bits = prefix_bits(d.size).max(8);
                Self::note_io(ext, d, bits);
            }
        });
    }

    // ------------------------------------------------------------ main

    fn run(&mut self) -> Option<Image> {
        let cfg = self.cfg.clone();
        let nres = cfg.resources.len();
        // Phase A: globals and I/O extents
        self.cfg_globals = self.declare_globals(&cfg.var_blocks, None);
        let mut extents: Vec<IoExtent> = (0..nres).map(|_| IoExtent::default()).collect();
        for g in self.cfg_globals.clone() {
            if let Some(l) = &g.located {
                let bits = self.cx.types.size(g.ty);
                if nres > 0 {
                    Self::note_io(&mut extents[0], l, bits);
                }
                self.check_location_type(l, g.ty, g.span);
            }
        }
        for (ri, r) in cfg.resources.iter().enumerate() {
            let globals = self.declare_globals(&r.var_blocks, Some(ri));
            for g in &globals {
                if let Some(l) = &g.located {
                    let bits = self.cx.types.size(g.ty);
                    Self::note_io(&mut extents[ri], l, bits);
                    self.check_location_type(l, g.ty, g.span);
                }
            }
            for b in &r.var_blocks {
                if b.kind == VarKind::External {
                    // VAR_EXTERNAL in a resource refers to configuration globals (table 49, 13a/13b)
                    if let VarItems::Vars(decls) = &b.items {
                        for d in decls {
                            for n in &d.names {
                                match self.cfg_globals.iter().find(|g| g.name == n.key()).cloned() {
                                    Some(g) => {
                                        if g.constant && b.qualifier != Some(VarQualifier::Constant) {
                                            self.cx.annex_e("2.4.3", format!("{} is declared VAR_GLOBAL CONSTANT and shall be declared VAR_EXTERNAL CONSTANT", n), n.span);
                                        }
                                    }
                                    None => self.cx.error_clause("2.7.1", format!("no global variable {} is declared in the configuration", n), n.span),
                                }
                            }
                        }
                    }
                }
            }
            let mut programs = Vec::new();
            for pc in &r.programs {
                let key = pc.type_name.key();
                match self.cx.pou_by_name.get(&key).copied() {
                    Some(p) if self.cx.pous[p].kind == PouKind::Program => {
                        if programs.iter().any(|x: &ProgInst| x.name == pc.name.key()) {
                            self.cx.error_clause("2.7.1", format!("duplicate program instance name {}", pc.name), pc.name.span);
                            continue;
                        }
                        self.scan_pou_io(p, &mut extents[ri], 0);
                        for el in &pc.elements {
                            match el {
                                ast::ProgConfElement::Input { source, .. } => self.scan_expr_io(source, &mut extents[ri]),
                                ast::ProgConfElement::Output { sink, .. } => self.scan_expr_io(sink, &mut extents[ri]),
                                _ => {}
                            }
                        }
                        programs.push(ProgInst { name: pc.name.key(), pou: p, addr: 0, cfg: pc.clone() });
                    }
                    Some(_) => self.cx.error_clause("2.7.1", format!("{} is not a program type", pc.type_name), pc.type_name.span),
                    None => self.cx.error(format!("unknown program type {}", pc.type_name), pc.type_name.span),
                }
            }
            for t in &r.tasks {
                for e in [&t.single, &t.interval].into_iter().flatten() {
                    self.scan_expr_io(e, &mut extents[ri]);
                }
            }
            self.res.push(ResLayout {
                name: r.name.as_ref().map(|n| n.key()).unwrap_or_default(),
                type_name: r.on.as_ref().map(|n| n.key()).unwrap_or_default(),
                globals,
                programs,
                io: HashMap::new(),
            });
        }
        // VAR_CONFIG and VAR_ACCESS direct variables
        for b in &cfg.var_blocks {
            match &b.items {
                VarItems::Config(inits) => {
                    for ci in inits {
                        if let Some(l) = &ci.location {
                            let ri = self.resource_of_path(&ci.path);
                            let ty = self.cx.resolve_spec(&ci.spec, false);
                            let bits = self.cx.types.size(ty);
                            Self::note_io(&mut extents[ri], l, bits);
                        }
                    }
                }
                VarItems::Access(decls) => {
                    for a in decls {
                        if let ast::AccessTarget::Direct(d) = &a.path.target {
                            let ri = self.resource_of_path(&a.path.prefix);
                            Self::note_io(&mut extents[ri], d, prefix_bits(d.size).max(8));
                        }
                    }
                }
                _ => {}
            }
        }

        // Phase B: memory layout
        self.top = round_up(POOL_BASE + self.cx.pool.len() as u64, 8);
        for i in 0..self.cfg_globals.len() {
            if self.cfg_globals[i].located.is_none() {
                let bits = self.cx.types.size(self.cfg_globals[i].ty);
                self.cfg_globals[i].addr = self.alloc(bits);
            }
        }
        #[allow(clippy::needless_range_loop)] // the resources are modified in the loop
        for ri in 0..self.res.len() {
            for gi in 0..self.res[ri].globals.len() {
                if self.res[ri].globals[gi].located.is_none() {
                    let bits = self.cx.types.size(self.res[ri].globals[gi].ty);
                    self.res[ri].globals[gi].addr = self.alloc(bits);
                }
            }
            for pi in 0..self.res[ri].programs.len() {
                let p = self.res[ri].programs[pi].pou;
                let bits = self.cx.pous[p].inst_size;
                self.res[ri].programs[pi].addr = self.alloc(bits);
            }
            // I/O areas: main areas first
            let mut keys: Vec<(char, Vec<u32>)> = extents[ri].sizes.keys().cloned().collect();
            for c in ['I', 'Q', 'M'] {
                if !keys.contains(&(c, vec![])) {
                    keys.push((c, vec![]));
                }
            }
            keys.sort();
            for k in keys {
                let need = extents[ri].sizes.get(&k).copied().unwrap_or(0);
                let size = round_up(need.max(if k.1.is_empty() { 64 } else { 8 }), 8);
                let base = self.alloc(size * 8) / 8;
                self.res[ri].io.insert(k, Area { base, size });
            }
        }
        // located globals
        for i in 0..self.cfg_globals.len() {
            if let Some(l) = self.cfg_globals[i].located.clone() {
                if let Some(a) = self.io_addr(0, &l) {
                    self.cfg_globals[i].addr = a;
                }
            }
        }
        for ri in 0..self.res.len() {
            for gi in 0..self.res[ri].globals.len() {
                if let Some(l) = self.res[ri].globals[gi].located.clone() {
                    if let Some(a) = self.io_addr(ri, &l) {
                        self.res[ri].globals[gi].addr = a;
                    }
                }
            }
        }

        // Phase C: code
        let config_init = self.config_init();
        for ri in 0..self.res.len() {
            let info = self.resource(ri);
            self.resources.push(info);
        }
        self.config_access();
        // Phase D: symbols
        for g in self.cfg_globals.clone() {
            let ty = self.desc(g.ty);
            self.symbols.push(Symbol { path: g.name.clone(), addr: g.addr, ty, retain: g.retain == Some(true), constant: g.constant });
            if g.retain == Some(true) {
                let bits = self.cx.types.size(g.ty);
                self.retain.push((g.addr, bits));
            }
        }
        if self.cx.diags.has_errors() {
            return None;
        }
        // Phase E: assembly
        Some(self.assemble(config_init))
    }

    fn resource_of_path(&self, path: &[structure_syntax::Ident]) -> usize {
        if let Some(first) = path.first() {
            if let Some(i) = self.cfg.resources.iter().position(|r| r.name.as_ref().map(|n| n.key()) == Some(first.key())) {
                return i;
            }
        }
        0
    }

    fn io_addr(&mut self, ri: usize, dv: &DirectVar) -> Option<u64> {
        if dv.incomplete {
            return None;
        }
        let (loc, path, off) = io_place(dv);
        let area = self.res.get(ri)?.io.get(&(loc, path))?;
        Some(area.base * 8 + off)
    }

    fn desc(&self, ty: TypeId) -> TypeDesc {
        let cx = &*self.cx;
        let f = |pou: usize| -> (String, Vec<FieldDesc>) { pou_fields(cx, pou) };
        cx.types.desc(ty, &f)
    }

    fn lowerer(&mut self) -> Lowerer<'_, 's> {
        let mut l = Lowerer::new(self.cx, self.cfg_pou);
        l.frame_top = 0;
        l
    }

    fn config_init(&mut self) -> Option<usize> {
        let r = self.cx.new_routine(format!("{}$init", self.cfg.name.key()));
        let globals = self.cfg_globals.clone();
        let mut out = Vec::new();
        let mut l = self.lowerer();
        for g in &globals {
            l.emit_init(&mut out, Loc::Abs(g.addr), g.ty, g.init.as_ref());
        }
        let frame = l.frame_top;
        let mut binds = Vec::new();
        for g in &globals {
            if let Some(p) = self.cx.types.instance_pou(g.ty) {
                let scopes = vec![self.cfg_scope()];
                self.bind_instance(p, g.addr, &g.name, &scopes, 0, &mut binds, None, &mut Vec::new());
            }
        }
        binds.extend(out);
        binds.push(St::Return);
        self.cx.routines[r].body = binds;
        self.cx.routines[r].frame_bytes = frame;
        Some(r)
    }

    fn cfg_scope(&self) -> Vec<(String, u64, TypeId, bool)> {
        self.cfg_globals.iter().map(|g| (g.name.clone(), g.addr, g.ty, g.constant)).collect()
    }

    fn res_scope(&self, ri: usize) -> Vec<(String, u64, TypeId, bool)> {
        self.res[ri].globals.iter().map(|g| (g.name.clone(), g.addr, g.ty, g.constant)).collect()
    }

    /// Binds external, located and incompletely located references of an
    /// instance tree. `scopes` lists global variable scopes from the
    /// innermost outwards.
    #[allow(clippy::too_many_arguments)]
    fn bind_instance(
        &mut self,
        pou: usize,
        addr: u64,
        path: &str,
        scopes: &[Vec<(String, u64, TypeId, bool)>],
        ri_depth: usize,
        out: &mut Vec<St>,
        ri: Option<usize>,
        tasked: &mut Vec<(String, u64)>,
    ) {
        if ri_depth > 64 {
            return;
        }
        let vars = self.cx.pous[pou].vars.clone();
        let is_program = self.cx.pous[pou].kind == PouKind::Program;
        // A program's VAR_GLOBAL declarations form the innermost scope of
        // its function block instances.
        let mut inner: Vec<Vec<(String, u64, TypeId, bool)>> = Vec::new();
        if is_program {
            let mut sc = Vec::new();
            for v in &vars {
                if v.kind == VKind::Global {
                    let a = match v.storage {
                        Storage::Inst(o) => addr + o,
                        _ => v.location.as_ref().and_then(|l| ri.and_then(|r| self.io_addr(r, l))).unwrap_or(0),
                    };
                    sc.push((v.name.clone(), a, v.ty, v.constant));
                }
            }
            inner.push(sc);
        }
        inner.extend(scopes.iter().cloned());
        for v in &vars {
            let slot = addr + v.offset();
            match v.kind {
                VKind::External => {
                    let found = scopes.iter().flat_map(|s| s.iter()).find(|g| g.0 == v.name).cloned();
                    match found {
                        Some((_, gaddr, gty, gconst)) => {
                            if !self.cx.types.compatible(gty, v.ty) {
                                self.cx.error_clause(
                                    "2.4.3",
                                    format!("the type of the external variable {} ({}) does not agree with the global declaration ({})", v.name, self.cx.types.name(v.ty), self.cx.types.name(gty)),
                                    v.span,
                                );
                            }
                            if gconst && !v.constant {
                                self.cx.annex_e("2.4.3", format!("{} is declared VAR_GLOBAL CONSTANT but VAR_EXTERNAL without CONSTANT (table 16b)", v.name), v.span);
                            }
                            out.push(St::Store(Ty::U64, Loc::Abs(slot), Ex::Const(Ty::U64, gaddr)));
                        }
                        None => {
                            self.cx.error_clause("2.4.3", format!("no global variable {} is visible to the instance {}", v.name, path), v.span);
                        }
                    }
                }
                _ if v.location.is_some() && v.is_ref() => {
                    let l = v.location.clone().unwrap();
                    if l.incomplete {
                        continue; // bound by VAR_CONFIG
                    }
                    if v.kind != VKind::Hidden {
                        self.check_location_type(&l, v.ty, v.span);
                    }
                    if let Some(a) = ri.and_then(|r| self.io_addr(r, &l)) {
                        out.push(St::Store(Ty::U64, Loc::Abs(slot), Ex::Const(Ty::U64, a)));
                    }
                }
                _ => {}
            }
            if v.is_ref() || v.kind == VKind::Temp {
                continue;
            }
            if let Some(fp) = self.cx.types.instance_pou(v.ty) {
                let p = format!("{}.{}", path, v.name);
                self.bind_instance(fp, slot, &p, &inner, ri_depth + 1, out, ri, tasked);
            }
            if let TypeKind::Array { elem, dims } = self.cx.types.kind(self.cx.types.resolve(v.ty)).clone() {
                if let Some(fp) = self.cx.types.instance_pou(elem) {
                    let count: i64 = dims.iter().map(|(l, h)| h - l + 1).product();
                    let stride = self.cx.types.stride(elem);
                    for i in 0..count {
                        let p = format!("{}.{}[{}]", path, v.name, i);
                        self.bind_instance(fp, slot + i as u64 * stride, &p, &inner, ri_depth + 1, out, ri, tasked);
                    }
                }
            }
        }
        // ACTION_CONTROL instances of SFC actions have no references.
        let _ = tasked;
    }

    /// Collects incompletely located variables of an instance tree.
    fn incomplete_locations(&self, pou: usize, addr: u64, path: &str, out: &mut Vec<(String, u64, TypeId, DirectVar, Span)>, depth: usize) {
        if depth > 64 {
            return;
        }
        for v in &self.cx.pous[pou].vars {
            if let Some(l) = &v.location {
                if l.incomplete {
                    out.push((format!("{}.{}", path, v.name), addr + v.offset(), v.ty, l.clone(), v.span));
                }
            }
            if v.is_ref() || v.kind == VKind::Temp {
                continue;
            }
            if let Some(fp) = self.cx.types.instance_pou(v.ty) {
                self.incomplete_locations(fp, addr + v.offset(), &format!("{}.{}", path, v.name), out, depth + 1);
            }
        }
    }

    fn resource(&mut self, ri: usize) -> ResourceInfo {
        let rname = self.res[ri].name.clone();
        let prefix = if rname.is_empty() { String::new() } else { format!("{}.", rname) };
        let init_r = self.cx.new_routine(format!("{}$init", if rname.is_empty() { "RESOURCE" } else { &rname }));
        let mut binds = Vec::new();
        let scopes = vec![self.res_scope(ri), self.cfg_scope()];
        let programs = self.res[ri].programs.clone();
        let mut tasked = Vec::new();
        for p in &programs {
            self.bind_instance(p.pou, p.addr, &format!("{}{}", prefix, p.name), &scopes, 0, &mut binds, Some(ri), &mut tasked);
        }
        // resource-level FB instance globals
        for g in self.res[ri].globals.clone() {
            if let Some(fp) = self.cx.types.instance_pou(g.ty) {
                let sc = vec![self.res_scope(ri), self.cfg_scope()];
                self.bind_instance(fp, g.addr, &format!("{}{}", prefix, g.name), &sc, 0, &mut binds, Some(ri), &mut Vec::new());
            }
        }
        // VAR_CONFIG: instance specific locations and initial values
        let mut config_inits: Vec<(Loc, TypeId, Option<InitValue>)> = Vec::new();
        let mut bound_incomplete: Vec<u64> = Vec::new();
        let cfg = self.cfg.clone();
        for b in &cfg.var_blocks {
            let VarItems::Config(inits) = &b.items else { continue };
            for ci in inits {
                if self.resource_of_path(&ci.path) != ri {
                    continue;
                }
                let Some((slot, var)) = self.config_target(ri, &ci.path, ci.span) else { continue };
                let ty = self.cx.resolve_spec(&ci.spec, false);
                if !self.cx.types.compatible(ty, var.ty) {
                    self.cx.error_clause("2.7.1", format!("the type {} does not agree with the declaration of {}", self.cx.types.name(ty), var.name), ci.span);
                }
                if matches!(var.kind, VKind::Temp | VKind::External | VKind::InOut) || var.constant {
                    self.cx.error_clause("2.7.1", "instance-specific initialisation is not allowed for VAR_TEMP, VAR_EXTERNAL, VAR CONSTANT or VAR_IN_OUT variables", ci.span);
                    continue;
                }
                if let Some(l) = &ci.location {
                    match &var.location {
                        Some(vl) if vl.incomplete => {
                            if vl.location != l.location {
                                self.cx.error_clause("2.7.1", format!("the location {} does not match the declared location prefix", l.to_text()), l.span);
                            }
                            self.check_location_type(l, var.ty, ci.span);
                            if let Some(a) = self.io_addr(ri, l) {
                                binds.push(St::Store(Ty::U64, Loc::Abs(slot), Ex::Const(Ty::U64, a)));
                                bound_incomplete.push(slot);
                            }
                        }
                        _ => self.cx.error_clause("2.7.1", "location assignments in VAR_CONFIG are only allowed for variables declared with the asterisk notation", l.span),
                    }
                }
                let init = ci.init.as_ref().and_then(|i| self.cx.initializer(i, var.ty));
                if init.is_some() {
                    let loc = if var.is_ref() { Loc::Abs(slot).deref() } else { Loc::Abs(slot) };
                    config_inits.push((loc, var.ty, init));
                }
            }
        }
        // missing VAR_CONFIG for "*" locations (2.4.1.1)
        for p in &programs {
            let mut inc = Vec::new();
            self.incomplete_locations(p.pou, p.addr, &format!("{}{}", prefix, p.name), &mut inc, 0);
            for (path, slot, _, _, span) in inc {
                if !bound_incomplete.contains(&slot) {
                    self.cx.annex_e("2.4.1.1", format!("missing VAR_CONFIG location for the incompletely specified variable {}", path), span);
                }
            }
        }
        // FB task associations: flag the instances and create jobs
        let mut jobs: Vec<(JobInfo, Vec<St>, u64)> = Vec::new();
        let tasks: Vec<ast::TaskDecl> = cfg.resources[ri].tasks.clone();
        let task_index = |name: &structure_syntax::Ident| tasks.iter().position(|t| t.name.key() == name.key());
        let mut l_out = Vec::new();
        {
            let globals = self.res[ri].globals.clone();
            let mut l = self.lowerer();
            for g in &globals {
                l.emit_init(&mut l_out, Loc::Abs(g.addr), g.ty, g.init.as_ref());
            }
            for p in &programs {
                if crate::lower::init::instance_needs_init(l.cx, p.pou) {
                    let r = l.cx.pous[p.pou].init_routine.unwrap();
                    l_out.push(St::CallFb { routine: r, inst: Ex::Const(Ty::U64, p.addr) });
                }
            }
            for (loc, ty, init) in &config_inits {
                l.emit_init(&mut l_out, loc.clone(), *ty, init.as_ref());
            }
            let frame = l.frame_top;
            self.cx.routines[init_r].frame_bytes = frame;
        }
        for p in &programs {
            let pou = self.cx.pous[p.pou].clone();
            let ptask = match &p.cfg.task {
                Some(t) => match task_index(t) {
                    Some(i) => Some(i),
                    None => {
                        self.cx.error_clause("2.7.2", format!("unknown task {}", t), t.span);
                        None
                    }
                },
                None => None,
            };
            // program job: input connections, call, output connections
            let jr = self.cx.new_routine(format!("{}{}$job", prefix, p.name));
            let mut body = Vec::new();
            let mut l = Lowerer::new(self.cx, self.cfg_pou);
            l.frame_top = 0;
            let mut outs = Vec::new();
            for el in &p.cfg.elements {
                match el {
                    ast::ProgConfElement::Input { var, source, span } => {
                        let Some(v) = pou.var(&var.key()).cloned() else {
                            l.cx.error(format!("{} has no input named {}", pou.name, var), var.span);
                            continue;
                        };
                        if v.kind != VKind::Input {
                            l.cx.error_clause("2.7.1", format!("{} is not an input of {}", var, pou.name), var.span);
                            continue;
                        }
                        let dst = Loc::Abs(p.addr + v.offset());
                        let val = cfg_value(&mut l, source, v.ty, ri, &self.res, &self.cfg_globals, *span);
                        if let Some(val) = val {
                            l.assign(&mut body, dst, v.ty, val, *span);
                        }
                    }
                    ast::ProgConfElement::Output { var, sink, span } => {
                        let Some(v) = pou.var(&var.key()).cloned() else {
                            l.cx.error(format!("{} has no output named {}", pou.name, var), var.span);
                            continue;
                        };
                        if v.kind != VKind::Output {
                            l.cx.error_clause("2.7.1", format!("{} is not an output of {}", var, pou.name), var.span);
                            continue;
                        }
                        match cfg_place(&mut l, sink, ri, &self.res, &self.cfg_globals) {
                            Some((dst, ty, constant)) => {
                                if constant {
                                    l.cx.annex_e("2.4.3", "a program output cannot be connected to a CONSTANT variable", sink.span);
                                }
                                let src = V::mem(Loc::Abs(p.addr + v.offset()), v.ty);
                                l.assign(&mut outs, dst, ty, src, *span);
                            }
                            None => l.cx.error_clause("2.7.1", "a program output can only be connected to a global or directly represented variable", sink.span),
                        }
                    }
                    ast::ProgConfElement::FbTask { fb, task, span } => {
                        let ExprKind::Name(fbn) = &fb.kind else {
                            l.cx.error("expected a function block instance name", fb.span);
                            continue;
                        };
                        let Some(v) = pou.var(&fbn.key()).cloned() else {
                            l.cx.error(format!("{} has no function block instance named {}", pou.name, fbn), fbn.span);
                            continue;
                        };
                        let Some(fbp) = l.cx.types.instance_pou(v.ty).filter(|_| v.kind == VKind::Local && !v.is_ref()) else {
                            l.cx.error_clause("2.7.2", format!("{} is not a function block instance declared in {}", fbn, pou.name), fbn.span);
                            continue;
                        };
                        let Some(ti) = task_index(task) else {
                            l.cx.error_clause("2.7.2", format!("unknown task {}", task), task.span);
                            continue;
                        };
                        let inst = p.addr + v.offset();
                        // mark the instance as task-controlled (offset 0 flags byte)
                        binds.push(St::Store(Ty::Bool, Loc::Abs(inst), Ex::bool(true)));
                        let size = l.cx.pous[fbp].inst_size;
                        let ostart = l.cx.pous[fbp].outputs_start;
                        let shadow_bits = size;
                        jobs.push((
                            JobInfo {
                                name: format!("{}{}.{}", prefix, p.name, v.name),
                                routine: 0,
                                instance: inst,
                                task: Some(ti),
                                stack: Area::default(),
                            },
                            vec![],
                            shadow_bits,
                        ));
                        let _ = (span, ostart);
                    }
                }
            }
            body.push(St::CallFb { routine: pou.routine.unwrap(), inst: Ex::Const(Ty::U64, p.addr) });
            body.extend(outs);
            body.push(St::Return);
            let frame = l.frame_top;
            self.cx.routines[jr].body = body;
            self.cx.routines[jr].frame_bytes = frame;
            jobs.insert(
                jobs.iter().position(|(j, _, _)| j.routine == 0 && j.instance != p.addr && j.name.starts_with(&format!("{}{}.", prefix, p.name))).unwrap_or(jobs.len()),
                (JobInfo { name: format!("{}{}", prefix, p.name), routine: jr as u32, instance: p.addr, task: ptask, stack: Area::default() }, vec![], 0),
            );
            // symbols for the program instance
            let ty = self.cx.pous[p.pou].inst_type.unwrap();
            let desc = self.desc(ty);
            self.symbols.push(Symbol { path: format!("{}{}", prefix, p.name), addr: p.addr, ty: desc, retain: false, constant: false });
            // retentive regions
            let inherit = match p.cfg.retain {
                Some(VarQualifier::Retain) => Some(true),
                Some(VarQualifier::NonRetain) => Some(false),
                _ => None,
            };
            self.retain_instance(p.pou, p.addr, inherit, ri, 0);
            // program-level access paths
            self.program_access(p, ri, &prefix);
        }
        // create FB job routines (shadow copies, 2.7.2 rule 7)
        for (job, _, shadow_bits) in jobs.iter_mut() {
            if job.routine != 0 || *shadow_bits == 0 {
                continue;
            }
            let shadow = self.alloc(*shadow_bits);
            let inst = job.instance;
            // find FB type from the instance: search programs
            let mut fbp = None;
            for p in &programs {
                for v in &self.cx.pous[p.pou].vars {
                    if p.addr + v.offset() == inst {
                        fbp = self.cx.types.instance_pou(v.ty);
                    }
                }
            }
            let Some(fbp) = fbp else { continue };
            let size = self.cx.pous[fbp].inst_size;
            let ostart = self.cx.pous[fbp].outputs_start;
            let r = self.cx.new_routine(format!("{}$job", job.name));
            let body = vec![
                St::Copy { dst: Loc::Abs(shadow), src: Loc::Abs(inst), bits: size },
                St::CallFb { routine: self.cx.pous[fbp].routine.unwrap(), inst: Ex::Const(Ty::U64, shadow) },
                St::Copy { dst: Loc::Abs(inst + ostart), src: Loc::Abs(shadow + ostart), bits: size - ostart },
                St::Return,
            ];
            self.cx.routines[r].body = body;
            job.routine = r as u32;
        }
        let mut init_body = binds;
        init_body.extend(l_out);
        init_body.push(St::Return);
        self.cx.routines[init_r].body = init_body;
        // resource globals symbols & retain
        for g in self.res[ri].globals.clone() {
            let desc = self.desc(g.ty);
            self.symbols.push(Symbol { path: format!("{}{}", prefix, g.name), addr: g.addr, ty: desc, retain: g.retain == Some(true), constant: g.constant });
            if g.retain == Some(true) {
                let bits = self.cx.types.size(g.ty);
                self.retain.push((g.addr, bits));
            }
        }
        // tasks
        let mut task_infos = Vec::new();
        for t in &tasks {
            let priority = match &t.priority {
                Some(e) => match self.cx.const_eval(e, None) {
                    Some(c) => match c.value {
                        ConstValue::Int(i) if i >= 0 && i <= u32::MAX as i128 => i as u32,
                        _ => {
                            self.cx.error_clause("2.7.2", "PRIORITY shall be a non-negative integer", e.span);
                            0
                        }
                    },
                    None => 0,
                },
                None => 0,
            };
            let single = t.single.as_ref().and_then(|e| self.task_source(e, ri, true));
            let interval = t.interval.as_ref().and_then(|e| self.task_source(e, ri, false));
            task_infos.push(TaskInfo { name: t.name.key(), priority, single, interval });
        }
        let io = &self.res[ri].io;
        let input = io.get(&('I', vec![])).copied().unwrap_or_default();
        let output = io.get(&('Q', vec![])).copied().unwrap_or_default();
        let memory = io.get(&('M', vec![])).copied().unwrap_or_default();
        let io_modules = io
            .iter()
            .filter(|(k, _)| !k.1.is_empty())
            .map(|(k, a)| IoModule { location: k.0, path: k.1.clone(), area: *a })
            .collect();
        for (j, _, _) in &jobs {
            self.jobs_for_stack.push((ri, j.routine as usize, 0));
        }
        ResourceInfo {
            name: rname,
            type_name: self.res[ri].type_name.clone(),
            input,
            output,
            memory,
            init_routine: init_r as u32,
            tasks: task_infos,
            jobs: jobs.into_iter().map(|(j, _, _)| j).collect(),
            init_stack: Area::default(),
            io_modules,
        }
    }

    /// Resolves the target of a VAR_CONFIG entry: returns the slot address
    /// and variable info.
    fn config_target(&mut self, ri: usize, path: &[structure_syntax::Ident], span: Span) -> Option<(u64, VarInfo)> {
        let mut segs: Vec<&structure_syntax::Ident> = path.iter().collect();
        if self.named_resources() {
            if segs.is_empty() || self.cfg.resources[ri].name.as_ref().map(|n| n.key()) != Some(segs[0].key()) {
                self.cx.error_clause("2.7.1", "instance-specific initialisations shall begin with the resource name", span);
                return None;
            }
            segs.remove(0);
        }
        let first = segs.first()?;
        let Some(prog) = self.res[ri].programs.iter().find(|p| p.name == first.key()).cloned() else {
            self.cx.error_clause("2.7.1", format!("unknown program instance {}", first), first.span);
            return None;
        };
        let mut pou = prog.pou;
        let mut addr = prog.addr;
        for (i, s) in segs[1..].iter().enumerate() {
            let Some(v) = self.cx.pous[pou].var(&s.key()).cloned() else {
                self.cx.error_clause("2.7.1", format!("{} has no variable named {}", self.cx.pous[pou].name, s), s.span);
                return None;
            };
            if i + 2 == segs.len() {
                return Some((addr + v.offset(), v));
            }
            match self.cx.types.instance_pou(v.ty) {
                Some(fp) if !v.is_ref() => {
                    pou = fp;
                    addr += v.offset();
                }
                _ => {
                    self.cx.error_clause("2.7.1", format!("{} is not a function block instance", s), s.span);
                    return None;
                }
            }
        }
        self.cx.error_clause("2.7.1", "the path of an instance-specific initialisation shall end with a variable name", span);
        None
    }

    fn task_source(&mut self, e: &ast::Expr, ri: usize, single: bool) -> Option<Source> {
        let want = if single { self.cx.types.elem(Elem::Bool) } else { self.cx.types.elem(Elem::Time) };
        if let ExprKind::Literal(_) | ExprKind::Unary { .. } = &e.kind {
            let c = self.cx.const_eval(e, Some(want))?;
            return Some(Source::Const(match c.value {
                ConstValue::Time(t) => t,
                ConstValue::Bool(b) => b as i64,
                ConstValue::Int(i) if single && (i == 0 || i == 1) => i as i64,
                _ => {
                    self.cx.error_clause("2.7.2", if single { "SINGLE shall be of type BOOL" } else { "INTERVAL shall be of type TIME" }, e.span);
                    return None;
                }
            }));
        }
        let mut l = Lowerer::new(self.cx, self.cfg_pou);
        let (loc, ty, _) = cfg_place(&mut l, e, ri, &self.res, &self.cfg_globals)?;
        if !self.cx.types.compatible(ty, want) {
            self.cx.error_clause("2.7.2", if single { "SINGLE shall be of type BOOL" } else { "INTERVAL shall be of type TIME" }, e.span);
            return None;
        }
        match loc {
            Loc::Abs(a) => Some(Source::Var { addr: a, ty: self.cx.types.scalar(ty).unwrap() }),
            _ => None,
        }
    }

    // ------------------------------------------------------------ retain

    fn retain_instance(&mut self, pou: usize, addr: u64, inherit: Option<bool>, ri: usize, depth: usize) {
        if depth > 64 {
            return;
        }
        let vars = self.cx.pous[pou].vars.clone();
        // SFC state of retentive instances is retentive (2.6.2).
        if inherit == Some(true) {
            if let Some(sfc) = &self.cx.pous[pou].sfc {
                for s in &sfc.steps {
                    self.retain.push((addr + s.x, 8 + 64 + 64));
                }
            }
        }
        for v in &vars {
            if matches!(v.kind, VKind::Temp | VKind::External | VKind::InOut) {
                continue;
            }
            let is_fb = self.cx.types.instance_pou(v.ty).is_some();
            let eff = match v.retain {
                Some(x) => Some(x),
                // members that are function blocks do not inherit (table 16a note c)
                None if is_fb => None,
                None => inherit,
            };
            if v.is_ref() {
                if eff == Some(true) {
                    if let Some(l) = &v.location {
                        if let Some(a) = self.io_addr(ri, l) {
                            let bits = self.cx.types.size(v.ty);
                            self.retain.push((a, bits));
                        }
                    }
                }
                continue;
            }
            if let Some(fp) = self.cx.types.instance_pou(v.ty) {
                self.retain_instance(fp, addr + v.offset(), eff, ri, depth + 1);
                continue;
            }
            if eff == Some(true) {
                let bits = self.cx.types.size(v.ty);
                self.retain.push((addr + v.offset(), bits));
            }
        }
    }

    // ------------------------------------------------------------ access paths

    #[allow(clippy::too_many_arguments)]
    fn add_access(&mut self, name: String, addr: u64, ty: TypeId, read_only: bool, declared: TypeId, span: Span, constant: bool) {
        if !self.cx.types.compatible(ty, declared) || (self.cx.types.is_string(ty).is_some() != self.cx.types.is_string(declared).is_some()) {
            self.cx.annex_e(
                "2.7.1",
                format!("data type conflict in VAR_ACCESS: {} is declared {} but the variable is {}", name, self.cx.types.name(declared), self.cx.types.name(ty)),
                span,
            );
        }
        if constant && !read_only {
            self.cx.error_clause("2.7.1", "access to variables declared CONSTANT shall be READ_ONLY", span);
        }
        if self.access.iter().any(|a| a.name == name) {
            self.cx.error_clause("2.7.1", format!("duplicate access path name {}", name), span);
            return;
        }
        let desc = self.desc(ty);
        self.access.push(AccessPathInfo { name, addr, ty: desc, read_only });
    }

    fn config_access(&mut self) {
        let cfg = self.cfg.clone();
        for b in &cfg.var_blocks {
            let VarItems::Access(decls) = &b.items else { continue };
            for a in decls {
                let declared = self.cx.resolve_spec(&a.spec, false);
                let ro = a.direction != Some(ast::Direction::ReadWrite);
                match &a.path.target {
                    ast::AccessTarget::Direct(d) => {
                        let ri = self.resource_of_path(&a.path.prefix);
                        if a.path.prefix.len() > 1 {
                            self.cx.error_clause("2.7.1", "invalid access path", a.path.span);
                            continue;
                        }
                        if let Some(addr) = self.io_addr(ri, d) {
                            // the declared type determines the representation
                            self.check_location_type(d, declared, a.span);
                            self.add_access(a.name.key(), addr, declared, ro, declared, a.span, false);
                        }
                    }
                    ast::AccessTarget::Symbolic(e) => {
                        if let Some((addr, ty, kind, constant)) = self.resolve_access(e) {
                            if matches!(kind, Some(VKind::Temp | VKind::External | VKind::InOut)) {
                                self.cx.error_clause("2.7.1", "access paths cannot be declared to VAR_TEMP, VAR_EXTERNAL or VAR_IN_OUT variables", a.span);
                                continue;
                            }
                            self.add_access(a.name.key(), addr, ty, ro, declared, a.span, constant);
                        }
                    }
                }
            }
        }
    }

    fn program_access(&mut self, p: &ProgInst, _ri: usize, prefix: &str) {
        let ast = self.cx.pous[p.pou].ast.clone();
        for b in &ast.var_blocks {
            let VarItems::Access(decls) = &b.items else { continue };
            for a in decls {
                let declared = self.cx.resolve_spec(&a.spec, false);
                let ro = a.direction != Some(ast::Direction::ReadWrite);
                let ast::AccessTarget::Symbolic(e) = &a.path.target else {
                    self.cx.error_clause("2.5.3", "access paths in programs refer to variables of the program", a.span);
                    continue;
                };
                let mut segs = Vec::new();
                if !flatten(e, &mut segs) {
                    self.cx.error("invalid access path", e.span);
                    continue;
                }
                if let Some((addr, ty, kind, constant)) = self.walk_instance(p.pou, p.addr, &segs, e.span) {
                    if matches!(kind, Some(VKind::Temp | VKind::External | VKind::InOut)) {
                        self.cx.error_clause("2.7.1", "access paths cannot be declared to VAR_TEMP, VAR_EXTERNAL or VAR_IN_OUT variables", a.span);
                        continue;
                    }
                    let name = if self.access.iter().any(|x| x.name == a.name.key()) { format!("{}{}.{}", prefix, p.name, a.name.key()) } else { a.name.key() };
                    self.add_access(name, addr, ty, ro, declared, a.span, constant);
                }
            }
        }
    }

    /// Resolves a configuration-level symbolic access path.
    fn resolve_access(&mut self, e: &ast::Expr) -> Option<(u64, TypeId, Option<VKind>, bool)> {
        let mut segs = Vec::new();
        if !flatten(e, &mut segs) {
            self.cx.error("invalid access path", e.span);
            return None;
        }
        let Seg::Name(first, _) = segs[0].clone() else { return None };
        // resource?
        let (ri, rest) = match self.cfg.resources.iter().position(|r| r.name.as_ref().map(|n| n.key()) == Some(first.clone())) {
            Some(ri) => (Some(ri), &segs[1..]),
            None => (None, &segs[..]),
        };
        let Some(Seg::Name(n, _)) = rest.first().cloned() else {
            self.cx.error("invalid access path", e.span);
            return None;
        };
        if let Some(ri) = ri.or(if self.named_resources() { None } else { Some(0) }) {
            if let Some(p) = self.res[ri].programs.iter().find(|p| p.name == n).cloned() {
                return self.walk_instance(p.pou, p.addr, &rest[1..], e.span);
            }
            if let Some(g) = self.res[ri].globals.iter().find(|g| g.name == n).cloned() {
                return self.walk_value(g.addr, g.ty, &rest[1..], Some(VKind::Global), g.constant, e.span);
            }
        }
        if ri.is_none() {
            if let Some(g) = self.cfg_globals.iter().find(|g| g.name == n).cloned() {
                return self.walk_value(g.addr, g.ty, &rest[1..], Some(VKind::Global), g.constant, e.span);
            }
        }
        self.cx.error_clause("2.7.1", format!("unknown access path {}", n), e.span);
        None
    }

    fn walk_instance(&mut self, pou: usize, addr: u64, segs: &[Seg], span: Span) -> Option<(u64, TypeId, Option<VKind>, bool)> {
        let Some(Seg::Name(n, nspan)) = segs.first() else {
            return Some((addr, self.cx.pous[pou].inst_type.unwrap(), None, false));
        };
        let nspan = *nspan;
        let v = match self.cx.pous[pou].var(n).cloned() {
            Some(v) if v.kind != VKind::Hidden => v,
            _ => {
                self.cx.error_clause("2.7.1", format!("{} has no variable named {}", self.cx.pous[pou].name, n), nspan);
                return None;
            }
        };
        let a = match v.storage {
            Storage::Inst(o) => addr + o,
            _ => {
                if let Some(l) = &v.location {
                    let ri = 0;
                    match self.io_addr(ri, l) {
                        Some(a) => a,
                        None => return Some((0, v.ty, Some(v.kind), v.constant)),
                    }
                } else {
                    return Some((0, v.ty, Some(v.kind), v.constant));
                }
            }
        };
        if let Some(fp) = self.cx.types.instance_pou(v.ty) {
            if segs.len() > 1 {
                return self.walk_instance(fp, a, &segs[1..], span);
            }
        }
        self.walk_value(a, v.ty, &segs[1..], Some(v.kind), v.constant, span)
    }

    fn walk_value(&mut self, addr: u64, ty: TypeId, segs: &[Seg], kind: Option<VKind>, constant: bool, span: Span) -> Option<(u64, TypeId, Option<VKind>, bool)> {
        let Some(s) = segs.first() else { return Some((addr, ty, kind, constant)) };
        let r = self.cx.types.resolve(ty);
        match (s, self.cx.types.kind(r).clone()) {
            (Seg::Name(n, nspan), TypeKind::Struct { fields, .. }) => {
                let Some(f) = fields.iter().find(|f| f.name == *n) else {
                    self.cx.error(format!("{} has no element named {}", self.cx.types.name(ty), n), *nspan);
                    return None;
                };
                self.walk_value(addr + f.offset, f.ty, &segs[1..], kind, constant, span)
            }
            (Seg::Name(_, _), TypeKind::Instance { pou }) => self.walk_instance(pou, addr, segs, span),
            (Seg::Index(idx), TypeKind::Array { dims, elem }) => {
                if idx.len() != dims.len() {
                    self.cx.error("wrong number of subscripts", span);
                    return None;
                }
                let stride = self.cx.types.stride(elem);
                let mut lin = 0i64;
                for ((lo, hi), i) in dims.iter().zip(idx) {
                    if i < lo || i > hi {
                        self.cx.error("subscript out of range", span);
                        return None;
                    }
                    lin = lin * (hi - lo + 1) + (i - lo);
                }
                self.walk_value(addr + lin as u64 * stride, elem, &segs[1..], kind, constant, span)
            }
            _ => {
                self.cx.error("invalid access path", span);
                None
            }
        }
    }

    // ------------------------------------------------------------ assembly

    fn assemble(&mut self, config_init: Option<usize>) -> Image {
        // generate code for all routines
        let n = self.cx.routines.len();
        let frames: Vec<u64> = self.cx.routines.iter().map(|r| r.frame_bytes).collect();
        let frame_of = |r: usize| frames.get(r).copied().unwrap_or(0);
        let mut codes: Vec<RoutineCode> = Vec::with_capacity(n);
        for r in 0..n {
            codes.push(codegen::generate(&self.cx.routines[r].body, &frame_of));
        }
        // recursion check and stack requirements
        let mut deep: Vec<Option<u64>> = vec![None; n];
        let mut state = vec![0u8; n];
        for r in 0..n {
            self.deep(r, &codes, &frames, &mut deep, &mut state);
        }
        let mut image = Image::default();
        let mut routines = Vec::with_capacity(n);
        for (r, code) in codes.iter_mut().enumerate() {
            let entry = image.code.len() as u32;
            codegen::relocate(&mut code.ops, entry);
            image.code.extend_from_slice(&code.ops);
            for (pc, span) in &code.lines {
                if (span.file.0 as usize) < self.cx.sources.len() {
                    let f = self.cx.sources.get(span.file);
                    let (line, column) = f.line_col(span.start);
                    image.lines.push((entry + pc, SourceLoc { file: f.name.clone(), line, column }));
                }
            }
            routines.push(Routine {
                name: self.cx.routines[r].name.clone(),
                entry,
                end: image.code.len() as u32,
                frame_size: self.cx.routines[r].frame_bytes as u32,
            });
        }
        image.lines.sort_by_key(|(p, _)| *p);
        image.routines = routines;
        // stacks
        let need = |r: usize| -> u64 { frames[r] + deep[r].unwrap_or(0) + 256 };
        if let Some(ci) = config_init {
            let size = round_up(need(ci), 8);
            let base = self.alloc(size * 8) / 8;
            image.config_init_stack = Area { base, size };
        }
        image.config_init = config_init.map(|r| r as u32);
        let mut resources = std::mem::take(&mut self.resources);
        for res in resources.iter_mut() {
            let size = round_up(need(res.init_routine as usize), 8);
            let base = self.alloc(size * 8) / 8;
            res.init_stack = Area { base, size };
            for j in res.jobs.iter_mut() {
                let size = round_up(need(j.routine as usize), 8);
                let base = self.alloc(size * 8) / 8;
                j.stack = Area { base, size };
            }
        }
        image.resources = resources;
        image.memory_size = round_up(self.top, 8) + 8;
        image.data = vec![(POOL_BASE, self.cx.pool.clone())];
        image.configuration = self.cfg.name.key();
        image.symbols = std::mem::take(&mut self.symbols);
        image.access_paths = std::mem::take(&mut self.access);
        let mut retain = std::mem::take(&mut self.retain);
        retain.sort();
        retain.dedup();
        image.retain = retain;
        image
    }

    fn deep(&mut self, r: usize, codes: &[RoutineCode], frames: &[u64], deep: &mut [Option<u64>], state: &mut [u8]) -> u64 {
        if let Some(d) = deep[r] {
            return d;
        }
        if state[r] == 1 {
            let name = self.cx.routines[r].name.clone();
            self.cx.diags.push(Diagnostic::error(format!("{} is invoked recursively; program organization units shall not be recursive", name), Span::default()).with_clause("2.5"));
            return 0;
        }
        state[r] = 1;
        let mut d = 0u64;
        for c in &codes[r].calls {
            if c.routine >= codes.len() {
                continue;
            }
            let inner = self.deep(c.routine, codes, frames, deep, state);
            let need = match c.kind {
                CallKind::Func => c.pending + inner,
                CallKind::Fb => c.pending + frames[c.routine] + inner,
            };
            d = d.max(need);
        }
        state[r] = 2;
        deep[r] = Some(d);
        d
    }
}

#[derive(Clone, Debug)]
enum Seg {
    Name(String, Span),
    Index(Vec<i64>),
}

fn flatten(e: &ast::Expr, out: &mut Vec<Seg>) -> bool {
    match &e.kind {
        ExprKind::Name(n) => {
            out.push(Seg::Name(n.key(), n.span));
            true
        }
        ExprKind::Field { base, field } => {
            if !flatten(base, out) {
                return false;
            }
            out.push(Seg::Name(field.key(), field.span));
            true
        }
        ExprKind::Index { base, indices } => {
            if !flatten(base, out) {
                return false;
            }
            let mut v = Vec::new();
            for i in indices {
                match &i.kind {
                    ExprKind::Literal(ast::Literal { kind: ast::LiteralKind::Int { value, negative, .. }, .. }) => {
                        v.push(if *negative { -(*value as i64) } else { *value as i64 })
                    }
                    ExprKind::Unary { op: ast::UnaryOp::Neg, operand } => match &operand.kind {
                        ExprKind::Literal(ast::Literal { kind: ast::LiteralKind::Int { value, .. }, .. }) => v.push(-(*value as i64)),
                        _ => return false,
                    },
                    _ => return false,
                }
            }
            out.push(Seg::Index(v));
            true
        }
        _ => false,
    }
}

/// Run-time field descriptions of a POU instance.
pub fn pou_fields(cx: &Ctx, pou: usize) -> (String, Vec<FieldDesc>) {
    let p = &cx.pous[pou];
    let mut fields = Vec::new();
    for v in &p.vars {
        if v.kind == VKind::Temp || matches!(v.storage, Storage::Frame(_) | Storage::FrameRef(_)) {
            continue;
        }
        let f = |q: usize| -> (String, Vec<FieldDesc>) { pou_fields(cx, q) };
        let mut d = cx.types.desc(v.ty, &f);
        if v.is_ref() {
            d = TypeDesc::Reference(Box::new(d));
        }
        let name = if v.kind == VKind::Hidden && !v.name.starts_with('%') { format!("__{}", v.name.trim_start_matches('_')) } else { v.name.clone() };
        fields.push(FieldDesc { name, offset: v.offset(), ty: d });
    }
    if let Some(sfc) = &p.sfc {
        for s in &sfc.steps {
            fields.push(FieldDesc {
                name: s.name.clone(),
                offset: s.x,
                ty: TypeDesc::Struct {
                    name: "STEP".into(),
                    fields: vec![
                        FieldDesc { name: "X".into(), offset: 0, ty: TypeDesc::Bool },
                        FieldDesc { name: "T".into(), offset: s.t - s.x, ty: TypeDesc::Time },
                    ],
                },
            });
        }
        if let Some(&ac) = cx.pou_by_name.get("__ACTION_CONTROL") {
            let q = cx.pous[ac].var("Q").map(|v| v.offset()).unwrap_or(0);
            let a = cx.pous[ac].var("A").map(|v| v.offset()).unwrap_or(0);
            for act in &sfc.actions {
                if p.var_map.contains_key(&act.name) {
                    continue;
                }
                fields.push(FieldDesc {
                    name: act.name.clone(),
                    offset: act.control,
                    ty: TypeDesc::Struct {
                        name: "ACTION".into(),
                        fields: vec![FieldDesc { name: "Q".into(), offset: q, ty: TypeDesc::Bool }, FieldDesc { name: "A".into(), offset: a, ty: TypeDesc::Bool }],
                    },
                });
            }
        }
    }
    (p.name.clone(), fields)
}

use crate::lower::V;

/// Value of a program connection source (constant, global variable,
/// directly represented variable).
fn cfg_value(l: &mut Lowerer, e: &ast::Expr, ty: TypeId, ri: usize, res: &[ResLayout], globals: &[GVar], span: Span) -> Option<V> {
    match &e.kind {
        ExprKind::Literal(_) | ExprKind::TypedEnum { .. } | ExprKind::Unary { .. } => {
            let c = l.cx.const_eval(e, Some(ty))?;
            match c.value {
                ConstValue::Str { wide, chars } => {
                    let a = l.cx.string_literal(wide, &chars);
                    let t = l.cx.types.string(wide, chars.len().max(1) as u32);
                    Some(V::mem(Loc::Abs(a), t))
                }
                v => {
                    let ex = l.const_to(v, ty, span);
                    Some(V::val(ex, ty))
                }
            }
        }
        ExprKind::Name(n) if l.cx.types.is_enum(ty) && cfg_lookup_global(&n.key(), ri, res, globals).is_none() => {
            let v = crate::ctx::enum_lookup(&l.cx.types, &n.key(), Some(ty)).ok()?;
            let ex = l.const_to(v, ty, span);
            Some(V::val(ex, ty))
        }
        _ => {
            let (loc, t, _) = cfg_place(l, e, ri, res, globals)?;
            Some(V::mem(loc, t))
        }
    }
}

fn cfg_lookup_global(name: &str, ri: usize, res: &[ResLayout], globals: &[GVar]) -> Option<(u64, TypeId, bool)> {
    if let Some(g) = res.get(ri).and_then(|r| r.globals.iter().find(|g| g.name == name)) {
        return Some((g.addr, g.ty, g.constant));
    }
    globals.iter().find(|g| g.name == name).map(|g| (g.addr, g.ty, g.constant))
}

/// Location of a global variable reference, program output reference or
/// directly represented variable in configuration context.
fn cfg_place(l: &mut Lowerer, e: &ast::Expr, ri: usize, res: &[ResLayout], globals: &[GVar]) -> Option<(Loc, TypeId, bool)> {
    match &e.kind {
        ExprKind::DirectVar(d) => {
            let (loc, path, off) = io_place(d);
            let area = res.get(ri)?.io.get(&(loc, path))?;
            // A declaration in VAR_GLOBAL gives the type of the direct variable.
            let key = d.to_text();
            let ty = res[ri]
                .globals
                .iter()
                .chain(globals.iter())
                .find(|g| g.name == key)
                .map(|g| g.ty)
                .unwrap_or_else(|| {
                    l.cx.types.elem(match d.size {
                        SizePrefix::None | SizePrefix::X => Elem::Bool,
                        SizePrefix::B => Elem::Byte,
                        SizePrefix::W => Elem::Word,
                        SizePrefix::D => Elem::DWord,
                        SizePrefix::L => Elem::LWord,
                    })
                });
            Some((Loc::Abs(area.base * 8 + off), ty, false))
        }
        ExprKind::Name(n) => match cfg_lookup_global(&n.key(), ri, res, globals) {
            Some((a, t, c)) => Some((Loc::Abs(a), t, c)),
            None => {
                l.cx.error_clause("2.7.1", format!("unknown global variable {}", n), n.span);
                None
            }
        },
        ExprKind::Field { base, field } => {
            // resource.global, program.output or global.element
            if let ExprKind::Name(b) = &base.kind {
                if let Some(r2) = l.cx.configs.first().and_then(|_| res.iter().position(|r| r.name == b.key())) {
                    if let Some((a, t, c)) = cfg_lookup_global(&field.key(), r2, res, &[]) {
                        return Some((Loc::Abs(a), t, c));
                    }
                }
                if let Some(p) = res.get(ri).and_then(|r| r.programs.iter().find(|p| p.name == b.key())) {
                    let v = l.cx.pous[p.pou].var(&field.key()).cloned();
                    return match v {
                        Some(v) if v.kind == VKind::Output => Some((Loc::Abs(p.addr + v.offset()), v.ty, false)),
                        _ => {
                            l.cx.error_clause("2.7.1", format!("{} is not an output of program {}", field, b), field.span);
                            None
                        }
                    };
                }
            }
            let (bl, bt, c) = cfg_place(l, base, ri, res, globals)?;
            let r = l.cx.types.resolve(bt);
            match l.cx.types.kind(r).clone() {
                TypeKind::Struct { fields, .. } => {
                    let f = fields.iter().find(|f| f.name == field.key())?;
                    Some((bl.offset(f.offset), f.ty, c))
                }
                _ => {
                    l.cx.error("invalid structure element reference", field.span);
                    None
                }
            }
        }
        _ => {
            l.cx.error_clause("2.7.1", "expected a global variable, program output or directly represented variable", e.span);
            None
        }
    }
}

impl<'s> Ctx<'s> {
    /// Adds the pseudo POU used to generate configuration-level code.
    fn config_pou(&mut self) -> usize {
        if let Some(&i) = self.pou_by_name.get("$CONFIG") {
            return i;
        }
        let span = Span::default();
        let ast = ast::Pou {
            kind: ast::PouKind::Program,
            name: structure_syntax::Ident::new("$CONFIG", span),
            var_blocks: vec![],
            body: ast::Body::Empty,
            span,
        };
        let idx = self.pous.len();
        self.pous.push(PouInfo {
            name: "$CONFIG".into(),
            kind: PouKind::Program,
            span,
            ast,
            std: false,
            result: None,
            vars: vec![],
            var_map: HashMap::new(),
            inst_type: None,
            inst_size: 0,
            outputs_start: 0,
            frame_base: 0,
            routine: None,
            init_routine: None,
            laid_out: true,
            sfc: None,
        });
        self.pou_by_name.insert("$CONFIG".into(), idx);
        idx
    }
}

/// Links the chosen configuration into an image.
pub fn link(cx: &mut Ctx) -> Option<Image> {
    let cfg = match &cx.options.configuration {
        Some(name) => match cx.configs.iter().find(|c| c.name.key() == name.to_uppercase()) {
            Some(c) => c.clone(),
            None => {
                cx.diags.push(Diagnostic::error(format!("no configuration named {}", name), Span::default()));
                return None;
            }
        },
        None => match cx.configs.len() {
            0 => {
                cx.diags.push(Diagnostic::error("no CONFIGURATION is declared; a configuration is required to build an executable image (1.4.1)", Span::default()));
                return None;
            }
            1 => cx.configs[0].clone(),
            _ => {
                let c = cx.configs[0].clone();
                cx.diags.push(Diagnostic::warning(format!("several configurations are declared; building {}", c.name), c.name.span));
                c
            }
        },
    };
    let cfg_pou = cx.config_pou();
    let mut l = Linker {
        cx,
        top: 0,
        cfg_pou,
        cfg,
        cfg_globals: Vec::new(),
        res: Vec::new(),
        symbols: Vec::new(),
        access: Vec::new(),
        retain: Vec::new(),
        resources: Vec::new(),
        jobs_for_stack: Vec::new(),
    };
    l.run()
}
