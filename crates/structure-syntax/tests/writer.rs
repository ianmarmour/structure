use structure_syntax::parser::parse;
use structure_syntax::source::SourceMap;
use structure_syntax::writer::write_unit;

fn round_trip(src: &str) {
    let mut sm = SourceMap::new();
    let id = sm.add("a.st", src);
    let r1 = parse(sm.get(id));
    assert!(r1.diagnostics.iter().all(|d| d.severity != structure_syntax::Severity::Error), "{:?}", r1.diagnostics);
    let t1 = write_unit(&r1.unit);
    let id2 = sm.add("b.st", t1.clone());
    let r2 = parse(sm.get(id2));
    assert!(r2.diagnostics.iter().all(|d| d.severity != structure_syntax::Severity::Error), "re-parse failed:\n{}\n{:?}", t1, r2.diagnostics);
    let t2 = write_unit(&r2.unit);
    assert_eq!(t1, t2);
}

#[test]
fn writes_all_element_kinds() {
    round_trip(
        r#"
TYPE ANALOG_DATA : INT (-4095..4095) := 0; STR10 : STRING[10] := 'AB$'$$C'; E : (A, B, C) := B;
  CH : STRUCT RANGE : E; MIN_SCALE : ANALOG_DATA := -4095; END_STRUCT;
  ARR : ARRAY [1..2, 1..3] OF INT := [1, 2, 3(4), 6];
END_TYPE
FUNCTION F : REAL
VAR_INPUT A, B : REAL; C : REAL := 1.0; END_VAR
VAR_IN_OUT COUNT : INT; END_VAR
F := A * (B + C) / -C ** 2.0;
IF A > B AND NOT (C = 0.0) THEN F := 0.0; ELSIF A < B THEN F := 1.0; ELSE F := 2.0; END_IF;
CASE COUNT OF 1, 2: COUNT := 0; 3..5: COUNT := -1; ELSE COUNT := 7; END_CASE;
FOR COUNT := 1 TO 10 BY 2 DO EXIT; END_FOR;
WHILE FALSE DO ; END_WHILE;
REPEAT RETURN; UNTIL TRUE END_REPEAT;
END_FUNCTION
FUNCTION_BLOCK FB
VAR_INPUT X : BOOL R_EDGE; END_VAR
VAR_OUTPUT Q : BOOL; END_VAR
VAR T1 : TON := (PT := T#1h2m3s4.5ms); D : DT := DT#1984-06-25-15:36:55.36; W : WSTRING := "$00C4"; END_VAR
  LD X
  ANDN Q
  OR( Q
  AND X
  )
  ST Q
  JMPC L1
  CAL T1(IN := X, PT := T#5s, Q => Q)
L1: RET
END_FUNCTION_BLOCK
PROGRAM P
VAR_INPUT x1 : BOOL; END_VAR
VAR_OUTPUT y1 : BYTE; END_VAR
VAR AT %QX5.1 : BOOL := 1; C2 AT %Q* : BYTE; END_VAR
INITIAL_STEP S1: A1(L, T#10s, DN1); A2(P); END_STEP
TRANSITION FROM S1 TO (S2, S3) := x1; END_TRANSITION
STEP S2: END_STEP
STEP S3: END_STEP
TRANSITION FROM (S2, S3) TO S1 :
  LD x1
END_TRANSITION
ACTION A1: y1 := 1; END_ACTION
END_PROGRAM
CONFIGURATION C
  VAR_GLOBAL w : UINT; END_VAR
  RESOURCE R ON CPU
    TASK T(INTERVAL := T#10ms, PRIORITY := 1);
    PROGRAM RETAIN P1 WITH T : P(x1 := %IX1.1, y1 => w);
  END_RESOURCE
  VAR_ACCESS A : R.P1.y1 : BYTE READ_ONLY; END_VAR
  VAR_CONFIG R.P1.C2 AT %QB25 : BYTE; END_VAR
END_CONFIGURATION
"#,
    );
}
