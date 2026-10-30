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
            NodeKind::Sink(Expr { kind: ExprKind::Name(i), .. }) => format!("Var({})", i.text),
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

// ---------------------------------------------------------------- annex F

const WEIGH_DECL: &str = "VAR_INPUT\n  weigh_command : BOOL;\n  gross_weight : WORD;\n  tare_weight : INT;\nEND_VAR";

fn weigh_expected() -> Vec<&'static str> {
    vec![
        "NO(weigh_command) -> BCD_TO_INT.EN",
        "BCD_TO_INT.ENO -> SUB.EN",
        "SUB.ENO -> INT_TO_BCD.EN",
        "Src(gross_weight) -> BCD_TO_INT.in1",
        "BCD_TO_INT.out1 -> SUB.in1",
        "Src(tare_weight) -> SUB.in2",
        "SUB.out1 -> INT_TO_BCD.in1",
        "INT_TO_BCD.out1 -> Var(WEIGH)",
    ]
}

#[test]
fn f1_weigh_ld() {
    let src = format!(
        "FUNCTION WEIGH : WORD\n{}\n{}END_FUNCTION\n",
        WEIGH_DECL,
        r#"
|                 +--------+      +-------+      +--------+             |
|                 |  BCD_  |      |  SUB  |      |  INT_  |             |
| weigh_command   | TO_INT |      |       |      | TO_BCD |    ENO      |
+------| |--------|EN   ENO|------|EN  ENO|------|EN   ENO|----( )------+
|                 |        |      |       |      |        |             |
| gross_weight----|        |------|       |------|        |---WEIGH     |
|                 +--------+      |       |      +--------+             |
| tare_weight---------------------|       |                             |
|                                 +-------+                             |
"#
    );
    let p = parse_src(&src).ok();
    let g = p.graphic();
    assert_eq!(g.language, GraphicLanguage::Ld);
    assert_eq!(g.networks.len(), 1);
    let mut exp = weigh_expected();
    exp.extend(["LeftRail -> NO(weigh_command)", "INT_TO_BCD.ENO -> Coil(ENO)", "Coil(ENO) -> RightRail"]);
    let nets = p.nets(&g.networks[0]);
    let mut exp: Vec<String> = exp.iter().map(|s| s.to_string()).collect();
    exp.sort();
    assert_eq!(nets, exp);
    // block pins: EN/ENO are ordinary named pins, the others are unnamed
    let sub = g.networks[0].nodes.iter().find(|n| matches!(&n.kind, NodeKind::Block { type_name, .. } if type_name.text == "SUB")).unwrap();
    let NodeKind::Block { inputs, outputs, instance, .. } = &sub.kind else { panic!() };
    assert!(instance.is_none());
    assert_eq!(inputs.len(), 3);
    assert_eq!(inputs[0].name.as_ref().unwrap().text, "EN");
    assert!(inputs[1].name.is_none() && inputs[2].name.is_none());
    assert_eq!(outputs.len(), 2);
    assert_eq!(outputs[0].name.as_ref().unwrap().text, "ENO");
}

#[test]
fn f1_weigh_fbd() {
    let src = format!(
        "FUNCTION WEIGH : WORD\n{}\n{}END_FUNCTION\n",
        WEIGH_DECL,
        r#"
                  +--------+      +-------+      +--------+
                  |  BCD_  |      |  SUB  |      |  INT_  |
                  | TO_INT |      |       |      | TO_BCD |
weigh_command-----|EN   ENO|------|EN  ENO|------|EN   ENO|---ENO
gross_weight------|        |------|       |------|        |---WEIGH
                  +--------+      |       |      +--------+
tare_weight-----------------------|       |
                                  +-------+
"#
    );
    let p = parse_src(&src).ok();
    let g = p.graphic();
    assert_eq!(g.language, GraphicLanguage::Fbd);
    assert_eq!(g.networks.len(), 1);
    let mut exp = weigh_expected();
    exp[0] = "Src(weigh_command) -> BCD_TO_INT.EN";
    exp.push("INT_TO_BCD.ENO -> Var(ENO)");
    let mut exp: Vec<String> = exp.iter().map(|s| s.to_string()).collect();
    exp.sort();
    assert_eq!(p.nets(&g.networks[0]), exp);
}

const CMD_MONITOR_DECL: &str = "VAR_INPUT AUTO_CMD, AUTO_MODE, MAN_CMD, MAN_CMD_CHK : BOOL; T_CMD_MAX : TIME; FDBK, ACK : BOOL; END_VAR
VAR_OUTPUT CMD, ALRM : BOOL; END_VAR
VAR CMD_TMR : TON; ALRM_FF : SR; END_VAR";

#[test]
fn f2_cmd_monitor_ld() {
    let src = format!(
        "FUNCTION_BLOCK CMD_MONITOR\n{}\n{}END_FUNCTION_BLOCK\n",
        CMD_MONITOR_DECL,
        r#"
|                                                |
| AUTO_MODE  AUTO_CMD                     CMD    |
+---| |--------| |-----------------+-----( )-----+
|                                  |             |
| AUTO_MODE  MAN_CMD   MAN_CMD_CHK |             |
+---|/|--------| |--------|/|------+             |
|                                                |
| ACK                                   ALRM     |
+---| |--------------------------------(R)-------+
|                                                |
|            CMD_TMR                             |
|            +-----+                             |
| CMD        | TON |       FDBK        ALRM      |
+---| |------|IN  Q|-------|/|---------(S)-------+
|            |     |                             |
| T_CMD_MAX--|PT ET|                             |
|            +-----+                             |
|                                                |
"#
    );
    let p = parse_src(&src).ok();
    let g = p.graphic();
    assert_eq!(g.language, GraphicLanguage::Ld);
    assert_eq!(g.networks.len(), 3);
    assert_nets!(
        p,
        &g.networks[0],
        [
            "LeftRail -> NO(AUTO_MODE)",
            "LeftRail -> NC(AUTO_MODE)",
            "NO(AUTO_MODE) -> NO(AUTO_CMD)",
            "NC(AUTO_MODE) -> NO(MAN_CMD)",
            "NO(MAN_CMD) -> NC(MAN_CMD_CHK)",
            "NC(MAN_CMD_CHK) + NO(AUTO_CMD) -> Coil(CMD)",
            "Coil(CMD) -> RightRail"
        ]
    );
    assert_nets!(p, &g.networks[1], ["LeftRail -> NO(ACK)", "NO(ACK) -> RCoil(ALRM)", "RCoil(ALRM) -> RightRail"]);
    assert_nets!(
        p,
        &g.networks[2],
        [
            "LeftRail -> NO(CMD)",
            "NO(CMD) -> CMD_TMR:TON.IN",
            "Src(T_CMD_MAX) -> CMD_TMR:TON.PT",
            "CMD_TMR:TON.Q -> NC(FDBK)",
            "NC(FDBK) -> SCoil(ALRM)",
            "SCoil(ALRM) -> RightRail"
        ]
    );
    // unconnected ET is still listed
    let tmr = g.networks[2].nodes.iter().find(|n| matches!(n.kind, NodeKind::Block { .. })).unwrap();
    let NodeKind::Block { outputs, .. } = &tmr.kind else { panic!() };
    assert_eq!(outputs.iter().map(|o| o.name.as_ref().unwrap().text.clone()).collect::<Vec<_>>(), ["Q", "ET"]);
}

#[test]
fn f2_cmd_monitor_fbd() {
    let src = format!(
        "FUNCTION_BLOCK CMD_MONITOR\n{}\n{}END_FUNCTION_BLOCK\n",
        CMD_MONITOR_DECL,
        r#"
                 +-+     +---+
AUTO_CMD---------|&|-----|>=1|--+--------------------------------CMD
AUTO_MODE---+----| |  +--|   |  |
            |    +-+  |  +---+  |
            |         |         |   CMD_TMR              ALRM_FF
            |    +-+  |         |   +-----+              +-----+
            +---O|&|  |         |   | TON |       +-+    | SR  |
MAN_CMD----------| |--+         +---|IN  Q|-------|&|----|S1 Q1|---ALRM
MAN_CMD_CHK-----O| |                |     |   +--O| | +--|R    |
                 +-+                |     |   |   +-+ |  +-----+
T_CMD_MAX---------------------------|PT ET|   |       |
                                    +-----+   |       |
FDBK------------------------------------------+       |
ACK---------------------------------------------------+
"#
    );
    let p = parse_src(&src).ok();
    let g = p.graphic();
    assert_eq!(g.language, GraphicLanguage::Fbd);
    assert_eq!(g.networks.len(), 1);
    assert_nets!(
        p,
        &g.networks[0],
        [
            "Src(AUTO_CMD) -> &.in0",
            "Src(AUTO_MODE) -> &.!in0, &.in1",
            "&.out0 -> >=1.in0",
            "&.out0 -> >=1.in1",
            "Src(MAN_CMD) -> &.in1",
            "Src(MAN_CMD_CHK) -> &.!in2",
            ">=1.out0 -> CMD_TMR:TON.IN, Var(CMD)",
            "Src(T_CMD_MAX) -> CMD_TMR:TON.PT",
            "CMD_TMR:TON.Q -> &.in0",
            "Src(FDBK) -> &.!in1",
            "&.out0 -> ALRM_FF:SR.S1",
            "Src(ACK) -> ALRM_FF:SR.R",
            "ALRM_FF:SR.Q1 -> Var(ALRM)"
        ]
    );
}

const STACK_INT_DECL: &str = "VAR_INPUT PUSH, POP : BOOL; R1 : BOOL; IN : INT; N : INT; END_VAR
VAR_OUTPUT EMPTY : BOOL := 1; OFLO : BOOL := 0; OUT : INT := 0; END_VAR
VAR STK : ARRAY[0..127] OF INT; NI : INT := 128; PTR : INT := -1; END_VAR";

