//! LD and FBD networks (4.2, 4.3), and the node construction shared with
//! the transition conditions of graphical SFC.

use std::collections::{BTreeMap, HashMap, HashSet};

use super::grid::Grid;
use super::net::{build_networks, Draft};
use super::scan::{BoxRect, Cell, ElemKind, Scene, Uf};
use super::{parse_expr_range, BlockPin, Network, NodeKind};
use crate::ast::{Edge, Expr, ExprKind, GraphicLanguage, Qualifier};
use crate::diag::Diagnostic;
use crate::ident::{check_identifier, Ident};
use crate::source::{SourceFile, Span};

/// True if the drawing has an LD left power rail: a vertical line with
/// horizontal links attached on its right only and nothing on its left.
pub fn has_left_rail(grid: &Grid) -> bool {
    let mut s = Scene::new(grid);
    s.find_labels();
    s.find_boxes();
    s.find_symbols();
    s.find_text();
    s.mark_lines();
    let ld_symbols = s.elems.iter().any(|e| matches!(e.kind, ElemKind::Contact(_) | ElemKind::Coil(_)));
    for (c, r0, r1) in s.vertical_segments() {
        let (l, r) = s.seg_attachments(c, r0, r1);
        if r.is_empty() || !l.is_empty() {
            continue;
        }
        let nothing_left = (r0..=r1).all(|rr| (0..c).all(|cc| grid.at(rr, cc) == ' ' || s.cellu(rr, cc) == Cell::Used));
        if !nothing_left {
            continue;
        }
        if grid.at(r0, c) == '|' || grid.at(r1, c) == '|' || ld_symbols {
            return true;
        }
    }
    false
}

pub fn parse_networks(file: &SourceFile, grid: &Grid, lang: GraphicLanguage, diags: &mut Vec<Diagnostic>) -> Vec<Network> {
    let mut cx = NetCtx::new(file, grid);
    let mut blocks = Vec::new();
    let mut actions = Vec::new();
    for i in 0..cx.s.boxes.len() {
        if cx.s.boxes[i].dividers.is_empty() {
            if cx.is_dfield(i) {
                cx.diags.push(Diagnostic::error("inline action bodies are only supported in SFC action blocks", cx.box_span(i)).with_clause("2.6.4.3"));
                continue;
            }
            blocks.push(i);
        } else {
            actions.push(i);
        }
        cx.scan_sides(i);
    }
    cx.scan_rest();
    if lang == GraphicLanguage::Ld {
        cx.ld_rails();
    }
    cx.connect(&|_, _| false);
    for &b in &blocks {
        cx.add_block(b);
    }
    for &a in &actions {
        cx.add_action_block(a);
    }
    cx.assign_variables();
    cx.add_symbols();
    cx.add_text_nodes();
    cx.report_free_text();
    let NetCtx { draft, mut uf, s, diags: mut d, .. } = cx;
    let nets = build_networks(draft, &mut uf, lang, grid, &s.labels, &mut d);
    check_jumps(&nets, &s.labels, &mut d);
    diags.extend(d);
    nets
}

/// Every jump shall designate a network label of the body (4.1.4).
pub(crate) fn check_jumps(nets: &[Network], labels: &[(usize, String, Span)], diags: &mut Vec<Diagnostic>) {
    let names: HashSet<String> = labels.iter().map(|l| l.1.to_uppercase()).collect();
    for n in nets {
        for node in &n.nodes {
            if let NodeKind::Jump(l) = &node.kind {
                if !names.contains(&l.key()) {
                    diags.push(Diagnostic::error(format!("jump to unknown network label {}", l), l.span).with_clause("4.1.4"));
                }
            }
        }
    }
}

/// A line attached to a side of a block.
#[derive(Clone, Debug)]
pub(crate) struct SidePin {
    pub row: usize,
    pub attach: Option<(usize, usize)>,
    pub negated: bool,
    pub edge: Option<Edge>,
}

/// The fields of an action block (table 44).
#[derive(Clone, Debug, Default)]
pub(crate) struct ActionFields {
    pub qualifier: Option<Qualifier>,
    pub time: Option<Expr>,
    pub action: Option<Ident>,
    /// Indicator variables: (text, span).
    pub indicators: Vec<(String, Span)>,
}

