//! Semigraphic LD, FBD and SFC (clause 4, 2.6) parsing tests. Drawings are
//! taken from the standard (tables 57-62, figures 23-24, annex F) and
//! redrawn with correct alignment.

use structure_syntax::ast::*;
use structure_syntax::graphic::{ContactKind, CoilKind, Network, Node, NodeKind};
use structure_syntax::parser::parse;
use structure_syntax::source::SourceMap;
use structure_syntax::Severity;

struct Parsed {
    src: String,
    unit: SourceUnit,
    errors: Vec<String>,
}

fn parse_src(src: &str) -> Parsed {
    let mut sm = SourceMap::new();
    let id = sm.add("test.st", src);
    let r = parse(sm.get(id));
    let errors = r
        .diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .map(|d| {
            let at = d.span.map(|s| format!(" at {:?} '{}'", sm.get(id).line_col(s.start), &src[s.start as usize..s.end as usize])).unwrap_or_default();
            format!("{}{}", d.message, at)
        })
        .collect();
    Parsed { src: src.to_string(), unit: r.unit, errors }
}

impl Parsed {
    fn body(&self, i: usize) -> &Body {
        match &self.unit.elements[i] {
            LibraryElement::Pou(p) => &p.body,
            _ => panic!("not a POU"),
        }
    }
    fn graphic(&self) -> &GraphicBody {
        match self.body(0) {
            Body::Graphic(g) => g,
            b => panic!("not a graphic body: {:?}", b),
        }
    }
    fn ok(self) -> Parsed {
        if !self.errors.is_empty() {
            panic!("unexpected errors:\n{}", self.errors.join("\n"));
        }
        self
    }
    fn text(&self, e: &Expr) -> String {
        self.src[e.span.start as usize..e.span.end as usize].to_string()
    }
    fn node(&self, n: &Node) -> String {
        match &n.kind {
            NodeKind::LeftRail => "LeftRail".into(),
            NodeKind::RightRail => "RightRail".into(),
            NodeKind::Contact { var, kind } => {
                let k = match kind {
                    ContactKind::NormallyOpen => "NO",
                    ContactKind::NormallyClosed => "NC",
                    ContactKind::Positive => "P",
                    ContactKind::Negative => "N",
                };
                format!("{}({})", k, self.text(var))
            }
            NodeKind::Coil { var, kind } => {
                let k = match kind {
                    CoilKind::Normal => "Coil",
                    CoilKind::Negated => "NCoil",
                    CoilKind::Set => "SCoil",
                    CoilKind::Reset => "RCoil",
                    CoilKind::Positive => "PCoil",
                    CoilKind::Negative => "NegCoil",
                };
                format!("{}({})", k, self.text(var))
            }
            NodeKind::Block { type_name, instance, .. } => match instance {
                Some(i) => format!("{}:{}", i.text, type_name.text),
                None => type_name.text.clone(),
            },
            NodeKind::Source(e) => format!("Src({})", self.text(e)),
            NodeKind::Sink(e) => format!("Var({})", self.text(e)),
            NodeKind::Jump(l) => format!("Jump({})", l.text),
            NodeKind::Return => "Return".into(),
            NodeKind::ConnectorOut(l) => format!("ConnOut({})", l.text),
            NodeKind::ConnectorIn(l) => format!("ConnIn({})", l.text),
            NodeKind::ActionBlock { qualifier, action, .. } => format!("Action({} {})", qualifier.map(|q| q.as_str()).unwrap_or("-"), action.text),
        }
    }
    fn pin(&self, nw: &Network, p: &structure_syntax::graphic::PinRef, driver: bool) -> String {
        let n = &nw.nodes[p.node];
        let base = self.node(n);
        match &n.kind {
            NodeKind::Block { inputs, outputs, .. } => {
                let list = if driver { outputs } else { inputs };
                let bp = &list[p.pin];
                let name = bp.name.as_ref().map(|n| n.text.clone()).unwrap_or_else(|| format!("{}{}", if driver { "out" } else { "in" }, p.pin));
                format!("{}.{}{}", base, if bp.negated { "!" } else { "" }, name)
            }
            _ => base,
        }
    }
    /// The nets of a network as "drivers -> sinks" strings.
    fn nets(&self, nw: &Network) -> Vec<String> {
        let mut out: Vec<String> = nw
            .nets
            .iter()
            .map(|n| {
                let mut d: Vec<String> = n.drivers.iter().map(|p| self.pin(nw, p, true)).collect();
                let mut s: Vec<String> = n.sinks.iter().map(|p| self.pin(nw, p, false)).collect();
                d.sort();
                s.sort();
                format!("{} -> {}", d.join(" + "), s.join(", "))
            })
            .collect();
        out.sort();
        out
    }
    fn nodes(&self, nw: &Network) -> Vec<String> {
        nw.nodes.iter().map(|n| self.node(n)).collect()
    }
}