#[test]
fn f4_stack_int_ld() {
    let src = format!(
        "FUNCTION_BLOCK STACK_INT\n{}\n{}END_FUNCTION_BLOCK\n",
        STACK_INT_DECL,
        r#"
|                                                                    |
|    R1                                                              |
+---| |--->>RESET                                                    |
|                                                                    |
|  POP   EMPTY                                                       |
+--| |----|/|--->>POP_STK                                            |
|                                                                    |
|  PUSH   OFLO                                                       |
+--| |----|/|--->>PUSH_STK                                           |
|                                                                    |
+---------<RETURN>                                                   |
|                                                                    |

RESET:
|      +--------+           +--------+           +-------+            |
|      |  MOVE  |           |  MOVE  |           | LIMIT |     OFLO   |
+------|EN   ENO|-----------|EN   ENO|-----------|EN  ENO|--+--(R)----+
|  0---|        |---OUT -1--|        |---PTR 128-|MX     |  |  EMPTY  |
|      +--------+           +--------+        N--|IN     |  +--(S)----+
|                                             1--|MN     |---NI       |
|                                                +-------+            |
+---------<RETURN>                                                    |
|                                                                     |

POP_STK:
|            +--------+       +--------+          |
|            |  SUB   |       |   LT   |          |
+------------|EN   ENO|-------|EN   ENO|   EMPTY  |
|      PTR---|        |--PTR--|        |----(S)---+
|        1---|        |    0--|        |          |
|            +--------+       +--------+          |
|                                                 |
|                  +-------+                      |
|                  |  SEL  |              OFLO    |
+------------------|EN  ENO|--------------(R)-----+
|  EMPTY           |       |                      |
+---| |------------|G      |---OUT                |
|      STK[PTR]----|IN0    |                      |
|             0----|IN1    |                      |
|                  +-------+                      |
+--------------------------------<RETURN>         |
|                                                 |

PUSH_STK:
|                                                 |
|            +--------+       +--------+          |
|            |  ADD   |       |   EQ   |          |
+------------|EN   ENO|-------|EN   ENO|   OFLO   |
|      PTR---|        |--PTR--|        |----(S)---+
|        1---|        |   NI--|        |          |
|            +--------+       +--------+          |
|                                                 |
|             +------+                            |
|  OFLO       | MOVE |                            |
+---|/|-------|EN ENO|----------------------------+
|       IN----|      |---STK[PTR]                 |
|             +------+                            |
|                                                 |
|             +-------+                           |
|             |  SEL  |                  EMPTY    |
+-------------|EN  ENO|------------------(R)------+
|  OFLO       |       |                           |
+---| |-------|G      |---OUT                     |
|        IN---|IN0    |                           |
|         0---|IN1    |                           |
|             +-------+                           |
"#
    );
    let p = parse_src(&src).ok();
    let g = p.graphic();
    assert_eq!(g.language, GraphicLanguage::Ld);
    let n = &g.networks;
    let summary: Vec<_> = n.iter().map(|x| (x.label.as_ref().map(|l| l.text.clone()), p.nodes(x))).collect();
    assert_eq!(n.len(), 12, "{:#?}", summary);
    assert_nets!(p, &n[0], ["LeftRail -> NO(R1)", "NO(R1) -> Jump(RESET)"]);
    assert_nets!(p, &n[1], ["LeftRail -> NO(POP)", "NO(POP) -> NC(EMPTY)", "NC(EMPTY) -> Jump(POP_STK)"]);
    assert_nets!(p, &n[2], ["LeftRail -> NO(PUSH)", "NO(PUSH) -> NC(OFLO)", "NC(OFLO) -> Jump(PUSH_STK)"]);
    assert_nets!(p, &n[3], ["LeftRail -> Return"]);
    assert_eq!(n[4].label.as_ref().unwrap().text, "RESET");
    assert_nets!(
        p,
        &n[4],
        [
            "LeftRail -> MOVE.EN",
            "Src(0) -> MOVE.in1",
            "MOVE.out1 -> Var(OUT)",
            "MOVE.ENO -> MOVE.EN",
            "Src(-1) -> MOVE.in1",
            "MOVE.out1 -> Var(PTR)",
            "MOVE.ENO -> LIMIT.EN",
            "Src(128) -> LIMIT.MX",
            "Src(N) -> LIMIT.IN",
            "Src(1) -> LIMIT.MN",
            "LIMIT.out1 -> Var(NI)",
            "LIMIT.ENO -> RCoil(OFLO), SCoil(EMPTY)",
            "RCoil(OFLO) -> RightRail",
            "SCoil(EMPTY) -> RightRail"
        ]
    );
    assert_nets!(p, &n[5], ["LeftRail -> Return"]);
    assert_eq!(n[6].label.as_ref().unwrap().text, "POP_STK");
    assert_nets!(
        p,
        &n[6],
        [
            "LeftRail -> SUB.EN",
            "Src(PTR) -> SUB.in1",
            "Src(1) -> SUB.in2",
            "SUB.ENO -> LT.EN",
            "SUB.out1 -> Var(PTR)",
            "Var(PTR) -> LT.in1",
            "Src(0) -> LT.in2",
            "LT.out1 -> SCoil(EMPTY)",
            "SCoil(EMPTY) -> RightRail"
        ]
    );
    assert_nets!(
        p,
        &n[7],
        [
            "LeftRail -> SEL.EN",
            "LeftRail -> NO(EMPTY)",
            "NO(EMPTY) -> SEL.G",
            "Src(STK[PTR]) -> SEL.IN0",
            "Src(0) -> SEL.IN1",
            "SEL.out1 -> Var(OUT)",
            "SEL.ENO -> RCoil(OFLO)",
            "RCoil(OFLO) -> RightRail"
        ]
    );
    assert_nets!(p, &n[8], ["LeftRail -> Return"]);
    assert_eq!(n[9].label.as_ref().unwrap().text, "PUSH_STK");
    assert_nets!(
        p,
        &n[9],
        [
            "LeftRail -> ADD.EN",
            "Src(PTR) -> ADD.in1",
            "Src(1) -> ADD.in2",
            "ADD.ENO -> EQ.EN",
            "ADD.out1 -> Var(PTR)",
            "Var(PTR) -> EQ.in1",
            "Src(NI) -> EQ.in2",
            "EQ.out1 -> SCoil(OFLO)",
            "SCoil(OFLO) -> RightRail"
        ]
    );
    assert_nets!(p, &n[10], ["LeftRail -> NC(OFLO)", "NC(OFLO) -> MOVE.EN", "Src(IN) -> MOVE.in1", "MOVE.out1 -> Var(STK[PTR])", "MOVE.ENO -> RightRail"]);
    assert_nets!(
        p,
        &n[11],
        [
            "LeftRail -> SEL.EN",
            "LeftRail -> NO(OFLO)",
            "NO(OFLO) -> SEL.G",
            "Src(IN) -> SEL.IN0",
            "Src(0) -> SEL.IN1",
            "SEL.out1 -> Var(OUT)",
            "SEL.ENO -> RCoil(EMPTY)",
            "RCoil(EMPTY) -> RightRail"
        ]
    );
}

#[test]
fn f4_stack_int_fbd_dispatch() {
    let src = format!(
        "FUNCTION_BLOCK STACK_INT\n{}\n{}END_FUNCTION_BLOCK\n",
        STACK_INT_DECL,
        r#"
R1-----+--->>RESET
       |                              +-+
       +-----------------------------O|&|---<RETURN>
       |    +-+    +-----------------O| |
       +---O|&|    |              +--O| |
POP---------| |----+--->>POP_STK  |   +-+
EMPTY------O| |    |              |
            +-+    |    +-+       |
                   +---O|&|-------+--->>PUSH_STK
R1---------------------O| |
PUSH--------------------| |
OFLO-------------------O| |
                        +-+
RESET:
1---->>X
POP_STK:
1---->>X
PUSH_STK:
1---->>X
X:
1---<RETURN>
"#
    );
    let p = parse_src(&src).ok();
    let g = p.graphic();
    assert_eq!(g.language, GraphicLanguage::Fbd);
    assert_eq!(g.networks.len(), 5);
    assert_nets!(
        p,
        &g.networks[0],
        [
            "Src(R1) -> &.!in0, &.!in0, Jump(RESET)",
            "Src(POP) -> &.in1",
            "Src(EMPTY) -> &.!in2",
            "&.out0 -> &.!in0, &.!in1, Jump(POP_STK)",
            "Src(R1) -> &.!in1",
            "Src(PUSH) -> &.in2",
            "Src(OFLO) -> &.!in3",
            "&.out0 -> &.!in2, Jump(PUSH_STK)",
            "&.out0 -> Return"
        ]
    );
    assert_eq!(g.networks[4].label.as_ref().unwrap().text, "X");
}

// ---------------------------------------------------------------- block features

fn block<'a>(nw: &'a Network, ty: &str) -> &'a Node {
    nw.nodes.iter().find(|n| matches!(&n.kind, NodeKind::Block { type_name, .. } if type_name.text == ty)).unwrap_or_else(|| panic!("no block {}", ty))
}

/// (formal name, negated, edge) of each pin.
type PinDesc = (Option<String>, bool, Option<Edge>);

fn pins(n: &Node) -> (Vec<PinDesc>, Vec<PinDesc>) {
    let NodeKind::Block { inputs, outputs, .. } = &n.kind else { panic!("not a block") };
    let f = |v: &Vec<structure_syntax::graphic::BlockPin>| v.iter().map(|p| (p.name.as_ref().map(|n| n.text.clone()), p.negated, p.edge)).collect();
    (f(inputs), f(outputs))
}

