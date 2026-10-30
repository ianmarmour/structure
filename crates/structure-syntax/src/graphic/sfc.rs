//! Graphical SFC (2.6): steps, transitions, directed links, divergences and
//! convergences, action blocks, and transition conditions written in ST or
//! drawn as LD/FBD networks.

use std::collections::{BTreeMap, HashMap, HashSet};

use super::grid::Grid;
use super::ladder::{check_jumps, err_expr, NetCtx};
use super::net::build_networks;
use super::scan::{is_ident_char, is_ident_start, BoxRect, Cell, Scene};
use super::{parse_box_body, parse_expr_range, Network, NodeKind};
use crate::ast::{Action, ActionAssociation, Expr, ExprKind, GraphicBody, GraphicLanguage, Sfc, Step, Transition, TransitionCondition};
use crate::diag::Diagnostic;
use crate::ident::Ident;
use crate::source::{SourceFile, Span};

/// Name of the variable assigned by the network of a transition condition
/// drawn next to the transition (table 41 features 2, 3, 4): the network
/// ends in a `Sink` of this (reserved) name.
pub const TRANSITION_MARKER: &str = "TRANSITION";

/// True if the drawing contains SFC steps: an initial step (double borders)
/// or a box with a directed link attached to its top or bottom.
pub fn looks_like_sfc(grid: &Grid) -> bool {
    let mut s = Scene::new(grid);
    s.find_labels();
    s.find_boxes();
    (0..s.boxes.len()).any(|i| step_like(&s, i))
}

fn link_char(c: char) -> bool {
    matches!(c, '|' | '+' | '*')
}

/// A box that is a step: no dividers, and either double borders or a
/// directed link attached to its top or bottom border, and no line
/// attached to its left side.
fn step_like(s: &Scene, i: usize) -> bool {
    let b = &s.boxes[i];
    if !b.dividers.is_empty() {
        return false;
    }
    if s.boxes.iter().any(|o| !o.dividers.is_empty() && o.r1 == b.r0 && o.c0 == b.c0) {
        return false; // "d" field of an action block
    }
    if b.double && b.double_bar {
        return true;
    }
    let left = (b.r0 + 1..b.r1).any(|r| b.c0 > 0 && matches!(s.chu(r, b.c0 - 1), '-' | 'O') && s.is_free(r, b.c0 - 1));
    if left {
        return false;
    }
    let top = b.r0 > 0 && (b.c0..=b.c1).any(|c| link_char(s.chu(b.r0 - 1, c)) && s.is_free(b.r0 - 1, c));
    let bottom = (b.c0..=b.c1).any(|c| link_char(s.chu(b.r1 + 1, c)) && s.is_free(b.r1 + 1, c));
    top || bottom
}

#[derive(Clone, Debug)]
enum Cond {
    None,
    /// Condition text on row, columns c0..=c1.
    Text(usize, usize, usize),
    /// Network drawn on the left of the transition.
    Graphic,
}

#[derive(Clone, Debug)]
struct TransDraft {
    row: usize,
    col: usize,
    cond: Cond,
    top: usize,
    bottom: usize,
    /// Marker node of a graphical condition.
    marker: Option<usize>,
}

struct StepDraft {
    bx: usize,
    name: Ident,
    initial: bool,
    top: usize,
    bottom: usize,
    right: usize,
}