fn has(list: &[String], s: &str) -> bool {
    list.iter().any(|x| x == s)
}

macro_rules! assert_nets {
    ($p:expr, $nw:expr, [$($s:expr),* $(,)?]) => {{
        let nets = $p.nets($nw);
        let mut expected: Vec<String> = vec![$($s.to_string()),*];
        expected.sort();
        assert_eq!(nets, expected, "nets of network {:?}", $p.nodes($nw));
    }};
}

fn pou(kind: &str, decls: &str, body: &str) -> String {
    let end = match kind {
        "FUNCTION_BLOCK" => "END_FUNCTION_BLOCK",
        "PROGRAM" => "END_PROGRAM",
        _ => "END_FUNCTION",
    };
    format!("{} X\n{}\n{}{}\n", kind, decls, body, end)
}

// ---------------------------------------------------------------- LD basics

#[test]
fn ld_simple_rung() {
    let p = parse_src(&pou(
        "FUNCTION_BLOCK",
        "VAR_INPUT a, b : BOOL; END_VAR\nVAR_OUTPUT c : BOOL; END_VAR",
        r#"
|  a     b      c  |
+--| |---| |--( )--+
|                  |
"#,
    ))
    .ok();
    let g = p.graphic();
    assert_eq!(g.language, GraphicLanguage::Ld);
    assert_eq!(g.networks.len(), 1);
    let nw = &g.networks[0];
    assert_nets!(p, nw, ["LeftRail -> NO(a)", "NO(a) -> NO(b)", "NO(b) -> Coil(c)", "Coil(c) -> RightRail"]);
}

#[test]
fn ld_task_example_misaligned_names() {
    // variable names not exactly above their symbols are matched in order
    let p = parse_src(
        "FUNCTION_BLOCK X\nVAR_INPUT a, b : BOOL; END_VAR\nVAR_OUTPUT c : BOOL; END_VAR\n| a b c |\n+--| |---| |--( )--+\n|                  |\nEND_FUNCTION_BLOCK\n",
    )
    .ok();
    let nw = &p.graphic().networks[0];
    assert_nets!(p, nw, ["LeftRail -> NO(a)", "NO(a) -> NO(b)", "NO(b) -> Coil(c)", "Coil(c) -> RightRail"]);
}

#[test]
fn ld_contacts_and_coils_tables_61_62() {
    let p = parse_src(&pou(
        "FUNCTION_BLOCK",
        "VAR a, b, c, d, e, f, g, h, o1, o2, o3, o4, o5, o6 : BOOL; END_VAR",
        r#"
|   a     b     c     d               o1         |
+--| |---|/|---|P|---|N|--------------( )--------+
|   e     f     g     h       o2    o3    o4     |
+--! !---!/!---!P!---!N!------(/)---(S)---(R)----+
|                             o5    o6           |
+-----------------------------(P)---(N)----------+
|                                                |
"#,
    ))
    .ok();
    let g = p.graphic();
    assert_eq!(g.networks.len(), 3);
    assert_nets!(p, &g.networks[0], ["LeftRail -> NO(a)", "NO(a) -> NC(b)", "NC(b) -> P(c)", "P(c) -> N(d)", "N(d) -> Coil(o1)", "Coil(o1) -> RightRail"]);
    assert_nets!(
        p,
        &g.networks[1],
        ["LeftRail -> NO(e)", "NO(e) -> NC(f)", "NC(f) -> P(g)", "P(g) -> N(h)", "N(h) -> NCoil(o2)", "NCoil(o2) -> SCoil(o3)", "SCoil(o3) -> RCoil(o4)", "RCoil(o4) -> RightRail"]
    );
    assert_nets!(p, &g.networks[2], ["LeftRail -> PCoil(o5)", "PCoil(o5) -> NegCoil(o6)", "NegCoil(o6) -> RightRail"]);
}

