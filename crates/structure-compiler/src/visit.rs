//! Syntax tree traversal helpers.

use structure_syntax::ast::*;
use structure_syntax::graphic::{Network, NodeKind};

/// Collects all directly represented variables referenced in a body.
pub fn direct_vars(body: &Body) -> Vec<DirectVar> {
    let mut out = Vec::new();
    body_exprs(body, &mut |e| collect(e, &mut out));
    out
}

fn collect(e: &Expr, out: &mut Vec<DirectVar>) {
    walk_expr(e, &mut |x| {
        if let ExprKind::DirectVar(d) = &x.kind {
            if !out.iter().any(|o| o.to_text() == d.to_text()) {
                out.push(d.clone());
            }
        }
    });
}

/// Calls `f` on every expression node (pre-order).
pub fn walk_expr(e: &Expr, f: &mut dyn FnMut(&Expr)) {
    f(e);
    match &e.kind {
        ExprKind::Field { base, .. } => walk_expr(base, f),
        ExprKind::Index { base, indices } => {
            walk_expr(base, f);
            for i in indices {
                walk_expr(i, f);
            }
        }
        ExprKind::Call(c) => call_exprs(c, f),
        ExprKind::Unary { operand, .. } => walk_expr(operand, f),
        ExprKind::Binary { lhs, rhs, .. } => {
            walk_expr(lhs, f);
            walk_expr(rhs, f);
        }
        ExprKind::Paren(x) => walk_expr(x, f),
        _ => {}
    }
}

fn call_exprs(c: &CallExpr, f: &mut dyn FnMut(&Expr)) {
    walk_expr(&c.callee, f);
    for a in &c.args {
        match a {
            Arg::Positional(e) | Arg::Input { value: e, .. } => walk_expr(e, f),
            Arg::Output { target: Some(t), .. } => walk_expr(t, f),
            _ => {}
        }
    }
}

pub fn stmt_exprs(stmts: &[Stmt], f: &mut dyn FnMut(&Expr)) {
    for s in stmts {
        match &s.kind {
            StmtKind::Assign { target, value } => {
                f(target);
                f(value);
            }
            StmtKind::Call(c) => {
                let e = Expr { kind: ExprKind::Call(c.clone()), span: c.span };
                f(&e);
            }
            StmtKind::If { branches, else_body } => {
                for (c, b) in branches {
                    f(c);
                    stmt_exprs(b, f);
                }
                if let Some(b) = else_body {
                    stmt_exprs(b, f);
                }
            }
            StmtKind::Case { selector, arms, else_body } => {
                f(selector);
                for a in arms {
                    stmt_exprs(&a.body, f);
                }
                if let Some(b) = else_body {
                    stmt_exprs(b, f);
                }
            }
            StmtKind::For { from, to, by, body, .. } => {
                f(from);
                f(to);
                if let Some(b) = by {
                    f(b);
                }
                stmt_exprs(body, f);
            }
            StmtKind::While { cond, body } => {
                f(cond);
                stmt_exprs(body, f);
            }
            StmtKind::Repeat { body, until } => {
                stmt_exprs(body, f);
                f(until);
            }
            _ => {}
        }
    }
}

pub fn il_exprs(list: &[IlInstr], f: &mut dyn FnMut(&Expr)) {
    for i in list {
        match &i.op {
            IlOp::Simple { operand: Some(e), .. } => f(e),
            IlOp::Deferred { operand, body, .. } => {
                if let Some(e) = operand {
                    f(e);
                }
                il_exprs(body, f);
            }
            IlOp::Function { operands, .. } => {
                for e in operands {
                    f(e);
                }
            }
            IlOp::FormalFunction { params, .. } => il_params(params, f),
            IlOp::Call { instance, args, .. } => {
                f(instance);
                match args {
                    IlCallArgs::Formal(p) => il_params(p, f),
                    IlCallArgs::NonFormal(ops) => {
                        for e in ops {
                            f(e);
                        }
                    }
                    IlCallArgs::None => {}
                }
            }
            _ => {}
        }
    }
}

fn il_params(params: &[IlParam], f: &mut dyn FnMut(&Expr)) {
    for p in params {
        match p {
            IlParam::Input { value: IlParamValue::Operand(e), .. } => f(e),
            IlParam::Input { value: IlParamValue::Nested(l), .. } => il_exprs(l, f),
            IlParam::Output { target, .. } => f(target),
        }
    }
}