pub fn parse_sfc(file: &SourceFile, grid: &Grid, diags: &mut Vec<Diagnostic>) -> (Sfc, Vec<Network>) {
    let mut cx = NetCtx::new(file, grid);
    let nboxes = cx.s.boxes.len();
    let mut is_step: Vec<bool> = (0..nboxes).map(|i| step_like(&cx.s, i)).collect();
    // action blocks: boxes with field dividers, or a single field (table 44
    // fields "a" and "c" omitted) joined by a straight line to a step
    let mut is_action: Vec<bool> = (0..nboxes).map(|i| !cx.s.boxes[i].dividers.is_empty() || (!is_step[i] && joined_to_step(&cx.s, &is_step, i))).collect();
    // boxes stacked below an action block: concatenated action blocks, or
    // the "d" field holding the action body
    let mut dfield_of: HashMap<usize, usize> = HashMap::new();
    let mut changed = true;
    while changed {
        changed = false;
        for i in 0..nboxes {
            if is_action[i] || dfield_of.values().any(|&d| d == i) {
                continue;
            }
            let b = cx.s.boxes[i].clone();
            let Some(a) = (0..nboxes).find(|&a| is_action[a] && cx.s.boxes[a].r1 == b.r0 && cx.s.boxes[a].c0 == b.c0) else { continue };
            is_step[i] = false;
            let (x, z) = b.inner_cols();
            let words: Vec<_> = (b.r0 + 1..b.r1).flat_map(|r| BoxRect::words(cx.g, r, x, z)).collect();
            let single = b.r1 == b.r0 + 2 && words.len() == 1 && words[0].2.chars().next().is_some_and(is_ident_start) && words[0].2.chars().all(is_ident_char);
            if single {
                is_action[i] = true;
            } else {
                dfield_of.insert(a, i);
            }
            changed = true;
        }
    }
    let mut blocks = Vec::new();
    let mut action_boxes = Vec::new();
    for i in 0..nboxes {
        if is_action[i] {
            action_boxes.push(i);
            cx.scan_sides(i);
        } else if !is_step[i] && !dfield_of.values().any(|&d| d == i) {
            blocks.push(i);
            cx.scan_sides(i);
        }
    }

    // ---------------------------------------------------------- transitions
    let mut trans: Vec<TransDraft> = Vec::new();
    find_transitions(&mut cx, &is_step, &mut trans);

    cx.scan_rest();
    sfc_rails(&mut cx);
    cx.connect(&|_, _| false);
    for &b in &blocks {
        cx.add_block(b);
    }
    cx.assign_variables();
    cx.add_symbols();
    cx.add_text_nodes();

    // graphical conditions: the line on the left of the transition shall
    // lead to a network; otherwise the cross is a plain junction
    for t in trans.iter_mut() {
        if !matches!(t.cond, Cond::Graphic) {
            continue;
        }
        let left = cx.s.idx(t.row, t.col - 1);
        let root = cx.uf.find(left);
        let mut network = false;
        for p in 0..cx.draft.pins.len() {
            if cx.draft.pins[p].attached && cx.uf.find(cx.draft.pins[p].uf) == root {
                network = true;
                break;
            }
        }
        if !network {
            for rp in 0..cx.draft.rail_pins.len() {
                if cx.uf.find(cx.draft.rail_pins[rp].uf) == root {
                    network = true;
                }
            }
        }
        if network {
            let span = grid.span(t.row, t.col, t.col + 1);
            let marker = Expr { kind: ExprKind::Name(Ident::new(TRANSITION_MARKER, span)), span };
            let node = cx.draft.node(NodeKind::Sink(marker), span, (t.row, t.col));
            let p = cx.draft.pin(&mut cx.uf, node, 0, false);
            cx.draft.attach(&cx.s, &mut cx.uf, p, t.row, t.col - 1);
            t.marker = Some(node);
        } else {
            // a junction: make it a line cell again
            t.cond = Cond::None;
            t.top = usize::MAX;
            cx.s.set(t.row, t.col, Cell::Line);
            let here = cx.s.idx(t.row, t.col);
            for (dr, dc) in [(-1isize, 0isize), (1, 0), (0, -1)] {
                let (r, c) = (t.row as isize + dr, t.col as isize + dc);
                if cx.s.is_line(r, c) {
                    let o = cx.s.idx(r as usize, c as usize);
                    cx.uf.union(here, o);
                }
            }
        }
    }
    trans.retain(|t| t.top != usize::MAX);
    for t in trans.iter_mut() {
        t.top = cx.uf.add();
        t.bottom = cx.uf.add();
    }
    for (i, t) in trans.iter().enumerate() {
        cx.s.set(t.row, t.col, Cell::Trans(i as u32));
    }
    let trans_at: HashMap<(usize, usize), usize> = trans.iter().enumerate().map(|(i, t)| ((t.row, t.col), i)).collect();

    // ---------------------------------------------------------- steps
    let mut steps: Vec<StepDraft> = Vec::new();
    for (i, &st) in is_step.iter().enumerate() {
        if !st {
            continue;
        }
        let b = cx.s.boxes[i].clone();
        let name = match step_name(&cx, i) {
            Some(n) => n,
            None => {
                cx.diags.push(Diagnostic::error("a step shall contain a single step name", cx.box_span(i)).with_clause("2.6.2"));
                Ident::new("?", cx.box_span(i))
            }
        };
        let (top, bottom, right) = (cx.uf.add(), cx.uf.add(), cx.uf.add());
        for c in b.c0..=b.c1 {
            if b.r0 > 0 {
                link_to(&mut cx, &trans, &trans_at, top, b.r0 - 1, c, true);
            }
            link_to(&mut cx, &trans, &trans_at, bottom, b.r1 + 1, c, false);
        }
        for r in b.r0 + 1..b.r1 {
            if cx.s.cellu(r, b.c1 + 1) == Cell::Line && matches!(cx.s.chu(r, b.c1 + 1), '-' | '=') {
                let idx = cx.s.idx(r, b.c1 + 1);
                cx.uf.union(right, idx);
            }
        }
        steps.push(StepDraft { bx: i, name, initial: b.double, top, bottom, right });
    }
    // transitions to the links above and below them
    for i in 0..trans.len() {
        let (r, c) = (trans[i].row, trans[i].col);
        let (top, bottom) = (trans[i].top, trans[i].bottom);
        if r > 0 {
            if let Some(&j) = trans_at.get(&(r - 1, c)) {
                let jb = trans[j].bottom;
                cx.uf.union(top, jb);
            } else if cx.s.cellu(r - 1, c) == Cell::Line {
                let idx = cx.s.idx(r - 1, c);
                cx.uf.union(top, idx);
            }
        }
        if cx.s.cellu(r + 1, c) == Cell::Line {
            let idx = cx.s.idx(r + 1, c);
            cx.uf.union(bottom, idx);
        }
    }
    // action blocks: stacks of concatenated blocks
    let mut heads: Vec<(usize, usize)> = Vec::new(); // (box, left pin)
    let mut stack_of: HashMap<usize, Vec<usize>> = HashMap::new();
    for &a in &action_boxes {
        let b = cx.s.boxes[a].clone();
        let above = action_boxes.iter().any(|&o| cx.s.boxes[o].r1 == b.r0 && cx.s.boxes[o].c0 == b.c0);
        if above {
            continue;
        }
        let mut stack = vec![a];
        let mut cur = a;
        while let Some(&n) = action_boxes.iter().find(|&&o| cx.s.boxes[o].r0 == cx.s.boxes[cur].r1 && cx.s.boxes[o].c0 == b.c0) {
            stack.push(n);
            cur = n;
        }
        let left = cx.uf.add();
        let (lp, _) = cx.sides.remove(&a).unwrap_or_default();
        for p in lp {
            if let Some((r, c)) = p.attach {
                if cx.s.cellu(r, c) == Cell::Line {
                    let idx = cx.s.idx(r, c);
                    cx.uf.union(left, idx);
                }
            }
        }
        heads.push((a, left));
        stack_of.insert(a, stack);
    }

    // ---------------------------------------------------------- link nets
    #[derive(Default)]
    struct Group {
        step_top: Vec<usize>,
        step_bottom: Vec<usize>,
        step_right: Vec<usize>,
        trans_top: Vec<usize>,
        trans_bottom: Vec<usize>,
        actions: Vec<usize>,
        double: bool,
        star: bool,
    }
    let mut groups: BTreeMap<usize, Group> = BTreeMap::new();
    for (i, s) in steps.iter().enumerate() {
        let (t, b, r) = (cx.uf.find(s.top), cx.uf.find(s.bottom), cx.uf.find(s.right));
        groups.entry(t).or_default().step_top.push(i);
        groups.entry(b).or_default().step_bottom.push(i);
        groups.entry(r).or_default().step_right.push(i);
    }
    for (i, t) in trans.iter().enumerate() {
        let (a, b) = (cx.uf.find(t.top), cx.uf.find(t.bottom));
        groups.entry(a).or_default().trans_top.push(i);
        groups.entry(b).or_default().trans_bottom.push(i);
    }
    for (k, &(_, left)) in heads.iter().enumerate() {
        let r = cx.uf.find(left);
        groups.entry(r).or_default().actions.push(k);
    }
    for r in 0..cx.s.h {
        for c in 0..cx.s.w {
            if cx.s.cellu(r, c) == Cell::Line {
                let root = cx.uf.find(cx.s.idx(r, c));
                if let Some(g) = groups.get_mut(&root) {
                    match cx.s.chu(r, c) {
                        '=' => g.double = true,
                        '*' => g.star = true,
                        _ => {}
                    }
                }
            }
        }
    }
    let network_roots: HashSet<usize> = (0..cx.draft.pins.len()).filter(|&p| cx.draft.pins[p].attached).map(|p| cx.uf.find(cx.draft.pins[p].uf)).collect();

    let mut from: Vec<Vec<usize>> = vec![Vec::new(); trans.len()];
    let mut to: Vec<Vec<usize>> = vec![Vec::new(); trans.len()];
    let mut prio: Vec<Option<u32>> = vec![None; trans.len()];
    let mut assoc_of: Vec<Vec<usize>> = vec![Vec::new(); steps.len()];
    let tspan = |t: &TransDraft| grid.span(t.row, t.col, t.col + 1);
    let mut d: Vec<Diagnostic> = Vec::new();
    for (root, g) in &groups {
        let names = |v: &Vec<usize>| v.iter().map(|&i| steps[i].name.text.clone()).collect::<Vec<_>>().join(", ");
        let at = g
            .step_bottom
            .first()
            .map(|&i| cx.box_span(steps[i].bx))
            .or_else(|| g.trans_top.first().map(|&i| tspan(&trans[i])))
            .or_else(|| g.trans_bottom.first().map(|&i| tspan(&trans[i])))
            .or_else(|| g.step_top.first().map(|&i| cx.box_span(steps[i].bx)));
        let ntrans = g.trans_top.len() + g.trans_bottom.len();
        let nsteps = g.step_top.len() + g.step_bottom.len();
        if ntrans + nsteps > 0 && network_roots.contains(root) {
            if let Some(at) = at {
                d.push(Diagnostic::error("a network is connected to a directed link of the chart", at).with_clause("2.6.3"));
            }
        }
        if !g.step_bottom.is_empty() && !g.step_top.is_empty() && ntrans == 0 {
            d.push(
                Diagnostic::error(format!("steps {} and {} are linked without a transition", names(&g.step_bottom), names(&g.step_top)), at.unwrap())
                    .with_clause("2.6.5"),
            );
        }
        if !g.trans_bottom.is_empty() && !g.trans_top.is_empty() && nsteps == 0 {
            d.push(Diagnostic::error("two transitions are linked without a step between them", tspan(&trans[g.trans_top[0]])).with_clause("2.6.5"));
        }
        if (!g.step_bottom.is_empty() && !g.trans_bottom.is_empty()) || (!g.step_top.is_empty() && !g.trans_top.is_empty()) {
            d.push(Diagnostic::error("invalid directed link: links shall go from the bottom of steps to transitions and from transitions to the top of steps", at.unwrap()).with_clause("2.6.5"));
        }
        // predecessor steps of transitions
        if !g.trans_top.is_empty() {
            if g.step_bottom.len() > 1 && g.trans_top.len() > 1 {
                d.push(Diagnostic::error("ambiguous divergence/convergence", at.unwrap()).with_clause("2.6.5"));
            }
            if g.step_bottom.len() > 1 && !g.double {
                d.push(Diagnostic::error("the convergence of simultaneous sequences shall be drawn with a double line", at.unwrap()).with_clause("2.6.5"));
            }
            if g.trans_top.len() > 1 && g.double {
                d.push(Diagnostic::error("a selection of sequences shall be drawn with a single line", at.unwrap()).with_clause("2.6.5"));
            }
            for &t in &g.trans_top {
                from[t] = g.step_bottom.clone();
            }
            if g.trans_top.len() > 1 && g.step_bottom.len() == 1 {
                // selection divergence: priorities (table 46, 2a/2b/2c)
                let mut numbered = Vec::new();
                for &t in &g.trans_top {
                    numbered.push(branch_number(&mut cx, &trans[t]));
                }
                if numbered.iter().any(|n| n.is_some()) {
                    for (k, &t) in g.trans_top.iter().enumerate() {
                        match numbered[k] {
                            Some(n) => prio[t] = Some(n),
                            None => d.push(Diagnostic::error("all branches of a numbered selection divergence shall be numbered", tspan(&trans[t])).with_clause("2.6.5")),
                        }
                    }
                } else if g.star {
                    let mut order = g.trans_top.clone();
                    order.sort_by_key(|&t| (trans[t].col, trans[t].row));
                    for (k, &t) in order.iter().enumerate() {
                        prio[t] = Some(k as u32);
                    }
                }
            }
        }
        if !g.trans_bottom.is_empty() {
            if g.step_top.len() > 1 && g.trans_bottom.len() > 1 {
                d.push(Diagnostic::error("ambiguous divergence/convergence", at.unwrap()).with_clause("2.6.5"));
            }
            if g.step_top.len() > 1 && !g.double {
                d.push(Diagnostic::error("the divergence of simultaneous sequences shall be drawn with a double line", at.unwrap()).with_clause("2.6.5"));
            }
            if g.trans_bottom.len() > 1 && g.double {
                d.push(Diagnostic::error("a convergence of selected sequences shall be drawn with a single line", at.unwrap()).with_clause("2.6.5"));
            }
            for &t in &g.trans_bottom {
                to[t] = g.step_top.clone();
            }
        }
        // action blocks
        if !g.actions.is_empty() {
            match g.step_right.as_slice() {
                [s] => assoc_of[*s].extend(g.actions.iter().copied()),
                [] => {
                    for &k in &g.actions {
                        d.push(Diagnostic::error("this action block is not connected to a step", cx.box_span(heads[k].0)).with_clause("2.6.4.2"));
                    }
                }
                _ => d.push(Diagnostic::error("an action block can be associated with one step only", cx.box_span(heads[g.actions[0]].0)).with_clause("2.6.4.2")),
            }
        } else if !g.step_right.is_empty() && network_roots.contains(root) {
            for &s in &g.step_right {
                d.push(Diagnostic::error("only action blocks can be connected to the right side of a step", cx.box_span(steps[s].bx)).with_clause("2.6.4.2"));
            }
        }
    }
    cx.diags.extend(d);

    // ---------------------------------------------------------- build
    let mut sfc = Sfc { span: grid.rows_span(0, grid.height().saturating_sub(1)), ..Sfc::default() };
    let mut order: Vec<usize> = (0..steps.len()).collect();
    order.sort_by_key(|&i| (cx.s.boxes[steps[i].bx].r0, cx.s.boxes[steps[i].bx].c0));
    let mut seen: HashMap<String, Span> = HashMap::new();
    for &i in &order {
        let st = &steps[i];
        if let Some(prev) = seen.get(&st.name.key()) {
            cx.diags.push(Diagnostic::error(format!("duplicate step name {}", st.name), st.name.span).with_clause("2.6.2").with_note(Some(*prev), "first defined here"));
        } else {
            seen.insert(st.name.key(), st.name.span);
        }
        let mut associations = Vec::new();
        let mut heads_here: Vec<usize> = assoc_of[i].clone();
        heads_here.sort_by_key(|&k| (cx.s.boxes[heads[k].0].r0, cx.s.boxes[heads[k].0].c0));
        for k in heads_here {
            for &bx in &stack_of[&heads[k].0].clone() {
                let f = cx.action_fields(bx);
                let span = cx.box_span(bx);
                let Some(action) = f.action.clone() else { continue };
                let indicators = f.indicators.iter().map(|(t, sp)| Ident::new(t.clone(), *sp)).collect();
                for (t, sp) in &f.indicators {
                    if !(t.chars().next().is_some_and(is_ident_start) && t.chars().all(is_ident_char)) {
                        cx.diags.push(Diagnostic::error(format!("the indicator '{}' shall be a Boolean variable name", t), *sp).with_clause("2.6.4.3"));
                    }
                }
                associations.push(ActionAssociation { action: action.clone(), qualifier: f.qualifier, time: f.time, indicators, span });
                if let Some(&dbox) = dfield_of.get(&bx) {
                    let db = cx.s.boxes[dbox].clone();
                    let (body, dd) = if db.r1 > db.r0 + 1 && db.c1 > db.c0 + 1 {
                        let sub = grid.subgrid(db.r0 + 1, db.r1 - 1, db.c0 + 1, db.c1 - 1);
                        parse_box_body(file, &sub)
                    } else {
                        (crate::ast::Body::Empty, Vec::new())
                    };
                    cx.diags.extend(dd);
                    sfc.actions.push(Action { name: action, body, span: cx.box_span(dbox) });
                }
            }
        }
        let b = &cx.s.boxes[st.bx];
        sfc.steps.push(Step { name: st.name.clone(), initial: st.initial, associations, span: grid.rect_span(b.r0, b.c0, b.r1, b.c1) });
    }
    for (&a, &dbox) in &dfield_of {
        if !heads.iter().any(|h| stack_of[&h.0].contains(&a)) {
            cx.diags.push(Diagnostic::error("this action body belongs to no action block", cx.box_span(dbox)).with_clause("2.6.4.3"));
        }
    }

    // conditions
    let mut conds: Vec<TransitionCondition> = Vec::new();
    for t in &trans {
        let c = match t.cond {
            Cond::Text(r, c0, c1) => {
                let text = grid.text(r, c0, c1);
                let span = grid.span(r, c0, c1 + 1);
                if text.chars().next().is_some_and(is_ident_start) && text.chars().all(is_ident_char) {
                    TransitionCondition::St(Expr { kind: ExprKind::Name(Ident::new(text, span)), span })
                } else {
                    let (e, dd) = parse_expr_range(file, span.start, span.end);
                    cx.diags.extend(dd);
                    TransitionCondition::St(e)
                }
            }
            Cond::Graphic => TransitionCondition::St(err_expr(tspan(t))), // replaced below
            Cond::None => {
                cx.diags.push(Diagnostic::error("transition without a transition condition", tspan(t)).with_clause("2.6.3"));
                TransitionCondition::St(err_expr(tspan(t)))
            }
        };
        conds.push(c);
    }

    cx.report_free_text();
    let markers: Vec<(usize, Span)> = trans.iter().enumerate().filter_map(|(i, t)| t.marker.map(|_| (i, tspan(t)))).collect();
    let crate::graphic::ladder::NetCtx { draft, mut uf, s, diags: mut dd, .. } = cx;
    let nets = build_networks(draft, &mut uf, GraphicLanguage::Ld, grid, &s.labels, &mut dd);
    check_jumps(&nets, &s.labels, &mut dd);
    let mut rest = Vec::new();
    let mut used = vec![false; nets.len()];
    for (ti, span) in markers {
        let k = nets.iter().position(|n| {
            n.nodes.iter().any(|node| matches!(&node.kind, NodeKind::Sink(e) if e.span == span && matches!(&e.kind, ExprKind::Name(i) if i.text == TRANSITION_MARKER)))
        });
        if let Some(k) = k {
            used[k] = true;
            let nw = nets[k].clone();
            let ld = nw.nodes.iter().any(|n| matches!(n.kind, NodeKind::LeftRail | NodeKind::RightRail | NodeKind::Contact { .. } | NodeKind::Coil { .. }));
            let language = if ld { GraphicLanguage::Ld } else { GraphicLanguage::Fbd };
            if !ld {
                for net in &nw.nets {
                    if net.drivers.len() > 1 {
                        dd.push(Diagnostic::error("outputs of elements shall not be connected together in FBD (no wired-OR, use an OR block)", nw.span).with_clause("4.3.2"));
                    }
                }
            }
            let (r0, r1) = nw.nodes.iter().fold((u32::MAX, 0u32), |(a, b), n| (a.min(n.pos.0), b.max(n.pos.0)));
            let text = (r0 as usize..=r1 as usize).map(|r| grid.rows.get(r).map(|row| row.iter().collect::<String>()).unwrap_or_default()).collect::<Vec<_>>().join("\n");
            conds[ti] = TransitionCondition::Graphic(GraphicBody { language, span: nw.span, networks: vec![nw], sfc: None, text });
        }
    }
    for (k, n) in nets.into_iter().enumerate() {
        if !used[k] {
            dd.push(Diagnostic::error("this network is not connected to any transition of the chart", n.span).with_clause("2.6.3"));
            rest.push(n);
        }
    }
    diags.extend(dd);

    let mut torder: Vec<usize> = (0..trans.len()).collect();
    torder.sort_by_key(|&i| (trans[i].row, trans[i].col));
    let step_name_of = |i: usize| steps[i].name.clone();
    for i in torder {
        let t = &trans[i];
        let mut f = from[i].clone();
        let mut tt = to[i].clone();
        let key = |k: &usize| (s.boxes[steps[*k].bx].r0, s.boxes[steps[*k].bx].c0);
        f.sort_by_key(key);
        tt.sort_by_key(key);
        let span = match t.cond {
            Cond::Text(r, _, c1) => grid.span(r, t.col, c1 + 1),
            _ => tspan(t),
        };
        if f.is_empty() {
            diags.push(Diagnostic::error("transition without a preceding step", span).with_clause("2.6.3"));
        }
        if tt.is_empty() {
            diags.push(Diagnostic::error("transition without a successor step", span).with_clause("2.6.3"));
        }
        sfc.transitions.push(Transition {
            name: None,
            priority: None,
            from: f.into_iter().map(step_name_of).collect(),
            to: tt.into_iter().map(step_name_of).collect(),
            condition: conds[i].clone(),
            branch_priority: prio[i],
            span,
        });
    }
    (sfc, rest)
}