#[test]
fn ld_coils_in_series_and_parallel_4_2_4() {
    let p = parse_src(&pou(
        "PROGRAM",
        "VAR a, b, c, d, e : BOOL; END_VAR",
        r#"
|   a    b       c    d    |
+--( )--| |--+--( )---( )--+
|            |      e      |
|            +-----( )-----+
|                          |
"#,
    ))
    .ok();
    let g = p.graphic();
    assert_eq!(g.networks.len(), 1);
    assert_nets!(
        p,
        &g.networks[0],
        ["LeftRail -> Coil(a)", "Coil(a) -> NO(b)", "NO(b) -> Coil(c), Coil(e)", "Coil(c) -> Coil(d)", "Coil(d) -> RightRail", "Coil(e) -> RightRail"]
    );
}

#[test]
fn ld_wired_or_figure_24a() {
    let p = parse_src(&pou(
        "PROGRAM",
        "VAR a, b, c : BOOL; END_VAR",
        r#"
|   a      c   |
+---||--+--()--+
|   b   |      |
+---||--+      |
|              |
"#,
    ))
    .ok();
    let g = p.graphic();
    assert_eq!(g.networks.len(), 1);
    assert_nets!(p, &g.networks[0], ["LeftRail -> NO(a)", "LeftRail -> NO(b)", "NO(a) + NO(b) -> Coil(c)", "Coil(c) -> RightRail"]);
}

#[test]
fn fbd_or_block_figure_24b() {
    let p = parse_src(&pou(
        "PROGRAM",
        "VAR a, b, c : BOOL; END_VAR",
        r#"
    +-----+
a---| >=1 |---c
b---|     |
    +-----+
"#,
    ))
    .ok();
    let g = p.graphic();
    assert_eq!(g.language, GraphicLanguage::Fbd);
    assert_nets!(p, &g.networks[0], ["Src(a) -> >=1.in0", "Src(b) -> >=1.in1", ">=1.out0 -> Var(c)"]);
}

#[test]
fn fbd_wired_or_is_an_error() {
    let p = parse_src(&pou(
        "PROGRAM",
        "VAR a, b, c, d : BOOL; END_VAR",
        r#"
    +---+
a---| & |---+---c
b---|   |   |
    +---+   |
d-----------+
"#,
    ));
    assert!(p.errors.iter().any(|e| e.contains("wired-OR")), "{:?}", p.errors);
}

// ---------------------------------------------------------------- feedback (figure 23)

#[test]
fn fbd_explicit_loop_figure_23a() {
    let p = parse_src(&pou(
        "PROGRAM",
        "VAR ENABLE, START1, START2 : BOOL; RUN : BOOL := FALSE; END_VAR",
        r#"
                 +---+
         ENABLE--| & |--RUN--+
              +--|   |       |
      +-----+ |  +---+       |
START1| >=1 |-+              |
------|     |                |
START2|     |                |
------|     |                |
   +--|     |                |
   |  +-----+                |
   +-------------------------+
"#,
    ));
    // START1/START2 are written above their lines here, which is not a
    // connection: expect errors for them, but the loop must be found.
    let g = p.graphic();
    let nw = g.networks.iter().find(|n| p.nodes(n).iter().any(|x| x == ">=1")).unwrap();
    let nets = p.nets(nw);
    assert!(has(&nets, ">=1.out0 -> &.in1"), "{:?}", nets);
    assert!(has(&nets, "&.out0 -> Var(RUN)") || nets.iter().any(|n| n.starts_with("&.out0 -> ")), "{:?}", nets);
}

