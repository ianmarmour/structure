mod common;
use common::*;
use structure_runtime::Value;

const CFG: &str = "CONFIGURATION C RESOURCE R ON VM TASK T(INTERVAL := T#10ms, PRIORITY := 1); PROGRAM P1 WITH T : P; END_RESOURCE END_CONFIGURATION";

/// Table 33 features 5a, 6a, 7a and figure 11 (FB instances passed as input,
/// in-out and external), figure 12 (cascaded in-out), feature 11 (VAR_TEMP).
#[test]
fn table_33_instance_passing() {
    let src = format!(
        r#"
FUNCTION_BLOCK INSIDE_A
VAR_INPUT I_TMR : TON; END_VAR
VAR_OUTPUT EXPIRED : BOOL; END_VAR
EXPIRED := I_TMR.Q;
END_FUNCTION_BLOCK

FUNCTION_BLOCK INSIDE_B
VAR_IN_OUT I_TMR : TON; END_VAR
VAR_INPUT TMR_GO : BOOL; END_VAR
VAR_OUTPUT EXPIRED : BOOL; END_VAR
I_TMR(IN := TMR_GO, PT := T#30ms);
EXPIRED := I_TMR.Q;
END_FUNCTION_BLOCK

FUNCTION_BLOCK INSIDE_C
VAR_INPUT TMR_GO : BOOL; END_VAR
VAR_OUTPUT EXPIRED : BOOL; END_VAR
VAR_EXTERNAL X_TMR : TON; END_VAR
X_TMR(IN := TMR_GO, PT := T#30ms);
EXPIRED := X_TMR.Q;
END_FUNCTION_BLOCK

FUNCTION_BLOCK ACCUM
VAR_IN_OUT A : INT; END_VAR
VAR_INPUT X : INT; END_VAR
VAR_TEMP T2 : INT; END_VAR
T2 := X;
A := A + T2;
END_FUNCTION_BLOCK

PROGRAM P
VAR_INPUT GO : BOOL; END_VAR
VAR_GLOBAL X_TMR : TON; END_VAR
VAR E_TMR : TON; A_BLK : INSIDE_A; B_BLK : INSIDE_B; C_BLK : INSIDE_C;
    DA, DB, DC : BOOL; ACC1, ACC2 : ACCUM; ACC, X1, X2, X3, X4 : INT; END_VAR
E_TMR(IN := GO, PT := T#30ms);
A_BLK(I_TMR := E_TMR);
DA := A_BLK.EXPIRED;
B_BLK(I_TMR := E_TMR, TMR_GO := GO);
DB := B_BLK.EXPIRED;
C_BLK(TMR_GO := GO);
DC := C_BLK.EXPIRED;
X1 := 2; X2 := 3; X3 := 4; X4 := 5;
ACC1(A := ACC, X := X1 * X2);
ACC2(A := ACC, X := X3 * X4);
END_PROGRAM
{CFG}"#
    );
    let mut rt = build(&src);
    rt.write("R.P1.GO", &Value::Bool(true)).unwrap();
    rt.run_for(20_000);
    assert!(!boolean(&rt, "R.P1.DA"));
    rt.run_for(40_000);
    assert!(boolean(&rt, "R.P1.DA"));
    assert!(boolean(&rt, "R.P1.DB"));
    assert!(boolean(&rt, "R.P1.DC"));
    assert!(boolean(&rt, "R.P1.X_TMR.Q"));
    // ACC := ACC + X1*X2 + X3*X4 per scan
    assert_eq!(int(&rt, "R.P1.ACC"), 6 * 26);
}

#[test]
fn table_17_18_located_variables() {
    let src = r#"
PROGRAM P
VAR
  INARY AT %IW6 : ARRAY [0..9] OF INT;
  OUTARY AT %QW6 : ARRAY [0..9] OF INT := [10(1)];
  BITS AT %QX0.0 : ARRAY [0..7] OF BOOL := [1,1,0,0,0,1,0,0];
  VALVE_POS AT %QW28 : INT := 100;
  B AT %MB3 : BYTE;
  SUM : INT;
END_VAR
VAR RETAIN AT %QW5 : WORD := 16#FF00; END_VAR
SUM := INARY[0] + INARY[1];
OUTARY[2] := SUM;
B := %IB12;
END_PROGRAM
CONFIGURATION C RESOURCE R ON VM TASK T(INTERVAL := T#10ms, PRIORITY := 1); PROGRAM P1 WITH T : P; END_RESOURCE END_CONFIGURATION
"#;
    let mut rt = build(src);
    // %IW6 is input word 6 = bytes 12..13
    {
        let i = rt.io(0, 'I');
        i[12] = 5;
        i[14] = 7;
    }
    rt.run_for(10_000);
    assert_eq!(int(&rt, "R.P1.SUM"), 12);
    assert_eq!(int(&rt, "R.P1.B"), 5);
    let q = rt.io(0, 'Q').to_vec();
    assert_eq!(q[0], 0b0010_0011, "bit-packed BOOL array located at %QX0.0");
    assert_eq!(u16::from_le_bytes([q[12], q[13]]), 1);
    assert_eq!(u16::from_le_bytes([q[16], q[17]]), 12);
    assert_eq!(u16::from_le_bytes([q[56], q[57]]), 100);
    assert_eq!(u16::from_le_bytes([q[10], q[11]]), 0xFF00);
    let m = rt.io(0, 'M').to_vec();
    assert_eq!(m[3], 5);
}

#[test]
fn table_31_enumerations() {
    let src = format!(
        r#"
TYPE SPEED : (SLOW, MEDIUM, FAST, VERY_FAST); END_TYPE
FUNCTION_BLOCK MOTOR
VAR_INPUT MOTOR_SPEED : SPEED; END_VAR
VAR_OUTPUT SPEED_OUT : SPEED; CODE : INT; END_VAR
CASE MOTOR_SPEED OF
  SLOW: CODE := 1;
  MEDIUM: CODE := 2;
  FAST: CODE := 3;
ELSE CODE := 4;
END_CASE;
SPEED_OUT := SEL(MOTOR_SPEED = FAST, SPEED#SLOW, MUX(1, SLOW, VERY_FAST));
END_FUNCTION_BLOCK
PROGRAM P
VAR M : MOTOR; S : SPEED := MEDIUM; C : INT; O : SPEED; DIFF : BOOL; END_VAR
M(MOTOR_SPEED := S); C := M.CODE; O := M.SPEED_OUT; DIFF := S <> FAST;
END_PROGRAM
{CFG}"#
    );
    let rt = run_prog_src(&src);
    assert_eq!(int(&rt, "R.P1.C"), 2);
    assert_eq!(rt.read("R.P1.O").unwrap(), Value::Enum("SLOW".into()));
    assert!(boolean(&rt, "R.P1.DIFF"));
}

fn run_prog_src(src: &str) -> structure_runtime::Runtime {
    let mut rt = build(src);
    rt.run_for(10_000);
    rt
}

#[test]
fn retain_qualifiers_in_function_blocks() {
    let src = format!(
        r#"
FUNCTION_BLOCK FB
VAR_INPUT IN : INT; END_VAR
VAR_OUTPUT RETAIN OUT : INT; END_VAR
VAR RETAIN R : INT; END_VAR
VAR NON_RETAIN N : INT; END_VAR
VAR X : INT; T1 : TON; END_VAR
OUT := OUT + IN; R := R + 1; N := N + 1; X := X + 1;
END_FUNCTION_BLOCK
PROGRAM P
VAR RETAIN WHOLE : FB; END_VAR
VAR PART : FB; END_VAR
WHOLE(IN := 1); PART(IN := 1);
END_PROGRAM
{CFG}"#
    );
    let mut rt = build(&src);
    rt.run_for(30_000);
    let snap = rt.retain_snapshot();
    rt.warm_start(&snap).unwrap();
    assert_eq!(int(&rt, "R.P1.WHOLE.OUT"), 3);
    assert_eq!(int(&rt, "R.P1.PART.OUT"), 3);
    // non-outputs are hidden from the outside but readable by path
    assert_eq!(rt.read("R.P1.WHOLE.R").unwrap(), Value::Int(3));
    assert_eq!(rt.read("R.P1.WHOLE.X").unwrap(), Value::Int(3));
    assert_eq!(rt.read("R.P1.WHOLE.N").unwrap(), Value::Int(0));
    assert_eq!(rt.read("R.P1.PART.X").unwrap(), Value::Int(0));
    assert_eq!(rt.read("R.P1.PART.R").unwrap(), Value::Int(3));
}

#[test]
fn program_connections_and_constants() {
    let src = r#"
TYPE MODE : (STOPPED, RUNNING); END_TYPE
PROGRAM F
VAR_INPUT x1 : BOOL; x2 : UINT; M : MODE; S : STRING; END_VAR
VAR_OUTPUT y1 : BYTE; Y2 : UINT; END_VAR
VAR_EXTERNAL CONSTANT LIMIT_W : UINT; END_VAR
IF x1 THEN y1 := 1; ELSE y1 := 2; END_IF;
IF M = RUNNING AND S = 'GO' THEN Y2 := x2 + LIMIT_W; END_IF;
END_PROGRAM
CONFIGURATION C
  VAR_GLOBAL CONSTANT LIMIT_W : UINT := 100; END_VAR
  VAR_GLOBAL z2 : BOOL := TRUE; OUT_W : UINT; END_VAR
  RESOURCE R ON VM
    VAR_GLOBAL AT %QW5 : INT; END_VAR
    TASK T(INTERVAL := T#10ms, PRIORITY := 1);
    PROGRAM P1 WITH T : F(x1 := z2, x2 := UINT#5, M := RUNNING, S := 'GO', y1 => %QB2, Y2 => OUT_W);
  END_RESOURCE
END_CONFIGURATION
"#;
    let mut rt = build(src);
    rt.run_for(10_000);
    assert_eq!(rt.io(0, 'Q')[2], 1);
    assert_eq!(int(&rt, "OUT_W"), 105);
    let e = errors(
        r#"
PROGRAM F
VAR_EXTERNAL LIMIT_W : UINT; END_VAR
LIMIT_W := 1;
END_PROGRAM
CONFIGURATION C
  VAR_GLOBAL CONSTANT LIMIT_W : UINT := 100; END_VAR
  RESOURCE R ON VM TASK T(INTERVAL := T#10ms, PRIORITY := 1); PROGRAM P1 WITH T : F; END_RESOURCE
END_CONFIGURATION
"#,
    );
    assert!(e.contains("table 16b"), "{}", e);
}
