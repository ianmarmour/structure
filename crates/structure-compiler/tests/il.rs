mod common;
use common::*;
use structure_runtime::Value;

const CFG: &str = "CONFIGURATION C RESOURCE R ON VM TASK T(INTERVAL := T#10ms, PRIORITY := 1); PROGRAM P1 WITH T : P; END_RESOURCE END_CONFIGURATION";

#[test]
fn stack_int_il() {
    let src = format!(r#"
FUNCTION_BLOCK STACK_INT
VAR_INPUT PUSH, POP: BOOL R_EDGE; R1 : BOOL ; IN : INT ; N : INT ; END_VAR
VAR_OUTPUT EMPTY : BOOL := 1 ; OFLO : BOOL := 0 ; OUT : INT := 0 ; END_VAR
VAR STK : ARRAY[0..127] OF INT; NI : INT :=128 ; PTR : INT := -1 ; END_VAR
          LD     R1      (* Dispatch on operations *)
          JMPC   RESET
          LD     POP
          ANDN   EMPTY   (* Don't pop empty stack *)
          JMPC   POP_STK
          LD     PUSH
          ANDN   OFLO
          JMPC   PUSH_STK
          RET
RESET:    LD     0
          ST     OFLO
          LD     1
          ST     EMPTY
          LD     -1
          ST     PTR
          LD     1
          LIMIT  N, 128
          ST     NI
          JMP    ZRO_OUT
POP_STK:  LD     0
          ST     OFLO
          LD     PTR
          SUB    1
          ST     PTR
          LT     0
          ST     EMPTY
          JMPC   ZRO_OUT
          LD     STK[PTR]
          JMP    SET_OUT
PUSH_STK: LD     0
          ST     EMPTY
          LD     PTR
          ADD    1
          ST     PTR
          EQ     NI
          ST     OFLO
          JMPC   ZRO_OUT
          LD     IN
          ST     STK[PTR]
          JMP    SET_OUT
ZRO_OUT:  LD     0
SET_OUT:  ST     OUT
END_FUNCTION_BLOCK

PROGRAM P
VAR S : STACK_INT; PUSH, POP, R1 : BOOL; IN : INT; OUT : INT; EMPTY, OFLO : BOOL; END_VAR
S(PUSH := PUSH, POP := POP, R1 := R1, IN := IN, N := 3);
OUT := S.OUT; EMPTY := S.EMPTY; OFLO := S.OFLO;
END_PROGRAM
{CFG}
"#);
    let mut rt = build(&src);
    let set = |rt: &mut structure_runtime::Runtime, k: &str, v: Value| rt.write(&format!("R.P1.{}", k), &v).unwrap();
    set(&mut rt, "R1", Value::Bool(true));
    rt.run_for(10_000);
    set(&mut rt, "R1", Value::Bool(false));
    rt.run_for(10_000);
    assert!(boolean(&rt, "R.P1.EMPTY"));
    for v in [10, 20, 30, 40] {
        set(&mut rt, "IN", Value::Int(v));
        set(&mut rt, "PUSH", Value::Bool(true));
        rt.run_for(10_000);
        set(&mut rt, "PUSH", Value::Bool(false));
        rt.run_for(10_000);
    }
    // depth 3: the 4th push overflows
    assert!(boolean(&rt, "R.P1.OFLO"));
    assert_eq!(int(&rt, "R.P1.OUT"), 0);
    set(&mut rt, "POP", Value::Bool(true));
    rt.run_for(10_000);
    set(&mut rt, "POP", Value::Bool(false));
    rt.run_for(10_000);
    assert!(!boolean(&rt, "R.P1.OFLO"));
    assert_eq!(int(&rt, "R.P1.OUT"), 30);
    assert!(rt.errors.is_empty(), "{:?}", rt.errors);
}

#[test]
fn cmd_monitor_il_and_input_operators() {
    let src = format!(r#"
FUNCTION_BLOCK CMD_MONITOR
VAR_INPUT AUTO_CMD, AUTO_MODE, MAN_CMD, MAN_CMD_CHK : BOOL; T_CMD_MAX : TIME; FDBK, ACK : BOOL; END_VAR
VAR_OUTPUT CMD, ALRM : BOOL; END_VAR
VAR CMD_TMR : TON ; ALRM_FF : SR ; END_VAR
      LD   T_CMD_MAX
      ST   CMD_TMR.PT   (* Store an input to the TON FB *)
      LD   AUTO_CMD
      AND  AUTO_MODE
      OR(  MAN_CMD
      ANDN AUTO_MODE
      ANDN MAN_CMD_CHK
      )
      ST   CMD
      IN   CMD_TMR   (* Invoke the TON FB *)
      LD   CMD_TMR.Q
      ANDN FDBK
      ST   ALRM_FF.S1
      LD   ACK
      R    ALRM_FF
      LD   ALRM_FF.Q1
      ST   ALRM
END_FUNCTION_BLOCK
PROGRAM P
VAR M : CMD_MONITOR; MAN, FDBK, ACK, CMD, ALRM : BOOL; END_VAR
M(AUTO_CMD := FALSE, AUTO_MODE := FALSE, MAN_CMD := MAN, MAN_CMD_CHK := FALSE, T_CMD_MAX := T#50ms, FDBK := FDBK, ACK := ACK);
CMD := M.CMD; ALRM := M.ALRM;
END_PROGRAM
{CFG}
"#);
    let mut rt = build(&src);
    rt.write("R.P1.MAN", &Value::Bool(true)).unwrap();
    rt.run_for(20_000);
    assert!(boolean(&rt, "R.P1.CMD"));
    assert!(!boolean(&rt, "R.P1.ALRM"));
    rt.run_for(60_000);
    assert!(boolean(&rt, "R.P1.ALRM"));
    rt.write("R.P1.MAN", &Value::Bool(false)).unwrap();
    rt.write("R.P1.ACK", &Value::Bool(true)).unwrap();
    rt.run_for(20_000);
    assert!(!boolean(&rt, "R.P1.ALRM"));
}

#[test]
fn table_53_invocations() {
    let src = format!(r#"
PROGRAM P
VAR C10 : CTU; CMD_TMR : TON; A, B : INT; ELAPSED : TIME; OUT, ERR, TEMPL, COND : BOOL; X : INT; R : INT; FF : SR; Q : BOOL; END_VAR
  CAL C10(%IX10, FALSE, A, OUT, B)
  CAL CMD_TMR(
     IN := TRUE,
     PT := T#300ms,
     Q => OUT,
     ET => ELAPSED,
     ENO => ERR)
  LD A
  ADD 5
  PV C10
  LD TRUE
  ST COND
  LIMIT(
    EN:= COND,
    IN:= 9,
    MN:= 1,
    MX:= 5,
    ENO=> TEMPL
  )
  ST X
  LD 1
  LIMIT 0, 5
  ST R
  CAL FF(
    S1:=(
       LD TRUE
       AND COND
    ),
    R:= FALSE,
    Q1 => Q
  )
END_PROGRAM
{CFG}
"#);
    let mut rt = build(&src);
    rt.run_for(10_000);
    assert_eq!(int(&rt, "R.P1.X"), 5);
    assert!(boolean(&rt, "R.P1.TEMPL"));
    assert_eq!(int(&rt, "R.P1.R"), 1);
    assert!(boolean(&rt, "R.P1.Q"));
    assert_eq!(int(&rt, "R.P1.C10.PV"), 5);
    assert!(boolean(&rt, "R.P1.ERR"));
}

#[test]
fn weigh_function_il() {
    let src = format!(r#"
FUNCTION WEIGH : WORD
VAR_INPUT weigh_command : BOOL; gross_weight : WORD ; tare_weight : INT ; END_VAR
          LD     weigh_command
          JMPC   WEIGH_NOW
          ST     ENO   (* No weighing, 0 to "ENO" *)
          RET
WEIGH_NOW: LD    gross_weight
          WORD_BCD_TO_INT
          SUB    tare_weight
          INT_TO_BCD_WORD   (* Return evaluated weight *)
          ST     WEIGH
END_FUNCTION
PROGRAM P
VAR W1, W2 : WORD; OK1, OK2 : BOOL; END_VAR
W1 := WEIGH(weigh_command := TRUE, gross_weight := WORD#16#0123, tare_weight := 23, ENO => OK1);
W2 := WEIGH(weigh_command := FALSE, gross_weight := WORD#16#0123, tare_weight := 23, ENO => OK2);
END_PROGRAM
{CFG}
"#);
    let mut rt = build(&src);
    rt.run_for(10_000);
    assert!(rt.errors.is_empty(), "{:?}", rt.errors);
    assert_eq!(int(&rt, "R.P1.W1"), 0x0100);
    assert!(boolean(&rt, "R.P1.OK1"));
    assert!(!boolean(&rt, "R.P1.OK2"));
}

#[test]
fn il_type_errors() {
    let e = errors(&format!("PROGRAM P\nVAR A : INT; B : REAL; END_VAR\n LD A\n ADD B\n ST A\nEND_PROGRAM\n{CFG}"));
    assert!(e.contains("type mismatch"), "{}", e);
    let e = errors(&format!("PROGRAM P\nVAR A : INT; END_VAR\n ST A\nEND_PROGRAM\n{CFG}"));
    assert!(e.contains("undefined"), "{}", e);
}
