//! Assembly of nodes and pins into nets and networks.

use std::collections::{BTreeMap, HashMap};

use super::grid::Grid;
use super::scan::{Cell, Scene, Uf};
use super::{Net, Network, Node, NodeKind, PinRef};
use crate::ast::GraphicLanguage;
use crate::diag::Diagnostic;
use crate::ident::Ident;
use crate::source::Span;

#[derive(Clone, Debug)]
pub(crate) struct PinDraft {
    pub node: usize,
    pub pin: usize,
    pub driver: bool,
    pub uf: usize,
    pub attached: bool,
}

#[derive(Clone, Debug)]
pub(crate) struct RailPin {
    pub rail: usize,
    pub left: bool,
    pub row: usize,
    pub col: usize,
    pub uf: usize,
}

/// Nodes and pins collected from a drawing, before nets are formed.
#[derive(Default)]
pub(crate) struct Draft {
    pub nodes: Vec<Node>,
    pub pins: Vec<PinDraft>,
    pub rail_pins: Vec<RailPin>,
}

impl Draft {
    pub fn node(&mut self, kind: NodeKind, span: Span, pos: (usize, usize)) -> usize {
        self.nodes.push(Node { kind, span, pos: (pos.0 as u32, pos.1 as u32) });
        self.nodes.len() - 1
    }

    /// Adds a pin to `node`; returns the pin index in `pins`.
    pub fn pin(&mut self, uf: &mut Uf, node: usize, pin: usize, driver: bool) -> usize {
        let u = uf.add();
        self.pins.push(PinDraft { node, pin, driver, uf: u, attached: false });
        self.pins.len() - 1
    }

    fn rail_pin(&mut self, scene: &Scene, uf: &mut Uf, rail: usize, row: usize) -> usize {
        if let Some(p) = self.rail_pins.iter().find(|p| p.rail == rail && p.row == row) {
            return p.uf;
        }
        let seg = &scene.rails[rail];
        let u = uf.add();
        self.rail_pins.push(RailPin { rail, left: seg.left, row, col: seg.col, uf: u });
        u
    }

    /// Connects pin `p` to whatever is drawn at cell (r, c): a line or a
    /// power rail. Returns true if something was found.
    pub fn attach(&mut self, scene: &Scene, uf: &mut Uf, p: usize, r: usize, c: usize) -> bool {
        match scene.cellu(r, c) {
            Cell::Line => {
                let u = self.pins[p].uf;
                uf.union(u, scene.idx(r, c));
                self.pins[p].attached = true;
                true
            }
            Cell::Rail(i) => {
                let rp = self.rail_pin(scene, uf, i as usize, r);
                let u = self.pins[p].uf;
                uf.union(u, rp);
                self.pins[p].attached = true;
                true
            }
            _ => false,
        }
    }

    /// Joins two pins directly (adjacent symbols without a line between).
    pub fn join(&mut self, uf: &mut Uf, a: usize, b: usize) {
        let (ua, ub) = (self.pins[a].uf, self.pins[b].uf);
        uf.union(ua, ub);
        self.pins[a].attached = true;
        self.pins[b].attached = true;
    }

    /// Creates the rail pins for every line attached to a rail segment.
    pub fn rail_attachments(&mut self, scene: &Scene, uf: &mut Uf) {
        for (i, seg) in scene.rails.iter().enumerate() {
            for r in seg.r0..=seg.r1 {
                let c = if seg.left { seg.col as isize + 1 } else { seg.col as isize - 1 };
                if c < 0 {
                    continue;
                }
                let ri = r as isize;
                if scene.h_line(ri, c) || scene.j_line(ri, c) {
                    let u = self.rail_pin(scene, uf, i, r);
                    uf.union(u, scene.idx(r, c as usize));
                }
            }
        }
    }
}

fn span_union(a: Span, b: Span) -> Span {
    if a.is_empty() && a.start == 0 {
        return b;
    }
    a.to(b)
}

