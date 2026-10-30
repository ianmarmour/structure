//! Standard function blocks (2.5.2.3, tables 34 to 37) and the action
//! control function block of SFC (2.6.4.5, figures 14 a and 15 a),
//! defined in Structured Text following the definitions of the standard.

fn counter_up(name: &str, ty: &str, max: &str) -> String {
    format!(
        "FUNCTION_BLOCK {name}
VAR_INPUT CU : BOOL R_EDGE; R : BOOL; PV : {ty}; END_VAR
VAR_OUTPUT Q : BOOL; CV : {ty}; END_VAR
IF R THEN CV := 0;
ELSIF CU AND (CV < {max}) THEN CV := CV + 1;
END_IF;
Q := (CV >= PV);
END_FUNCTION_BLOCK
"
    )
}

fn counter_down(name: &str, ty: &str, min: &str) -> String {
    format!(
        "FUNCTION_BLOCK {name}
VAR_INPUT CD : BOOL R_EDGE; LD : BOOL; PV : {ty}; END_VAR
VAR_OUTPUT Q : BOOL; CV : {ty}; END_VAR
IF LD THEN CV := PV;
ELSIF CD AND (CV > {min}) THEN CV := CV - 1;
END_IF;
Q := (CV <= 0);
END_FUNCTION_BLOCK
"
    )
}

fn counter_up_down(name: &str, ty: &str, min: &str, max: &str) -> String {
    format!(
        "FUNCTION_BLOCK {name}
VAR_INPUT CU : BOOL R_EDGE; CD : BOOL R_EDGE; R : BOOL; LD : BOOL; PV : {ty}; END_VAR
VAR_OUTPUT QU : BOOL; QD : BOOL; CV : {ty}; END_VAR
IF R THEN CV := 0;
ELSIF LD THEN CV := PV;
ELSE
  IF NOT (CU AND CD) THEN
    IF CU AND (CV < {max}) THEN CV := CV + 1;
    ELSIF CD AND (CV > {min}) THEN CV := CV - 1;
    END_IF;
  END_IF;
END_IF;
QU := (CV >= PV);
QD := (CV <= 0);
END_FUNCTION_BLOCK
"
    )
}

const FIXED: &str = r#"
FUNCTION_BLOCK SR
VAR_INPUT S1 : BOOL; R : BOOL; END_VAR
VAR_OUTPUT Q1 : BOOL; END_VAR
Q1 := S1 OR (NOT R AND Q1);
END_FUNCTION_BLOCK

FUNCTION_BLOCK RS
VAR_INPUT S : BOOL; R1 : BOOL; END_VAR
VAR_OUTPUT Q1 : BOOL; END_VAR
Q1 := NOT R1 AND (S OR Q1);
END_FUNCTION_BLOCK

FUNCTION_BLOCK R_TRIG
VAR_INPUT CLK : BOOL; END_VAR
VAR_OUTPUT Q : BOOL; END_VAR
VAR M : BOOL; END_VAR
Q := CLK AND NOT M;
M := CLK;
END_FUNCTION_BLOCK

FUNCTION_BLOCK F_TRIG
VAR_INPUT CLK : BOOL; END_VAR
VAR_OUTPUT Q : BOOL; END_VAR
VAR M : BOOL; END_VAR
Q := NOT CLK AND NOT M;
M := NOT CLK;
END_FUNCTION_BLOCK

FUNCTION_BLOCK TP
VAR_INPUT IN : BOOL; PT : TIME; END_VAR
VAR_OUTPUT Q : BOOL; ET : TIME; END_VAR
VAR START : TIME; PREV : BOOL; END_VAR
IF Q THEN
  ET := __NOW() - START;
  IF ET >= PT THEN
    Q := FALSE;
    IF IN THEN ET := PT; ELSE ET := T#0s; END_IF;
  END_IF;
ELSIF IN AND NOT PREV THEN
  START := __NOW();
  ET := T#0s;
  Q := PT > T#0s;
ELSIF NOT IN THEN
  ET := T#0s;
END_IF;
PREV := IN;
END_FUNCTION_BLOCK

FUNCTION_BLOCK TON
VAR_INPUT IN : BOOL; PT : TIME; END_VAR
VAR_OUTPUT Q : BOOL; ET : TIME; END_VAR
VAR START : TIME; RUNNING : BOOL; END_VAR
IF IN THEN
  IF NOT RUNNING THEN
    RUNNING := TRUE;
    START := __NOW();
  END_IF;
  ET := __NOW() - START;
  IF ET >= PT THEN
    ET := PT;
    Q := TRUE;
  ELSE
    Q := FALSE;
  END_IF;
ELSE
  RUNNING := FALSE;
  Q := FALSE;
  ET := T#0s;
END_IF;
END_FUNCTION_BLOCK

FUNCTION_BLOCK TOF
VAR_INPUT IN : BOOL; PT : TIME; END_VAR
VAR_OUTPUT Q : BOOL; ET : TIME; END_VAR
VAR START : TIME; RUNNING : BOOL; END_VAR
IF IN THEN
  RUNNING := FALSE;
  Q := TRUE;
  ET := T#0s;
