use structure_syntax::ast::*;
use structure_syntax::parser::parse;
use structure_syntax::source::SourceMap;

fn parse_ok(src: &str) -> SourceUnit {
    let mut sm = SourceMap::new();
    let id = sm.add("test.st", src);
    let r = parse(sm.get(id));
    let errs: Vec<_> = r.diagnostics.iter().filter(|d| d.severity == structure_syntax::Severity::Error).cloned().collect();
    if !errs.is_empty() {
        let mut ds = structure_syntax::Diagnostics::new();
        ds.extend(errs);
        panic!("unexpected diagnostics:\n{}", ds.render(&sm));
    }
    r.unit
}

#[test]
fn data_types_table_12_14() {
    let u = parse_ok(
        r#"
TYPE RU_REAL : REAL ; END_TYPE
TYPE ANALOG_SIGNAL_TYPE : (SINGLE_ENDED, DIFFERENTIAL) ; END_TYPE
TYPE ANALOG_DATA : INT (-4095..4095) ; END_TYPE
TYPE ANALOG_16_INPUT_DATA : ARRAY [1..16] OF ANALOG_DATA ; END_TYPE
TYPE
  ANALOG_CHANNEL_CONFIGURATION :
  STRUCT
    RANGE : ANALOG_SIGNAL_RANGE ;
    MIN_SCALE : ANALOG_DATA ;
    MAX_SCALE : ANALOG_DATA ;
  END_STRUCT ;
  ANALOG_16_INPUT_CONFIGURATION :
  STRUCT
    SIGNAL_TYPE : ANALOG_SIGNAL_TYPE ;
    FILTER_PARAMETER : SINT (0..99) ;
    CHANNEL : ARRAY [1..16] OF ANALOG_CHANNEL_CONFIGURATION ;
  END_STRUCT ;
END_TYPE
TYPE FREQ : REAL := 50.0 ; END_TYPE
TYPE ANALOG_SIGNAL_RANGE :
  (BIPOLAR_10V,  (* -10 to +10 VDC *)
   UNIPOLAR_10V, (* 0 to +10 VDC *)
   UNIPOLAR_1_5V,
   UNIPOLAR_0_5V,
   UNIPOLAR_4_20_MA,
   UNIPOLAR_0_20_MA
  ) := UNIPOLAR_1_5V ;
END_TYPE
TYPE ANALOG_DATAZ : INT (-4095..4095) := 0 ; END_TYPE
TYPE ANALOG_16_INPUT_DATAI :
  ARRAY [1..16] OF ANALOG_DATA := [8(-4095), 8(4095)] ;
END_TYPE
TYPE ANALOG_CHANNEL_CONFIGZ :
  ANALOG_CHANNEL_CONFIGURATIONI
  := (MIN_SCALE := 0, MAX_SCALE := 4000);
END_TYPE
TYPE STR10 : STRING[10] := 'ABCDEF'; END_TYPE
"#,
    );
    assert_eq!(u.elements.len(), 11);
}

#[test]
fn function_block_st_and_il() {
    let u = parse_ok(
        r#"
FUNCTION_BLOCK CMD_MONITOR
VAR_INPUT AUTO_CMD : BOOL ; (* Automated command *)
  AUTO_MODE : BOOL ;
  MAN_CMD : BOOL ;
  MAN_CMD_CHK : BOOL ;
  T_CMD_MAX : TIME ;
  FDBK : BOOL ;
  ACK : BOOL ;
END_VAR
VAR_OUTPUT CMD : BOOL ; ALRM : BOOL ; END_VAR
VAR CMD_TMR : TON ; ALRM_FF : SR ; END_VAR
CMD := AUTO_CMD & AUTO_MODE
  OR MAN_CMD & NOT MAN_CMD_CHK & NOT AUTO_MODE ;
CMD_TMR (IN := CMD, PT := T_CMD_MAX);
ALRM_FF (S1 := CMD_TMR.Q & NOT FDBK, R := ACK);
ALRM := ALRM_FF.Q1;
END_FUNCTION_BLOCK

FUNCTION_BLOCK CMD_MONITOR_IL
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
"#,
    );
    let LibraryElement::Pou(p) = &u.elements[1] else { panic!() };
    let Body::Il(list) = &p.body else { panic!("expected IL body, got {:?}", p.body) };
    assert_eq!(list.len(), 14);
    let LibraryElement::Pou(p) = &u.elements[0] else { panic!() };
    assert!(matches!(p.body, Body::St(ref s) if s.len() == 4));
}