/// Construction of network nodes from a decomposed drawing.
pub(crate) struct NetCtx<'a> {
    pub file: &'a SourceFile,
    pub g: &'a Grid,
    pub s: Scene<'a>,
    pub uf: Uf,
    pub draft: Draft,
    pub diags: Vec<Diagnostic>,
    pub sides: HashMap<usize, (Vec<SidePin>, Vec<SidePin>)>,
    /// Variable (text run index) of each contact and coil.
    var_of: HashMap<usize, usize>,
    /// Node of each box.
    pub box_node: HashMap<usize, usize>,
}

const OPERATOR_BLOCKS: &[&str] = &["&", ">=1", "=1", "=2K+1", "+", "*", "-", "/", ":=", "<", ">", "=", "<=", ">=", "<>", "**", "T---0", "0---T"];

impl<'a> NetCtx<'a> {
    pub fn new(file: &'a SourceFile, g: &'a Grid) -> NetCtx<'a> {
        let mut s = Scene::new(g);
        s.find_labels();
        s.find_boxes();
        let uf = Uf::new(s.h * s.w);
        NetCtx { file, g, s, uf, draft: Draft::default(), diags: Vec::new(), sides: HashMap::new(), var_of: HashMap::new(), box_node: HashMap::new() }
    }

    pub fn box_span(&self, i: usize) -> Span {
        let b = &self.s.boxes[i];
        self.g.rect_span(b.r0, b.c0, b.r1, b.c1)
    }

    pub fn cells_span(&self, r: usize, c0: usize, c1: usize) -> Span {
        self.g.span(r, c0, c1 + 1)
    }

    /// A box without dividers right below an action block, sharing its
    /// bottom border: the "d" field of that action block.
    pub fn is_dfield(&self, i: usize) -> bool {
        let b = &self.s.boxes[i];
        self.s.boxes.iter().any(|o| !o.dividers.is_empty() && o.r1 == b.r0 && o.c0 == b.c0)
    }

    /// Finds the lines attached to the left and right sides of box `bi`,
    /// with negation circles (table 19) and edge markers (`>`, `<`).
    pub fn scan_sides(&mut self, bi: usize) {
        let b = self.s.boxes[bi].clone();
        let g = self.g;
        let line = |s: &Scene, r: usize, c: isize| c >= 0 && s.cell(r as isize, c) == Cell::Free && matches!(g.get(r as isize, c), '-' | '+' | '=');
        let mut left = Vec::new();
        let mut right = Vec::new();
        for r in b.r0 + 1..b.r1 {
            let c0 = b.c0 as isize;
            let side = g.at(r, b.c0);
            let edge = match side {
                '>' => Some(Edge::Rising),
                '<' => Some(Edge::Falling),
                _ => None,
            };
            if g.get(r as isize, c0 - 1) == 'O' && self.s.cell(r as isize, c0 - 1) == Cell::Free && line(&self.s, r, c0 - 2) {
                self.s.set(r, b.c0 - 1, Cell::Used);
                left.push(SidePin { row: r, attach: Some((r, b.c0 - 2)), negated: true, edge });
            } else if line(&self.s, r, c0 - 1) {
                left.push(SidePin { row: r, attach: Some((r, b.c0 - 1)), negated: false, edge });
            } else if edge.is_some() {
                left.push(SidePin { row: r, attach: None, negated: false, edge });
            }
            let c1 = b.c1 as isize;
            if g.get(r as isize, c1 + 1) == 'O' && self.s.cell(r as isize, c1 + 1) == Cell::Free && line(&self.s, r, c1 + 2) {
                self.s.set(r, b.c1 + 1, Cell::Used);
                right.push(SidePin { row: r, attach: Some((r, b.c1 + 2)), negated: true, edge: None });
            } else if line(&self.s, r, c1 + 1) {
                right.push(SidePin { row: r, attach: Some((r, b.c1 + 1)), negated: false, edge: None });
            }
        }
        self.sides.insert(bi, (left, right));
    }

    /// Symbols, text and lines outside the boxes.
    pub fn scan_rest(&mut self) {
        self.s.find_symbols();
        self.s.find_text();
        self.s.mark_lines();
        self.s.attach_text();
    }

    /// LD power rails: vertical lines with links attached on one side only.
    pub fn ld_rails(&mut self) {
        for (c, r0, r1) in self.s.vertical_segments() {
            let (l, r) = self.s.seg_attachments(c, r0, r1);
            if !r.is_empty() && l.is_empty() {
                self.s.make_rail(c, r0, r1, true);
            } else if !l.is_empty() && r.is_empty() {
                self.s.make_rail(c, r0, r1, false);
            }
        }
    }

    pub fn connect(&mut self, skip: &dyn Fn(usize, usize) -> bool) {
        self.s.connect_lines(&mut self.uf, skip);
        self.draft.rail_attachments(&self.s, &mut self.uf);
    }

    fn pin_at(&mut self, node: usize, pin: usize, driver: bool, at: Option<(usize, usize)>) -> usize {
        let p = self.draft.pin(&mut self.uf, node, pin, driver);
        if let Some((r, c)) = at {
            self.draft.attach(&self.s, &mut self.uf, p, r, c);
        }
        p
    }

    /// Free text runs on row `r` overlapping columns `c0..=c1`.
    fn free_text_over(&self, r: usize, c0: usize, c1: usize) -> Option<usize> {
        let mut best: Option<(usize, usize)> = None;
        for (i, t) in self.s.toks.iter().enumerate() {
            if t.row != r || !t.is_free() || t.c1 < c0 || t.c0 > c1 {
                continue;
            }
            let ov = t.c1.min(c1) - t.c0.max(c0) + 1;
            if best.is_none_or(|(_, b)| ov > b) {
                best = Some((i, ov));
            }
        }
        best.map(|b| b.0)
    }

    fn tok_ident(&self, t: usize) -> Ident {
        let t = &self.s.toks[t];
        Ident::new(t.text.clone(), self.cells_span(t.row, t.c0, t.c1))
    }

    fn tok_expr(&mut self, t: usize) -> Expr {
        let t = &self.s.toks[t];
        let (e, d) = parse_expr_range(self.file, self.g.offset(t.row, t.c0), self.g.cell_end(t.row, t.c1));
        self.diags.extend(d);
        e
    }

    /// Function or function block (2.5.1.1, 2.5.2.2).
    pub fn add_block(&mut self, bi: usize) -> usize {
        let b = self.s.boxes[bi].clone();
        let g = self.g;
        let (a, z) = b.inner_cols();
        let (lp, rp) = self.sides.remove(&bi).unwrap_or_default();
        let lrows: HashMap<usize, SidePin> = lp.into_iter().map(|p| (p.row, p)).collect();
        let rrows: HashMap<usize, SidePin> = rp.into_iter().map(|p| (p.row, p)).collect();
        let mut lname: BTreeMap<usize, (usize, usize, String)> = BTreeMap::new();
        let mut rname: BTreeMap<usize, (usize, usize, String)> = BTreeMap::new();
        let mut middle: BTreeMap<usize, Vec<(usize, usize, String)>> = BTreeMap::new();
        let mut first_text_row = None;
        for r in b.r0 + 1..b.r1 {
            let words = BoxRect::words(g, r, a, z);
            if !words.is_empty() && first_text_row.is_none() {
                first_text_row = Some(r);
            }
            for (s0, e0, t) in words {
                let tl = s0 == a;
                let tr = e0 == z;
                if tl && tr {
                    if lrows.contains_key(&r) && rrows.contains_key(&r) && t.contains("--") {
                        // VAR_IN_OUT drawn through the block: `|V---V|`
                        let i = t.find("--").unwrap_or(0);
                        let j = t.rfind("--").map(|j| j + 2).unwrap_or(t.len());
                        let l = t[..i].trim_end_matches('-').to_string();
                        let rr = t[j..].trim_start_matches('-').to_string();
                        if !l.is_empty() {
                            lname.insert(r, (s0, s0 + l.chars().count() - 1, l));
                        }
                        if !rr.is_empty() {
                            rname.insert(r, (e0 + 1 - rr.chars().count(), e0, rr));
                        }
                        continue;
                    }
                    middle.entry(r).or_default().push((s0, e0, t));
                } else if tl {
                    lname.insert(r, (s0, e0, t));
                } else if tr {
                    rname.insert(r, (s0, e0, t));
                } else {
                    middle.entry(r).or_default().push((s0, e0, t));
                }
            }
        }
        // type name: first row with text in the middle, continued on the
        // next row when it ends with an underline (`BCD_` / `TO_INT`)
        let mut parts: Vec<(usize, usize, usize, String)> = Vec::new();
        if let Some(&r) = middle.keys().next() {
            let mut cur = r;
            loop {
                let Some(ws) = middle.remove(&cur) else { break };
                for (s0, e0, t) in ws {
                    parts.push((cur, s0, e0, t));
                }
                if !parts.last().is_some_and(|p| p.3.ends_with('_')) {
                    break;
                }
                cur += 1;
            }
        }
        if parts.is_empty() {
            // a type name written against a side (`|SEL   |`)
            if let Some(r) = first_text_row {
                if !lrows.contains_key(&r) && !rrows.contains_key(&r) {
                    let only_l = lname.contains_key(&r) && !rname.contains_key(&r);
                    let only_r = rname.contains_key(&r) && !lname.contains_key(&r);
                    if only_l {
                        let (s0, e0, t) = lname.remove(&r).unwrap();
                        parts.push((r, s0, e0, t));
                    } else if only_r {
                        let (s0, e0, t) = rname.remove(&r).unwrap();
                        parts.push((r, s0, e0, t));
                    }
                }
            }
        }
        // other text in the middle: pin names not written against the side
        let mid = a + z;
        for (r, ws) in std::mem::take(&mut middle) {
            for (s0, e0, t) in ws {
                if s0 + e0 < mid && lrows.contains_key(&r) && !lname.contains_key(&r) {
                    lname.insert(r, (s0, e0, t));
                } else if s0 + e0 >= mid && rrows.contains_key(&r) && !rname.contains_key(&r) {
                    rname.insert(r, (s0, e0, t));
                } else {
                    self.diags.push(Diagnostic::error(format!("unexpected text '{}' inside the block", t), self.cells_span(r, s0, e0)).with_clause("2.5.1.1"));
                }
            }
        }
        let type_name = if parts.is_empty() {
            self.diags.push(Diagnostic::error("block without a function or function block name", self.box_span(bi)).with_clause("2.5.1.1"));
            Ident::new("?", self.box_span(bi))
        } else {
            let text: String = parts.iter().map(|p| p.3.as_str()).collect();
            let first = &parts[0];
            let last = &parts[parts.len() - 1];
            let span = g.rect_span(first.0, first.1, last.0, last.2);
            let up = text.to_uppercase();
            let ident_like = text.chars().next().is_some_and(super::scan::is_ident_start) && text.chars().all(super::scan::is_ident_char);
            if ident_like {
                if let Some(p) = check_identifier(&text) {
                    self.diags.push(Diagnostic::syntax(format!("invalid block name '{}': {}", text, p), span).with_clause("2.1.2"));
                }
            } else if !OPERATOR_BLOCKS.contains(&up.as_str()) {
                self.diags.push(Diagnostic::error(format!("'{}' is not a function, function block or operator name", text), span).with_clause("2.5.1.1"));
            }
            Ident::new(if OPERATOR_BLOCKS.contains(&up.as_str()) { up } else { text }, span)
        };
        // instance name centred above the block
        let instance = if b.r0 > 0 {
            self.free_text_over(b.r0 - 1, b.c0, b.c1).map(|t| {
                self.s.toks[t].used = true;
                self.tok_ident(t)
            })
        } else {
            None
        };
        let mut in_rows: Vec<usize> = lrows.keys().chain(lname.keys()).copied().collect();
        in_rows.sort();
        in_rows.dedup();
        let mut out_rows: Vec<usize> = rrows.keys().chain(rname.keys()).copied().collect();
        out_rows.sort();
        out_rows.dedup();
        let mk = |rows: &[usize], names: &BTreeMap<usize, (usize, usize, String)>, sides: &HashMap<usize, SidePin>, col: usize| -> Vec<BlockPin> {
            rows.iter()
                .map(|r| {
                    let name = names.get(r).map(|(s0, e0, t)| Ident::new(t.clone(), g.span(*r, *s0, *e0 + 1)));
                    let side = sides.get(r);
                    BlockPin {
                        span: name.as_ref().map(|n| n.span).unwrap_or_else(|| g.span(*r, col, col + 1)),
                        name,
                        negated: side.is_some_and(|s| s.negated),
                        edge: side.and_then(|s| s.edge),
                    }
                })
                .collect()
        };
        let inputs = mk(&in_rows, &lname, &lrows, b.c0);
        let outputs = mk(&out_rows, &rname, &rrows, b.c1);
        for p in inputs.iter().chain(outputs.iter()) {
            if let Some(n) = &p.name {
                if !(n.text.chars().next().is_some_and(super::scan::is_ident_start) && n.text.chars().all(super::scan::is_ident_char)) {
                    self.diags.push(Diagnostic::syntax(format!("'{}' is not a valid formal parameter name", n.text), n.span).with_clause("2.5.1.1"));
                }
            }
        }
        let span = self.box_span(bi);
        let node = self.draft.node(NodeKind::Block { type_name, instance, inputs, outputs }, span, (b.r0, b.c0));
        for (k, r) in in_rows.iter().enumerate() {
            let at = lrows.get(r).and_then(|p| p.attach);
            self.pin_at(node, k, false, at);
        }
        for (k, r) in out_rows.iter().enumerate() {
            let at = rrows.get(r).and_then(|p| p.attach);
            self.pin_at(node, k, true, at);
        }
        self.box_node.insert(bi, node);
        node
    }

    /// Reads the fields of the action block `bi` (table 44): qualifier and
    /// duration ("a"), action name ("b") and indicators ("c").
    pub fn action_fields(&mut self, bi: usize) -> ActionFields {
        let b = self.s.boxes[bi].clone();
        let g = self.g;
        let mut bounds = vec![b.c0];
        bounds.extend(b.dividers.iter().copied());
        bounds.push(b.c1);
        let fields: Vec<Vec<(usize, usize, usize, String)>> = bounds
            .windows(2)
            .map(|w| {
                let mut v = Vec::new();
                for r in b.r0 + 1..b.r1 {
                    if w[1] > w[0] + 1 {
                        for (s0, e0, t) in BoxRect::words(g, r, w[0] + 1, w[1] - 1) {
                            v.push((r, s0, e0, t));
                        }
                    }
                }
                v
            })
            .collect();
        let mut out = ActionFields::default();
        let first_is_q = fields[0].first().is_some_and(|w| Qualifier::from_upper(&w.3.to_uppercase()).is_some());
        let (qf, bf) = if fields.len() >= 3 || first_is_q { (Some(0), 1) } else { (None, 0) };
        if let Some(qf) = qf {
            let ws = &fields[qf];
            if let Some(w) = ws.first() {
                match Qualifier::from_upper(&w.3.to_uppercase()) {
                    Some(q) => out.qualifier = Some(q),
                    None => self.diags.push(Diagnostic::error(format!("'{}' is not an action qualifier", w.3), self.cells_span(w.0, w.1, w.2)).with_clause("2.6.4.4")),
                }
                if ws.len() > 1 {
                    let row = ws[1].0;
                    let rest: Vec<_> = ws[1..].iter().filter(|x| x.0 == row).collect();
                    let (s0, e0) = (rest[0].1, rest[rest.len() - 1].2);
                    let (e, d) = parse_expr_range(self.file, g.offset(row, s0), g.cell_end(row, e0));
                    self.diags.extend(d);
                    out.time = Some(e);
                    if ws[1..].iter().any(|x| x.0 != row) {
                        self.diags.push(Diagnostic::error("unexpected text in the qualifier field", self.box_span(bi)).with_clause("2.6.4.3"));
                    }
                }
            }
        }
        if let Some(ws) = fields.get(bf) {
            match ws.len() {
                0 => self.diags.push(Diagnostic::error("action block without an action name", self.box_span(bi)).with_clause("2.6.4.3")),
                1 => {
                    let w = &ws[0];
                    if !(w.3.chars().next().is_some_and(super::scan::is_ident_start) && w.3.chars().all(super::scan::is_ident_char)) {
                        self.diags.push(Diagnostic::syntax(format!("'{}' is not a valid action name", w.3), self.cells_span(w.0, w.1, w.2)));
                    }
                    out.action = Some(Ident::new(w.3.clone(), self.cells_span(w.0, w.1, w.2)));
                }
                _ => {
                    let w = &ws[1];
                    self.diags.push(Diagnostic::error("the action name field shall contain a single name", self.cells_span(w.0, w.1, w.2)).with_clause("2.6.4.3"));
                    let w = &ws[0];
                    out.action = Some(Ident::new(w.3.clone(), self.cells_span(w.0, w.1, w.2)));
                }
            }
        }
        for f in fields.iter().skip(bf + 1) {
            for w in f {
                out.indicators.push((w.3.clone(), self.cells_span(w.0, w.1, w.2)));
            }
        }
        out
    }

    /// Action block used as an element of a network (table 44 features 8, 9).
    pub fn add_action_block(&mut self, bi: usize) -> usize {
        let f = self.action_fields(bi);
        let indicator = f.indicators.first().map(|(_, sp)| {
            let (e, d) = parse_expr_range(self.file, sp.start, sp.end);
            self.diags.extend(d);
            e
        });
        let span = self.box_span(bi);
        let b = self.s.boxes[bi].clone();
        let action = f.action.unwrap_or_else(|| Ident::new("?", span));
        let node = self.draft.node(NodeKind::ActionBlock { qualifier: f.qualifier, time: f.time, action, indicator }, span, (b.r0, b.c0));
        let (lp, rp) = self.sides.remove(&bi).unwrap_or_default();
        let lp: Vec<_> = lp.into_iter().filter(|p| p.attach.is_some()).collect();
        let rp: Vec<_> = rp.into_iter().filter(|p| p.attach.is_some()).collect();
        if lp.len() > 1 || rp.len() > 1 {
            self.diags.push(Diagnostic::error("an action block has a single input and a single output", span).with_clause("2.6.4.3"));
        }
        self.pin_at(node, 0, false, lp.first().and_then(|p| p.attach));
        self.pin_at(node, 0, true, rp.first().and_then(|p| p.attach));
        self.box_node.insert(bi, node);
        node
    }

    /// Associates each contact and coil with the variable written above it
    /// (table 61, 62).
    pub fn assign_variables(&mut self) {
        let mut by_row: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
        for (i, e) in self.s.elems.iter().enumerate() {
            if matches!(e.kind, ElemKind::Contact(_) | ElemKind::Coil(_)) {
                by_row.entry(e.row).or_default().push(i);
            }
        }
        for (r, mut els) in by_row {
            els.sort_by_key(|&i| self.s.elems[i].c0);
            let mut cands: Vec<usize> = if r == 0 { Vec::new() } else { (0..self.s.toks.len()).filter(|&t| self.s.toks[t].row == r - 1 && self.s.toks[t].is_free()).collect() };
            cands.sort_by_key(|&t| self.s.toks[t].c0);
            if cands.len() == els.len() {
                for (e, t) in els.iter().zip(cands.iter()) {
                    self.var_of.insert(*e, *t);
                    self.s.toks[*t].used = true;
                }
                continue;
            }
            let centers: Vec<usize> = els.iter().map(|&i| self.s.elems[i].c0 + self.s.elems[i].c1).collect();
            for (k, &e) in els.iter().enumerate() {
                let lo = if k > 0 { (centers[k - 1] + centers[k]) / 2 } else { 0 };
                let hi = if k + 1 < els.len() { (centers[k] + centers[k + 1]) / 2 } else { usize::MAX };
                let mut best: Option<(usize, usize)> = None;
                for &t in &cands {
                    let tk = &self.s.toks[t];
                    if tk.used {
                        continue;
                    }
                    let c = tk.center2();
                    if c < lo || c > hi {
                        continue;
                    }
                    let d = c.abs_diff(centers[k]);
                    if best.is_none_or(|(_, bd)| d < bd) {
                        best = Some((t, d));
                    }
                }
                if let Some((t, _)) = best {
                    self.var_of.insert(e, t);
                    self.s.toks[t].used = true;
                }
            }
        }
    }

    /// Nodes for contacts, coils, jumps, returns and connectors.
    pub fn add_symbols(&mut self) {
        let mut pins_of: Vec<(Option<usize>, Option<usize>)> = Vec::new();
        for i in 0..self.s.elems.len() {
            let e = self.s.elems[i].clone();
            let span = self.cells_span(e.row, e.c0, e.c1);
            let pos = (e.row, e.c0);
            let left = if e.c0 > 0 { Some((e.row, e.c0 - 1)) } else { None };
            let right = Some((e.row, e.c1 + 1));
            let name_ident = |cx: &Self| Ident::new(e.name_cols_text(cx.g), cx.cells_span(e.row, e.name_cols.0, e.name_cols.1));
            let (kind, has_in, has_out) = match &e.kind {
                ElemKind::Contact(k) => {
                    let var = self.elem_var(i, span, "contact");
                    (NodeKind::Contact { var, kind: *k }, true, true)
                }
                ElemKind::Coil(k) => {
                    let var = self.elem_var(i, span, "coil");
                    (NodeKind::Coil { var, kind: *k }, true, true)
                }
                ElemKind::Jump(_) => (NodeKind::Jump(name_ident(self)), true, false),
                ElemKind::Return => (NodeKind::Return, true, false),
                ElemKind::ConnOut(_) => (NodeKind::ConnectorOut(name_ident(self)), true, false),
                ElemKind::ConnIn(_) => (NodeKind::ConnectorIn(name_ident(self)), false, true),
            };
            let node = self.draft.node(kind, span, pos);
            let pi = if has_in { Some(self.pin_at(node, 0, false, left)) } else { None };
            let po = if has_out { Some(self.pin_at(node, 0, true, right)) } else { None };
            if let Some(p) = pi {
                if !self.draft.pins[p].attached && matches!(e.kind, ElemKind::Contact(_) | ElemKind::Coil(_)) {
                    // may still be joined directly to the symbol on its left
                    let joined = e.c0 > 0 && matches!(self.s.cellu(e.row, e.c0 - 1), Cell::Elem(_));
                    if !joined {
                        self.diags.push(Diagnostic::error("the left side of this element is not connected", span).with_clause("4.2.2"));
                    }
                }
            }
            pins_of.push((pi, po));
        }
        // symbols drawn directly against each other (`|/|( )`)
        for i in 0..self.s.elems.len() {
            let e = &self.s.elems[i];
            if let Cell::Elem(j) = self.s.cellu(e.row, e.c1 + 1) {
                let j = j as usize;
                if let (Some(po), Some(pi)) = (pins_of[i].1, pins_of[j].0) {
                    if !self.draft.pins[po].attached && !self.draft.pins[pi].attached {
                        self.draft.join(&mut self.uf, po, pi);
                    }
                }
            }
        }
    }

    fn elem_var(&mut self, i: usize, span: Span, what: &str) -> Expr {
        match self.var_of.get(&i).copied() {
            Some(t) => self.tok_expr(t),
            None => {
                self.diags.push(Diagnostic::error(format!("no variable is written above this {}", what), span).with_clause("4.2.3"));
                err_expr(span)
            }
        }
    }

    /// Variables and literals connected to lines (`X---`, `---Y`).
    pub fn add_text_nodes(&mut self) {
        for t in 0..self.s.toks.len() {
            let tk = self.s.toks[t].clone();
            if tk.used || (tk.left.is_none() && tk.right.is_none()) {
                continue;
            }
            let e = self.tok_expr(t);
            let span = self.cells_span(tk.row, tk.c0, tk.c1);
            let kind = if tk.left.is_some() { NodeKind::Sink(e) } else { NodeKind::Source(e) };
            let node = self.draft.node(kind, span, (tk.row, tk.c0));
            if let Some(at) = tk.left {
                self.pin_at(node, 0, false, Some(at));
            }
            if let Some(at) = tk.right {
                self.pin_at(node, 0, true, Some(at));
            }
            self.s.toks[t].used = true;
        }
    }

    pub fn report_free_text(&mut self) {
        for t in &self.s.toks {
            if !t.used {
                self.diags.push(
                    Diagnostic::error(format!("'{}' is not connected to any element of the drawing", t.text), self.g.span(t.row, t.c0, t.c1 + 1)).with_clause("4.1.1"),
                );
            }
        }
    }
}

impl super::scan::Elem {
    fn name_cols_text(&self, g: &Grid) -> String {
        if self.name_cols.1 >= self.name_cols.0 {
            g.text(self.row, self.name_cols.0, self.name_cols.1)
        } else {
            String::new()
        }
    }
}

pub(crate) fn err_expr(span: Span) -> Expr {
    Expr { kind: ExprKind::Error, span }
}