/// A box whose left side is joined by a straight horizontal line to the
/// right side of a step.
fn joined_to_step(s: &Scene, is_step: &[bool], i: usize) -> bool {
    let b = &s.boxes[i];
    for r in b.r0 + 1..b.r1 {
        let mut c = b.c0;
        let mut dashes = 0;
        while c > 0 && s.chu(r, c - 1) == '-' && s.is_free(r, c - 1) {
            c -= 1;
            dashes += 1;
        }
        if dashes == 0 || c == 0 {
            continue;
        }
        if let Some(j) = s.box_at(r, c - 1) {
            let st = &s.boxes[j];
            if is_step[j] && st.c1 == c - 1 && r > st.r0 && r < st.r1 {
                return true;
            }
        }
    }
    false
}

/// Connects the step pin `pin` to what is drawn at (r, c) next to its top
/// (`top`) or bottom border.
fn link_to(cx: &mut NetCtx, trans: &[TransDraft], trans_at: &HashMap<(usize, usize), usize>, pin: usize, r: usize, c: usize, top: bool) {
    if let Some(&t) = trans_at.get(&(r, c)) {
        let o = if top { trans[t].bottom } else { trans[t].top };
        cx.uf.union(pin, o);
        return;
    }
    if cx.s.cellu(r, c) == Cell::Line && link_char(cx.s.chu(r, c)) {
        let idx = cx.s.idx(r, c);
        cx.uf.union(pin, idx);
    }
}

