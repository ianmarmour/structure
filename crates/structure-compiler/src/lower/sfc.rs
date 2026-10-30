//! Sequential Function Charts (2.6).
//!
//! Each invocation of a POU structured as an SFC performs, in order:
//! 1. update of the elapsed time `T` of active steps;
//! 2. action control (2.6.4.5): the inputs of the ACTION_CONTROL instance
//!    of every action are computed from the active associations, the
//!    instance is invoked, and actions whose `A` output is TRUE (Boolean
//!    actions: `Q`) are executed;
//! 3. evolution (2.6.5): the transitions whose predecessor steps are all
//!    active and whose condition is true are cleared, in priority order;
//!    all predecessor steps of cleared transitions are deactivated, then all
//!    successor steps are activated.
//!
//! Because transitions are evaluated after the actions of the scan, the
//! effects of the activation of a step have propagated before its successor
//! transitions are tested, and a step is active for at least one execution
//! of its actions.

use super::*;
use std::collections::{HashMap, HashSet};
use structure_runtime::error::TrapCode;
use structure_runtime::image::CmpOp;
use structure_syntax::ast::{self, Body, Qualifier, Sfc, TransitionCondition};

const QUALIFIER_INPUTS: [(&str, Qualifier); 11] = [
    ("N", Qualifier::N),
    ("R", Qualifier::R),
    ("S", Qualifier::S),
    ("L", Qualifier::L),
    ("D", Qualifier::D),
    ("P", Qualifier::P),
    ("P1", Qualifier::P1),
    ("P0", Qualifier::P0),
    ("SD", Qualifier::SD),
    ("DS", Qualifier::DS),
    ("SL", Qualifier::SL),
];

struct Assoc {
    step: usize,
    qualifier: Qualifier,
    time: Option<ast::Expr>,
    span: Span,
}

/// The declarations of the SFC of a POU: steps and actions are local to
/// the POU, whichever chart (that of the body, or one included in an
/// action, table 42 feature 2s) declares them.
pub struct SfcScope {
    actions: HashMap<String, ast::Action>,
    /// Chart (identified by its span) whose evolution controls each
    /// action: the chart whose steps are associated with it, otherwise
    /// the chart declaring it.
    action_chart: HashMap<String, Span>,
    step_chart: HashMap<String, Span>,
}

impl<'c, 's> Lowerer<'c, 's> {
    /// Lowers the SFC of a POU body.
    pub fn sfc_body(&mut self, out: &mut Vec<St>, sfc: &Sfc, graphic: bool) {
        if self.cx.pous[self.pou].sfc.is_none() {
            self.cx.error_clause("2.6.1", "SFC elements can only be used in function blocks and programs", sfc.span);
            return;
        }
        let charts = crate::visit::sfc_charts(sfc);
        let mut step_chart = HashMap::new();
        for c in &charts {
            for s in &c.steps {
                if step_chart.insert(s.name.key(), c.span).is_some() {
                    self.cx.error_clause("2.6.2", format!("duplicate step name {}", s.name), s.name.span);
                }
                if self.cx.pous[self.pou].var_map.contains_key(&s.name.key()) {
                    self.cx.error_clause("2.6.2", format!("the step name {} is already used for a variable", s.name), s.name.span);
                }
            }
        }
        let mut actions = HashMap::new();
        for c in &charts {
            for a in &c.actions {
                if actions.insert(a.name.key(), a.clone()).is_some() {
                    self.cx.error_clause("2.6.4.1", format!("duplicate action {}", a.name), a.name.span);
                }
            }
        }
        let mut action_chart: HashMap<String, Span> = HashMap::new();
        for c in &charts {
            for s in &c.steps {
                for a in &s.associations {
                    match action_chart.get(&a.action.key()) {
                        Some(other) if *other != c.span => self.cx.error_clause(
                            "2.6.4.2",
                            format!("the action {} is associated with steps of different charts (implementation restriction)", a.action),
                            a.action.span,
                        ),
                        _ => {
                            action_chart.insert(a.action.key(), c.span);
                        }
                    }
                }
            }
        }
        for c in &charts {
            for a in &c.actions {
                action_chart.entry(a.name.key()).or_insert(c.span);
            }
        }
        let saved = self.sfc_scope.replace(std::rc::Rc::new(SfcScope { actions, action_chart, step_chart }));
        self.sfc_chart(out, sfc, graphic, None);
        self.sfc_scope = saved;
    }

