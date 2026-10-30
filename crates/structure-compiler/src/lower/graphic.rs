//! LD and FBD networks (clause 4).
//!
//! Networks are evaluated top to bottom (4.2.6, and the documented order for
//! FBD, 4.3.3); jumps and returns take effect when the evaluation of their
//! network is complete (4.1.3 rule 3). Within a network, an element is
//! evaluated once all its inputs are known (4.1.3 rules 1 and 2); among
//! ready elements the one drawn highest, then leftmost, goes first. Every
//! net holds one value: the power flow (LD) or the signal (FBD); several
//! drivers form a wired OR (4.2.2). Feedback paths (4.1.3) are broken at a
//! net that is connected to a variable, whose current value is used.

use super::*;
use std::collections::{HashMap, HashSet};
use structure_syntax::ast::{self, Arg, CallExpr, Expr, ExprKind, GraphicBody};
use structure_syntax::graphic::{BlockPin, CoilKind, ContactKind, Net, Network, Node, NodeKind, PinRef};
use structure_syntax::Ident;

#[derive(Clone, Default)]
struct NetVal {
    /// Synthetic variable holding the value.
    var: Option<String>,
    loc: Option<Loc>,
    ty: Option<TypeId>,
    /// Number of drivers that have stored their value.
    driven: usize,
    /// Validity flag (FALSE when the driving function was not executed).
    valid: Option<Loc>,
    /// The variable carried by the net: a function block instance, or the
    /// variable of an in-out connection (figure 12 c).
    reference: Option<Expr>,
}

/// Network after merging continuations through connectors.
struct Unit {
    label: Option<Ident>,
    nodes: Vec<Node>,
    nets: Vec<Net>,
}

fn symbol_function(name: &str) -> Option<&'static str> {
    Some(match name {
        "&" => "AND",
        ">=1" => "OR",
        "=2K+1" => "XOR",
        "+" => "ADD",
        "*" => "MUL",
        "-" => "SUB",
        "/" => "DIV",
        "**" => "EXPT",
        ":=" => "MOVE",
        "<" => "LT",
        ">" => "GT",
        "=" => "EQ",
        "<=" => "LE",
        ">=" => "GE",
        "<>" => "NE",
        "T---0" => "TON",
        "0---T" => "TOF",
        _ => return None,
    })
}

impl<'c, 's> Lowerer<'c, 's> {
    pub fn graphic_body(&mut self, out: &mut Vec<St>, g: &GraphicBody) {
        let units = self.merge_connectors(&g.networks);
        // network labels
        let mut labels: HashMap<String, u32> = HashMap::new();
        for u in &units {
            if let Some(l) = &u.label {
                if labels.insert(l.key(), self.label()).is_some() {
                    self.cx.error_clause("4.1.2", format!("duplicate network label {}", l), l.span);
                }
                if self.cx.pous[self.pou].var_map.contains_key(&l.key()) {
                    self.cx.error_clause("4.1.2", format!("the network label {} is already used for a variable", l), l.span);
                }
            }
        }
        for u in &units {
            if let Some(l) = &u.label {
                out.push(St::Label(labels[&l.key()]));
            }
            self.network(out, u, &labels, None);
        }
    }

    /// Evaluates a network giving a transition condition (2.6.3 features 2,
    /// 3, 4 and 7): the value written to the transition name, or the power
    /// flow / signal reaching the right end of the network.
    pub fn graphic_condition(&mut self, out: &mut Vec<St>, g: &GraphicBody, name: Option<&Ident>) -> Ex {
        let b = self.bool_ty();
        let result = self.temp(8);
        let key = name.map(|n| n.key()).unwrap_or_else(|| "$TRANSITION".into());
        self.synth.insert(key.clone(), (result.clone(), b));
        let units = self.merge_connectors(&g.networks);
        let labels = HashMap::new();
        for u in &units {
            self.network(out, u, &labels, Some(&result));
        }
        self.synth.remove(&key);
        Ex::Load(Ty::Bool, result)
    }