#[test]
fn negation_table_19_and_figure_5() {
    let p = parse_src(&pou(
        "PROGRAM",
        "VAR A, B, C, D, ENABLE, NO_ERR, X : INT; END_VAR",
        r#"
    +-----+
    | ADD |
B---|     |---A
C---|     |
D---|     |
    +-----+

    +-----+
    | SHL |
B---|IN   |---A
C---|N    |
    +-----+

         +---------+
         |   SHL   |
ENABLE---|EN    ENO|O---NO_ERR
     B---|IN       |---A
     C---|N        |
         +---------+

    +-----+
    | INC |
    |     |---A
X---|V---V|---X
    +-----+
"#,
    ))
    .ok();
    let g = p.graphic();
    let n = &g.networks;
    assert_nets!(p, &n[0], ["Src(B) -> ADD.in0", "Src(C) -> ADD.in1", "Src(D) -> ADD.in2", "ADD.out0 -> Var(A)"]);
    assert_nets!(p, &n[1], ["Src(B) -> SHL.IN", "Src(C) -> SHL.N", "SHL.out0 -> Var(A)"]);
    assert_nets!(p, &n[2], ["Src(ENABLE) -> SHL.EN", "Src(B) -> SHL.IN", "Src(C) -> SHL.N", "SHL.!ENO -> Var(NO_ERR)", "SHL.out1 -> Var(A)"]);
    let (i, o) = pins(block(&n[2], "SHL"));
    assert_eq!(i, vec![(Some("EN".into()), false, None), (Some("IN".into()), false, None), (Some("N".into()), false, None)]);
    assert_eq!(o, vec![(Some("ENO".into()), true, None), (None, false, None)]);
    assert_nets!(p, &n[3], ["Src(X) -> INC.V", "INC.out0 -> Var(A)", "INC.V -> Var(X)"]);
    let (i, o) = pins(block(&n[3], "INC"));
    assert_eq!(i, vec![(Some("V".into()), false, None)]);
    assert_eq!(o, vec![(None, false, None), (Some("V".into()), false, None)]);
}

#[test]
fn negated_inputs_and_outputs() {
    let p = parse_src(&pou(
        "PROGRAM",
        "VAR A, B, C, D : BOOL; END_VAR",
        r#"
     +---+
A---O| & |---B
C----|   |O--D
     +---+
"#,
    ))
    .ok();
    let nw = &p.graphic().networks[0];
    assert_nets!(p, nw, ["Src(A) -> &.!in0", "Src(C) -> &.in1", "&.out0 -> Var(B)", "&.!out1 -> Var(D)"]);
}

#[test]
fn en_eno_table_20() {
    let p = parse_src(&pou(
        "PROGRAM",
        "VAR A, B, C, ADD_EN, ADD_OK : INT; END_VAR",
        r#"
|          +-------+          |
| ADD_EN   |  ADD  |  ADD_OK  |
+---| |----|EN  ENO|---( )----+
|          |       |          |
|     A----|       |----C     |
|     B----|       |          |
|          +-------+          |
"#,
    ))
    .ok();
    let g = p.graphic();
    assert_eq!(g.language, GraphicLanguage::Ld);
    assert_nets!(
        p,
        &g.networks[0],
        ["LeftRail -> NO(ADD_EN)", "NO(ADD_EN) -> ADD.EN", "ADD.ENO -> Coil(ADD_OK)", "Coil(ADD_OK) -> RightRail", "Src(A) -> ADD.in1", "Src(B) -> ADD.in2", "ADD.out1 -> Var(C)"]
    );
    // features 2, 3, 4: operator symbol in a pin row, EN only, ENO only
    let p = parse_src(&pou(
        "PROGRAM",
        "VAR A, B, C, ADD_EN, ADD_OK : INT; END_VAR",
        r#"
    +-----+
A---|  +  |---C
B---|     |
    +-----+

         +-----+
ADD_EN---|EN   |
     A---|  +  |---C
     B---|     |
         +-----+

    +-----+
    |  ENO|---ADD_OK
A---|  +  |---C
B---|     |
    +-----+
"#,
    ))
    .ok();
    let n = &p.graphic().networks;
    assert_eq!(n.len(), 3);
    assert_nets!(p, &n[0], ["Src(A) -> +.in0", "Src(B) -> +.in1", "+.out0 -> Var(C)"]);
    assert_nets!(p, &n[1], ["Src(ADD_EN) -> +.EN", "Src(A) -> +.in1", "Src(B) -> +.in2", "+.out0 -> Var(C)"]);
    assert_nets!(p, &n[2], ["Src(A) -> +.in0", "Src(B) -> +.in1", "+.ENO -> Var(ADD_OK)", "+.out1 -> Var(C)"]);
}

#[test]
fn edge_inputs_and_timer_symbols() {
    let p = parse_src(&pou(
        "PROGRAM",
        "VAR PUSH, POP, R1, E, O, X, Y, Q1, Q2 : BOOL; IN, N, OUT : INT; T1 : TIME; S : STACK_INT; TM1, TM2 : TON; END_VAR",
        r#"
             S
       +-----------+
       | STACK_INT |
PUSH--->PUSH  EMPTY|---E
POP---->POP    OFLO|---O
R1-----|R1      OUT|---OUT
IN-----|IN         |
N------|N          |
       +-----------+

       TM1
    +-------+
X---|IN    Q|---Q1
T1--|PT     |
    | T---0 |
    +-------+

       TM2
    +-------+
    | 0---T |
Y---<IN    Q|---Q2
T1--|PT     |
    +-------+
"#,
    ))
    .ok();
    let n = &p.graphic().networks;
    assert_eq!(n.len(), 3);
    let (i, o) = pins(block(&n[0], "STACK_INT"));
    assert_eq!(i[0], (Some("PUSH".into()), false, Some(Edge::Rising)));
    assert_eq!(i[1], (Some("POP".into()), false, Some(Edge::Rising)));
    assert_eq!(i[2], (Some("R1".into()), false, None));
    assert_eq!(o.len(), 3);
    assert_nets!(
        p,
        &n[0],
        [
            "Src(PUSH) -> S:STACK_INT.PUSH",
            "Src(POP) -> S:STACK_INT.POP",
            "Src(R1) -> S:STACK_INT.R1",
            "Src(IN) -> S:STACK_INT.IN",
            "Src(N) -> S:STACK_INT.N",
            "S:STACK_INT.EMPTY -> Var(E)",
            "S:STACK_INT.OFLO -> Var(O)",
            "S:STACK_INT.OUT -> Var(OUT)"
        ]
    );
    assert_nets!(p, &n[1], ["Src(X) -> TM1:T---0.IN", "Src(T1) -> TM1:T---0.PT", "TM1:T---0.Q -> Var(Q1)"]);
    let (i, _) = pins(block(&n[2], "0---T"));
    assert_eq!(i[0], (Some("IN".into()), false, Some(Edge::Falling)));
}

#[test]
fn f6_limits_alarm_fbd() {
    let p = parse_src(&pou(
        "FUNCTION_BLOCK",
        "VAR_INPUT H, X, L, EPS : REAL; END_VAR VAR_OUTPUT QH, Q, QL : BOOL; END_VAR VAR HIGH_ALARM, LOW_ALARM : HYSTERESIS; END_VAR",
        r#"
                                    HIGH_ALARM
                                  +------------+
                                  | HYSTERESIS |
X------------------------+--------|XIN1       Q|--+----------QH
                  +---+  |        |            |  |
H-----------------| - |--|--------|XIN2        |  |
               +--|   |  |        |            |  |
               |  +---+  |  +-----|EPS         |  |  +-----+
       +---+   |         |  |     +------------+  +--| >=1 |
EPS----| / |---+---------|--+                        |     |---Q
2.0----|   |   |         |  |      LOW_ALARM      +--|     |
       +---+   |         |  |     +------------+  |  +-----+
               |  +---+  |  |     | HYSTERESIS |  |
L-----------------| + |--|--|-----|XIN1       Q|--+----------QL
               |  |   |  +--|-----|XIN2        |
               +--|   |     |     |            |
                  +---+     +-----|EPS         |
                                  +------------+
"#,
    ))
    .ok();
    let g = p.graphic();
    assert_eq!(g.networks.len(), 1);
    assert_nets!(
        p,
        &g.networks[0],
        [
            "Src(X) -> HIGH_ALARM:HYSTERESIS.XIN1, LOW_ALARM:HYSTERESIS.XIN2",
            "Src(H) -> -.in0",
            "Src(EPS) -> /.in0",
            "Src(2.0) -> /.in1",
            "/.out0 -> +.in1, -.in1, HIGH_ALARM:HYSTERESIS.EPS, LOW_ALARM:HYSTERESIS.EPS",
            "-.out0 -> HIGH_ALARM:HYSTERESIS.XIN2",
            "Src(L) -> +.in0",
            "+.out0 -> LOW_ALARM:HYSTERESIS.XIN1",
            "HIGH_ALARM:HYSTERESIS.Q -> >=1.in0, Var(QH)",
            "LOW_ALARM:HYSTERESIS.Q -> >=1.in1, Var(QL)",
            ">=1.out0 -> Var(Q)"
        ]
    );
}

#[test]
fn two_line_block_names() {
    let p = parse_src(&pou(
        "FUNCTION_BLOCK",
        "VAR_INPUT X : REAL; L : ANALOG_LIMITS; END_VAR VAR_OUTPUT SE : BOOL; END_VAR VAR SIGNAL_ALARM : LIMITS_ALARM; END_VAR",
        r#"
       SIGNAL_ALARM
     +--------------+
     |   LIMITS_    |
     |    ALARM     |
L.HS-|H            Q|---SE
X----|X             |
L.LS-|L             |
EPS--|EPS           |
     +--------------+
"#,
    ))
    .ok();
    let nw = &p.graphic().networks[0];
    let b = block(nw, "LIMITS_ALARM");
    let NodeKind::Block { instance, .. } = &b.kind else { panic!() };
    assert_eq!(instance.as_ref().unwrap().text, "SIGNAL_ALARM");
    assert_nets!(
        p,
        nw,
        [
            "Src(L.HS) -> SIGNAL_ALARM:LIMITS_ALARM.H",
            "Src(X) -> SIGNAL_ALARM:LIMITS_ALARM.X",
            "Src(L.LS) -> SIGNAL_ALARM:LIMITS_ALARM.L",
            "Src(EPS) -> SIGNAL_ALARM:LIMITS_ALARM.EPS",
            "SIGNAL_ALARM:LIMITS_ALARM.Q -> Var(SE)"
        ]
    );
}