/// The step name written inside step box `i`.
fn step_name(cx: &NetCtx, i: usize) -> Option<Ident> {
    let b = &cx.s.boxes[i];
    let (a, z) = b.inner_cols();
    let mut words = Vec::new();
    for r in b.r0 + 1..b.r1 {
        for (s0, e0, t) in BoxRect::words(cx.g, r, a, z) {
            words.push((r, s0, e0, t));
        }
    }
    let w = words.first()?;
    if words.len() > 1 || !(w.3.chars().next().is_some_and(is_ident_start) && w.3.chars().all(is_ident_char)) {
        return None;
    }
    Some(Ident::new(w.3.clone(), cx.g.span(w.0, w.1, w.2 + 1)))
}

/// Number written right after a branch of a selection divergence (`|2`),
/// looked up on the directed link above transition `t`.
fn branch_number(cx: &mut NetCtx, t: &TransDraft) -> Option<u32> {
    let c = t.col;
    let mut r = t.row;
    while r > 0 {
        r -= 1;
        if cx.s.cellu(r, c) != Cell::Line || !matches!(cx.s.chu(r, c), '|' | '+' | '*') {
            break;
        }
        if let Cell::Tok(k) = cx.s.cellu(r, c + 1) {
            let tk = &cx.s.toks[k as usize];
            if tk.c0 == c + 1 && !tk.used && tk.text.chars().all(|ch| ch.is_ascii_digit()) {
                let n = tk.text.parse().ok();
                cx.s.toks[k as usize].used = true;
                return n;
            }
        }
        if cx.s.chu(r, c) != '|' {
            break;
        }
    }
    None
}