ELSIF Q THEN
  IF NOT RUNNING THEN
    RUNNING := TRUE;
    START := __NOW();
  END_IF;
  ET := __NOW() - START;
  IF ET >= PT THEN
    ET := PT;
    Q := FALSE;
    RUNNING := FALSE;
  END_IF;
END_IF;
END_FUNCTION_BLOCK

FUNCTION_BLOCK RTC
VAR_INPUT IN : BOOL; PDT : DT; END_VAR
VAR_OUTPUT Q : BOOL; CDT : DT; END_VAR
VAR PREV : BOOL; START : TIME; BASE : DT; END_VAR
IF IN THEN
  IF NOT PREV THEN
    BASE := PDT;
    START := __NOW();
  END_IF;
  CDT := ADD_DT_TIME(BASE, __NOW() - START);
END_IF;
Q := IN;
PREV := IN;
END_FUNCTION_BLOCK

FUNCTION_BLOCK __ACTION_CONTROL
VAR_INPUT N, R, S, L, D, P, P1, P0, SD, DS, SL : BOOL; T : TIME; END_VAR
VAR_OUTPUT Q, A : BOOL; END_VAR
VAR
  S_FF : RS; L_TMR : TON; D_TMR : TON; P_TRIG : R_TRIG;
  SD_FF : RS; SD_TMR : TON; DS_TMR : TON; DS_FF : RS;
  SL_FF : RS; SL_TMR : TON;
  P1_TRIG : R_TRIG;
  (* Falling edges of Q and P0. Unlike F_TRIG (table 35, note), these do
     not report an edge on the first execution after a cold restart. *)
  PREV_Q, PREV_P0 : BOOL;
END_VAR
(* 2.6.4.5 rule 4 b) and c): action control contention *)
IF (SD AND SL_FF.Q1) OR (SL AND SD_FF.Q1) THEN
  __TRAP_CONTENTION();
END_IF;
S_FF(S := S, R1 := R);
L_TMR(IN := L, PT := T);
D_TMR(IN := D, PT := T);
P_TRIG(CLK := P);
SD_FF(S := SD, R1 := R);
SD_TMR(IN := SD_FF.Q1, PT := T);
DS_TMR(IN := DS, PT := T);
DS_FF(S := DS_TMR.Q, R1 := R);
SL_FF(S := SL, R1 := R);
SL_TMR(IN := SL_FF.Q1, PT := T);
Q := NOT R AND (N OR S_FF.Q1 OR (L AND NOT L_TMR.Q) OR D_TMR.Q OR P_TRIG.Q
     OR SD_TMR.Q OR DS_FF.Q1 OR (SL_FF.Q1 AND NOT SL_TMR.Q));
P1_TRIG(CLK := P1);
A := Q OR (PREV_Q AND NOT Q) OR P1_TRIG.Q OR (PREV_P0 AND NOT P0);
PREV_Q := Q;
PREV_P0 := P0;
END_FUNCTION_BLOCK
"#;

/// Source text of the standard library.
pub fn source() -> String {
    let mut s = String::from(FIXED);
    // Counters (table 36). PVmax and PVmin are the limits of the type (annex D).
    for (name, ty, max) in [
        ("CTU", "INT", "INT#32767"),
        ("CTU_DINT", "DINT", "DINT#2147483647"),
        ("CTU_LINT", "LINT", "LINT#9223372036854775807"),
        ("CTU_UDINT", "UDINT", "UDINT#4294967295"),
        ("CTU_ULINT", "ULINT", "ULINT#18446744073709551615"),
    ] {
        s.push_str(&counter_up(name, ty, max));
    }
    for (name, ty, min) in [
        ("CTD", "INT", "INT#-32768"),
        ("CTD_DINT", "DINT", "DINT#-2147483648"),
        ("CTD_LINT", "LINT", "LINT#-9223372036854775808"),
        ("CTD_UDINT", "UDINT", "UDINT#0"),
        ("CTD_ULINT", "ULINT", "ULINT#0"),
    ] {
        s.push_str(&counter_down(name, ty, min));
    }
    for (name, ty, min, max) in [
        ("CTUD", "INT", "INT#-32768", "INT#32767"),
        ("CTUD_DINT", "DINT", "DINT#-2147483648", "DINT#2147483647"),
        ("CTUD_LINT", "LINT", "LINT#-9223372036854775808", "LINT#9223372036854775807"),
        ("CTUD_UDINT", "UDINT", "UDINT#0", "UDINT#4294967295"),
        ("CTUD_ULINT", "ULINT", "ULINT#0", "ULINT#18446744073709551615"),
    ] {
        s.push_str(&counter_up_down(name, ty, min, max));
    }
    s
}

/// Names of the standard function blocks.
pub const STANDARD_FUNCTION_BLOCKS: &[&str] = &[
    "SR", "RS", "R_TRIG", "F_TRIG", "CTU", "CTU_DINT", "CTU_LINT", "CTU_UDINT", "CTU_ULINT", "CTD", "CTD_DINT", "CTD_LINT", "CTD_UDINT", "CTD_ULINT",
    "CTUD", "CTUD_DINT", "CTUD_LINT", "CTUD_UDINT", "CTUD_ULINT", "TP", "TON", "TOF", "RTC",
];