#[test]
fn literals_and_spaced_sources() {
    let p = parse_src(&pou(
        "PROGRAM",
        "VAR A : INT; S : STRING; T : TIME; END_VAR",
        r#"
         +------+
T#10s ---|  :=  |---T
         +------+
         +------+
'A B'----|  :=  |---S
         +------+
         +------+
-1 ------|  :=  |---A
         +------+
"#,
    ))
    .ok();
    let n = &p.graphic().networks;
    assert_eq!(n.len(), 3);
    assert_nets!(p, &n[0], ["Src(T#10s) -> :=.in0", ":=.out0 -> Var(T)"]);
    assert_nets!(p, &n[1], ["Src('A B') -> :=.in0", ":=.out0 -> Var(S)"]);
    assert_nets!(p, &n[2], ["Src(-1) -> :=.in0", ":=.out0 -> Var(A)"]);
}

#[test]
fn action_blocks_in_ld_and_fbd_table_44() {
    let p = parse_src(&pou(
        "PROGRAM",
        "VAR OK1, DN1 : BOOL; END_VAR",
        r#"
| S8.X %IX7.5 +---+------+---+ OK1 |
+--| |---| |--| N | ACT1 |DN1|-( )-+
|             +---+------+---+     |
"#,
    ))
    .ok();
    let g = p.graphic();
    assert_eq!(g.language, GraphicLanguage::Ld);
    assert_nets!(
        p,
        &g.networks[0],
        ["LeftRail -> NO(S8.X)", "NO(S8.X) -> NO(%IX7.5)", "NO(%IX7.5) -> Action(N ACT1)", "Action(N ACT1) -> Coil(OK1)", "Coil(OK1) -> RightRail"]
    );
    let NodeKind::ActionBlock { qualifier, time, indicator, .. } = &g.networks[0].nodes.iter().find(|n| matches!(n.kind, NodeKind::ActionBlock { .. })).unwrap().kind else { panic!() };
    assert_eq!(*qualifier, Some(Qualifier::N));
    assert!(time.is_none());
    assert_eq!(p.text(indicator.as_ref().unwrap()), "DN1");

    let p = parse_src(&pou(
        "PROGRAM",
        "VAR OK1, DN1 : BOOL; END_VAR",
        r#"
         +---+     +-----+------+-----+
  S8.X---| & |-----|  L  | ACT1 | DN1 |---OK1
%IX7.5---|   |     |T#5s |      |     |
         +---+     +-----+------+-----+
"#,
    ))
    .ok();
    let g = p.graphic();
    assert_eq!(g.language, GraphicLanguage::Fbd);
    assert_nets!(p, &g.networks[0], ["Src(S8.X) -> &.in0", "Src(%IX7.5) -> &.in1", "&.out0 -> Action(L ACT1)", "Action(L ACT1) -> Var(OK1)"]);
    let NodeKind::ActionBlock { qualifier, time, .. } = &g.networks[0].nodes.iter().find(|n| matches!(n.kind, NodeKind::ActionBlock { .. })).unwrap().kind else { panic!() };
    assert_eq!(*qualifier, Some(Qualifier::L));
    assert_eq!(p.text(time.as_ref().unwrap()), "T#5s");
}

#[test]
fn errors_are_reported_not_panics() {
    for body in [
        "\n+--| |--\n",
        "\n|  a\n+--| |--( )\n",
        "\n    +---+\n----|   |\n    +---+\n",
        "\n  +--+\n  |  |\n",
        "\n>>\n<RETURN>\n( )\n| |\n",
        "\n+===+\n|| ||\n+===+\n   |\n   + \n",
        "\nX---+\n    |\n    +---<RETURN>\n    |\n",
        "\n| a |\n+-|P|-+\n",
        "\n        +---+\n    ----|   |----\n        +---+\n",
    ] {
        let p = parse_src(&pou("PROGRAM", "VAR a : BOOL; END_VAR", body));
        let _ = p.errors;
    }
    // unconnected contact input and missing variable
    let p = parse_src(&pou("PROGRAM", "VAR a : BOOL; END_VAR", "\n|\n+---| |---( )--+\n|               |\n"));
    assert!(p.errors.iter().any(|e| e.contains("no variable")), "{:?}", p.errors);
    let p = parse_src(&pou("PROGRAM", "VAR a : BOOL; END_VAR", "\n|  a\n+---|\n|    xyz\n"));
    assert!(!p.errors.is_empty());
}

#[test]
fn action_block_fields_are_one_element() {
    let p = parse_src(&pou(
        "PROGRAM",
        "VAR S8, IN1, OK1, DN1 : BOOL; END_VAR",
        r#"
|  S8    IN1      +---+------+---+      OK1    |
+--| |---| |------| N | ACT1 |DN1|------( )----+
|                 +---+------+---+             |
"#,
    ))
    .ok();
    let nw = &p.graphic().networks[0];
    let mut nodes = p.nodes(nw);
    nodes.sort();
    assert_eq!(nodes, ["Action(N ACT1)", "Coil(OK1)", "LeftRail", "NO(IN1)", "NO(S8)", "RightRail"]);
    assert_nets!(p, nw, ["LeftRail -> NO(S8)", "NO(S8) -> NO(IN1)", "NO(IN1) -> Action(N ACT1)", "Action(N ACT1) -> Coil(OK1)", "Coil(OK1) -> RightRail"]);
    let p = parse_src(&pou(
        "PROGRAM",
        "VAR S8, IN2, OK2 : BOOL; END_VAR",
        r#"
           +---+     +-------+------+
S8---------| & |-----| L     | ACT2 |---OK2
IN2--------|   |     |T#30ms |      |
           +---+     +-------+------+
"#,
    ))
    .ok();
    let nw = &p.graphic().networks[0];
    let mut nodes = p.nodes(nw);
    nodes.sort();
    assert_eq!(nodes, ["&", "Action(L ACT2)", "Src(IN2)", "Src(S8)", "Var(OK2)"]);
    let NodeKind::ActionBlock { time, indicator, .. } = &nw.nodes.iter().find(|n| matches!(n.kind, NodeKind::ActionBlock { .. })).unwrap().kind else { panic!() };
    assert_eq!(p.text(time.as_ref().unwrap()), "T#30ms");
    assert!(indicator.is_none());
}

#[test]
fn in_out_parameters_drawn_through_blocks_figures_11_12() {
    let p = parse_src(&pou(
        "PROGRAM",
        "VAR E_TMR2 : TON; GO, DONE2 : BOOL; X3, X4 : INT; I_BLK2 : INSIDE_B; END_VAR",
        r#"
                    I_BLK2
              +---------------+
              |   INSIDE_B    |
   E_TMR2-----|I_TMR-----I_TMR|
       GO-----|TMR_GO  EXPIRED|---DONE2
              +---------------+

              +---------------+
              |   INSIDE_B    |
   E_TMR2-----|I_TMR----I_TMR |
              +---------------+

          +-----+
          | INC |
   X3-----|A---A|---X4
          +-----+
"#,
    ))
    .ok();
    let n = &p.graphic().networks;
    assert_eq!(n.len(), 3);
    let (i, o) = pins(block(&n[0], "INSIDE_B"));
    assert_eq!(i, vec![(Some("I_TMR".into()), false, None), (Some("TMR_GO".into()), false, None)]);
    assert_eq!(o, vec![(Some("I_TMR".into()), false, None), (Some("EXPIRED".into()), false, None)]);
    assert_nets!(p, &n[0], ["Src(E_TMR2) -> I_BLK2:INSIDE_B.I_TMR", "Src(GO) -> I_BLK2:INSIDE_B.TMR_GO", "I_BLK2:INSIDE_B.EXPIRED -> Var(DONE2)"]);
    let (i, o) = pins(block(&n[1], "INSIDE_B"));
    assert_eq!(i, vec![(Some("I_TMR".into()), false, None)]);
    assert_eq!(o, vec![(Some("I_TMR".into()), false, None)]);
    assert_nets!(p, &n[1], ["Src(E_TMR2) -> INSIDE_B.I_TMR"]);
    let (i, o) = pins(block(&n[2], "INC"));
    assert_eq!(i, vec![(Some("A".into()), false, None)]);
    assert_eq!(o, vec![(Some("A".into()), false, None)]);
    assert_nets!(p, &n[2], ["Src(X3) -> INC.A", "INC.A -> Var(X4)"]);
}

// ---------------------------------------------------------------- SFC

fn sfc_of(p: &Parsed) -> &Sfc {
    let g = p.graphic();
    assert_eq!(g.language, GraphicLanguage::Sfc);
    g.sfc.as_deref().expect("sfc")
}

fn cond_text(p: &Parsed, c: &TransitionCondition) -> String {
    match c {
        TransitionCondition::St(e) => p.text(e),
        TransitionCondition::Named => "<named>".into(),
        TransitionCondition::Graphic(_) => "<graphic>".into(),
        TransitionCondition::Il(_) => "<il>".into(),
    }
}

/// "FROM -> TO : condition [priority]" for every transition.
fn transitions(p: &Parsed) -> Vec<String> {
    sfc_of(p)
        .transitions
        .iter()
        .map(|t| {
            let f: Vec<_> = t.from.iter().map(|s| s.text.clone()).collect();
            let to: Vec<_> = t.to.iter().map(|s| s.text.clone()).collect();
            let pr = t.branch_priority.map(|b| format!(" [{}]", b)).unwrap_or_default();
            format!("{} -> {} : {}{}", f.join(","), to.join(","), cond_text(p, &t.condition), pr)
        })
        .collect()
}

fn steps(p: &Parsed) -> Vec<String> {
    sfc_of(p)
        .steps
        .iter()
        .map(|s| {
            let a: Vec<String> = s
                .associations
                .iter()
                .map(|a| {
                    let mut x = format!("{}({}", a.action.text, a.qualifier.map(|q| q.as_str()).unwrap_or("-"));
                    if let Some(t) = &a.time {
                        x.push_str(&format!(",{}", p.text(t)));
                    }
                    for i in &a.indicators {
                        x.push_str(&format!(",{}", i.text));
                    }
                    x.push(')');
                    x
                })
                .collect();
            format!("{}{} {}", if s.initial { "*" } else { "" }, s.name.text, a.join(" ")).trim_end().to_string()
        })
        .collect()
}

const SFC_DECL: &str = "VAR a, b, c, d, e, f, h, j, DN1 : BOOL; END_VAR";