/// Finds the transition crosses: `+` on a vertical directed link with a
/// condition written on its right, or a line coming from its left.
fn find_transitions(cx: &mut NetCtx, is_step: &[bool], out: &mut Vec<TransDraft>) {
    let g = cx.g;
    let s = &cx.s;
    let step_border = |r: usize, c: usize, bottom: bool| -> bool {
        match s.box_at(r, c) {
            Some(i) => is_step[i] && if bottom { s.boxes[i].r1 == r } else { s.boxes[i].r0 == r },
            None => false,
        }
    };
    let link_above = |r: usize, c: usize| r > 0 && ((s.is_free(r - 1, c) && link_char(g.at(r - 1, c))) || step_border(r - 1, c, true));
    let link_below = |r: usize, c: usize| (s.is_free(r + 1, c) && link_char(g.at(r + 1, c))) || step_border(r + 1, c, false);
    let mut found: Vec<(usize, usize, Cond)> = Vec::new();
    for r in 0..s.h {
        for c in 0..g.rows[r].len() {
            if g.at(r, c) != '+' || !s.is_free(r, c) {
                continue;
            }
            if !link_above(r, c) || !link_below(r, c) {
                continue;
            }
            let right = g.at(r, c + 1);
            if matches!(right, '-' | '=' | '+' | '*') && s.is_free(r, c + 1) {
                continue;
            }
            let left_line = c > 0 && g.at(r, c - 1) == '-' && s.is_free(r, c - 1);
            // condition text on the right
            let mut x = c + 1;
            while g.at(r, x) == ' ' && x < g.rows[r].len() {
                x += 1;
            }
            let mut text = None;
            if x < g.rows[r].len() && s.is_free(r, x) && !structural(s, g, r, x, &link_above, &link_below) {
                let start = x;
                let mut end = x;
                let mut blanks = 0;
                while x < g.rows[r].len() {
                    if !s.is_free(r, x) || structural(s, g, r, x, &link_above, &link_below) {
                        break;
                    }
                    if g.at(r, x) == ' ' {
                        blanks += 1;
                        if blanks >= 4 {
                            break;
                        }
                    } else {
                        blanks = 0;
                        end = x;
                    }
                    x += 1;
                }
                text = Some((start, end));
            }
            let cond = match (text, left_line) {
                (Some((a, b)), _) => Cond::Text(r, a, b),
                (None, true) => Cond::Graphic,
                (None, false) => Cond::None,
            };
            found.push((r, c, cond));
        }
    }
    for (r, c, cond) in found {
        if let Cond::Text(_, a, b) = cond {
            for x in a..=b {
                cx.s.set(r, x, Cell::Used);
            }
            if c > 0 && g.at(r, c - 1) == '-' && cx.s.is_free(r, c - 1) {
                cx.diags.push(Diagnostic::error("a transition condition cannot be both written and drawn", g.span(r, c, c + 1)).with_clause("2.6.3"));
            }
        }
        cx.s.set(r, c, Cell::Trans(out.len() as u32));
        out.push(TransDraft { row: r, col: c, cond, top: 0, bottom: 0, marker: None });
    }
}