#[test]
fn stack_int_il_with_labels() {
    let u = parse_ok(
        r#"
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
"#,
    );
    let LibraryElement::Pou(p) = &u.elements[0] else { panic!() };
    let Body::Il(list) = &p.body else { panic!() };
    assert_eq!(list.len(), 42);
}

#[test]
fn il_calls() {
    parse_ok(
        r#"
FUNCTION_BLOCK X
VAR C10 : CTU; CMD_TMR : TON; A, B : INT; ELAPSED : TIME; OUT, ERR, TEMPL, COND : BOOL; FWD_REV_FF : SR; END_VAR
  CAL C10(%IX10, FALSE, A, OUT, B)
  CAL CMD_TMR(
     IN := %IX5,
     PT := T#300ms,
     Q => OUT,
     ET => ELAPSED,
     ENO => ERR)
  LD A
  ADD 5
  PV C10
  LIMIT(
    EN:= COND,
    IN:= B,
    MN:= 1,
    MX:= 5,
    ENO=> TEMPL
  )
  ST A
  LD 1
  LIMIT B, 5
  ST A
  CAL FWD_REV_FF(
    S1:=(
       LD A
       AND B
    ),
    R:= COND,
    Q1 => OUT
  )
  AND( %IX1
  OR %IX2
  )
  AND(
  LD %IX1
  OR %IX2
  )
  &N COND
END_FUNCTION_BLOCK
"#,
    );
}

#[test]
fn st_statements_table_56() {
    parse_ok(
        r#"
PROGRAM P
VAR A, B, C, D, X1, X2 : REAL; NROOTS, TW, DISPLAY, J, I : INT; WORDS : ARRAY[1..100] OF STRING; FLAG: BOOL; SUM : INT; END_VAR
D := B*B - 4.0*A*C ;
IF D < 0.0 THEN NROOTS := 0 ;
ELSIF D = 0.0 THEN
  NROOTS := 1 ;
  X1 := - B/(2.0*A) ;
ELSE
  NROOTS := 2 ;
  X1 := (- B + SQRT(D))/(2.0*A) ;
  X2 := (- B - SQRT(D))/(2.0*A) ;
END_IF ;
CASE TW OF
  1,5: DISPLAY := 1;
  2: DISPLAY := 2;
  3: DISPLAY := 3 - 2;
  4,6..10: DISPLAY := 4;
ELSE DISPLAY := 0 ;
  TW := 1;
END_CASE;
J := 101 ;
FOR I := 1 TO 100 BY 2 DO
  IF WORDS[I] = 'KEY' THEN
    J := I ;
    EXIT ;
  END_IF ;
END_FOR ;
J := 1;
WHILE J <= 100 & WORDS[J] <> 'KEY' DO
  J := J+2 ;
END_WHILE ;
J := -1 ;
REPEAT
  J := J+2 ;
UNTIL J = 101 OR WORDS[J] = 'KEY'
END_REPEAT ;
SUM := 0 ;
FOR I := 1 TO 3 DO
  FOR J := 1 TO 2 DO
    IF FLAG THEN EXIT ; END_IF
    SUM := SUM + J ;
  END_FOR ;
  SUM := SUM + I ;
END_FOR ;
RETURN ;
;
END_PROGRAM
"#,
    );
}