#[test]
fn sfc_steps_and_transition_table_40_41() {
    let p = parse_src(&pou(
        "FUNCTION_BLOCK",
        "VAR X1, X2 : BOOL; END_VAR",
        r#"
   +=======+
   ||  S1 ||   (* initial step, *)
   ||     ||   (* table 40     *)
   +===+===+
       |
       + %IX2.4 & %IX2.3
       |
    +-----+
    |STEP7|
    +-----+
       |
       + TRAN78
       |
    +-----+
    |STEP8|
    +--+--+
       |
       + X1 OR X2
       |
   +---+---+
   | STEP9 |
   +---+---+
       |
       + TRUE
       |
   +---+---+
   | STEP10|
   +-------+
"#,
    ))
    .ok();
    assert_eq!(steps(&p), ["*S1", "STEP7", "STEP8", "STEP9", "STEP10"]);
    assert_eq!(transitions(&p), ["S1 -> STEP7 : %IX2.4 & %IX2.3", "STEP7 -> STEP8 : TRAN78", "STEP8 -> STEP9 : X1 OR X2", "STEP9 -> STEP10 : TRUE"]);
    assert!(matches!(&sfc_of(&p).transitions[3].condition, TransitionCondition::St(Expr { kind: ExprKind::Literal(_), .. })));
    assert!(matches!(&sfc_of(&p).transitions[1].condition, TransitionCondition::St(Expr { kind: ExprKind::Name(_), .. })));
    let t = &sfc_of(&p).transitions[1];
    assert!(t.name.is_none() && t.branch_priority.is_none());
}

#[test]
fn sfc_named_transition_table_41_feature_7() {
    let src = "FUNCTION_BLOCK X\nVAR DN1 : BOOL; END_VAR\n
+====+====+
||  STEP7 ||
+====+====+
     |
     + TRAN78
     |
+----+----+
|  STEP8  |
+----+----+
     |
     + DN1
     |
+----+----+
|  STEP9  |
+---------+
TRANSITION TRAN78 FROM STEP7 TO STEP8 :
  |  %IX2.4  %IX2.3  TRAN78  |
  +---| |-----| |-----( )----+
  |                          |
END_TRANSITION
END_FUNCTION_BLOCK
";
    let p = parse_src(src);
    let s = sfc_of(&p);
    // with the textual tail merged by the parser, the textual TRANSITION
    // TRAN78 is a third entry holding the condition of the named one
    assert!(s.transitions.len() == 2 || s.transitions.len() == 3, "{:?}", s.transitions);
    if let Some(t) = s.transitions.get(2) {
        assert_eq!(t.name.as_ref().unwrap().text, "TRAN78");
        assert!(!matches!(t.condition, TransitionCondition::Named));
    }
    assert!(matches!(s.transitions[0].condition, TransitionCondition::Named));
    assert_eq!(s.transitions[0].name.as_ref().unwrap().text, "TRAN78");
    assert!(matches!(&s.transitions[1].condition, TransitionCondition::St(e) if matches!(&e.kind, ExprKind::Name(n) if n.text == "DN1")));
    assert!(s.transitions[1].name.is_none());
    // the resolution is idempotent and can be applied to a merged chart
    let mut copy = s.clone();
    structure_syntax::graphic::resolve_transition_names(&mut copy);
    assert!(matches!(copy.transitions[0].condition, TransitionCondition::Named));
    assert!(matches!(copy.transitions[1].condition, TransitionCondition::St(_)));
}

#[test]
fn sfc_graphic_conditions_table_41_features_2_3_4() {
    let p = parse_src(&pou(
        "FUNCTION_BLOCK",
        "VAR X : BOOL; END_VAR",
        r#"
                    +===+===+
                    || S6  ||
                    +===+===+
                        |
| %IX2.4  %IX2.3        |
+---| |-----| |---------+
|                       |
                     +--+--+
                     |STEP7|
                     +--+--+
         +-------+      |
         |   &   |      |
%IX2.4---|       |------+
%IX2.3---|       |      |
         +-------+   +--+--+
                     |STEP8|
                     +--+--+
                        |
>TRANX>-----------------+
                        |
                     +--+--+
                     |STEP9|
                     +--+--+
                        |
                        + X
                        |
                     +--+--+
                     | S10 |
                     +-----+

| %IX2.4  %IX2.3
+---| |-----| |---->TRANX>
|
"#,
    ))
    .ok();
    let s = sfc_of(&p);
    assert_eq!(steps(&p), ["*S6", "STEP7", "STEP8", "STEP9", "S10"]);
    let t = transitions(&p);
    assert_eq!(t, ["S6 -> STEP7 : <graphic>", "STEP7 -> STEP8 : <graphic>", "STEP8 -> STEP9 : <graphic>", "STEP9 -> S10 : X"]);
    let TransitionCondition::Graphic(g) = &s.transitions[0].condition else { panic!() };
    assert_eq!(g.language, GraphicLanguage::Ld);
    assert_eq!(g.networks.len(), 1);
    assert_nets!(p, &g.networks[0], ["LeftRail -> NO(%IX2.4)", "NO(%IX2.4) -> NO(%IX2.3)", "NO(%IX2.3) -> Var(TRANSITION)"]);
    let TransitionCondition::Graphic(g) = &s.transitions[1].condition else { panic!() };
    assert_eq!(g.language, GraphicLanguage::Fbd);
    assert_nets!(p, &g.networks[0], ["Src(%IX2.4) -> &.in0", "Src(%IX2.3) -> &.in1", "&.out0 -> Var(TRANSITION)"]);
    let TransitionCondition::Graphic(g) = &s.transitions[2].condition else { panic!() };
    assert_eq!(g.language, GraphicLanguage::Ld);
    assert_nets!(
        p,
        &g.networks[0],
        ["LeftRail -> NO(%IX2.4)", "NO(%IX2.4) -> NO(%IX2.3)", "NO(%IX2.3) -> ConnOut(TRANX)", "ConnIn(TRANX) -> Var(TRANSITION)"]
    );
}

#[test]
fn sfc_selection_divergence_priorities_table_46() {
    // 2a: asterisk, left-to-right priority
    let p = parse_src(&pou(
        "FUNCTION_BLOCK",
        SFC_DECL,
        r#"
     +===+===+
     || S5  ||
     +===+===+
         |
   +-----*------+
   |            |
   + e          + f
   |            |
+--+--+      +--+--+
| S6  |      | S8  |
+-----+      +-----+
"#,
    ))
    .ok();
    assert_eq!(transitions(&p), ["S5 -> S6 : e [0]", "S5 -> S8 : f [1]"]);
    // 2b: numbered branches
    let p = parse_src(&pou(
        "FUNCTION_BLOCK",
        SFC_DECL,
        r#"
     +===+===+
     || S5  ||
     +===+===+
         |
   +-----*------+
   |2           |1
   + e          + f
   |            |
+--+--+      +--+--+
| S6  |      | S8  |
+-----+      +-----+
"#,
    ))
    .ok();
    assert_eq!(transitions(&p), ["S5 -> S6 : e [2]", "S5 -> S8 : f [1]"]);
    // 2c: no priority
    let p = parse_src(&pou(
        "FUNCTION_BLOCK",
        SFC_DECL,
        r#"
     +===+===+
     || S5  ||
     +===+===+
         |
   +-----+------+
   |            |
   +e           +NOT e & f
   |            |
+--+--+      +--+--+
| S6  |      | S8  |
+-----+      +-----+
"#,
    ))
    .ok();
    assert_eq!(transitions(&p), ["S5 -> S6 : e", "S5 -> S8 : NOT e & f"]);
}

#[test]
fn sfc_convergences_and_simultaneous_table_46() {
    let p = parse_src(&pou(
        "FUNCTION_BLOCK",
        SFC_DECL,
        r#"
         +===+===+
         || S11 ||
         +===+===+
             |
             + b
             |
    ===+=====+=====+===
       |           |
    +--+--+     +--+--+
    | S12 |     | S14 |
    +--+--+     +--+--+
       |           |
       + h         + j
       |           |
    +--+--+     +--+--+
    | S13 |     | S15 |
    +--+--+     +--+--+
       |           |
    ===+=====+=====+===
             |
             + d
             |
          +--+--+
          | S16 |
          +--+--+
             |
       +-----*-----+
       |           |
       + e         + f
       |           |
    +--+--+     +--+--+
    | S7  |     | S9  |
    +--+--+     +--+--+
       |           |
       + h         + j
       |           |
       +-----+-----+
             |
          +--+--+
          | S10 |
          +-----+
"#,
    ))
    .ok();
    assert_eq!(
        transitions(&p),
        ["S11 -> S12,S14 : b", "S12 -> S13 : h", "S14 -> S15 : j", "S13,S15 -> S16 : d", "S16 -> S7 : e [0]", "S16 -> S9 : f [1]", "S7 -> S10 : h", "S9 -> S10 : j"]
    );
}

#[test]
fn sfc_skip_loop_and_arrows_table_46_features_5_6_7() {
    let p = parse_src(&pou(
        "FUNCTION_BLOCK",
        SFC_DECL,
        r#"
      +==+==+
      || S30 ||
      +==+==+
         |
         +---*---+
         |       |
         + a     + d
         |       |
      +--+--+    |
      | S31 |    |
      +--+--+    |
         |       |
         + b     |
         |       |
         +---+---+
             |
          +--+--+
          | S33 |
          +--+--+
             |
             + c
             |
          +--+--+
          | S34 |<-----+
          +--+--+      |
             |         |
             *-----+   |
             |     |   |
             + e   + f |
             |     |   |
          +--+--+  +->-+
          | S35 |
          +-----+
"#,
    ));
    // `<` directly against a box is not an arrow: the loop does not reach S34
    assert!(!p.errors.is_empty());
    let p = parse_src(&pou(
        "FUNCTION_BLOCK",
        SFC_DECL,
        r#"
      +===+===+
      || S30 ||
      +===+===+
          |
          +---*---+
          |       |
          + a     + d
          |       |
       +--+--+    |
       | S31 |    |
       +--+--+    |
          |       |
          + b     |
          |       |
          +---+---+
              |
           +--+--+
           | S33 |
           +--+--+
              |
              + c
              |
              +----<----+
              |         |
           +--+--+      |
           | S34 |      |
           +--+--+      |
              |         |
              *-----+   |
              |     |   |
              + e   + f |
              |     |   |
           +--+--+  +->-+
           | S35 |
           +-----+
"#,
    ))
    .ok();
    assert_eq!(
        transitions(&p),
        ["S30 -> S31 : a [0]", "S30 -> S33 : d [1]", "S31 -> S33 : b", "S33 -> S34 : c", "S34 -> S35 : e [0]", "S34 -> S34 : f [1]"]
    );
}

