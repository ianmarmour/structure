//! Declaration of the variables of program organisation units (2.4.3,
//! 2.5) and layout of instances and call frames.

use crate::ctx::*;
use crate::types::*;
use std::collections::HashMap;
use structure_syntax::ast::{self, Body, Edge, VarItems, VarKind, VarQualifier};
use structure_syntax::diag::Diagnostic;
use structure_syntax::Span;

impl<'s> Ctx<'s> {
    pub fn layout_all(&mut self) {
        let mut stack = Vec::new();
        for i in 0..self.pous.len() {
            self.layout_pou(i, &mut stack);
        }
    }

    pub fn layout_pou(&mut self, i: usize, stack: &mut Vec<usize>) {
        if self.pous[i].laid_out {
            return;
        }
        if stack.contains(&i) {
            let span = self.pous[i].span;
            self.error_clause("2.5", format!("{} is instantiated recursively; program organization units shall not be recursive", self.pous[i].name), span);
            return;
        }
        stack.push(i);
        let saved_std = self.in_std;
        self.in_std = self.pous[i].std;
        self.declare_vars(i, stack);
        self.in_std = saved_std;
        stack.pop();
        self.pous[i].laid_out = true;
    }

    fn declare_vars(&mut self, pi: usize, stack: &mut Vec<usize>) {
        let ast = self.pous[pi].ast.clone();
        let kind = self.pous[pi].kind;
        let mut vars: Vec<VarInfo> = Vec::new();
        let mut map: HashMap<String, usize> = HashMap::new();

        if let ast::PouKind::Function { result } = &ast.kind {
            let t = self.resolve_spec(result, false);
            if self.types.instance_pou(t).is_some() {
                self.error_clause("2.5.1.3", "a function cannot return a function block instance", result.span());
            }
            self.pous[pi].result = Some(t);
        }

        for block in &ast.var_blocks {
            let vk = block.kind;
            // Which blocks are allowed where (B.1.5).
            let allowed = match kind {
                PouKind::Function => matches!(vk, VarKind::Input | VarKind::Output | VarKind::InOut | VarKind::Var),
                PouKind::FunctionBlock => {
                    matches!(vk, VarKind::Input | VarKind::Output | VarKind::InOut | VarKind::Var | VarKind::Temp | VarKind::External)
                }
                PouKind::Program => !matches!(vk, VarKind::Config),
            };
            if !allowed {
                let clause = if kind == PouKind::Function && vk == VarKind::External { "2.5.1" } else { "B.1.5" };
                let msg = format!("{} declarations are not allowed in a {}", vk.keyword(), pou_kind_name(kind));
                if clause == "2.5.1" {
                    self.annex_e("2.5.1", format!("{} (improper use of external variables in functions)", msg), block.span);
                } else {
                    self.error_clause(clause, msg, block.span);
                }
                continue;
            }
            // Qualifiers (table 16a and its footnotes).
            let q = block.qualifier;
            let q_ok = match (vk, q) {
                (_, None) => true,
                (VarKind::Var, _) => true,
                (VarKind::Input | VarKind::Output, Some(VarQualifier::Retain | VarQualifier::NonRetain)) => kind != PouKind::Function,
                (VarKind::Global, _) => true,
                (VarKind::External, Some(VarQualifier::Constant)) => true,
                _ => false,
            };
            if !q_ok {
                self.error_clause(
                    "2.4.3",
                    format!("qualifier {:?} is not allowed in {} declarations here", q.unwrap(), vk.keyword()).to_uppercase().replace("NONRETAIN", "NON_RETAIN"),
                    block.span,
                );
            }
            if kind == PouKind::Function && matches!(q, Some(VarQualifier::Retain | VarQualifier::NonRetain)) {
                self.error_clause("2.5.1", "functions contain no internal state; RETAIN and NON_RETAIN are not allowed", block.span);
            }
            let constant = q == Some(VarQualifier::Constant);
            let retain = qualifier_retain(q);
            let decls = match &block.items {
                VarItems::Vars(d) => d,
                VarItems::Access(_) => continue, // handled when linking
                VarItems::Config(_) => continue,
            };
            for d in decls {
                let ty = self.resolve_spec(&d.spec, false);
                let inst_pou = self.types.instance_pou(ty);
                if let Some(fp) = inst_pou {
                    if matches!(self.pous[fp].name.as_str(), "CTUD_UDINT" | "RTC") && !self.in_std {
                        self.extension(format!("the function block {} is an extension", self.pous[fp].name), d.spec.span());
                    }
                    if self.pous[fp].kind == PouKind::Program {
                        self.error_clause("2.5.3", "programs can only be instantiated within resources", d.spec.span());
                    }
                    self.layout_pou(fp, stack);
                    if kind == PouKind::Function && vk != VarKind::Input {
                        self.error_clause("2.5.1", "function block instances cannot be declared in a function", d.span);
                    }
                    if constant {
                        self.error_clause("2.4.3", "the CONSTANT qualifier shall not be used in the declaration of function block instances", d.span);
                    }
                }
                // Arrays of FB instances need their element layout.
                if let TypeKind::Array { elem, .. } = self.types.kind(self.types.resolve(ty)).clone() {
                    if let Some(fp) = self.types.instance_pou(elem) {
                        self.layout_pou(fp, stack);
                    }
                }
                if let Some(edge) = d.edge {
                    if vk != VarKind::Input || kind == PouKind::Function {
                        self.error_clause("2.5.2.2", "R_EDGE and F_EDGE can only be used on inputs of function blocks and programs", d.span);
                    } else if !self.types.is_bool(ty) {
                        self.error_clause("2.5.2.2", "edge detection can only be applied to BOOL inputs", d.span);
                    }
                    let _ = edge;
                }
                if d.init.is_some() && vk == VarKind::Temp {
                    self.extension("initial values in VAR_TEMP declarations are an extension (B.1.5.2 temp_var_decl)", d.span);
                }
                if d.init.is_some() && matches!(vk, VarKind::External | VarKind::InOut) {
                    self.error_clause("2.4.3.2", format!("initial values cannot be given in {} declarations", vk.keyword()), d.span);
                }
                if let Some(loc) = &d.location {
                    let ok = match kind {
                        PouKind::Function => {
                            self.annex_e("2.5.1", "directly represented variables cannot be used in functions", d.span);
                            false
                        }
                        PouKind::FunctionBlock => {
                            if !loc.incomplete {
                                self.error_clause("2.4.3.1", "fully located variables can only be declared in PROGRAM and VAR_GLOBAL declarations (table 17 footnote a)", d.span);
                            }
                            vk == VarKind::Var
                        }
                        PouKind::Program => {
                            if loc.incomplete && vk != VarKind::Var {
                                self.error_clause("2.4.3.1", "incompletely located variables can only be internal variables", d.span);
                            }
                            matches!(vk, VarKind::Var | VarKind::Global)
                        }
                    };
                    if !ok && kind != PouKind::Function && !matches!(vk, VarKind::Var | VarKind::Global) {
                        self.error_clause("2.4.3.1", format!("location assignments are not allowed in {} declarations", vk.keyword()), d.span);
                    }
                    if loc.incomplete && d.names.is_empty() {
                        self.error_clause("2.4.1.1", "an incompletely specified location requires a variable name", d.span);
                    }
                    if d.names.len() > 1 {
                        self.error_clause("B.1.4.3", "only one variable can be declared with a location assignment", d.span);
                    }
                    if self.types.instance_pou(ty).is_some() {
                        self.error_clause("2.4.3", "function block instances cannot be located", d.span);
                    }
                }
                let init = match &d.init {
                    Some(i) if !matches!(vk, VarKind::External | VarKind::InOut) => self.initializer(i, ty),
                    _ => None,
                };
                let names: Vec<(String, Span)> = if d.names.is_empty() {
                    match &d.location {
                        Some(l) => vec![(l.to_text(), l.span)],
                        None => vec![],
                    }
                } else {
                    d.names.iter().map(|n| (n.key(), n.span)).collect()
                };
                for (name, span) in names {
                    if map.contains_key(&name) {
                        self.error_clause("2.4.3", format!("duplicate declaration of {}", name), span);
                        continue;
                    }
                    if self.is_reserved_name(&name) && !self.in_std {
                        self.error_clause("2.1.3", format!("{} is a reserved name and cannot be used as a variable name", name), span);
                    }
                    if kind == PouKind::Function && name == self.pous[pi].name {
                        self.error_clause("2.5.1.3", "a variable cannot have the name of the function", span);
                    }
                    let vkind = match vk {
                        VarKind::Input => VKind::Input,
                        VarKind::Output => VKind::Output,
                        VarKind::InOut => VKind::InOut,
                        VarKind::Var => VKind::Local,
                        VarKind::Temp => VKind::Temp,
                        VarKind::External => VKind::External,
                        VarKind::Global => VKind::Global,
                        _ => VKind::Local,
                    };
                    if (name == "EN" && vkind != VKind::Input) || (name == "ENO" && vkind != VKind::Output) {
                        self.error_clause("2.5.1.2", format!("{} shall be declared as {}", name, if name == "EN" { "an input" } else { "an output" }), span);
                    }
                    if (name == "EN" || name == "ENO") && !self.types.is_bool(ty) {
                        self.error_clause("2.5.1.2", format!("{} shall be of type BOOL", name), span);
                    }
                    map.insert(name.clone(), vars.len());
                    vars.push(VarInfo {
                        name,
                        kind: vkind,
                        ty,
                        storage: Storage::Inst(0),
                        constant: constant || (vk == VarKind::External && q == Some(VarQualifier::Constant)),
                        retain,
                        edge: d.edge,
                        edge_result: None,
                        init: init.clone(),
                        location: d.location.clone(),
                        span,
                        implicit: false,
                    });
                }
            }
        }

        let span = self.pous[pi].span;
        // Implicit EN / ENO (2.5.1.2, 2.5.2.1a) for functions and function blocks.
        if kind != PouKind::Program {
            if !map.contains_key("EN") {
                map.insert("EN".into(), vars.len());
                vars.push(hidden_var("EN", VKind::Input, self.types.elem(Elem::Bool), Some(InitValue::Const(ConstValue::Bool(true))), span));
            }
            if !map.contains_key("ENO") {
                map.insert("ENO".into(), vars.len());
                vars.push(hidden_var("ENO", VKind::Output, self.types.elem(Elem::Bool), None, span));
            }
        }
        // Edge detection state.
        let n = vars.len();
        for i in 0..n {
            if let Some(edge) = vars[i].edge {
                let base = vars[i].name.clone();
                let b = self.types.elem(Elem::Bool);
                let q = vars.len();
                vars.push(hidden_var(&format!("__EDGE_Q_{}", base), VKind::Hidden, b, None, span));
                let m = vars.len();
                vars.push(hidden_var(&format!("__EDGE_M_{}", base), VKind::Hidden, b, None, span));
                map.insert(vars[q].name.clone(), q);
                map.insert(vars[m].name.clone(), m);
                vars[i].edge_result = Some(q);
                let _ = edge == Edge::Rising;
            }
        }
        // Previous states of transition-sensing contacts and coils.
        let edges = crate::visit::edge_elements(&ast.body);
        if !edges.is_empty() && kind == PouKind::Function {
            self.error_clause("2.5.1", "transition-sensing contacts and coils need state and cannot be used in functions", edges[0].span);
        }
        for node in &edges {
            let name = crate::visit::edge_memory_name(node);
            if !map.contains_key(&name) {
                map.insert(name.clone(), vars.len());
                vars.push(hidden_var(&name, VKind::Hidden, self.types.elem(Elem::Bool), None, node.span));
            }
        }
        // Directly represented variables referenced in the body.
        if kind != PouKind::Function {
            for dv in crate::visit::direct_vars(&ast.body) {
                let name = dv.to_text();
                if map.contains_key(&name) || dv.incomplete {
                    continue;
                }
                let e = match dv.size {
                    ast::SizePrefix::None | ast::SizePrefix::X => Elem::Bool,
                    ast::SizePrefix::B => Elem::Byte,
                    ast::SizePrefix::W => Elem::Word,
                    ast::SizePrefix::D => Elem::DWord,
                    ast::SizePrefix::L => Elem::LWord,
                };
                map.insert(name.clone(), vars.len());
                let mut v = hidden_var(&name, VKind::Hidden, self.types.elem(e), None, dv.span);
                v.location = Some(dv.clone());
                vars.push(v);
            }
        } else if let Some(dv) = crate::visit::direct_vars(&ast.body).first() {
            self.annex_e("2.5.1", "directly represented variables cannot be used in functions", dv.span);
        }
        if kind == PouKind::Function {
            map.insert("__ASSIGNED".into(), vars.len());
            vars.push(hidden_var("__ASSIGNED", VKind::Hidden, self.types.elem(Elem::Bool), None, span));
        }

        // SFC state.
        let sfc = match &ast.body {
            Body::Sfc(s) => Some(s.clone()),
            Body::Graphic(g) => g.sfc.as_ref().map(|s| (**s).clone()),
            _ => None,
        };
        let mut sfc_steps: Vec<(String, bool, bool)> = Vec::new();
        let mut sfc_actions: Vec<String> = Vec::new();
        if let Some(s) = &sfc {
            if kind == PouKind::Function {
                self.error_clause("2.6.1", "SFC elements can only be used in function blocks and programs", span);
            }
            // the chart of the body and the charts included in actions
            for (ci, chart) in crate::visit::sfc_charts(s).into_iter().enumerate() {
                for st in &chart.steps {
                    if !sfc_steps.iter().any(|(n, _, _)| *n == st.name.key()) {
                        sfc_steps.push((st.name.key(), st.initial, ci > 0));
                    }
                    for a in &st.associations {
                        if !sfc_actions.contains(&a.action.key()) {
                            sfc_actions.push(a.action.key());
                        }
                    }
                }
                for a in &chart.actions {
                    if !sfc_actions.contains(&a.name.key()) {
                        sfc_actions.push(a.name.key());
                    }
                }
            }
        }

        // ---- storage layout
        let is_fn = kind == PouKind::Function;
        let order = |v: &VarInfo| -> u32 {
            match v.kind {
                VKind::Input if v.name == "EN" => 1,
                VKind::Input => 2,
                VKind::InOut => 3,
                VKind::Output if v.name == "ENO" => 4,
                VKind::Output => 5,
                VKind::Result => 6,
                VKind::Local | VKind::Global => 7,
                VKind::External => 8,
                VKind::Hidden => 9,
                VKind::Temp => 10,
            }
        };
        let mut idx: Vec<usize> = (0..vars.len()).collect();
        idx.sort_by_key(|i| (order(&vars[*i]), *i));
        let mut off: u64 = if is_fn { 0 } else { 8 };
        let mut temp_off: u64 = 0;
        let mut outputs_start = None;
        let mut result_off = None;
        for i in idx {
            let v = &vars[i];
            let o = order(v);
            if o >= 4 && outputs_start.is_none() && !is_fn {
                outputs_start = Some(round8(off));
            }
            if is_fn && o >= 6 && result_off.is_none() {
                result_off = Some(round8(off));
                off = round8(off) + self.pous[pi].result.map(|t| self.types.size(t)).unwrap_or(0);
            }
            let is_ref = matches!(v.kind, VKind::InOut | VKind::External)
                || (v.kind == VKind::Input && self.types.instance_pou(v.ty).is_some())
                || v.location.is_some();
            let size = if is_ref { 64 } else { self.types.size(v.ty) };
            if v.kind == VKind::Temp {
                temp_off = round8(temp_off);
                vars[i].storage = if is_ref { Storage::FrameRef(temp_off) } else { Storage::Frame(temp_off) };
                temp_off += size;
                continue;
            }
            off = round8(off);
            vars[i].storage = match (is_fn, is_ref) {
                (true, true) => Storage::FrameRef(off),
                (true, false) => Storage::Frame(off),
                (false, true) => Storage::InstRef(off),
                (false, false) => Storage::Inst(off),
            };
            off += size;
        }
        if is_fn && result_off.is_none() {
            result_off = Some(round8(off));
            off = round8(off) + self.pous[pi].result.map(|t| self.types.size(t)).unwrap_or(0);
        }
        // SFC state after all variables.
        let mut sfc_layout = None;
        if sfc.is_some() && !is_fn {
            let mut layout = SfcLayout::default();
            for (name, initial, nested) in &sfc_steps {
                off = round8(off);
                let x = off;
                let t = x + 8;
                let start = t + 64;
                off = start + 64;
                layout.steps.push(SfcStepLayout { name: name.clone(), initial: *initial, nested: *nested, x, t, start });
            }
            let ac = self.pou_by_name.get("__ACTION_CONTROL").copied();
            if let Some(ac) = ac {
                self.layout_pou(ac, stack);
                let size = self.pous[ac].inst_size;
                for a in &sfc_actions {
                    off = round8(off);
                    layout.actions.push(SfcActionLayout { name: a.clone(), control: off });
                    off += size;
                }
            }
            sfc_layout = Some(layout);
        }
        let p = &mut self.pous[pi];
        p.vars = vars;
        p.var_map = map;
        p.sfc = sfc_layout;
        if is_fn {
            p.frame_base = round8(off) / 8;
            if let Some(r) = result_off {
                // record the result as a pseudo variable
                let rt = p.result.unwrap_or(self.types.error);
                let name = p.name.clone();
                p.var_map.insert(name.clone(), p.vars.len());
                p.vars.push(VarInfo {
                    name,
                    kind: VKind::Result,
                    ty: rt,
                    storage: Storage::Frame(r),
                    constant: false,
                    retain: None,
                    edge: None,
                    edge_result: None,
                    init: None,
                    location: None,
                    span,
                    implicit: true,
                });
            }
        } else {
            p.inst_size = round8(off).max(8);
            p.outputs_start = outputs_start.unwrap_or(p.inst_size);
            p.frame_base = round8(temp_off) / 8;
            let t = p.inst_type.unwrap();
            self.types.list[t].size = self.pous[pi].inst_size;
        }
        // Array of FB instance sizes may have been computed with size 0; fix.
        self.fix_array_sizes();
    }