#[test]
fn fbd_explicit_loop_figure_23a_redrawn() {
    let p = parse_src(&pou(
        "PROGRAM",
        "VAR ENABLE, START1, START2 : BOOL; RUN : BOOL := FALSE; END_VAR",
        r#"
                    +---+
           ENABLE---| & |---RUN---+
                +---|   |         |
         +---+  |   +---+         |
START1---|>=1|--+                 |
START2---|   |                    |
      +--|   |                    |
      |  +---+                    |
      +---------------------------+
"#,
    ))
    .ok();
    let g = p.graphic();
    assert_eq!(g.networks.len(), 1);
    assert_nets!(
        p,
        &g.networks[0],
        ["Src(ENABLE) -> &.in0", ">=1.out0 -> &.in1", "&.out0 -> Var(RUN)", "Var(RUN) -> >=1.in2", "Src(START1) -> >=1.in0", "Src(START2) -> >=1.in1"]
    );
    // RUN is a variable between two lines: assigned, then fed back
    let run = g.networks[0].nodes.iter().find(|n| matches!(&n.kind, NodeKind::Sink(e) if p.text(e) == "RUN")).unwrap();
    let _ = run;
}

#[test]
fn fbd_implicit_loop_figure_23b() {
    let p = parse_src(&pou(
        "PROGRAM",
        "VAR ENABLE, START1, START2 : BOOL; RUN : BOOL := FALSE; END_VAR",
        r#"
                    +---+
           ENABLE---| & |---RUN
                +---|   |
         +---+  |   +---+
START1---|>=1|--+
START2---|   |
   RUN---|   |
         +---+
"#,
    ))
    .ok();
    let g = p.graphic();
    assert_eq!(g.networks.len(), 1);
    assert_nets!(
        p,
        &g.networks[0],
        ["Src(ENABLE) -> &.in0", ">=1.out0 -> &.in1", "&.out0 -> Var(RUN)", "Src(RUN) -> >=1.in2", "Src(START1) -> >=1.in0", "Src(START2) -> >=1.in1"]
    );
}

#[test]
fn ld_feedback_figure_23c() {
    let p = parse_src(&pou(
        "PROGRAM",
        "VAR ENABLE, START1, START2 : BOOL; RUN : BOOL := FALSE; END_VAR",
        r#"
| START1     ENABLE     RUN   |
+---| |----+---| |------( )---+
| START2   |                  |
+---| |----+                  |
|   RUN    |                  |
+---| |----+                  |
|                             |
"#,
    ))
    .ok();
    let g = p.graphic();
    assert_eq!(g.language, GraphicLanguage::Ld);
    assert_eq!(g.networks.len(), 1);
    assert_nets!(
        p,
        &g.networks[0],
        [
            "LeftRail -> NO(START1)",
            "LeftRail -> NO(START2)",
            "LeftRail -> NO(RUN)",
            "NO(RUN) + NO(START1) + NO(START2) -> NO(ENABLE)",
            "NO(ENABLE) -> Coil(RUN)",
            "Coil(RUN) -> RightRail"
        ]
    );
}

// ---------------------------------------------------------------- execution control (table 58)

#[test]
fn fbd_jumps_and_labels_table_58() {
    let p = parse_src(&pou(
        "PROGRAM",
        "VAR X : BOOL; END_VAR",
        r#"
1---->>LABELA

X---->>LABELB

        +---+
%IX20---| & |--->>NEXT
%MX50---|   |
        +---+
LABELA:
X---<RETURN>
LABELB:
NEXT:
        +---+
%IX25---|>=1|---%QX100
%MX60---|   |
        +---+
"#,
    ));
    let g = p.graphic();
    assert_eq!(g.language, GraphicLanguage::Fbd);
    // LABELB is followed directly by NEXT: error, and NEXT labels the OR network
    assert!(p.errors.iter().any(|e| e.contains("LABELB")), "{:?}", p.errors);
    assert_eq!(p.errors.len(), 1, "{:?}", p.errors);
    assert_eq!(g.networks.len(), 5);
    assert_nets!(p, &g.networks[0], ["Src(1) -> Jump(LABELA)"]);
    assert_nets!(p, &g.networks[1], ["Src(X) -> Jump(LABELB)"]);
    assert_nets!(p, &g.networks[2], ["Src(%IX20) -> &.in0", "Src(%MX50) -> &.in1", "&.out0 -> Jump(NEXT)"]);
    assert_eq!(g.networks[3].label.as_ref().unwrap().text, "LABELA");
    assert_nets!(p, &g.networks[3], ["Src(X) -> Return"]);
    assert_eq!(g.networks[4].label.as_ref().unwrap().text, "NEXT");
    assert_nets!(p, &g.networks[4], ["Src(%IX25) -> >=1.in0", "Src(%MX60) -> >=1.in1", ">=1.out0 -> Var(%QX100)"]);
}