#[test]
fn sfc_action_blocks_table_43() {
    let p = parse_src(&pou(
        "FUNCTION_BLOCK",
        "VAR DN1 : BOOL; END_VAR",
        r#"
  +==+===+
  || S7 ||
  +==+===+
     |
     + DN1
     |
  +--+-+   +-----+----------+---+
  | S8 |---|  L  | ACTION_1 |DN1|
  +--+-+   |t#10s|          |   |
     |     +-----+----------+---+
     + DN1
     |
  +--+-+   +-----+---------------------+---+
  | S9 |---|  L  |        ACTION_1     |DN1|
  +--+-+   |t#10s|                     |   |
     |     +-----+---------------------+---+
     +DN1  |  P  |        ACTION_2     |   |
     |     +-----+---------------------+---+
     |     |  N  |        ACTION_3     |   |
     |     +-----+---------------------+---+
     |
  +--+--+   +-----+----------------------+---+
  | S10 |---| N   |      ACTION_4        |   |
  +-----+   +-----+----------------------+---+
            | %QX17 := %IX1 & %MX3 & S8.X ;  |
            | FF28 (S1 := (C<D));            |
            | %MX10 := FF28.Q;               |
            +--------------------------------+
"#,
    ))
    .ok();
    assert_eq!(
        steps(&p),
        [
            "*S7",
            "S8 ACTION_1(L,t#10s,DN1)",
            "S9 ACTION_1(L,t#10s,DN1) ACTION_2(P) ACTION_3(N)",
            "S10 ACTION_4(N)",
        ]
    );
    let s = sfc_of(&p);
    assert_eq!(s.actions.len(), 1);
    assert_eq!(s.actions[0].name.text, "ACTION_4");
    let Body::St(stmts) = &s.actions[0].body else { panic!("{:?}", s.actions[0].body) };
    assert_eq!(stmts.len(), 3);
}

#[test]
fn sfc_link_errors_2_6_5() {
    let p = parse_src(&pou(
        "FUNCTION_BLOCK",
        SFC_DECL,
        r#"
  +==+===+
  || S1 ||
  +==+===+
     |
  +--+--+
  | S2  |
  +--+--+
     |
     + a
     |
     + b
     |
  +--+--+
  | S3  |
  +-----+
"#,
    ));
    assert!(p.errors.iter().any(|e| e.contains("linked without a transition")), "{:?}", p.errors);
    assert!(p.errors.iter().any(|e| e.contains("two transitions")), "{:?}", p.errors);
    // simultaneous divergence drawn with a single line
    let p = parse_src(&pou(
        "FUNCTION_BLOCK",
        SFC_DECL,
        r#"
  +==+===+
  || S1 ||
  +==+===+
     |
     + a
     |
     +-------+
     |       |
  +--+--+ +--+--+
  | S2  | | S3  |
  +-----+ +-----+
"#,
    ));
    assert!(p.errors.iter().any(|e| e.contains("double line")), "{:?}", p.errors);
}

#[test]
fn f5_mix_2_brix_sfc() {
    let src = format!(
        "FUNCTION_BLOCK MIX_2_BRIX\n{}\n{}END_FUNCTION_BLOCK\n",
        "VAR_INPUT ST, d, S0, S1 : BOOL; WC : WORD; z, WA, WB : INT; t1 : TIME; END_VAR\nVAR_OUTPUT DONE, VA, VB, VC, MT, MR, MP0, MP1 : BOOL; END_VAR",
        r#"
     +-------------->-----------------+
     |                                |
     |                           +====+====+   +---+------+
     |                           || START ||---| N | DONE |
     |                           +====+====+   +---+------+
     |                                |
     |                                + ST & S0 & BCD_TO_INT(WC) <= z
     |                                |
     |     ===+=======================+=================+=======
     |        |                                         |
     |   +----+----+   +---+----+                   +---+----+   +---+----+
     |   | WEIGH_A |---| N | VA |                   | BRICK1 |---| S | MT |
     |   +----+----+   +---+----+                   +---+----+   +---+----+
     |        |                                         |
     |        + BCD_TO_INT(WC) >= WA+z                  + d
     |        |                                         |
     |   +----+----+   +---+----+                   +---+----+
     |   | WEIGH_B |---| N | VB |                   | DROP_1 |
     |   +----+----+   +---+----+                   +---+----+
     |        |                                         |
     |        + BCD_TO_INT(WC) >= WA+WB+z               + NOT d
     |        |                                         |
     |     +--+---+   +---+----+                    +---+----+
     |     | FILL |---| N | VC |                    | BRICK2 |
     |     +--+---+   +---+----+                    +---+----+
     |        |                                         + d
     |        |                                     +---+----+   +---+----+
     |        |                                     | DROP_2 |---| R | MT |
     |        |                                     +---+----+   +---+----+
     |        |                                         |
     |     ===+=======================+=================+=======
     |                                |
     |                                + BCD_TO_INT(WC) <= z & NOT d
     |                                |
     |                             +--+--+   +---+----+
     |                             | MIX |---| S | MR |
     |                             +--+--+   +---+----+
     |                                |
     |                                + MIX.T >= t1
     |                                |
     |                             +--+--+   +---+-----+----+
     |                             | TIP |---| N | MP1 | S1 |
     |                             +--+--+   +---+-----+----+
     |                                |
     |                                + S1
     |                                |
     |                            +---+---+   +---+-----+----+
     |                            | RAISE |---| R | MR  |    |
     |                            +---+---+   +---+-----+----+
     |                                +S0     | N | MP0 | S0 |
     |                                |       +---+-----+----+
     +--------------<-----------------+
"#
    );
    let p = parse_src(&src).ok();
    assert_eq!(
        steps(&p),
        [
            "*START DONE(N)",
            "WEIGH_A VA(N)",
            "BRICK1 MT(S)",
            "WEIGH_B VB(N)",
            "DROP_1",
            "FILL VC(N)",
            "BRICK2",
            "DROP_2 MT(R)",
            "MIX MR(S)",
            "TIP MP1(N,S1)",
            "RAISE MR(R) MP0(N,S0)",
        ]
    );
    assert_eq!(
        transitions(&p),
        [
            "START -> WEIGH_A,BRICK1 : ST & S0 & BCD_TO_INT(WC) <= z",
            "WEIGH_A -> WEIGH_B : BCD_TO_INT(WC) >= WA+z",
            "BRICK1 -> DROP_1 : d",
            "WEIGH_B -> FILL : BCD_TO_INT(WC) >= WA+WB+z",
            "DROP_1 -> BRICK2 : NOT d",
            "BRICK2 -> DROP_2 : d",
            "FILL,DROP_2 -> MIX : BCD_TO_INT(WC) <= z & NOT d",
            "MIX -> TIP : MIX.T >= t1",
            "TIP -> RAISE : S1",
            "RAISE -> START : S0",
        ]
    );
}

#[test]
fn f7_gravel_sfc_figure_f4() {
    let src = format!(
        "PROGRAM GRAVEL\n{}\n{}END_PROGRAM\n",
        "VAR_INPUT FILL_PB, LOAD_PB, BIN_EMPTY_LS : BOOL; END_VAR VAR OK_TO_RUN : BOOL; RUN_IN_TIME, RUNOUT_TIME : TIME; LEVEL_CTR : CTU; END_VAR\nVAR_OUTPUT SILO_VALVE, BIN_VALVE : BOOL; END_VAR",
        r#"
  +----------------->-------------------+
  |                                     |
  |                                +====+====+
  |                                || START ||
  |                                +====+====+
  |                                     |
  |                                     + FILL_PB & CONTROL.X
  |                                     |
  |                                +----+-----+   +---+------------+
  |                                | FILL_BIN |---| N | SILO_VALVE |
  |                                +----+-----+   +---+------------+
  |                                     |
  |   +---------------------------------*
  |   |                                 |
  |   + NOT FILL_PB OR NOT CONTROL.X    + LEVEL_CTR.Q
  |   |                                 |
  +---+                                 |
  |      +--------------->--------------+
  |      |                        +-----+-----+
  |      |                        | LOAD_WAIT |
  |      |                        +-----+-----+
  |      |                              |
  |      |                              + LOAD_PB & OK_TO_RUN
  |      |                              |
  |      |                          +---+----+
  |      |                          | RUN_IN |
  |      |                          +---+----+
  |      |                              |
  |      |   +--------------------------*
  |      |   |                          |
  |      |   + NOT OK_TO_RUN            + RUN_IN.T >= RUN_IN_TIME
  |      |   |                          |
  |      +---+                          |
  |      |                         +----+-----+   +---+-----------+
  |      |                         | DUMP_BIN |---| N | BIN_VALVE |
  |      |                         +----+-----+   +---+-----------+
  |      |                              |
  |      |   +--------------------------*
  |      |   |                          |
  |      |   + NOT OK_TO_RUN            + BIN_EMPTY_LS
  |      |   |                          |
  |      +---+                          |
  |      |                          +---+----+
  |      |                          | RUNOUT |
  |      |                          +---+----+
  |      |                              |
  |      |   +--------------------------*
  |      |   |                          |
  |      |   + NOT OK_TO_RUN            + RUNOUT.T >= RUNOUT_TIME
  |      |   |                          |
  |      +---+                          |
  |                                     |
  +-------------------------------------+
"#
    );
    let p = parse_src(&src).ok();
    assert_eq!(steps(&p), ["*START", "FILL_BIN SILO_VALVE(N)", "LOAD_WAIT", "RUN_IN", "DUMP_BIN BIN_VALVE(N)", "RUNOUT"]);
    assert_eq!(
        transitions(&p),
        [
            "START -> FILL_BIN : FILL_PB & CONTROL.X",
            "FILL_BIN -> START : NOT FILL_PB OR NOT CONTROL.X [0]",
            "FILL_BIN -> LOAD_WAIT : LEVEL_CTR.Q [1]",
            "LOAD_WAIT -> RUN_IN : LOAD_PB & OK_TO_RUN",
            "RUN_IN -> LOAD_WAIT : NOT OK_TO_RUN [0]",
            "RUN_IN -> DUMP_BIN : RUN_IN.T >= RUN_IN_TIME [1]",
            "DUMP_BIN -> LOAD_WAIT : NOT OK_TO_RUN [0]",
            "DUMP_BIN -> RUNOUT : BIN_EMPTY_LS [1]",
            "RUNOUT -> LOAD_WAIT : NOT OK_TO_RUN [0]",
            "RUNOUT -> START : RUNOUT.T >= RUNOUT_TIME [1]",
        ]
    );
}