    /// Lowers one chart. A chart included in an action (`enclosing` gives
    /// the Q output of the action's control block) is executed with the
    /// action: its initial step is activated when the action becomes
    /// active, it evolves while the action is active, and all its steps
    /// are deactivated in the final scan of the action, which is also the
    /// final scan of its own active actions (enclosure, as in IEC 60848).
    fn sfc_chart(&mut self, out: &mut Vec<St>, sfc: &Sfc, graphic: bool, enclosing: Option<Ex>) {
        let scope = self.sfc_scope.clone().expect("SFC scope");
        let full = self.cx.pous[self.pou].sfc.clone().unwrap();
        let layout = crate::ctx::SfcLayout {
            steps: full.steps.iter().filter(|s| scope.step_chart.get(&s.name) == Some(&sfc.span)).cloned().collect(),
            actions: full.actions.iter().filter(|a| scope.action_chart.get(&a.name).map_or(enclosing.is_none(), |c| *c == sfc.span)).cloned().collect(),
        };
        let saved_2c = self.graphic_2c;
        self.graphic_2c = graphic;
        let step_index: HashMap<String, usize> = layout.steps.iter().enumerate().map(|(i, s)| (s.name.clone(), i)).collect();
        let action_decl: HashMap<String, &ast::Action> = scope.actions.iter().map(|(k, a)| (k.clone(), a)).collect();
        // Transitions, with named transitions resolved.
        let mut named: HashMap<String, &ast::Transition> = HashMap::new();
        for t in &sfc.transitions {
            if let (Some(n), false) = (&t.name, matches!(t.condition, TransitionCondition::Named)) {
                named.insert(n.key(), t);
            }
        }
        let mut transitions: Vec<(&ast::Transition, Vec<usize>, Vec<usize>)> = Vec::new();
        for t in &sfc.transitions {
            // A textual definition referenced by a graphical transition name
            // is not a transition of its own.
            if t.name.is_some() && !matches!(t.condition, TransitionCondition::Named) {
                let referenced = sfc.transitions.iter().any(|o| matches!(o.condition, TransitionCondition::Named) && o.name.as_ref().map(|n| n.key()) == t.name.as_ref().map(|n| n.key()));
                if referenced {
                    continue;
                }
            }
            let mut from = Vec::new();
            let mut to = Vec::new();
            for (list, dst) in [(&t.from, &mut from), (&t.to, &mut to)] {
                for s in list.iter() {
                    match step_index.get(&s.key()) {
                        Some(i) => dst.push(*i),
                        None if scope.step_chart.contains_key(&s.key()) => {
                            self.cx.error_clause("2.6.3", format!("the step {} belongs to another chart", s), s.span)
                        }
                        None => self.cx.error_clause("2.6.3", format!("unknown step {}", s), s.span),
                    }
                }
            }
            if from.is_empty() || to.is_empty() {
                continue;
            }
            transitions.push((t, from, to));
        }
        self.sfc_static_checks(&layout, &transitions, sfc);
        let time = Ty::I64;

        // ---------------------------------------------------------- 0. enclosure
        if let Some(q) = &enclosing {
            let x = |s: &crate::ctx::SfcStepLayout| Ex::Load(Ty::Bool, Loc::Ip(s.x as u32));
            let any = layout.steps.iter().map(x).reduce(Ex::or).unwrap_or(Ex::bool(false));
            let mut activate = Vec::new();
            for s in layout.steps.iter().filter(|s| s.initial) {
                activate.push(St::Store(Ty::Bool, Loc::Ip(s.x as u32), Ex::bool(true)));
                activate.push(St::Store(time, Loc::Ip(s.start as u32), Ex::Now));
                activate.push(St::Store(time, Loc::Ip(s.t as u32), Ex::Const(time, 0)));
            }
            let mut deactivate = Vec::new();
            for s in &layout.steps {
                let et = Ex::Bin(BinOp::Sub, time, Box::new(Ex::Now), Box::new(Ex::Load(time, Loc::Ip(s.start as u32))));
                deactivate.push(St::If(
                    x(s),
                    vec![St::Store(time, Loc::Ip(s.t as u32), et), St::Store(Ty::Bool, Loc::Ip(s.x as u32), Ex::bool(false))],
                    vec![],
                ));
            }
            out.push(St::If(q.clone(), vec![St::If(Ex::not(any), activate, vec![])], deactivate));
        }

        // ---------------------------------------------------------- 1. step times
        for s in &layout.steps {
            let x = Ex::Load(Ty::Bool, Loc::Ip(s.x as u32));
            let et = Ex::Bin(BinOp::Sub, time, Box::new(Ex::Now), Box::new(Ex::Load(time, Loc::Ip(s.start as u32))));
            out.push(St::If(x, vec![St::Store(time, Loc::Ip(s.t as u32), et)], vec![]));
        }

        // ---------------------------------------------------------- 2. actions
        let ac = self.cx.pou_by_name["__ACTION_CONTROL"];
        let ac_info = self.cx.pous[ac].clone();
        let ac_routine = ac_info.routine.unwrap();
        let mut assocs: HashMap<String, Vec<Assoc>> = HashMap::new();
        for s in &sfc.steps {
            let si = step_index[&s.name.key()];
            for a in &s.associations {
                let q = a.qualifier.unwrap_or(Qualifier::N);
                if q.is_timed() && a.time.is_none() {
                    self.cx.error_clause("2.6.4.4", format!("the qualifier {} requires a duration", q.as_str()), a.span);
                }
                if !q.is_timed() && a.time.is_some() {
                    self.cx.error_clause("2.6.4.4", format!("the qualifier {} has no duration", q.as_str()), a.span);
                }
                for ind in &a.indicators {
                    let ok = self.cx.pous[self.pou].var(&ind.key()).map(|v| self.cx.types.is_bool(v.ty)).unwrap_or(false);
                    if !ok {
                        self.cx.error_clause("2.6.4.3", format!("the indicator {} shall be a BOOL variable", ind), ind.span);
                    }
                }
                assocs.entry(a.action.key()).or_default().push(Assoc { step: si, qualifier: q, time: a.time.clone(), span: a.span });
            }
        }
        for a in &layout.actions {
            let ctrl = Loc::Ip(a.control as u32);
            let list = assocs.get(&a.name).map(|v| v.as_slice()).unwrap_or(&[]);
            // inputs of the action control block (rule 2)
            for (input, q) in QUALIFIER_INPUTS {
                let mut e = Ex::bool(false);
                for asc in list.iter().filter(|x| x.qualifier == q) {
                    let x = Ex::Load(Ty::Bool, Loc::Ip(layout.steps[asc.step].x as u32));
                    e = match e {
                        Ex::Const(Ty::Bool, 0) => x,
                        prev => Ex::or(prev, x),
                    };
                }
                let v = ac_info.var(input).unwrap();
                out.push(St::Store(Ty::Bool, ctrl.clone().offset(v.offset()), e));
            }
            // T input (rule 3) and contention (rule 4 a)
            let timed: Vec<&Assoc> = list.iter().filter(|x| x.qualifier.is_timed()).collect();
            let tv = ac_info.var("T").unwrap();
            let mut t_expr = Ex::Const(time, 0);
            let tt = self.elem_ty(Elem::Time);
            for asc in timed.iter().rev() {
                let Some(te) = &asc.time else { continue };
                let v = self.expr(te, Some(tt));
                let d = self.coerce(v, tt, te.span);
                let x = Ex::Load(Ty::Bool, Loc::Ip(layout.steps[asc.step].x as u32));
                t_expr = Ex::Select(Box::new(x), Box::new(d), Box::new(t_expr));
            }
            out.push(St::Store(time, ctrl.clone().offset(tv.offset()), t_expr));
            if timed.len() > 1 {
                let mut count = Ex::Const(Ty::I64, 0);
                for asc in &timed {
                    let x = Ex::Conv(Ty::Bool, Ty::I64, Box::new(Ex::Load(Ty::Bool, Loc::Ip(layout.steps[asc.step].x as u32))));
                    count = Ex::Bin(BinOp::Add, Ty::I64, Box::new(count), Box::new(x));
                }
                out.push(St::If(
                    Ex::Cmp(CmpOp::Gt, Ty::I64, Box::new(count), Box::new(Ex::Const(Ty::I64, 1))),
                    vec![St::Trap(TrapCode::ActionControlContention)],
                    vec![],
                ));
            }
            for asc in list {
                let _ = asc.span;
            }
            out.push(St::CallFb { routine: ac_routine, inst: Ex::Addr(ctrl.clone()) });
        }
        // execute actions
        for a in &layout.actions {
            let ctrl = Loc::Ip(a.control as u32);
            let qoff = ac_info.var("Q").unwrap().offset();
            let aoff = ac_info.var("A").unwrap().offset();
            if let Some(decl) = action_decl.get(&a.name) {
                if self.cx.pous[self.pou].var_map.contains_key(&a.name) {
                    self.cx.error_clause("2.6.4.1", format!("the action name {} is already used for a variable", a.name), decl.name.span);
                }
                let mut body = Vec::new();
                let saved_ret = self.return_label;
                let end = self.label();
                self.return_label = end;
                let saved_region = self.region;
                self.region = Region::Action;
                if self.sfc_lowering.contains(&a.name) {
                    self.cx.error_clause("2.6.4.1", format!("the action {} includes a chart that controls the action itself", decl.name), decl.name.span);
                } else {
                    self.sfc_lowering.push(a.name.clone());
                    match crate::visit::body_sfc(&decl.body) {
                        // table 42 feature 2s
                        Some(chart) => {
                            let graphic = matches!(decl.body, Body::Graphic(_));
                            let saved_lang = self.language;
                            if graphic {
                                self.language = Lang::Graphic;
                            }
                            self.sfc_chart(&mut body, chart, graphic, Some(Ex::Load(Ty::Bool, ctrl.clone().offset(qoff))));
                            self.language = saved_lang;
                        }
                        None => self.action_body(&mut body, &decl.body),
                    }
                    self.sfc_lowering.pop();
                }
                self.region = saved_region;
                self.return_label = saved_ret;
                body.push(St::Label(end));
                out.push(St::If(Ex::Load(Ty::Bool, ctrl.clone().offset(aoff)), body, vec![]));
            } else {
                // Boolean variable action (table 42 feature 1)
                let v = self.cx.pous[self.pou].var(&a.name).cloned();
                match v {
                    Some(v) if self.cx.types.is_bool(v.ty) && matches!(v.kind, VKind::Local | VKind::Output | VKind::Global) => {
                        if v.constant {
                            self.cx.annex_e("2.4.3", format!("{} is declared CONSTANT and cannot be used as an action", v.name), v.span);
                        }
                        out.push(St::Store(Ty::Bool, v.loc(), Ex::Load(Ty::Bool, ctrl.clone().offset(qoff))));
                    }
                    Some(v) => self.cx.error_clause("2.6.4.1", format!("{} is not a BOOL variable declared in VAR or VAR_OUTPUT and cannot be an action", v.name), v.span),
                    None => {
                        let span = sfc.steps.iter().flat_map(|s| s.associations.iter()).find(|x| x.action.key() == a.name).map(|x| x.action.span).unwrap_or(sfc.span);
                        self.cx.error_clause("2.6.4.1", format!("unknown action {}", a.name), span);
                    }
                }
            }
        }

        // ---------------------------------------------------------- 3. evolution
        // (an included chart evolves only while its action is active)
        let evolution_start = out.len();
        // conditions (no side effects allowed, 2.6.3)
        let n = transitions.len();
        let mut conds = Vec::new();
        for (t, _, _) in &transitions {
            let cond_src: &ast::Transition = match (&t.condition, &t.name) {
                (TransitionCondition::Named, Some(name)) => match named.get(&name.key()) {
                    Some(d) => d,
                    None => {
                        self.cx.error_clause("2.6.3", format!("no TRANSITION definition named {}", name), name.span);
                        conds.push(Ex::bool(false));
                        continue;
                    }
                },
                _ => t,
            };
            let tmp = self.temp(8);
            let saved_region = self.region;
            self.region = Region::Transition;
            let saved_lang = self.language;
            let mut code = Vec::new();
            let ex = match &cond_src.condition {
                TransitionCondition::St(e) => {
                    self.language = Lang::St;
                    let b = self.bool_expr(e);
                    Self::check_pure(self, &b, e.span);
                    b
                }
                TransitionCondition::Il(list) => {
                    self.language = Lang::Il;
                    self.il_condition(&mut code, list, cond_src.span)
                }
                TransitionCondition::Graphic(g) => {
                    self.language = Lang::Graphic;
                    self.graphic_condition(&mut code, g, cond_src.name.as_ref())
                }
                TransitionCondition::Named => Ex::bool(false),
            };
            self.language = saved_lang;
            self.region = saved_region;
            code.push(St::Store(Ty::Bool, tmp.clone(), ex));
            out.extend(code);
            conds.push(Ex::Load(Ty::Bool, tmp));
        }
        // priority order: explicit PRIORITY / branch numbers, then
        // declaration (left to right) order
        let mut order: Vec<usize> = (0..n).collect();
        let mut prio = Vec::new();
        for (t, _, _) in &transitions {
            let p = match &t.priority {
                Some(e) => self.cx.const_int(e, None).unwrap_or(0) as i64,
                None => t.branch_priority.map(|b| b as i64).unwrap_or(0),
            };
            prio.push(p);
        }
        order.sort_by_key(|i| (prio[*i], *i));
        // per-step "consumed" flags for this evolution
        let consumed: Vec<Loc> = (0..layout.steps.len()).map(|_| self.temp(8)).collect();
        for c in &consumed {
            out.push(St::Store(Ty::Bool, c.clone(), Ex::bool(false)));
        }
        let cleared: Vec<Loc> = (0..n).map(|_| self.temp(8)).collect();
        for &i in &order {
            let (t, from, _) = &transitions[i];
            let mut enabled = conds[i].clone();
            for &s in from {
                enabled = Ex::and(enabled, Ex::Load(Ty::Bool, Loc::Ip(layout.steps[s].x as u32)));
                enabled = Ex::and(enabled, Ex::not(Ex::Load(Ty::Bool, consumed[s].clone())));
            }
            let mut set = vec![St::Store(Ty::Bool, cleared[i].clone(), Ex::bool(true))];
            for &s in from {
                set.push(St::Store(Ty::Bool, consumed[s].clone(), Ex::bool(true)));
            }
            // Non-prioritised simultaneous transitions (table 46 2c): error.
            if t.branch_priority.is_none() && self.is_graphic_unprioritised(t) {
                let mut others = Ex::bool(false);
                for (j, (_, f2, _)) in transitions.iter().enumerate() {
                    if j != i && f2.iter().any(|s| from.contains(s)) {
                        others = Ex::or(others, Ex::Load(Ty::Bool, cleared[j].clone()));
                    }
                }
                set.insert(0, St::If(others, vec![St::Trap(TrapCode::SimultaneousTransitions)], vec![]));
            }
            out.push(St::Store(Ty::Bool, cleared[i].clone(), Ex::bool(false)));
            out.push(St::If(enabled, set, vec![]));
        }
        // deactivation of predecessor steps
        for (i, (_, from, _)) in transitions.iter().enumerate() {
            let mut body = Vec::new();
            for &s in from {
                let st = &layout.steps[s];
                body.push(St::Store(time, Loc::Ip(st.t as u32), Ex::Bin(BinOp::Sub, time, Box::new(Ex::Now), Box::new(Ex::Load(time, Loc::Ip(st.start as u32))))));
                body.push(St::Store(Ty::Bool, Loc::Ip(st.x as u32), Ex::bool(false)));
            }
            out.push(St::If(Ex::Load(Ty::Bool, cleared[i].clone()), body, vec![]));
        }
        // activation of successor steps (an already active step means an
        // unsafe SFC, 2.6.5)
        for (i, (_, _, to)) in transitions.iter().enumerate() {
            let mut body = Vec::new();
            for &s in to {
                let st = &layout.steps[s];
                body.push(St::If(Ex::Load(Ty::Bool, Loc::Ip(st.x as u32)), vec![St::Trap(TrapCode::UnsafeSfc)], vec![]));
                body.push(St::Store(Ty::Bool, Loc::Ip(st.x as u32), Ex::bool(true)));
                body.push(St::Store(time, Loc::Ip(st.start as u32), Ex::Now));
                body.push(St::Store(time, Loc::Ip(st.t as u32), Ex::Const(time, 0)));
            }
            out.push(St::If(Ex::Load(Ty::Bool, cleared[i].clone()), body, vec![]));
        }
        if let Some(q) = enclosing {
            let evolution: Vec<St> = out.drain(evolution_start..).collect();
            out.push(St::If(q, evolution, vec![]));
        }
        self.graphic_2c = saved_2c;
    }