#[test]
fn sfc_textual_gravel() {
    let u = parse_ok(
        r#"
PROGRAM GRAVEL
VAR_INPUT FILL_PB, OFF_PB, ON_PB, LOAD_PB : BOOL; END_VAR
VAR OK_TO_RUN : BOOL; RUN_IN_TIME, RUNOUT_TIME : TIME; LEVEL_CTR : CTU; SILO_VALVE, BIN_VALVE, BIN_EMPTY_LS: BOOL; END_VAR
INITIAL_STEP START : END_STEP
TRANSITION FROM START TO FILL_BIN
   := FILL_PB & CONTROL.X ; END_TRANSITION
STEP FILL_BIN: SILO_VALVE(N); END_STEP
TRANSITION FROM FILL_BIN TO START
   := NOT FILL_PB OR NOT CONTROL.X ; END_TRANSITION
TRANSITION FROM FILL_BIN TO LOAD_WAIT := LEVEL_CTR.Q ;
END_TRANSITION
STEP LOAD_WAIT : END_STEP
TRANSITION FROM LOAD_WAIT TO RUN_IN
   := LOAD_PB & OK_TO_RUN ; END_TRANSITION
STEP RUN_IN : END_STEP
TRANSITION FROM RUN_IN TO DUMP_BIN
   := RUN_IN.T > RUN_IN_TIME;
END_TRANSITION
STEP DUMP_BIN: BIN_VALVE(N); END_STEP
TRANSITION FROM DUMP_BIN TO RUNOUT := BIN_EMPTY_LS ;
END_TRANSITION
STEP RUNOUT : END_STEP
TRANSITION FROM RUNOUT TO START
   := RUNOUT.T >= RUNOUT_TIME ; END_TRANSITION
INITIAL_STEP CONTROL_OFF: END_STEP
TRANSITION FROM CONTROL_OFF TO CONTROL
   := ON_PB & NOT OFF_PB ; END_TRANSITION
STEP CONTROL: CONTROL_ACTION(N); END_STEP
ACTION CONTROL_ACTION:
   OK_TO_RUN := CONTROL.X ;
END_ACTION
TRANSITION FROM CONTROL TO CONTROL_OFF := OFF_PB ;
END_TRANSITION
STEP S8: ACTION_1(L,t#10s,DN1) ; ACTION_2(P) ; ACTION_3(N) ; END_STEP
TRANSITION FROM S8 TO (A, B) : 
  LD %IX2.4
  AND %IX2.3
END_TRANSITION
ACTION ACTION_4:
   LD S8.X
   AND %IX1
   ST %QX17
END_ACTION
END_PROGRAM
"#,
    );
    let LibraryElement::Pou(p) = &u.elements[0] else { panic!() };
    let Body::Sfc(s) = &p.body else { panic!() };
    assert_eq!(s.steps.len(), 9);
    assert_eq!(s.transitions.len(), 10);
    assert_eq!(s.actions.len(), 2);
}

#[test]
fn configuration_figure_20() {
    let u = parse_ok(
        r#"
CONFIGURATION CELL_1
  VAR_GLOBAL w: UINT; END_VAR
  RESOURCE STATION_1 ON PROCESSOR_TYPE_1
    VAR_GLOBAL z1: BYTE; END_VAR
    TASK SLOW_1(INTERVAL := t#20ms, PRIORITY := 2) ;
    TASK FAST_1(INTERVAL := t#10ms, PRIORITY := 1) ;
    PROGRAM P1 WITH SLOW_1 :
       F(x1 := %IX1.1) ;
    PROGRAM P2 : G(OUT1 => w,
                   FB1 WITH SLOW_1,
                   FB2 WITH FAST_1) ;
  END_RESOURCE
  RESOURCE STATION_2 ON PROCESSOR_TYPE_2
    VAR_GLOBAL z2 : BOOL ;
               AT %QW5 : INT ;
    END_VAR
    TASK PER_2(INTERVAL := t#50ms, PRIORITY := 2) ;
    TASK INT_2(SINGLE := z2, PRIORITY := 1) ;
    PROGRAM P1 WITH PER_2 :
       F(x1 := z2, x2 := w) ;
    PROGRAM P4 WITH INT_2 :
       H(HOUT1 => %QW5,
         FB1 WITH PER_2);
  END_RESOURCE
  VAR_ACCESS
    ABLE  : STATION_1.%IX1.1 : BOOL READ_ONLY ;
    BAKER : STATION_1.P1.x2 : UINT READ_WRITE ;
    CHARLIE : STATION_1.z1 : BYTE ;
    DOG : w : UINT READ_ONLY ;
    ALPHA : STATION_2.P1.y1 : BYTE READ_ONLY ;
    BETA : STATION_2.P4.HOUT1 : INT READ_ONLY ;
    GAMMA : STATION_2.z2 : BOOL READ_WRITE ;
    S1_COUNT : STATION_1.P1.COUNT : INT;
    THETA : STATION_2.P4.FB2.d1 : BOOL READ_WRITE;
    ZETA : STATION_2.P4.FB1.c1 : BOOL READ_ONLY;
    OMEGA : STATION_2.P4.FB1.C3 : INT READ_WRITE;
  END_VAR
  VAR_CONFIG
    STATION_1.P1.COUNT : INT := 1;
    STATION_2.P1.COUNT : INT := 100;
    STATION_1.P1.TIME1 : TON := (PT := T#2.5s);
    STATION_2.P1.TIME1 : TON := (PT := T#4.5s);
    STATION_2.P4.FB1.C2 AT %QB25 : BYTE;
  END_VAR
END_CONFIGURATION
"#,
    );
    let LibraryElement::Configuration(c) = &u.elements[0] else { panic!() };
    assert_eq!(c.resources.len(), 2);
    assert_eq!(c.resources[0].programs.len(), 2);
    assert_eq!(c.var_blocks.len(), 3);
}

#[test]
fn literals_and_initialisation() {
    parse_ok(
        r#"
PROGRAM L
VAR
  a : DINT := DINT#5; b : UINT := UINT#16#9AF; c : BOOL := BOOL#TRUE; d : BOOL := BOOL#0;
  e : TIME := T#25h_15m; f : TIME := TIME#-14ms; g : DATE := D#1984-06-25;
  h : TOD := TOD#15:36:55.36; i : DT := DT#1984-06-25-15:36:55.36;
  s : STRING[10] := 'OK'; w : WSTRING := "$00C4$00CB"; ws : WSTRING := WSTRING#'OK';
  BITS : ARRAY[0..7] OF BOOL := [1,1,0,0,0,1,0,0];
  TBT : ARRAY [1..2,1..3] OF INT := [1,2,3(4),6];
  MODULE_8_CONFIG: ANALOG_16_INPUT_CONFIGURATION :=
     (SIGNAL_TYPE := DIFFERENTIAL,
      CHANNEL := [4((RANGE := UNIPOLAR_1_5V)),
                  (RANGE:= BIPOLAR_10_V, MIN_SCALE := 0, MAX_SCALE := 500)]);
  TempLoop : PID := (PropBand := 2.5, Integral := T#5s);
  r : LREAL := -1.34E-12;
END_VAR
VAR CONSTANT PI : REAL := 3.141592 ; END_VAR
VAR AT %QX5.1 : BOOL :=1; AT %MW6 : INT := 8 ; END_VAR
VAR RETAIN AT %QW5 : WORD := 16#FF00 ; END_VAR
VAR VALVE_POS AT %QW28 : INT := 100; C2 AT %Q* : BYTE; END_VAR
VAR OUTARY AT %QW6 : ARRAY[0..9] OF INT := [10(1)]; END_VAR
a := a + 1;
END_PROGRAM
"#,
    );
}