#[test]
fn f7_gravel_control_sfc_figure_f5_with_fbd_action() {
    let src = r#"PROGRAM GRAVEL
VAR_INPUT OFF_PB, ON_PB, TRUCK_ON_RAMP, JOG_PB : BOOL; END_VAR
VAR OK_TO_RUN, CONVEYOR_MOTOR : BOOL; BLINK_TIME : TIME; BLINK, BLANK : TON; END_VAR
 +---------+
 |         |
 |         + OFF_PB
 |         |
 | +=======+=======+     +=====+=====+   +---+----------------+
 | || CONTROL_OFF ||     || MONITOR ||---| N | MONITOR_ACTION |
 | +=======+=======+     +===========+   +---+----------------+
 |         |
 |         + ON_PB & NOT OFF_PB
 |         |
 |     +---+---+   +---+---------------------------------+------+
 |     |CONTROL|---| N |       CONTROL_ACTION            |      |
 |     +---+---+   +---+---------------------------------+------+
 |         |       |      +---------------------------------+   |
 +---------+       |      |             BLINK       BLANK   |   |
                   |      |    +-+     +-----+     +-----+  |   |
                   |      +---O|&|     | TON |     | TON |  |   |
                   |CONTROL.X--| |-----|IN  Q|-----|IN  Q|--+   |
                   |           +-+  +--|PT   |  +--|PT   |      |
                   |                |  +-----+  |  +-----+      |
                   |    BLINK_TIME--+-----------+               |
                   |               +-+                          |
                   |CONTROL.X------|&|                          |
                   |TRUCK_ON_RAMP--| |---+---------OK_TO_RUN    |
                   |               +-+   |                      |
                   |                     |  +-+                 |
                   |            +-----+  +--|&|--CONVEYOR_MOTOR |
                   |JOG_PB------| >=1 |-----| |                 |
                   |RUN_IN.X----|     |     +-+                 |
                   |DUMP_BIN.X--|     |                         |
                   |RUNOUT.X----|     |                         |
                   |            +-----+                         |
                   +--------------------------------------------+
END_PROGRAM
"#;
    let p = parse_src(src).ok();
    assert_eq!(steps(&p), ["*CONTROL_OFF", "*MONITOR MONITOR_ACTION(N)", "CONTROL CONTROL_ACTION(N)"]);
    // the transition below CONTROL goes up, around and down into CONTROL_OFF
    assert_eq!(transitions(&p), ["CONTROL -> CONTROL_OFF : OFF_PB", "CONTROL_OFF -> CONTROL : ON_PB & NOT OFF_PB"]);
    let s = sfc_of(&p);
    assert_eq!(s.actions.len(), 1);
    assert_eq!(s.actions[0].name.text, "CONTROL_ACTION");
    let Body::Graphic(g) = &s.actions[0].body else { panic!() };
    assert_eq!(g.language, GraphicLanguage::Fbd);
    assert_eq!(g.networks.len(), 2);
    assert_nets!(
        p,
        &g.networks[0],
        [
            "Src(CONTROL.X) -> &.in1",
            "BLINK:TON.Q -> BLANK:TON.IN",
            "&.out0 -> BLINK:TON.IN",
            "BLANK:TON.Q -> &.!in0",
            "Src(BLINK_TIME) -> BLANK:TON.PT, BLINK:TON.PT"
        ]
    );
    assert_nets!(p, &g.networks[1], ["Src(CONTROL.X) -> &.in0", "Src(TRUCK_ON_RAMP) -> &.in1", "&.out0 -> &.in0, Var(OK_TO_RUN)", "&.out0 -> Var(CONVEYOR_MOTOR)", ">=1.out0 -> &.in1", "Src(JOG_PB) -> >=1.in0", "Src(RUN_IN.X) -> >=1.in1", "Src(DUMP_BIN.X) -> >=1.in2", "Src(RUNOUT.X) -> >=1.in3"]);
}

#[test]
fn f8_agv_sfc() {
    let src = format!(
        "PROGRAM AGV\n{}\n{}END_PROGRAM\n",
        "VAR_INPUT SINGLE_PB, DOUBLE_PB : BOOL; END_VAR VAR CYCLE : BOOL; END_VAR",
        r#"
+--------------------------+
|                          |
|                     +====+====+
|                     || START ||
|                     +====+====+
|                          |
|       +------------------*------------+
|       |                               |
|       + READY.X & SINGLE_PB           + READY.X & DOUBLE_PB
|       |                               |
|   +---+----+ +---+-------+       +----+-----+ +---+-------+
|   | SINGLE |-| N | CYCLE |       | DOUBLE_1 |-| N | CYCLE |
|   +---+----+ +---+-------+       +----+-----+ +---+-------+
|       |                               |
|       *---------+                     + DONE.X
|       |         |                     |
|       |         + DONE.X & DOUBLE_PB  |
|       |         +---------------------+
|       |                               |
|       + DONE.X & NOT DOUBLE_PB +------+------+
|       |                        | DOUBLE_WAIT |
|       |                        +------+------+
|       |                               |
|       |                               + READY.X
|       |                               |
|       |                          +----+-----+ +---+-------+
|       |                          | DOUBLE_2 |-| N | CYCLE |
|       |                          +----+-----+ +---+-------+
|       |                               |
|       |                               + DONE.X
|       |                               |
|       +---------------+---------------+
|                       |
|                 +-----+------+
|                 | NON_REPEAT |
|                 +-----+------+
|                       |
|                       +NOT(SINGLE_PB OR DOUBLE_PB)
|                       |
+-----------------------+
"#
    );
    let p = parse_src(&src).ok();
    assert_eq!(steps(&p), ["*START", "SINGLE CYCLE(N)", "DOUBLE_1 CYCLE(N)", "DOUBLE_WAIT", "DOUBLE_2 CYCLE(N)", "NON_REPEAT"]);
    assert_eq!(
        transitions(&p),
        [
            "START -> SINGLE : READY.X & SINGLE_PB [0]",
            "START -> DOUBLE_1 : READY.X & DOUBLE_PB [1]",
            "DOUBLE_1 -> DOUBLE_WAIT : DONE.X",
            "SINGLE -> DOUBLE_WAIT : DONE.X & DOUBLE_PB [1]",
            "SINGLE -> NON_REPEAT : DONE.X & NOT DOUBLE_PB [0]",
            "DOUBLE_WAIT -> DOUBLE_2 : READY.X",
            "DOUBLE_2 -> NON_REPEAT : DONE.X",
            "NON_REPEAT -> START : NOT(SINGLE_PB OR DOUBLE_PB)",
        ]
    );
}

// ---------------------------------------------------------------- robustness

/// Random drawings made of drawing characters and short names must never
/// make the parser panic.
#[test]
fn random_drawings_do_not_panic() {
    use structure_syntax::graphic::parse_graphic;
    use structure_syntax::source::{FileId, SourceFile};
    let pieces = [
        "-", "-", "-", "|", "|", "+", "+", "=", "*", " ", " ", " ", " ", "O", "<", ">", "(", ")", "/", "a", "X1", "| |", "( )", "+---+", "|IN", "Q|", ">>L",
        "<RETURN>", ">C>", "+ x", "||", "+===+", "S1", "N", "T#1s", "'s'", "[", "]", "#", "%IX1", "-1", ":", "L:", "\t", "é",
    ];
    let mut seed: u64 = 0x2545_F491_4F6C_DD1D;
    let mut next = move || {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        seed
    };
    for _ in 0..3000 {
        let rows = (next() % 12) as usize + 1;
        let mut text = String::new();
        for _ in 0..rows {
            let n = (next() % 14) as usize;
            for _ in 0..n {
                text.push_str(pieces[(next() % pieces.len() as u64) as usize]);
            }
            text.push('\n');
        }
        let file = SourceFile::new(FileId(0), "fuzz", text.clone());
        let _ = parse_graphic(&file, 0, &text);
    }
    // structured noise: real drawings with characters knocked out
    let samples = [
        "|  a     b      c  |\n+--| |---| |--( )--+\n|                  |\n",
        "    +-----+\na---| >=1 |---c\nb---|     |\n    +-----+\n",
        "  +==+===+\n  || S1 ||\n  +==+===+\n     |\n     + a\n     |\n  +--+--+   +---+----+\n  | S2  |---| N | A1 |\n  +-----+   +---+----+\n",
        "     +-----*------+\n     |            |\n     + e          + f\n     |            |\n  +--+--+      +--+--+\n  | S6  |      | S8  |\n  +-----+      +-----+\n",
    ];
    for s in samples {
        let chars: Vec<char> = s.chars().collect();
        for k in 0..chars.len() {
            for rep in [' ', '+', '-', '|', '='] {
                let mut c = chars.clone();
                if c[k] == '\n' {
                    continue;
                }
                c[k] = rep;
                let text: String = c.into_iter().collect();
                let file = SourceFile::new(FileId(0), "fuzz", text.clone());
                let _ = parse_graphic(&file, 0, &text);
            }
        }
    }
}

