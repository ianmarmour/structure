//! Lowering of complete program organisation units to routines.

use super::*;
use structure_runtime::error::TrapCode;
use structure_syntax::ast::{Body, Edge};

impl<'s> Ctx<'s> {
    /// Allocates routines for all POUs, then lowers all bodies.
    pub fn lower_all(&mut self) {
        for pi in 0..self.pous.len() {
            let name = self.pous[pi].name.clone();
            let r = self.new_routine(name.clone());
            self.pous[pi].routine = Some(r);
            if self.pous[pi].kind != PouKind::Function {
                let ir = self.new_routine(format!("{}$init", name));
                self.pous[pi].init_routine = Some(ir);
            }
        }
        for pi in 0..self.pous.len() {
            let saved = self.in_std;
            self.in_std = self.pous[pi].std;
            self.lower_pou(pi);
            self.lower_instance_init(pi);
            self.in_std = saved;
        }
    }

    fn lower_pou(&mut self, pi: usize) {
        let ast = self.pous[pi].ast.clone();
        let kind = self.pous[pi].kind;
        let routine = self.pous[pi].routine.unwrap();
        let mut l = Lowerer::new(self, pi);
        l.return_label = l.label();
        let mut out = Vec::new();
        out.push(St::At(ast.name.span));
        if kind != PouKind::Program {
            // EN / ENO (2.5.1.2, 2.5.2.1a)
            let en = l.cx.pous[pi].var("EN").unwrap().loc();
            let eno = l.cx.pous[pi].var("ENO").unwrap().loc();
            out.push(St::If(Ex::not(Ex::Load(Ty::Bool, en)), vec![St::Store(Ty::Bool, eno.clone(), Ex::bool(false)), St::Return], vec![]));
            out.push(St::Store(Ty::Bool, eno, Ex::bool(true)));
        }
        if kind == PouKind::Function {
            // Function variables are (re)initialised at every invocation.
            l.init_vars(&mut out, &[VKind::Output, VKind::Local, VKind::Result]);
        } else {
            l.init_vars(&mut out, &[VKind::Temp]);
            // edge detection (2.5.2.2 rule 6, table 33 note 2)
            let vars = l.cx.pous[pi].vars.clone();
            for v in &vars {
                if let (Some(edge), Some(q)) = (v.edge, v.edge_result) {
                    let qloc = vars[q].loc();
                    let mloc = l.cx.pous[pi].var(&format!("__EDGE_M_{}", v.name)).unwrap().loc();
                    let clk = Ex::Load(Ty::Bool, v.loc());
                    let clk = if edge == Edge::Falling { Ex::not(clk) } else { clk };
                    out.push(St::Store(Ty::Bool, qloc, Ex::and(clk.clone(), Ex::not(Ex::Load(Ty::Bool, mloc.clone())))));
                    out.push(St::Store(Ty::Bool, mloc, clk));
                }
            }
        }
        match &ast.body {
            Body::Empty => {}
            Body::St(stmts) => {
                l.language = Lang::St;
                l.stmts(&mut out, stmts);
            }
            Body::Il(list) => {
                l.language = Lang::Il;
                l.il_body(&mut out, list);
            }
            Body::Sfc(sfc) => l.sfc_body(&mut out, sfc, false),
            Body::Graphic(g) => {
                l.language = Lang::Graphic;
                if let Some(sfc) = &g.sfc {
                    l.sfc_body(&mut out, sfc, true);
                } else {
                    l.graphic_body(&mut out, g);
                }
            }
        }
        out.push(St::Label(l.return_label));
        if kind == PouKind::Function {
            // 3.3.2.1: returning with ENO = TRUE requires an assigned result.
            let eno = l.cx.pous[pi].var("ENO").unwrap().loc();
            let assigned = l.cx.pous[pi].var("__ASSIGNED").unwrap().loc();
            if !l.result_definitely_assigned(&ast.body) {
                out.push(St::If(
                    Ex::and(Ex::Load(Ty::Bool, eno), Ex::not(Ex::Load(Ty::Bool, assigned))),
                    vec![St::Trap(TrapCode::NoValueAssigned)],
                    vec![],
                ));
            }
        }
        out.push(St::Return);
        let frame = l.frame_top;
        self.routines[routine].body = out;
        self.routines[routine].frame_bytes = frame;
    }

    /// Generates the routine that initialises an instance of a function
    /// block or program (cold start values).
    fn lower_instance_init(&mut self, pi: usize) {
        let Some(r) = self.pous[pi].init_routine else { return };
        let mut l = Lowerer::new(self, pi);
        l.frame_top = 0;
        let mut out = Vec::new();
        l.init_vars(&mut out, &[VKind::Input, VKind::Output, VKind::Local, VKind::Global, VKind::Hidden]);
        if let Some(sfc) = l.cx.pous[pi].sfc.clone() {
            for s in &sfc.steps {
                if s.initial && !s.nested {
                    out.push(St::Store(Ty::Bool, Loc::Ip(s.x as u32), Ex::bool(true)));
                }
            }
            if let Some(&ac) = l.cx.pou_by_name.get("__ACTION_CONTROL") {
                if init::instance_needs_init(l.cx, ac) {
                    let ar = l.cx.pous[ac].init_routine.unwrap();
                    for a in &sfc.actions {
                        out.push(St::CallFb { routine: ar, inst: Ex::Addr(Loc::Ip(a.control as u32)) });
                    }
                }
            }
        }
        out.push(St::Return);
        let frame = l.frame_top;
        self.routines[r].body = out;
        self.routines[r].frame_bytes = frame;
    }
}

impl<'c, 's> Lowerer<'c, 's> {
    /// Conservative check that the function result is assigned on every
    /// path (a top-level assignment before any RETURN).
    fn result_definitely_assigned(&self, body: &Body) -> bool {
        let name = &self.cx.pous[self.pou].name;
        match body {
            Body::St(stmts) => {
                for s in stmts {
                    match &s.kind {
                        structure_syntax::ast::StmtKind::Assign { target, .. } => {
                            if let structure_syntax::ast::ExprKind::Name(n) = &target.kind {
                                if n.key() == *name {
                                    return true;
                                }
                            }
                        }
                        structure_syntax::ast::StmtKind::Return => return false,
                        _ => {
                            if contains_return(s) {
                                return false;
                            }
                        }
                    }
                }
                false
            }
            _ => false,
        }
    }
}

fn contains_return(s: &structure_syntax::ast::Stmt) -> bool {
    use structure_syntax::ast::StmtKind::*;
    match &s.kind {
        Return => true,
        If { branches, else_body } => branches.iter().any(|(_, b)| b.iter().any(contains_return)) || else_body.as_ref().map(|b| b.iter().any(contains_return)).unwrap_or(false),
        Case { arms, else_body, .. } => arms.iter().any(|a| a.body.iter().any(contains_return)) || else_body.as_ref().map(|b| b.iter().any(contains_return)).unwrap_or(false),
        For { body, .. } | While { body, .. } | Repeat { body, .. } => body.iter().any(contains_return),
        _ => false,
    }
}