    /// Joins networks linked by connectors (table 57 feature 13).
    fn merge_connectors(&mut self, nets: &[Network]) -> Vec<Unit> {
        let n = nets.len();
        let mut parent: Vec<usize> = (0..n).collect();
        fn find(p: &mut [usize], x: usize) -> usize {
            let mut r = x;
            while p[r] != r {
                r = p[r];
            }
            p[x] = r;
            r
        }
        let mut outs: HashMap<String, usize> = HashMap::new();
        for (i, net) in nets.iter().enumerate() {
            for node in &net.nodes {
                if let NodeKind::ConnectorOut(c) = &node.kind {
                    if outs.insert(c.key(), i).is_some() {
                        self.cx.error_clause("4.1.1", format!("duplicate connector {}", c), c.span);
                    }
                    if self.cx.pous[self.pou].var_map.contains_key(&c.key()) || self.cx.pou_by_name.contains_key(&c.key()) {
                        self.cx.annex_e("4.1.1", format!("the connector label {} is the same as the name of another element", c), c.span);
                    }
                }
            }
        }
        for (i, net) in nets.iter().enumerate() {
            for node in &net.nodes {
                if let NodeKind::ConnectorIn(c) = &node.kind {
                    match outs.get(&c.key()) {
                        Some(&j) => {
                            let (a, b) = (find(&mut parent, i), find(&mut parent, j));
                            parent[a] = b;
                        }
                        None => self.cx.error_clause("4.1.1", format!("connector {} has no source", c), c.span),
                    }
                }
            }
        }
        let mut groups: Vec<Vec<usize>> = Vec::new();
        let mut index: HashMap<usize, usize> = HashMap::new();
        for i in 0..n {
            let r = find(&mut parent, i);
            match index.get(&r) {
                Some(&g) => groups[g].push(i),
                None => {
                    index.insert(r, groups.len());
                    groups.push(vec![i]);
                }
            }
        }
        let mut units = Vec::new();
        for g in groups {
            let mut nodes = Vec::new();
            let mut list = Vec::new();
            let mut label = None;
            for &i in &g {
                let base = nodes.len();
                if label.is_none() {
                    label = nets[i].label.clone();
                }
                nodes.extend(nets[i].nodes.iter().cloned());
                for net in &nets[i].nets {
                    list.push(Net {
                        drivers: net.drivers.iter().map(|p| PinRef { node: p.node + base, pin: p.pin }).collect(),
                        sinks: net.sinks.iter().map(|p| PinRef { node: p.node + base, pin: p.pin }).collect(),
                    });
                }
            }
            // connector out -> connector in: one net
            let mut by_name: HashMap<String, (usize, usize)> = HashMap::new();
            for (ni, node) in nodes.iter().enumerate() {
                match &node.kind {
                    NodeKind::ConnectorOut(c) => by_name.entry(c.key()).or_insert((usize::MAX, usize::MAX)).0 = ni,
                    NodeKind::ConnectorIn(c) => by_name.entry(c.key()).or_insert((usize::MAX, usize::MAX)).1 = ni,
                    _ => {}
                }
            }
            for (_, (o, i)) in by_name {
                if o == usize::MAX || i == usize::MAX {
                    continue;
                }
                let into = list.iter().position(|n| n.sinks.contains(&PinRef { node: o, pin: 0 }));
                let from = list.iter().position(|n| n.drivers.contains(&PinRef { node: i, pin: 0 }));
                if let (Some(a), Some(b)) = (into, from) {
                    if a != b {
                        let moved = list[b].clone();
                        list[a].sinks.extend(moved.sinks.into_iter());
                        list[a].drivers.extend(moved.drivers.into_iter().filter(|p| p.node != i));
                        list[b] = Net::default();
                    }
                }
            }
            // nets driven by the same output pin carry the same signal
            for a in 0..list.len() {
                for b in a + 1..list.len() {
                    if list[b].drivers.iter().any(|d| list[a].drivers.contains(d)) {
                        let moved = std::mem::take(&mut list[b]);
                        for d in moved.drivers {
                            if !list[a].drivers.contains(&d) {
                                list[a].drivers.push(d);
                            }
                        }
                        list[a].sinks.extend(moved.sinks);
                    }
                }
            }
            list.retain(|n| !n.drivers.is_empty() || !n.sinks.is_empty());
            units.push(Unit { label, nodes, nets: list });
        }
        units
    }