pub fn network_exprs(nets: &[Network], f: &mut dyn FnMut(&Expr)) {
    for n in nets {
        for node in &n.nodes {
            match &node.kind {
                NodeKind::Contact { var, .. } | NodeKind::Coil { var, .. } | NodeKind::Source(var) | NodeKind::Sink(var) => f(var),
                NodeKind::ActionBlock { time, indicator, .. } => {
                    if let Some(t) = time {
                        f(t);
                    }
                    if let Some(i) = indicator {
                        f(i);
                    }
                }
                _ => {}
            }
        }
    }
}

pub fn sfc_exprs(s: &Sfc, f: &mut dyn FnMut(&Expr)) {
    for st in &s.steps {
        for a in &st.associations {
            if let Some(t) = &a.time {
                f(t);
            }
        }
    }
    for t in &s.transitions {
        match &t.condition {
            TransitionCondition::St(e) => f(e),
            TransitionCondition::Il(l) => il_exprs(l, f),
            TransitionCondition::Graphic(g) => network_exprs(&g.networks, f),
            TransitionCondition::Named => {}
        }
    }
    for a in &s.actions {
        body_exprs(&a.body, f);
    }
}

/// Calls `f` on every top-level expression of a body.
pub fn body_exprs(body: &Body, f: &mut dyn FnMut(&Expr)) {
    match body {
        Body::Empty => {}
        Body::St(s) => stmt_exprs(s, f),
        Body::Il(l) => il_exprs(l, f),
        Body::Sfc(s) => sfc_exprs(s, f),
        Body::Graphic(g) => {
            network_exprs(&g.networks, f);
            if let Some(s) = &g.sfc {
                sfc_exprs(s, f);
            }
        }
    }
}

/// Name of the hidden variable holding the previous state of a
/// transition-sensing contact or coil (tables 61, 62).
pub fn edge_memory_name(node: &structure_syntax::graphic::Node) -> String {
    format!("__GEDGE_{}_{}", node.span.file.0, node.span.start)
}

fn graphic_bodies<'a>(body: &'a Body, out: &mut Vec<&'a GraphicBody>) {
    let sfc = match body {
        Body::Graphic(g) => {
            out.push(g);
            g.sfc.as_deref()
        }
        Body::Sfc(s) => Some(s),
        _ => None,
    };
    if let Some(s) = sfc {
        for t in &s.transitions {
            if let TransitionCondition::Graphic(g) = &t.condition {
                out.push(g);
            }
        }
        for a in &s.actions {
            graphic_bodies(&a.body, out);
        }
    }
}

/// Transition-sensing contacts and coils of all networks of a body.
pub fn edge_elements(body: &Body) -> Vec<structure_syntax::graphic::Node> {
    use structure_syntax::graphic::{CoilKind, ContactKind};
    let mut bodies = Vec::new();
    graphic_bodies(body, &mut bodies);
    let mut out = Vec::new();
    for g in bodies {
        for n in &g.networks {
            for node in &n.nodes {
                let edge = matches!(
                    node.kind,
                    NodeKind::Contact { kind: ContactKind::Positive | ContactKind::Negative, .. }
                        | NodeKind::Coil { kind: CoilKind::Positive | CoilKind::Negative, .. }
                );
                if edge {
                    out.push(node.clone());
                }
            }
        }
    }
    out
}

/// The SFC of a body, textual or graphical.
pub fn body_sfc(b: &Body) -> Option<&Sfc> {
    match b {
        Body::Sfc(s) => Some(s),
        Body::Graphic(g) => g.sfc.as_deref(),
        _ => None,
    }
}

/// A chart and the charts included in its actions (table 42 feature 2s),
/// depth first; the first element is `sfc` itself.
pub fn sfc_charts(sfc: &Sfc) -> Vec<&Sfc> {
    let mut out = vec![sfc];
    let mut i = 0;
    while i < out.len() {
        let c = out[i];
        for a in &c.actions {
            if let Some(n) = body_sfc(&a.body) {
                // an action can (erroneously) include its own chart
                if !out.iter().any(|x| std::ptr::eq(*x, n)) {
                    out.push(n);
                }
            }
        }
        i += 1;
    }
    out
}

/// Action blocks used as elements of networks (table 44 features 8, 9).
pub fn network_action_blocks(body: &Body) -> Vec<structure_syntax::graphic::Node> {
    let mut bodies = Vec::new();
    graphic_bodies(body, &mut bodies);
    bodies
        .iter()
        .flat_map(|g| g.networks.iter())
        .flat_map(|n| n.nodes.iter())
        .filter(|n| matches!(n.kind, NodeKind::ActionBlock { .. }))
        .cloned()
        .collect()
}

/// Name of the hidden variable holding the input of an action block used
/// in a network (the state of its association, 2.6.4.5 rule 2).
pub fn action_block_flag_name(node: &structure_syntax::graphic::Node) -> String {
    format!("__GACT_{}_{}", node.span.file.0, node.span.start)
}