/// A cell that belongs to the chart structure (a directed link or another
/// transition), which ends the text of a transition condition.
fn structural(s: &Scene, g: &Grid, r: usize, c: usize, above: &dyn Fn(usize, usize) -> bool, below: &dyn Fn(usize, usize) -> bool) -> bool {
    if !s.is_free(r, c) {
        return true;
    }
    match g.at(r, c) {
        '|' => above(r, c) || below(r, c),
        '+' => above(r, c) && below(r, c),
        _ => false,
    }
}

/// Left power rails of LD transition conditions: vertical lines with links
/// attached on their right only, free ends, and nothing on their left.
fn sfc_rails(cx: &mut NetCtx) {
    for (c, r0, r1) in cx.s.vertical_segments() {
        let (l, r) = cx.s.seg_attachments(c, r0, r1);
        if r.is_empty() || !l.is_empty() {
            continue;
        }
        let g = cx.g;
        let end_free = |rr: isize| !(link_char(g.get(rr, c as isize)) || matches!(cx.s.cell(rr, c as isize), Cell::Box(_) | Cell::Trans(_)));
        if !end_free(r0 as isize - 1) || !end_free(r1 as isize + 1) {
            continue;
        }
        if g.at(r0, c) != '|' && g.at(r1, c) != '|' {
            continue;
        }
        let nothing_left = (r0..=r1).all(|rr| (0..c).all(|cc| g.at(rr, cc) == ' ' || cx.s.cellu(rr, cc) == Cell::Used));
        if nothing_left {
            cx.s.make_rail(c, r0, r1, true);
        }
    }
}