    fn network(&mut self, out: &mut Vec<St>, u: &Unit, labels: &HashMap<String, u32>, transition: Option<&Loc>) {
        let n = u.nodes.len();
        let mut input_net: HashMap<(usize, usize), usize> = HashMap::new();
        let mut output_net: HashMap<(usize, usize), usize> = HashMap::new();
        for (i, net) in u.nets.iter().enumerate() {
            for s in &net.sinks {
                input_net.insert((s.node, s.pin), i);
            }
            for d in &net.drivers {
                output_net.insert((d.node, d.pin), i);
            }
            if net.drivers.len() > 1 && self.language == Lang::Graphic && !self.is_ld(u) {
                self.cx.error_clause("4.3.2", "outputs of function blocks shall not be connected together in FBD (use an explicit OR block)", u.nodes[net.drivers[0].node].span);
            }
        }
        // dependencies
        let mut deps: Vec<HashSet<usize>> = vec![HashSet::new(); n];
        for net in &u.nets {
            for s in &net.sinks {
                for d in &net.drivers {
                    if d.node != s.node {
                        deps[s.node].insert(d.node);
                    }
                }
            }
        }
        let mut vals: Vec<NetVal> = vec![NetVal::default(); u.nets.len()];
        // pre-valued nets (feedback variables)
        let mut done = vec![false; n];
        let mut order = Vec::new();
        loop {
            let mut ready: Vec<usize> = (0..n).filter(|i| !done[*i] && deps[*i].iter().all(|d| done[*d])).collect();
            if ready.is_empty() {
                let remaining: Vec<usize> = (0..n).filter(|i| !done[*i]).collect();
                if remaining.is_empty() {
                    break;
                }
                // Feedback path: use the current value of a feedback variable.
                let mut broken = false;
                let mut cand = remaining.clone();
                cand.sort_by_key(|i| u.nodes[*i].pos);
                // An explicit loop through a variable written in the middle
                // of a line (figure 23 a): the elements after the variable
                // use its current value; it is assigned at the end of the loop.
                'explicit: for &i in &cand {
                    for (&(node, _), &ni) in input_net.iter() {
                        if node != i || vals[ni].var.is_some() {
                            continue;
                        }
                        let var = u.nets[ni].drivers.iter().find_map(|d| match &u.nodes[d.node].kind {
                            NodeKind::Sink(e) if !done[d.node] => Some((d.node, e.clone())),
                            _ => None,
                        });
                        if let Some((sink, e)) = var {
                            let v = self.expr(&e, None);
                            let t = v.ty;
                            if self.cx.types.scalar(t).is_none() {
                                continue;
                            }
                            let loc = self.temp_typed(t);
                            self.assign(out, loc.clone(), t, v, e.span);
                            let name = self.synth_var("N", loc.clone(), t);
                            vals[ni] = NetVal { var: Some(name), loc: Some(loc), ty: Some(t), driven: 0, valid: None, reference: None };
                            for s in &u.nets[ni].sinks {
                                deps[s.node].remove(&sink);
                            }
                            broken = true;
                            break 'explicit;
                        }
                    }
                }
                if broken {
                    continue;
                }
                'outer: for &i in &cand {
                    for (&(node, pin), &ni) in input_net.iter() {
                        if node != i || vals[ni].var.is_some() {
                            continue;
                        }
                        let _ = pin;
                        // a variable connected to the same net
                        let var = u.nets[ni].sinks.iter().find_map(|s| match &u.nodes[s.node].kind {
                            NodeKind::Sink(e) => Some(e.clone()),
                            _ => None,
                        });
                        if let Some(e) = var {
                            if u.nets[ni].drivers.iter().any(|d| !done[d.node]) {
                                let v = self.expr(&e, None);
                                let t = v.ty;
                                if self.cx.types.scalar(t).is_none() {
                                    continue;
                                }
                                let loc = self.temp_typed(t);
                                self.assign(out, loc.clone(), t, v, e.span);
                                let name = self.synth_var("N", loc.clone(), t);
                                vals[ni] = NetVal { var: Some(name), loc: Some(loc), ty: Some(t), driven: 0, valid: None, reference: None };
                                for d in &u.nets[ni].drivers {
                                    deps[i].remove(&d.node);
                                }
                                broken = true;
                                break 'outer;
                            }
                        }
                    }
                }
                // A loop through an output of a function block instance: the
                // output keeps its value from the previous evaluation of the
                // instance (e.g. the BLINK/BLANK timers of figure F.5).
                if !broken {
                    'fb: for &i in &cand {
                        for (&(node, _), &ni) in input_net.iter() {
                            if node != i || vals[ni].var.is_some() {
                                continue;
                            }
                            let Some((bn, inst, type_name, outputs, pin)) = u.nets[ni].drivers.iter().find_map(|d| match &u.nodes[d.node].kind {
                                NodeKind::Block { instance: Some(inst), type_name, outputs, .. } if !done[d.node] => {
                                    Some((d.node, inst.clone(), type_name.clone(), outputs.clone(), d.pin))
                                }
                                _ => None,
                            }) else {
                                continue;
                            };
                            let name = match outputs.get(pin).and_then(|p| p.name.clone()) {
                                Some(n) if n.is("ENO") => continue,
                                Some(n) => n.key(),
                                None => {
                                    let k = outputs[..pin].iter().filter(|p| p.name.is_none()).count();
                                    let Some(&fb) = self.cx.pou_by_name.get(&type_name.key()) else { continue };
                                    let declared: Vec<String> =
                                        self.cx.pous[fb].vars.iter().filter(|v| v.kind == VKind::Output && v.name != "ENO" && !v.implicit).map(|v| v.name.clone()).collect();
                                    match declared.get(k) {
                                        Some(n) => n.clone(),
                                        None => continue,
                                    }
                                }
                            };
                            let fe = Expr {
                                kind: ExprKind::Field { base: Box::new(Expr { kind: ExprKind::Name(inst.clone()), span: inst.span }), field: Ident::new(name, inst.span) },
                                span: inst.span,
                            };
                            let v = self.expr(&fe, None);
                            let t = v.ty;
                            if self.cx.types.scalar(t).is_none() {
                                continue;
                            }
                            let loc = self.temp_typed(t);
                            self.assign(out, loc.clone(), t, v, inst.span);
                            let var = self.synth_var("N", loc.clone(), t);
                            vals[ni] = NetVal { var: Some(var), loc: Some(loc), ty: Some(t), driven: 0, valid: None, reference: None };
                            for sk in &u.nets[ni].sinks {
                                deps[sk.node].remove(&bn);
                            }
                            broken = true;
                            break 'fb;
                        }
                    }
                }
                if !broken {
                    let span = u.nodes[cand[0]].span;
                    self.cx.annex_e("4.1.3", "feedback path without a feedback variable: a variable shall be connected to the loop so that it can be initialized", span);
                    break;
                }
                continue;
            }
            ready.sort_by_key(|i| u.nodes[*i].pos);
            let i = ready[0];
            done[i] = true;
            order.push(i);
        }
        // evaluation
        let mut post = Vec::new();
        for i in order {
            let node = &u.nodes[i];
            out.push(St::At(node.span));
            self.node(out, &mut post, u, i, node, &input_net, &output_net, &mut vals, labels, transition);
        }
        out.extend(post);
    }

    fn is_ld(&self, u: &Unit) -> bool {
        u.nodes.iter().any(|n| matches!(n.kind, NodeKind::LeftRail | NodeKind::Contact { .. } | NodeKind::Coil { .. }))
    }

    /// Value of an input pin (BOOL FALSE if unconnected power flow).
    fn pin_value(&mut self, node: usize, pin: usize, input_net: &HashMap<(usize, usize), usize>, vals: &[NetVal]) -> Option<(Ex, TypeId, String)> {
        let ni = *input_net.get(&(node, pin))?;
        let v = &vals[ni];
        let (loc, ty, var) = (v.loc.clone()?, v.ty?, v.var.clone()?);
        Some((Ex::Load(self.mty(ty), loc), ty, var))
    }

    /// Stores an output value into its net (OR for several drivers).
    fn drive(&mut self, out: &mut Vec<St>, ni: usize, v: V, vals: &mut [NetVal], u: &Unit, span: Span) {
        let drivers = u.nets[ni].drivers.len();
        if vals[ni].loc.is_none() {
            let v = self.settle(v, span);
            let t = v.ty;
            if self.cx.types.is_error(t) {
                return;
            }
            let loc = self.temp_typed(t);
            let name = self.synth_var("N", loc.clone(), t);
            self.assign(out, loc.clone(), t, v, span);
            vals[ni] = NetVal { var: Some(name), loc: Some(loc), ty: Some(t), driven: 1, valid: None, reference: None };
            return;
        }
        let (loc, t) = (vals[ni].loc.clone().unwrap(), vals[ni].ty.unwrap());
        if drivers > 1 && vals[ni].driven > 0 {
            if !self.cx.types.is_bool(t) {
                self.cx.error_clause("4.2.2", "only BOOL signals can be connected together (wired OR)", span);
                return;
            }
            let b = self.bool_ty();
            let ex = self.coerce(v, b, span);
            out.push(St::Store(Ty::Bool, loc.clone(), Ex::or(Ex::Load(Ty::Bool, loc), ex)));
        } else {
            self.assign(out, loc, t, v, span);
        }
        vals[ni].driven += 1;
    }

    /// Pre-allocates the net of an output pin with a known type.
    fn net_target(&mut self, ni: usize, ty: TypeId, vals: &mut [NetVal]) -> String {
        if let Some(v) = &vals[ni].var {
            return v.clone();
        }
        let loc = self.temp_typed(ty);
        let name = self.synth_var("N", loc.clone(), ty);
        vals[ni] = NetVal { var: Some(name.clone()), loc: Some(loc), ty: Some(ty), driven: 0, valid: None, reference: None };
        name
    }

    fn edge_memory(&mut self, node: &Node) -> Option<Loc> {
        let name = crate::visit::edge_memory_name(node);
        self.cx.pous[self.pou].var(&name).map(|v| v.loc())
    }

    #[allow(clippy::too_many_arguments)]
    fn node(
        &mut self,
        out: &mut Vec<St>,
        post: &mut Vec<St>,
        u: &Unit,
        idx: usize,
        node: &Node,
        input_net: &HashMap<(usize, usize), usize>,
        output_net: &HashMap<(usize, usize), usize>,
        vals: &mut [NetVal],
        labels: &HashMap<String, u32>,
        transition: Option<&Loc>,
    ) {
        let span = node.span;
        let bool_ty = self.bool_ty();
        let power_in = |s: &mut Self, vals: &[NetVal]| -> Ex {
            match s.pin_value(idx, 0, input_net, vals) {
                Some((ex, t, _)) if s.cx.types.is_bool(t) => ex,
                Some((ex, _, _)) => ex,
                None => Ex::bool(false),
            }
        };
        match &node.kind {
            NodeKind::LeftRail => {
                for (&(n, p), &ni) in output_net.iter() {
                    if n == idx {
                        let _ = p;
                        self.drive(out, ni, V::val(Ex::bool(true), bool_ty), vals, u, span);
                    }
                }
            }
            NodeKind::RightRail => {
                if let Some(t) = transition {
                    let v = power_in(self, vals);
                    out.push(St::Store(Ty::Bool, t.clone(), v));
                }
            }
            NodeKind::Source(e) => {
                if let Some(&ni) = output_net.get(&(idx, 0)) {
                    let v = self.expr(e, None);
                    if self.cx.types.instance_pou(v.ty).is_some() {
                        vals[ni].reference = Some(e.clone());
                        return;
                    }
                    let power = u.nets[ni].sinks.iter().any(|s| {
                        matches!(
                            u.nodes[s.node].kind,
                            NodeKind::Contact { .. } | NodeKind::Coil { .. } | NodeKind::Jump(_) | NodeKind::Return | NodeKind::RightRail | NodeKind::ActionBlock { .. }
                        )
                    });
                    let v = match &v.kind {
                        // a literal feeding power flow is BOOL
                        VK::IntLit(_) if power => {
                            let ex = self.coerce(v, bool_ty, e.span);
                            V::val(ex, bool_ty)
                        }
                        _ => v,
                    };
                    self.drive(out, ni, v, vals, u, span);
                }
            }
            NodeKind::Sink(e) => {
                let Some((ex, t, var)) = self.pin_value(idx, 0, input_net, vals) else { return };
                let ni = input_net[&(idx, 0)];
                let valid = vals[ni].valid.clone();
                // the transition name used as a variable
                if let ExprKind::Name(n) = &e.kind {
                    if let (Some(tl), true) = (transition, self.synth.contains_key(&n.key())) {
                        let b = self.coerce(V::val(ex, t), bool_ty, span);
                        out.push(St::Store(Ty::Bool, tl.clone(), b));
                        return;
                    }
                }
                let mut code = Vec::new();
                let value = Self::synth_expr(&var, span);
                self.assignment(&mut code, e, &value);
                match valid {
                    Some(f) => out.push(St::If(Ex::Load(Ty::Bool, f), code, vec![])),
                    None => out.extend(code),
                }
                // a variable in the middle of a line (`--PTR--`) also drives
                // the rest of the line with its new value
                if let Some(&no) = output_net.get(&(idx, 0)) {
                    let v = self.expr(e, None);
                    self.drive(out, no, v, vals, u, span);
                }
            }
            NodeKind::Contact { var, kind } => {
                let pin = power_in(self, vals);
                let b = self.bool_expr(var);
                let cond = match kind {
                    ContactKind::NormallyOpen => b,
                    ContactKind::NormallyClosed => Ex::not(b),
                    ContactKind::Positive | ContactKind::Negative => {
                        let Some(mem) = self.edge_memory(node) else { return };
                        let cur = self.temp(8);
                        out.push(St::Store(Ty::Bool, cur.clone(), b));
                        let (now, prev) = (Ex::Load(Ty::Bool, cur.clone()), Ex::Load(Ty::Bool, mem.clone()));
                        let edge = if *kind == ContactKind::Positive { Ex::and(now.clone(), Ex::not(prev)) } else { Ex::and(Ex::not(now.clone()), prev) };
                        let e = self.temp(8);
                        out.push(St::Store(Ty::Bool, e.clone(), edge));
                        out.push(St::Store(Ty::Bool, mem, now));
                        Ex::Load(Ty::Bool, e)
                    }
                };
                if let Some(&ni) = output_net.get(&(idx, 0)) {
                    self.drive(out, ni, V::val(Ex::and(pin, cond), bool_ty), vals, u, span);
                }
            }
            NodeKind::Coil { var, kind } => {
                let pin = power_in(self, vals);
                let p = self.temp(8);
                out.push(St::Store(Ty::Bool, p.clone(), pin));
                let pv = Ex::Load(Ty::Bool, p.clone());
                let tname = match &var.kind {
                    ExprKind::Name(n) if transition.is_some() && self.synth.contains_key(&n.key()) => Some(n.key()),
                    _ => None,
                };
                if let (Some(_), Some(tl)) = (tname, transition) {
                    out.push(St::Store(Ty::Bool, tl.clone(), pv.clone()));
                } else {
                    let Some(place) = self.target(var) else { return };
                    if !self.cx.types.is_bool(place.ty) {
                        self.cx.error_clause("4.2.4", "the variable of a coil shall be of type BOOL", var.span);
                    }
                    let set = |v: Ex| St::Store(Ty::Bool, place.loc.clone(), v);
                    match kind {
                        CoilKind::Normal => out.push(set(pv.clone())),
                        CoilKind::Negated => out.push(set(Ex::not(pv.clone()))),
                        CoilKind::Set => out.push(St::If(pv.clone(), vec![set(Ex::bool(true))], vec![])),
                        CoilKind::Reset => out.push(St::If(pv.clone(), vec![set(Ex::bool(false))], vec![])),
                        CoilKind::Positive | CoilKind::Negative => {
                            if let Some(mem) = self.edge_memory(node) {
                                let prev = Ex::Load(Ty::Bool, mem.clone());
                                let edge = if *kind == CoilKind::Positive { Ex::and(pv.clone(), Ex::not(prev)) } else { Ex::and(Ex::not(pv.clone()), prev) };
                                out.push(set(edge));
                                out.push(St::Store(Ty::Bool, mem, pv.clone()));
                            }
                        }
                    }
                    if place.is_result {
                        self.mark_assigned(out);
                    }
                }
                if let Some(&ni) = output_net.get(&(idx, 0)) {
                    self.drive(out, ni, V::val(pv, bool_ty), vals, u, span);
                }
            }
            NodeKind::Jump(label) => {
                let c = power_in(self, vals);
                let t = self.temp(8);
                out.push(St::Store(Ty::Bool, t.clone(), c));
                match labels.get(&label.key()) {
                    Some(&l) => post.push(St::JumpIf(Ex::Load(Ty::Bool, t), l)),
                    None => self.cx.error_clause("4.1.4", format!("the jump target {} is not a network label of this program organization unit", label), label.span),
                }
            }
            NodeKind::Return => {
                if self.region == Region::Transition {
                    self.cx.error("RETURN is not allowed in a transition condition", span);
                    return;
                }
                let c = power_in(self, vals);
                let t = self.temp(8);
                out.push(St::Store(Ty::Bool, t.clone(), c));
                post.push(St::JumpIf(Ex::Load(Ty::Bool, t), self.return_label));
            }
            NodeKind::ConnectorOut(_) | NodeKind::ConnectorIn(_) => {
                // merged into one net by merge_connectors
            }
            NodeKind::ActionBlock { action, indicator, .. } => {
                // table 44 features 8, 9: the input of the block is an
                // association of the action (evaluated by the action control)
                if self.region == Region::Transition {
                    self.cx.error_clause("2.6.3", "action blocks are not allowed in transition conditions", span);
                    return;
                }
                let pin = power_in(self, vals);
                if let Some(v) = self.cx.pous[self.pou].var(&crate::visit::action_block_flag_name(node)).cloned() {
                    out.push(St::Store(Ty::Bool, v.loc(), pin));
                }
                // the output is the indicator variable, or the action itself
                // when it is a Boolean variable, otherwise FALSE (2.6.4.3)
                let ind = match indicator {
                    Some(e) => self.bool_expr(e),
                    None => match self.cx.pous[self.pou].var(&action.key()) {
                        Some(v) if self.cx.types.is_bool(v.ty) => Ex::Load(Ty::Bool, v.loc()),
                        _ => Ex::bool(false),
                    },
                };
                if let Some(&ni) = output_net.get(&(idx, 0)) {
                    self.drive(out, ni, V::val(ind, bool_ty), vals, u, span);
                }
            }
            NodeKind::Block { type_name, instance, inputs, outputs } => {
                self.block(out, u, idx, node, type_name, instance.as_ref(), inputs, outputs, input_net, output_net, vals);
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn block(
        &mut self,
        out: &mut Vec<St>,
        u: &Unit,
        idx: usize,
        node: &Node,
        type_name: &Ident,
        instance: Option<&Ident>,
        inputs: &[BlockPin],
        outputs: &[BlockPin],
        input_net: &HashMap<(usize, usize), usize>,
        output_net: &HashMap<(usize, usize), usize>,
        vals: &mut [NetVal],
    ) {
        let span = node.span;
        let raw = type_name.text.to_uppercase();
        let fname = symbol_function(&raw).map(|s| s.to_string()).unwrap_or(raw.clone());
        let callee_ident = Ident::new(fname.clone(), type_name.span);
        let pou = self.cx.pou_by_name.get(&fname).copied();
        let is_fb = match pou {
            Some(p) => self.cx.pous[p].kind != PouKind::Function,
            None => false,
        };
        // Input arguments
        let mut args: Vec<Arg> = Vec::new();
        let mut positional = 0usize;
        let declared_inputs: Vec<String> = match pou {
            Some(p) => self.cx.pous[p].vars.iter().filter(|v| v.kind == VKind::Input && v.name != "EN" && !v.implicit).map(|v| v.name.clone()).collect(),
            None => vec![],
        };
        let mut has_en = false;
        let in_outs: Vec<String> = match pou {
            Some(p) => self.cx.pous[p].vars.iter().filter(|v| v.kind == VKind::InOut).map(|v| v.name.clone()).collect(),
            None => vec![],
        };
        // actual variables of in-out connections, by name
        let mut in_out_actual: HashMap<String, Expr> = HashMap::new();
        for (k, pin) in inputs.iter().enumerate() {
            let ni = input_net.get(&(idx, k)).copied();
            // A variable or literal connected to the input is passed as such
            // (a literal then takes the type of the input as in the textual
            // languages); so is the variable carried by an in-out connection.
            let direct = ni.and_then(|ni| match u.nets[ni].drivers.as_slice() {
                [d] => match &u.nodes[d.node].kind {
                    NodeKind::Source(e) => Some(e.clone()),
                    _ => None,
                },
                _ => None,
            });
            let direct = direct.or_else(|| ni.and_then(|ni| vals[ni].reference.clone()));
            let in_out = pin.name.as_ref().map(|n| in_outs.contains(&n.key())).unwrap_or(false);
            let (mut value, t) = match &direct {
                Some(e) => (e.clone(), None),
                None => match self.pin_value(idx, k, input_net, vals) {
                    Some((_, t, var)) => (Self::synth_expr(&var, pin.span), Some(t)),
                    None => {
                        if pin.name.is_none() && !is_fb {
                            positional += 1;
                        }
                        continue;
                    }
                },
            };
            if in_out {
                if direct.is_none() || direct.as_ref().map(is_literal).unwrap_or(false) {
                    self.cx.annex_e(
                        "2.5.1.1",
                        format!("the in-out variable {} shall be connected to a variable or to the in-out variable of another block (figure 12)", pin.name.as_ref().unwrap()),
                        pin.span,
                    );
                }
                in_out_actual.insert(pin.name.as_ref().unwrap().key(), value.clone());
            }
            if pin.edge.is_some() {
                self.cx.error_clause("2.5.2.2", "edge markers can only be used in graphical declarations of function block inputs", pin.span);
            }
            if pin.negated {
                if t.map(|t| !self.cx.types.is_bool(t)).unwrap_or(false) {
                    self.cx.error_clause("2.5.1.1", "graphical negation can only be applied to BOOL signals (table 19)", pin.span);
                }
                value = Expr { kind: ExprKind::Unary { op: ast::UnaryOp::Not, operand: Box::new(value) }, span: pin.span };
            }
            match &pin.name {
                Some(n) => {
                    if n.is("EN") {
                        has_en = true;
                    }
                    args.push(Arg::Input { name: n.clone(), value })
                }
                None if is_fb => {
                    let name = declared_inputs.get(positional).cloned();
                    positional += 1;
                    match name {
                        Some(nm) => args.push(Arg::Input { name: Ident::new(nm, pin.span), value }),
                        None => self.cx.error(format!("{} has no input at this position", fname), pin.span),
                    }
                }
                None => {
                    positional += 1;
                    args.push(Arg::Positional(value));
                }
            }
        }
        // A function with formal names for some inputs and positional ones
        // for others: name the positional ones IN1, IN2, ... (2.5.1.1 rule 9a).
        let any_named = args.iter().any(|a| matches!(a, Arg::Input { name, .. } if !name.is("EN")));
        let any_pos = args.iter().any(|a| matches!(a, Arg::Positional(_)));
        if (any_named || has_en) && any_pos {
            let mut k = 0;
            let single = args.iter().filter(|a| matches!(a, Arg::Positional(_))).count() == 1 && inputs.iter().filter(|p| p.name.is_none()).count() == 1;
            let std_single_in = matches!(crate::stdlib::parse(&fname).map(|f| crate::stdlib::params(&f).fixed.to_vec()), Some(v) if v == ["IN"]);
            for a in args.iter_mut() {
                if let Arg::Positional(e) = a {
                    k += 1;
                    let name = if single && std_single_in { "IN".to_string() } else { format!("IN{}", k) };
                    *a = Arg::Input { name: Ident::new(name, e.span), value: e.clone() };
                }
            }
        }
        // ENO output (also used as validity of the other outputs)
        let eno_idx = outputs.iter().position(|p| p.name.as_ref().map(|n| n.is("ENO")).unwrap_or(false));
        let bool_ty = self.bool_ty();
        let mut eno_loc = None;
        if has_en || eno_idx.is_some() {
            let loc = self.temp(8);
            let name = self.synth_var("E", loc.clone(), bool_ty);
            args.push(Arg::Output { name: Ident::new("ENO", span), target: Some(Self::synth_expr(&name, span)), negated: false, span });
            eno_loc = Some(loc);
        }
        if is_fb {
            let Some(inst) = instance else {
                self.cx.error_clause("2.5.2.1", format!("an instance name shall be written above the function block {}", fname), span);
                return;
            };
            let inst_expr = Expr { kind: ExprKind::Name(inst.clone()), span: inst.span };
            match self.place(&inst_expr, false) {
                Some(p) if self.cx.types.instance_pou(p.ty) == pou => {}
                Some(_) => {
                    self.cx.error(format!("{} is not an instance of {}", inst, fname), inst.span);
                    return;
                }
                None => return,
            }
            let call = CallExpr { callee: Box::new(inst_expr.clone()), args, span };
            self.fb_call(out, &call, span);
            let fb = pou.unwrap();
            let declared_outputs: Vec<String> = self.cx.pous[fb].vars.iter().filter(|v| v.kind == VKind::Output && v.name != "ENO" && !v.implicit).map(|v| v.name.clone()).collect();
            let mut pos = 0;
            for (k, pin) in outputs.iter().enumerate() {
                let Some(&ni) = output_net.get(&(idx, k)) else {
                    if pin.name.is_none() {
                        pos += 1;
                    }
                    continue;
                };
                let name = match &pin.name {
                    Some(n) => n.key(),
                    None => {
                        pos += 1;
                        match declared_outputs.get(pos - 1) {
                            Some(n) => n.clone(),
                            None => continue,
                        }
                    }
                };
                if let Some(actual) = in_out_actual.get(&name).cloned() {
                    self.in_out_output(out, u, ni, actual, vals, pin.span);
                    continue;
                }
                let v = if name == "ENO" {
                    match &eno_loc {
                        Some(l) => V::val(Ex::Load(Ty::Bool, l.clone()), bool_ty),
                        None => continue,
                    }
                } else {
                    let fe = Expr { kind: ExprKind::Field { base: Box::new(inst_expr.clone()), field: Ident::new(name, pin.span) }, span: pin.span };
                    self.expr(&fe, None)
                };
                let v = self.negate_pin(v, pin);
                self.drive(out, ni, v, vals, u, span);
            }
            return;
        }
        if instance.is_some() && pou.is_none() && crate::stdlib::parse(&fname).is_some() {
            self.cx.error(format!("{} is a function; functions have no instance name", fname), span);
        }
        // Function: user-declared or standard.
        let result_pin = outputs.iter().position(|p| p.name.as_ref().map(|n| !n.is("ENO")).unwrap_or(true) && !matches!(&p.name, Some(n) if pou.map(|pi| self.cx.pous[pi].var(&n.key()).map(|v| matches!(v.kind, VKind::Output | VKind::InOut)).unwrap_or(false)).unwrap_or(false)));
        // named function outputs
        for (k, pin) in outputs.iter().enumerate() {
            let Some(name) = &pin.name else { continue };
            if name.is("ENO") || Some(k) == result_pin || in_outs.contains(&name.key()) {
                continue;
            }
            let Some(&ni) = output_net.get(&(idx, k)) else { continue };
            let ty = pou.and_then(|p| self.cx.pous[p].var(&name.key()).map(|v| v.ty));
            let Some(ty) = ty else {
                self.cx.error(format!("{} has no output named {}", fname, name), name.span);
                continue;
            };
            let var = self.net_target(ni, ty, vals);
            args.push(Arg::Output { name: name.clone(), target: Some(Self::synth_expr(&var, pin.span)), negated: pin.negated, span: pin.span });
            vals[ni].driven += 1;
        }
        let call = Expr { kind: ExprKind::Call(CallExpr { callee: Box::new(Expr { kind: ExprKind::Name(callee_ident.clone()), span: type_name.span }), args, span }), span };
        let expected = result_pin.and_then(|k| output_net.get(&(idx, k))).and_then(|ni| {
            u.nets[*ni].sinks.iter().find_map(|s| match &u.nodes[s.node].kind {
                NodeKind::Sink(e) => {
                    let saved = self.cx.diags.items.len();
                    let p = self.place(e, false);
                    self.cx.diags.items.truncate(saved);
                    p.map(|p| p.ty)
                }
                _ => None,
            })
        });
        let v = self.expr(&call, expected);
        match result_pin.and_then(|k| output_net.get(&(idx, k)).map(|ni| (k, *ni))) {
            Some((k, ni)) => {
                let v = self.negate_pin(v, &outputs[k]);
                self.drive(out, ni, v, vals, u, span);
                if let Some(l) = &eno_loc {
                    vals[ni].valid = Some(l.clone());
                }
            }
            None => {
                // evaluate for the side effects (outputs, ENO)
                out.extend(v.pre.clone());
                if let VK::Val(ex) = v.kind {
                    out.push(St::Eval(ex));
                }
            }
        }
        if let (Some(k), Some(l)) = (eno_idx, &eno_loc) {
            if let Some(&ni) = output_net.get(&(idx, k)) {
                let v = self.negate_pin(V::val(Ex::Load(Ty::Bool, l.clone()), bool_ty), &outputs[k]);
                self.drive(out, ni, v, vals, u, span);
            }
        }
        // in-out variables of the function
        for (k, pin) in outputs.iter().enumerate() {
            let Some(name) = &pin.name else { continue };
            let (Some(actual), Some(&ni)) = (in_out_actual.get(&name.key()).cloned(), output_net.get(&(idx, k))) else { continue };
            self.in_out_output(out, u, ni, actual, vals, pin.span);
        }
        // named outputs share the validity of ENO
        if let Some(l) = &eno_loc {
            for (k, pin) in outputs.iter().enumerate() {
                if pin.name.is_some() && Some(k) != eno_idx {
                    if let Some(&ni) = output_net.get(&(idx, k)) {
                        vals[ni].valid = Some(l.clone());
                    }
                }
            }
        }
    }

    /// The right-hand connection of an in-out variable carries the variable
    /// connected to its left (figure 12); it may only lead to one variable or
    /// in-out input (2.5.1.1, annex E: ambiguous value).
    fn in_out_output(&mut self, out: &mut Vec<St>, u: &Unit, ni: usize, actual: Expr, vals: &mut [NetVal], span: Span) {
        let sinks = &u.nets[ni].sinks;
        if sinks.len() > 1 {
            self.cx.annex_e("2.5.1.1", "ambiguous value: the output connection of an in-out variable shall lead to a single variable or in-out variable", span);
        }
        if sinks.iter().any(|s| !matches!(u.nodes[s.node].kind, NodeKind::Sink(_) | NodeKind::Block { .. })) {
            self.cx.annex_e("2.5.1.1", "the output connection of an in-out variable can only lead to a variable or to an in-out variable", span);
        }
        let v = self.expr(&actual, None);
        if self.cx.types.instance_pou(v.ty).is_none() {
            self.drive(out, ni, v, vals, u, span);
        }
        vals[ni].reference = Some(actual);
    }

    fn negate_pin(&mut self, v: V, pin: &BlockPin) -> V {
        if !pin.negated {
            return v;
        }
        let b = self.bool_ty();
        if !self.cx.types.is_bool(v.ty) {
            self.cx.error_clause("2.5.1.1", "graphical negation can only be applied to BOOL signals (table 19)", pin.span);
            return v;
        }
        let ex = self.coerce(v, b, pin.span);
        V::val(Ex::not(ex), b)
    }
}

/// A literal, possibly signed.
fn is_literal(e: &Expr) -> bool {
    match &e.kind {
        ExprKind::Literal(_) => true,
        ExprKind::Unary { op: ast::UnaryOp::Neg, operand } => matches!(operand.kind, ExprKind::Literal(_)),
        _ => false,
    }
}
