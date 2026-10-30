//! Graphical declarations of the external interface of POUs (figures 6 b,
//! 11, 12 a; tables 20a and 33).
mod common;
use common::*;
use structure_runtime::Value;

const CFG: &str = "CONFIGURATION C RESOURCE R ON VM TASK T(INTERVAL := T#10ms, PRIORITY := 1); PROGRAM P1 WITH T : P; END_RESOURCE END_CONFIGURATION";

#[test]
fn table_33_feature_9_edge_inputs() {
    let src = format!(
        r#"
FUNCTION_BLOCK
          +-----------+     (* External interface *)
          | AND_EDGE  |
  BOOL---->X         Z|---BOOL
          |           |
  BOOL----<Y          |
          +-----------+
Z := X AND Y ;
END_FUNCTION_BLOCK
PROGRAM P
VAR E : AND_EDGE; A, B, Z, SEEN : BOOL; END_VAR
E(X := A, Y := B); Z := E.Z; IF Z THEN SEEN := TRUE; END_IF;
END_PROGRAM
{CFG}
"#
    );
    let mut rt = build(&src);
    rt.write("R.P1.B", &Value::Bool(true)).unwrap();
    rt.run_for(10_000);
    // X rises while Y falls
    rt.write("R.P1.A", &Value::Bool(true)).unwrap();
    rt.write("R.P1.B", &Value::Bool(false)).unwrap();
    rt.run_for(10_000);
    assert!(boolean(&rt, "R.P1.SEEN"));
    rt.run_for(10_000);
    assert!(!boolean(&rt, "R.P1.Z"));
}

#[test]
fn figure_6_function_with_result_and_in_out() {
    let src = format!(
        r#"
FUNCTION
        +-------------+ (* External interface specification *)
        | SIMPLE_FUN  |
REAL----|A            |----REAL
REAL----|B            |
REAL----|C            |
INT-----|COUNT---COUNT|----INT
        +-------------+
VAR COUNTP1 : INT ; END_VAR
COUNTP1 := ADD(COUNT,1);
COUNT := COUNTP1 ;
SIMPLE_FUN := A*B/C;
END_FUNCTION
PROGRAM P
VAR X : REAL := 6.0; Y : REAL := 2.0; Z : REAL := 4.0; RESULT : REAL; COUNT1, COUNT2 : INT; END_VAR
RESULT := SIMPLE_FUN(A:=X,B:=Y,C:=Z,COUNT:=COUNT1);
COUNT2 := COUNT1;
END_PROGRAM
{CFG}
"#
    );
    let mut rt = build(&src);
    rt.run_for(20_000);
    assert_eq!(real(&rt, "R.P1.RESULT"), 3.0);
    assert_eq!(int(&rt, "R.P1.COUNT2"), 2);
}

#[test]
fn figure_11_instances_as_inputs_and_in_outs() {
    let src = format!(
        r#"
FUNCTION_BLOCK
    +--------------+   (* External interface *)
    |   INSIDE_A   |
TON-|I_TMR  EXPIRED|---BOOL
    +--------------+
EXPIRED := I_TMR.Q;
END_FUNCTION_BLOCK

FUNCTION_BLOCK
      +--------------+   (* External interface *)
      |   INSIDE_B   |
TON---|I_TMR----I_TMR|---TON
BOOL--|TMR_GO EXPIRED|---BOOL
      +--------------+
I_TMR(IN := TMR_GO, PT := T#30ms);
EXPIRED := I_TMR.Q;
END_FUNCTION_BLOCK

PROGRAM
       +--------------+   (* External interface *)
       |      P       |
BOOL---|GO        DONE|---BOOL
BOOL---|GO2      DONE2|---BOOL
       +--------------+
VAR E_TMR : TON; I_BLK : INSIDE_A; E_TMR2 : TON; I_BLK2 : INSIDE_B; END_VAR
E_TMR(IN := GO, PT := T#30ms);
I_BLK(I_TMR := E_TMR);
DONE := I_BLK.EXPIRED;
I_BLK2(I_TMR := E_TMR2, TMR_GO := GO2);
DONE2 := I_BLK2.EXPIRED;
END_PROGRAM
{CFG}
"#
    );
    let mut rt = build(&src);
    rt.write("R.P1.GO", &Value::Bool(true)).unwrap();
    rt.write("R.P1.GO2", &Value::Bool(true)).unwrap();
    rt.run_for(20_000);
    assert!(!boolean(&rt, "R.P1.DONE") && !boolean(&rt, "R.P1.DONE2"));
    rt.run_for(40_000);
    assert!(boolean(&rt, "R.P1.DONE") && boolean(&rt, "R.P1.DONE2"));
    assert!(boolean(&rt, "R.P1.E_TMR2.Q"), "the in-out instance is the program's");
}

#[test]
fn graphical_declaration_errors() {
    let e = errors(
        r#"
FUNCTION
       +-------+
       |  F1   |
INT----|A      |
       +-------+
F1 := A;
END_FUNCTION
FUNCTION_BLOCK
       +-------+
       |  FB1  |
INT----|A----B |---INT
BOOL---<       |
       |       |---BOOL
       +-------+
END_FUNCTION_BLOCK
FUNCTION_BLOCK
       +-------+
       |  FB2  |
INT----|A
       +-------+
END_FUNCTION_BLOCK
"#,
    );
    assert!(e.contains("shall show the type of its result"), "{}", e);
    assert!(e.contains("both ends of an in-out variable shall have the same name"), "{}", e);
    assert!(e.contains("an input of a graphical declaration shall be named inside the box"), "{}", e);
    assert!(e.contains("an output of a graphical declaration shall be named inside the box"), "{}", e);
    assert!(e.contains("expected the sides of the box"), "{}", e);
}

#[test]
fn graphical_declaration_written_as_text() {
    let src = r#"
FUNCTION_BLOCK
        +-----------+
        | STACK_INT |
BOOL---->PUSH  EMPTY|---BOOL
BOOL---->POP    OFLO|---BOOL
BOOL----|R1      OUT|---INT
INT-----|IN         |
INT-----|N          |
        +-----------+
VAR STK : ARRAY[0..127] OF INT; END_VAR
END_FUNCTION_BLOCK
"#;
    let mut sm = structure_syntax::SourceMap::new();
    let id = sm.add("a.st", src);
    let r = structure_syntax::parser::parse(sm.get(id));
    assert!(r.diagnostics.is_empty(), "{:?}", r.diagnostics.iter().map(|d| d.message.clone()).collect::<Vec<_>>());
    let text = structure_syntax::writer::write_unit(&r.unit);
    assert!(text.contains("FUNCTION_BLOCK STACK_INT"), "{}", text);
    assert!(text.contains("PUSH : BOOL R_EDGE;"), "{}", text);
    assert!(text.contains("OUT : INT;"), "{}", text);
}