    /// Recomputes sizes of array types whose element is an instance type.
    fn fix_array_sizes(&mut self) {
        for id in 0..self.types.list.len() {
            if let TypeKind::Array { dims, elem } = self.types.list[id].kind.clone() {
                if self.types.instance_pou(elem).is_some() {
                    let count: u64 = dims.iter().map(|(l, h)| (h - l + 1) as u64).product();
                    let stride = self.types.stride(elem);
                    self.types.list[id].size = round8(stride * count);
                }
            }
        }
    }

    /// Checks VAR_EXTERNAL CONSTANT agreement and similar cross-POU rules
    /// that need the full declaration information.
    pub fn check_declarations(&mut self) {
        for pi in 0..self.pous.len() {
            if self.pous[pi].std {
                continue;
            }
            if self.pous[pi].kind == PouKind::Function {
                let has_input = self.pous[pi].vars.iter().any(|v| v.kind == VKind::Input && !v.implicit);
                if !has_input {
                    let span = self.pous[pi].span;
                    self.diags.push(Diagnostic::warning("a function should have at least one input variable", span).with_clause("B.1.5.1"));
                }
            }
        }
    }
}

fn hidden_var(name: &str, kind: VKind, ty: TypeId, init: Option<InitValue>, span: Span) -> VarInfo {
    VarInfo {
        name: name.to_string(),
        kind,
        ty,
        storage: Storage::Inst(0),
        constant: false,
        retain: None,
        edge: None,
        edge_result: None,
        init,
        location: None,
        span,
        implicit: true,
    }
}

pub fn pou_kind_name(k: PouKind) -> &'static str {
    match k {
        PouKind::Function => "function",
        PouKind::FunctionBlock => "function block",
        PouKind::Program => "program",
    }
}