    fn is_graphic_unprioritised(&self, t: &ast::Transition) -> bool {
        // Only graphical selection divergences drawn without '*' (feature
        // 2c) carry no priority; textual transitions are prioritised in
        // declaration order.
        t.branch_priority.is_none() && t.priority.is_none() && self.language == Lang::Graphic && self.graphic_2c
    }

    fn check_pure(&mut self, _e: &Ex, _span: Span) {}

    fn action_body(&mut self, out: &mut Vec<St>, body: &Body) {
        match body {
            Body::Empty => {}
            Body::St(s) => {
                let saved = self.language;
                self.language = Lang::St;
                self.stmts(out, s);
                self.language = saved;
            }
            Body::Il(l) => {
                let saved = self.language;
                self.language = Lang::Il;
                self.il_body(out, l);
                self.language = saved;
            }
            Body::Graphic(g) => {
                let saved = self.language;
                self.language = Lang::Graphic;
                self.graphic_body(out, g);
                self.language = saved;
            }
            // charts included in actions are lowered by sfc_chart
            Body::Sfc(_) => {}
        }
    }

    /// Structural checks of 2.6.2 and 2.6.5: exactly one initial step per
    /// SFC network, and possible unsafe or unreachable behaviour.
    fn sfc_static_checks(&mut self, layout: &crate::ctx::SfcLayout, transitions: &[(&ast::Transition, Vec<usize>, Vec<usize>)], sfc: &Sfc) {
        let ns = layout.steps.len();
        // networks: connected components of steps through transitions
        let mut parent: Vec<usize> = (0..ns).collect();
        fn find(p: &mut [usize], x: usize) -> usize {
            let mut r = x;
            while p[r] != r {
                r = p[r];
            }
            let mut y = x;
            while p[y] != r {
                let n = p[y];
                p[y] = r;
                y = n;
            }
            r
        }
        for (_, from, to) in transitions {
            let all: Vec<usize> = from.iter().chain(to.iter()).copied().collect();
            for w in all.windows(2) {
                let (a, b) = (find(&mut parent, w[0]), find(&mut parent, w[1]));
                parent[a] = b;
            }
        }
        let mut groups: HashMap<usize, Vec<usize>> = HashMap::new();
        for s in 0..ns {
            let r = find(&mut parent, s);
            groups.entry(r).or_default().push(s);
        }
        let step_span = |name: &str| sfc.steps.iter().find(|s| s.name.key() == name).map(|s| s.name.span).unwrap_or(sfc.span);
        for members in groups.values() {
            let initial: Vec<usize> = members.iter().copied().filter(|s| layout.steps[*s].initial).collect();
            if initial.len() != 1 {
                let span = step_span(&layout.steps[members[0]].name);
                self.cx.annex_e(
                    "2.6.2",
                    format!("an SFC network shall contain exactly one initial step (found {})", initial.len()),
                    span,
                );
            }
        }
        // Explore the markings reachable when every enabled transition may
        // fire (conditions ignored).
        if ns == 0 || ns > 128 {
            return;
        }
        let initial: u128 = layout.steps.iter().enumerate().filter(|(_, s)| s.initial).fold(0u128, |m, (i, _)| m | (1 << i));
        let masks: Vec<(u128, u128)> = transitions
            .iter()
            .map(|(_, f, t)| (f.iter().fold(0u128, |m, s| m | (1 << s)), t.iter().fold(0u128, |m, s| m | (1 << s))))
            .collect();
        let mut seen: HashSet<u128> = HashSet::new();
        let mut stack = vec![initial];
        let mut fired = vec![false; transitions.len()];
        let mut reached: u128 = initial;
        let mut unsafe_t: Option<usize> = None;
        seen.insert(initial);
        while let Some(m) = stack.pop() {
            if seen.len() > 20_000 {
                return; // too large to analyse exhaustively
            }
            for (i, (pre, post)) in masks.iter().enumerate() {
                if m & pre == *pre {
                    fired[i] = true;
                    let rest = m & !pre;
                    if rest & post != 0 && unsafe_t.is_none() {
                        unsafe_t = Some(i);
                    }
                    let next = rest | post;
                    reached |= next;
                    if seen.insert(next) {
                        stack.push(next);
                    }
                }
            }
        }
        if let Some(i) = unsafe_t {
            self.cx.annex_e("2.6.5", "unsafe SFC: this transition can activate a step that is already active (uncontrolled proliferation of tokens)", transitions[i].0.span);
        }
        for (i, f) in fired.iter().enumerate() {
            if !f {
                self.cx.annex_e("2.6.5", "unreachable SFC: this transition can never be enabled because its predecessor steps cannot be active simultaneously", transitions[i].0.span);
            }
        }
        for (i, s) in layout.steps.iter().enumerate() {
            if reached & (1 << i) == 0 {
                self.cx.annex_e("2.6.5", format!("unreachable SFC: the step {} can never become active", s.name), step_span(&s.name));
            }
        }
    }
}