/// Groups the pins into nets and the nodes into networks (4.1.2: a network
/// is a maximal set of interconnected elements, rails excluded). Networks
/// joined through a connector pair are one network. Networks are ordered
/// top to bottom; label lines label the network that follows them.
pub(crate) fn build_networks(
    draft: Draft,
    uf: &mut Uf,
    lang: GraphicLanguage,
    grid: &Grid,
    labels: &[(usize, String, Span)],
    diags: &mut Vec<Diagnostic>,
) -> Vec<Network> {
    let Draft { nodes, pins, rail_pins } = draft;
    // nets: root -> (pins, rail pins)
    let mut groups: BTreeMap<usize, (Vec<usize>, Vec<usize>)> = BTreeMap::new();
    for (i, p) in pins.iter().enumerate() {
        if !p.attached {
            continue;
        }
        let root = uf.find(p.uf);
        groups.entry(root).or_default().0.push(i);
    }
    for (i, rp) in rail_pins.iter().enumerate() {
        let root = uf.find(rp.uf);
        groups.entry(root).or_default().1.push(i);
    }
    // node connectivity
    let mut nuf = Uf::new(nodes.len());
    for (ps, _) in groups.values() {
        for w in ps.windows(2) {
            nuf.union(pins[w[0]].node, pins[w[1]].node);
        }
    }
    let mut outs: HashMap<String, Vec<usize>> = HashMap::new();
    let mut ins: Vec<(usize, String)> = Vec::new();
    for (i, n) in nodes.iter().enumerate() {
        match &n.kind {
            NodeKind::ConnectorOut(id) => outs.entry(id.key()).or_default().push(i),
            NodeKind::ConnectorIn(id) => ins.push((i, id.key())),
            _ => {}
        }
    }
    for (name, list) in &outs {
        if list.len() > 1 {
            for &i in &list[1..] {
                diags.push(Diagnostic::error(format!("connector {} is defined more than once", name), nodes[i].span).with_clause("4.1.1"));
            }
        }
    }
    for (i, name) in &ins {
        match outs.get(name) {
            Some(list) => nuf.union(*i, list[0]),
            None => diags.push(Diagnostic::error(format!("no connector {} leads to this continuation", name), nodes[*i].span).with_clause("4.1.1")),
        }
    }
    // components
    let mut comps: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
    for i in 0..nodes.len() {
        let r = nuf.find(i);
        comps.entry(r).or_default().push(i);
    }
    let mut comp_of = vec![0usize; nodes.len()];
    let comp_list: Vec<Vec<usize>> = comps.into_values().collect();
    for (ci, members) in comp_list.iter().enumerate() {
        for &m in members {
            comp_of[m] = ci;
        }
    }
    let mut comp_groups: Vec<Vec<usize>> = vec![Vec::new(); comp_list.len()];
    for (root, (ps, _)) in &groups {
        if let Some(&p) = ps.first() {
            comp_groups[comp_of[pins[p].node]].push(*root);
        }
    }
    let mut networks: Vec<(usize, usize, Network)> = Vec::new();
    for (ci, members) in comp_list.iter().enumerate() {
        let mut members = members.clone();
        members.sort_by_key(|&i| (nodes[i].pos.0, nodes[i].pos.1));
        let mut local: HashMap<usize, usize> = HashMap::new();
        let mut out_nodes: Vec<Node> = Vec::new();
        for &m in &members {
            local.insert(m, out_nodes.len());
            out_nodes.push(nodes[m].clone());
        }
        let mut left_rail: Option<usize> = None;
        let mut right_rail: Option<usize> = None;
        let mut nets = Vec::new();
        let mut gs = comp_groups[ci].clone();
        gs.sort_by_key(|root| {
            let (ps, _) = &groups[root];
            ps.iter().map(|&p| (nodes[pins[p].node].pos, pins[p].driver as u8)).min()
        });
        for root in gs {
            let (ps, rps) = &groups[&root];
            let mut net = Net::default();
            for &rp in rps {
                let rp = &rail_pins[rp];
                let slot = if rp.left { &mut left_rail } else { &mut right_rail };
                let idx = match *slot {
                    Some(i) => i,
                    None => {
                        let kind = if rp.left { NodeKind::LeftRail } else { NodeKind::RightRail };
                        out_nodes.push(Node { kind, span: grid.span(rp.row, rp.col, rp.col + 1), pos: (rp.row as u32, rp.col as u32) });
                        *slot = Some(out_nodes.len() - 1);
                        out_nodes.len() - 1
                    }
                };
                let pr = PinRef { node: idx, pin: 0 };
                let list = if rp.left { &mut net.drivers } else { &mut net.sinks };
                if !list.contains(&pr) {
                    list.push(pr);
                }
                // keep the rail position at its topmost attachment
                let n = &mut out_nodes[idx];
                if (rp.row as u32) < n.pos.0 {
                    n.pos = (rp.row as u32, rp.col as u32);
                    n.span = grid.span(rp.row, rp.col, rp.col + 1);
                }
            }
            for &p in ps {
                let pd = &pins[p];
                let pr = PinRef { node: local[&pd.node], pin: pd.pin };
                if pd.driver {
                    net.drivers.push(pr);
                } else {
                    net.sinks.push(pr);
                }
            }
            if lang == GraphicLanguage::Fbd && net.drivers.len() > 1 {
                let n = &out_nodes[net.drivers[1].node];
                diags.push(
                    Diagnostic::error("outputs of elements shall not be connected together in FBD (no wired-OR, use an OR block)", n.span)
                        .with_clause("4.3.2"),
                );
            }
            if net.drivers.is_empty() && !net.sinks.is_empty() {
                let n = &out_nodes[net.sinks[0].node];
                if !matches!(n.kind, NodeKind::RightRail) {
                    diags.push(Diagnostic::error("this input is connected to a line that nothing drives", n.span).with_clause("4.1.2"));
                }
            }
            nets.push(net);
        }
        let first = members.iter().map(|&m| nodes[m].pos).min().unwrap_or((0, 0));
        let mut span = Span::default();
        for n in &out_nodes {
            span = span_union(span, n.span);
        }
        networks.push((first.0 as usize, first.1 as usize, Network { label: None, nodes: out_nodes, nets, span }));
    }
    networks.sort_by_key(|(r, c, _)| (*r, *c));
    // labels
    let mut labels: Vec<&(usize, String, Span)> = labels.iter().collect();
    labels.sort_by_key(|l| l.0);
    for (k, (row, name, span)) in labels.iter().enumerate() {
        let next_label = labels.get(k + 1).map(|l| l.0).unwrap_or(usize::MAX);
        match networks.iter_mut().find(|(r, _, n)| *r > *row && n.label.is_none()) {
            Some((r, _, n)) if *r < next_label => n.label = Some(Ident::new(name.clone(), *span)),
            _ => diags.push(Diagnostic::error(format!("network label {} is not followed by a network", name), *span).with_clause("4.1.2")),
        }
    }
    networks.into_iter().map(|(_, _, n)| n).collect()
}