#[test]
fn sfc_inside_an_action_body_table_42_2s() {
    let p = parse_src(&pou(
        "FUNCTION_BLOCK",
        SFC_DECL,
        r#"
  +==+===+
  || S1 ||
  +==+===+
     |
     + a
     |
  +--+--+   +---+-----------------+
  | S2  |---| N |   OPEN_VALVE_1  |
  +--+--+   +---+-----------------+
     |      |                     |
     + b    | +=======+=======+   |
     |      | ||  V1_READY   ||   |
  +--+--+   | +=======+=======+   |
  | S3  |   |         |           |
  +-----+   |         + STEP8.X   |
            |         |           |
            |    +----+-----+     |
            |    |V1_OPENING|     |
            |    +----------+     |
            +---------------------+
"#,
    ))
    .ok();
    assert_eq!(steps(&p), ["*S1", "S2 OPEN_VALVE_1(N)", "S3"]);
    assert_eq!(transitions(&p), ["S1 -> S2 : a", "S2 -> S3 : b"]);
    let s = sfc_of(&p);
    assert_eq!(s.actions.len(), 1);
    let Body::Graphic(g) = &s.actions[0].body else { panic!() };
    assert_eq!(g.language, GraphicLanguage::Sfc);
    let inner = g.sfc.as_deref().unwrap();
    assert_eq!(inner.steps.iter().map(|s| s.name.text.clone()).collect::<Vec<_>>(), ["V1_READY", "V1_OPENING"]);
    assert!(inner.steps[0].initial);
    assert_eq!(inner.transitions.len(), 1);
}

#[test]
fn textual_elements_after_a_chart_are_not_part_of_the_drawing() {
    use structure_syntax::graphic::{parse_graphic, textual_tail};
    use structure_syntax::source::{FileId, SourceFile};
    let text = "  +==+===+\n  || S1 ||\n  +==+===+\n     |\n     + T1\n     |\n  +--+--+\n  | S2  |\n  +-----+\nTRANSITION T1 FROM S1 TO S2 := TRUE; END_TRANSITION\n  ACTION A1: X := 1; END_ACTION\n";
    assert_eq!(textual_tail(text), Some(9));
    assert_eq!(textual_tail("| a |\n+-| |-+\n"), None);
    let file = SourceFile::new(FileId(0), "t", text);
    let (g, diags) = parse_graphic(&file, 0, text);
    assert!(diags.iter().all(|d| d.severity != Severity::Error), "{:?}", diags);
    let s = g.sfc.as_deref().unwrap();
    assert_eq!(s.steps.len(), 2);
    assert_eq!(s.transitions.len(), 1);
    assert!(matches!(s.transitions[0].condition, TransitionCondition::Named));
    assert_eq!(s.transitions[0].name.as_ref().unwrap().text, "T1");
}

#[test]
fn f3_fwd_rev_mon_ld_function_blocks_with_contacts() {
    let p = parse_src(&pou(
        "FUNCTION_BLOCK",
        "VAR_INPUT AUTO, ACK, AUTO_FWD, MAN_FWD, MAN_FWD_CHK, FWD_FDBK : BOOL; T_FWD_MAX : TIME; END_VAR
VAR_OUTPUT FWD_ALRM : BOOL; END_VAR VAR FWD_MON : CMD_MONITOR; END_VAR",
        r#"
|                   FWD_MON                  |
|              +---------------+             |
| AUTO_FWD     |  CMD_MONITOR  |             |
+--| |---------|AUTO_CMD    CMD|             |
| AUTO         |               |  FWD_ALRM   |
+--| |---------|AUTO_MODE  ALRM|-----( )-----+
| MAN_FWD      |               |             |
+--| |---------|MAN_CMD        |             |
| MAN_FWD_CHK  |               |             |
+--| |---------|MAN_CMD_CHK    |             |
| FWD_FDBK     |               |             |
+--| |---------|FDBK           |             |
| ACK          |               |             |
+--| |---------|ACK            |             |
|              |               |             |
| T_FWD_MAX----|T_CMD_MAX      |             |
|              +---------------+             |
|                                            |
"#,
    ))
    .ok();
    let g = p.graphic();
    assert_eq!(g.language, GraphicLanguage::Ld);
    assert_eq!(g.networks.len(), 1);
    let nets = p.nets(&g.networks[0]);
    for c in ["AUTO_FWD", "AUTO", "MAN_FWD", "MAN_FWD_CHK", "FWD_FDBK", "ACK"] {
        assert!(has(&nets, &format!("LeftRail -> NO({})", c)), "{:?}", nets);
    }
    assert!(has(&nets, "NO(AUTO_FWD) -> FWD_MON:CMD_MONITOR.AUTO_CMD"), "{:?}", nets);
    assert!(has(&nets, "NO(MAN_FWD_CHK) -> FWD_MON:CMD_MONITOR.MAN_CMD_CHK"), "{:?}", nets);
    assert!(has(&nets, "Src(T_FWD_MAX) -> FWD_MON:CMD_MONITOR.T_CMD_MAX"), "{:?}", nets);
    assert!(has(&nets, "FWD_MON:CMD_MONITOR.ALRM -> Coil(FWD_ALRM)"), "{:?}", nets);
    let (i, o) = pins(block(&g.networks[0], "CMD_MONITOR"));
    assert_eq!(i.len(), 7);
    assert_eq!(o.iter().map(|x| x.0.clone().unwrap()).collect::<Vec<_>>(), ["CMD", "ALRM"]);
}

#[test]
fn sfc_single_field_action_blocks_table_44() {
    let p = parse_src(&pou(
        "FUNCTION_BLOCK",
        "VAR a, b, X : BOOL; END_VAR",
        r#"
  +==+===+   +------+
  || S1 ||---| ACT1 |
  +==+===+   +------+
     |       | ACT2 |
     + a     +------+
     |
  +--+--+   +-----------+
  | S2  |---|   ACT3    |
  +--+--+   +-----------+
     |      | X := NOT X; |
     + b    +-------------+
     |
  +--+--+
  | S3  |
  +-----+
"#,
    ));
    // the "d" field must be as wide as its action block
    assert!(!p.errors.is_empty());
    let p = parse_src(&pou(
        "FUNCTION_BLOCK",
        "VAR a, b, X : BOOL; END_VAR",
        r#"
  +==+===+   +------+
  || S1 ||---| ACT1 |
  +==+===+   +------+
     |       | ACT2 |
     + a     +------+
     |
  +--+--+   +-------------+
  | S2  |---|    ACT3     |
  +--+--+   +-------------+
     |      | X := NOT X; |
     + b    +-------------+
     |
  +--+--+
  | S3  |
  +-----+
"#,
    ))
    .ok();
    assert_eq!(steps(&p), ["*S1 ACT1(-) ACT2(-)", "S2 ACT3(-)", "S3"]);
    let s = sfc_of(&p);
    assert_eq!(s.actions.len(), 1);
    assert_eq!(s.actions[0].name.text, "ACT3");
    assert!(matches!(&s.actions[0].body, Body::St(v) if v.len() == 1));
}

#[test]
fn f6_analog_monitor_fbd_side_by_side_networks() {
    let p = parse_src(&pou(
        "FUNCTION_BLOCK",
        "VAR_INPUT X : REAL; L : ANALOG_LIMITS; END_VAR VAR_OUTPUT SE, ME, ALRM, WARN, QH : BOOL; END_VAR
VAR SIGNAL_ALARM, MEAS_ALARM, ALARM, WARNING : LIMITS_ALARM; END_VAR",
        r#"
         SIGNAL_ALARM                        MEAS_ALARM
       +--------------+                    +--------------+
       | LIMITS_ALARM |                    | LIMITS_ALARM |
L.HS---|H            Q|---SE        L.HM---|H            Q|---ME
X------|X             |             X------|X             |
L.LS---|L             |             L.LM---|L             |
EPS----|EPS           |             EPS----|EPS           |
       +--------------+                    +--------------+

            ALARM                           WARNING
       +--------------+                +--------------+
       | LIMITS_ALARM |                | LIMITS_ALARM |
L.HA---|H            Q|---ALRM L.HW----|H            Q|---WARN
X------|X             |            X---|X             |
L.LA---|L             |         L.LW---|L             |
EPS----|EPS           |          EPS---|EPS           |
       +--------------+                +--------------+
                          +------+
       SIGNAL_ALARM.QH----| >= 1 |---QH
       MEAS_ALARM.QH------|      |
       ALARM.QH-----------|      |
       WARNING.QH---------|      |
                          +------+
"#,
    ))
    .ok();
    let n = &p.graphic().networks;
    assert_eq!(n.len(), 5);
    let names: Vec<String> = n.iter().map(|nw| p.nodes(nw).into_iter().find(|x| x.contains("LIMITS_ALARM") || x == ">=1").unwrap()).collect();
    assert_eq!(names, ["SIGNAL_ALARM:LIMITS_ALARM", "MEAS_ALARM:LIMITS_ALARM", "ALARM:LIMITS_ALARM", "WARNING:LIMITS_ALARM", ">=1"]);
    assert_nets!(
        p,
        &n[2],
        [
            "Src(L.HA) -> ALARM:LIMITS_ALARM.H",
            "Src(X) -> ALARM:LIMITS_ALARM.X",
            "Src(L.LA) -> ALARM:LIMITS_ALARM.L",
            "Src(EPS) -> ALARM:LIMITS_ALARM.EPS",
            "ALARM:LIMITS_ALARM.Q -> Var(ALRM)"
        ]
    );
    assert_nets!(
        p,
        &n[4],
        ["Src(SIGNAL_ALARM.QH) -> >=1.in0", "Src(MEAS_ALARM.QH) -> >=1.in1", "Src(ALARM.QH) -> >=1.in2", "Src(WARNING.QH) -> >=1.in3", ">=1.out0 -> Var(QH)"]
    );
}

#[test]
fn numeric_network_labels() {
    let p = parse_src(&pou(
        "PROGRAM",
        "VAR a, b : BOOL; END_VAR",
        r#"
a---->>10
10:
b---<RETURN>
"#,
    ))
    .ok();
    let n = &p.graphic().networks;
    assert_eq!(n.len(), 2);
    assert_eq!(n[1].label.as_ref().unwrap().text, "10");
}