#[test]
fn ld_jumps_and_returns_table_58() {
    let p = parse_src(&pou(
        "FUNCTION_BLOCK",
        "VAR X : BOOL; END_VAR",
        r#"
|
+---->>LABELA
|
| X
+-| |---->>LABELB
|
|   %IX20   %MX50
+---| |-----| |--->>NEXT
|
|   X
+--| |---<RETURN>
|
LABELA:
|
+---<RETURN>
|
LABELB:
|
NEXT:
|   %IX25      %QX100 |
+----| |----+----( )---+
|   %MX60   |          |
+----| |----+          |
|                      |
"#,
    ));
    assert!(p.errors.iter().any(|e| e.contains("LABELB")), "{:?}", p.errors);
    let g = p.graphic();
    assert_eq!(g.language, GraphicLanguage::Ld);
    let n = &g.networks;
    assert_eq!(n.len(), 6, "{:?}", n.iter().map(|x| p.nodes(x)).collect::<Vec<_>>());
    assert_nets!(p, &n[0], ["LeftRail -> Jump(LABELA)"]);
    assert_nets!(p, &n[1], ["LeftRail -> NO(X)", "NO(X) -> Jump(LABELB)"]);
    assert_nets!(p, &n[2], ["LeftRail -> NO(%IX20)", "NO(%IX20) -> NO(%MX50)", "NO(%MX50) -> Jump(NEXT)"]);
    assert_nets!(p, &n[3], ["LeftRail -> NO(X)", "NO(X) -> Return"]);
    assert_eq!(n[4].label.as_ref().unwrap().text, "LABELA");
    assert_nets!(p, &n[4], ["LeftRail -> Return"]);
    assert_eq!(n[5].label.as_ref().unwrap().text, "NEXT");
    assert_nets!(
        p,
        &n[5],
        ["LeftRail -> NO(%IX25)", "LeftRail -> NO(%MX60)", "NO(%IX25) + NO(%MX60) -> Coil(%QX100)", "Coil(%QX100) -> RightRail"]
    );
}

#[test]
fn connectors_table_57_feature_13() {
    let p = parse_src(&pou(
        "PROGRAM",
        "VAR a, b, c : BOOL; END_VAR",
        r#"
    +---+
a---| & |---------->OTTO>
b---|   |
    +---+

>OTTO>----------c
"#,
    ))
    .ok();
    let g = p.graphic();
    assert_eq!(g.networks.len(), 1, "connected by the connector pair");
    assert_nets!(p, &g.networks[0], ["Src(a) -> &.in0", "Src(b) -> &.in1", "&.out0 -> ConnOut(OTTO)", "ConnIn(OTTO) -> Var(c)"]);
}

#[test]
fn connector_without_source_is_an_error() {
    let p = parse_src(&pou("PROGRAM", "VAR c : BOOL; END_VAR", "\n>OTTO>----------c\n"));
    assert!(p.errors.iter().any(|e| e.contains("OTTO")), "{:?}", p.errors);
}

#[test]
fn line_crossings_and_corners_table_57() {
    let p = parse_src(&pou(
        "PROGRAM",
        "VAR a, b, c, d : BOOL; END_VAR",
        r#"
          +---+
a-----+---|   |
      |   | & |----c
b-----|---|   |
      |   +---+
      |   +---+
      +---|   |
          |>=1|----d
      +---|   |
      |   +---+
b-----+
"#,
    ))
    .ok();
    let g = p.graphic();
    assert_eq!(g.networks.len(), 1);
    assert_nets!(
        p,
        &g.networks[0],
        ["Src(a) -> &.in0, >=1.in0", "Src(b) -> &.in1", "Src(b) -> >=1.in1", "&.out0 -> Var(c)", ">=1.out0 -> Var(d)"]
    );
}
