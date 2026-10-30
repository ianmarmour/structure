//! Examples of annex F (textual forms).
mod common;
use common::*;
use structure_runtime::Value;

const CMD_MONITOR: &str = r#"
FUNCTION_BLOCK CMD_MONITOR
VAR_INPUT AUTO_CMD : BOOL ; (* Automated command *)
  AUTO_MODE : BOOL ; (* AUTO_CMD enable *)
  MAN_CMD : BOOL ; (* Manual Command *)
  MAN_CMD_CHK : BOOL ; (* Negated MAN_CMD to debounce *)
  T_CMD_MAX : TIME ; (* Max time from CMD to FDBK *)
  FDBK : BOOL ; (* Confirmation of CMD completion by operative unit *)
  ACK : BOOL ; (* Acknowledge/cancel ALRM *)
END_VAR
VAR_OUTPUT CMD : BOOL ; (* Command to operative unit *)
  ALRM : BOOL ; (* T_CMD_MAX expired without FDBK *)
END_VAR
VAR CMD_TMR : TON ; (* CMD-to-FDBK timer *)
  ALRM_FF : SR ; (* Note over-riding S input: *)
END_VAR (* Command must be cancelled before "ACK" can cancel alarm *)
CMD := AUTO_CMD & AUTO_MODE
  OR MAN_CMD & NOT MAN_CMD_CHK & NOT AUTO_MODE ;
CMD_TMR (IN := CMD, PT := T_CMD_MAX);
ALRM_FF (S1 := CMD_TMR.Q & NOT FDBK, R := ACK);
ALRM := ALRM_FF.Q1;
END_FUNCTION_BLOCK
"#;

const FWD_REV_MON_DECL: &str = r#"
FUNCTION_BLOCK FWD_REV_MON
VAR_INPUT AUTO : BOOL ;(* Enable automated commands *)
  ACK : BOOL ; (* Acknowledge/cancel all alarms *)
  AUTO_FWD : BOOL ; (* Automated forward command *)
  MAN_FWD : BOOL ; (* Manual forward command *)
  MAN_FWD_CHK : BOOL ; (* Negated MAN_FWD for debouncing *)
  T_FWD_MAX : TIME ; (* Maximum time from FWD_CMD to FWD_FDBK *)
  FWD_FDBK : BOOL ; (* Confirmation of FWD_CMD completion *)
  AUTO_REV : BOOL ; (* Automated reverse command *)
  MAN_REV : BOOL ; (* Manual reverse command *)
  MAN_REV_CHK : BOOL ; (* Negated MAN_REV for debouncing *)
  T_REV_MAX : TIME ; (* Maximum time from REV_CMD to REV_FDBK *)
  REV_FDBK : BOOL ; (* Confirmation of REV_CMD completion *)
END_VAR
VAR_OUTPUT KLAXON : BOOL ; (* Any alarm active *)
  FWD_REV_ALRM : BOOL; (* Forward/reverse command conflict *)
  FWD_CMD : BOOL ; (* "Forward" command to operative unit *)
  FWD_ALRM : BOOL ; (* T_FWD_MAX expired without FWD_FDBK *)
  REV_CMD : BOOL ; (* "Reverse" command to operative unit *)
  REV_ALRM : BOOL ; (* T_REV_MAX expired without REV_FDBK *)
END_VAR
VAR FWD_MON : CMD_MONITOR; (* "Forward" command monitor *)
  REV_MON : CMD_MONITOR; (* "Reverse" command monitor *)
  FWD_REV_FF : SR ; (* Forward/Reverse contention latch *)
END_VAR
"#;

const FWD_REV_MON_ST: &str = r#"
(* Evaluate internal function blocks *)
FWD_MON (AUTO_MODE := AUTO, ACK := ACK, AUTO_CMD := AUTO_FWD, MAN_CMD := MAN_FWD,
         MAN_CMD_CHK := MAN_FWD_CHK, T_CMD_MAX := T_FWD_MAX, FDBK := FWD_FDBK);
REV_MON (AUTO_MODE := AUTO, ACK := ACK, AUTO_CMD := AUTO_REV, MAN_CMD := MAN_REV,
         MAN_CMD_CHK := MAN_REV_CHK, T_CMD_MAX := T_REV_MAX, FDBK := REV_FDBK);
FWD_REV_FF (S1 := FWD_MON.CMD & REV_MON.CMD, R := ACK);
(* Transfer data to outputs *)
FWD_REV_ALRM := FWD_REV_FF.Q1;
FWD_CMD := FWD_MON.CMD & NOT FWD_REV_ALRM;
FWD_ALRM := FWD_MON.ALRM;
REV_CMD := REV_MON.CMD & NOT FWD_REV_ALRM;
REV_ALRM := REV_MON.ALRM;
KLAXON := FWD_ALRM OR REV_ALRM OR FWD_REV_ALRM;
END_FUNCTION_BLOCK
"#;

// The IL body of F.3 (with Q1 instead of the misprinted Q output of SR).
const FWD_REV_MON_IL: &str = r#"
(* Evaluate internal function blocks *)
    CAL FWD_MON(
      AUTO_MODE:= AUTO,
      ACK:= ACK,
      AUTO_CMD:= AUTO_FWD,
      MAN_CMD:= MAN_FWD,
      MAN_CMD_CHK:= MAN_FWD_CHK,
      T_CMD_MAX:= T_FWD_MAX,
      FDBK:= FWD_FDBK
    )
    CAL REV_MON(
      AUTO_MODE:= AUTO,
      ACK:= ACK,
      AUTO_CMD:= AUTO_REV,
      MAN_CMD:= MAN_REV,
      MAN_CMD_CHK:= MAN_REV_CHK,
      T_CMD_MAX:= T_REV_MAX,
      FDBK:= REV_FDBK
    )
    CAL FWD_REV_FF(
      S1:=(
        LD FWD_MON.CMD
        AND REV_MON.CMD
      ),
      R:= ACK,
      Q1 => FWD_REV_ALRM (* Contention alarm *)
    )
(* Transfer data to outputs *)
    LD   FWD_MON.CMD (* "Forward" command and alarm *)
    ANDN FWD_REV_ALRM
    ST   FWD_CMD
    LD   FWD_MON.ALRM
    ST   FWD_ALRM
    LD   REV_MON.CMD (* "Reverse" command and alarm *)
    ANDN FWD_REV_ALRM
    ST   REV_CMD
    LD   REV_MON.ALRM
    ST   REV_ALRM
    OR   FWD_ALRM (* OR all alarms *)
    OR   FWD_REV_ALRM
    ST   KLAXON
END_FUNCTION_BLOCK
"#;

const TEST_PROGRAM: &str = r#"
PROGRAM P
VAR M : FWD_REV_MON; AF, AR, ACK, KL, FC, RC, CONFLICT : BOOL; END_VAR
M(AUTO := TRUE, ACK := ACK, AUTO_FWD := AF, MAN_FWD := FALSE, MAN_FWD_CHK := FALSE, T_FWD_MAX := T#100ms, FWD_FDBK := FALSE,
  AUTO_REV := AR, MAN_REV := FALSE, MAN_REV_CHK := FALSE, T_REV_MAX := T#100ms, REV_FDBK := FALSE);
KL := M.KLAXON; FC := M.FWD_CMD; RC := M.REV_CMD; CONFLICT := M.FWD_REV_ALRM;
END_PROGRAM
CONFIGURATION C RESOURCE R ON VM TASK T(INTERVAL := T#10ms, PRIORITY := 1); PROGRAM P1 WITH T : P; END_RESOURCE END_CONFIGURATION
"#;

fn fwd_rev(body: &str) {
    let src = format!("{}{}{}{}", CMD_MONITOR, FWD_REV_MON_DECL, body, TEST_PROGRAM);
    let mut rt = build(&src);
    rt.write("R.P1.AF", &Value::Bool(true)).unwrap();
    rt.run_for(20_000);
    assert!(boolean(&rt, "R.P1.FC"));
    assert!(!boolean(&rt, "R.P1.KL"));
    // timeout without feedback raises the alarm
    rt.run_for(120_000);
    assert!(boolean(&rt, "R.P1.KL"));
    // contention
    rt.write("R.P1.AR", &Value::Bool(true)).unwrap();
    rt.run_for(20_000);
    assert!(boolean(&rt, "R.P1.CONFLICT"));
    assert!(!boolean(&rt, "R.P1.FC") && !boolean(&rt, "R.P1.RC"));
    rt.write("R.P1.AF", &Value::Bool(false)).unwrap();
    rt.write("R.P1.AR", &Value::Bool(false)).unwrap();
    rt.write("R.P1.ACK", &Value::Bool(true)).unwrap();
    rt.run_for(20_000);
    assert!(!boolean(&rt, "R.P1.KL"));
    assert!(rt.errors.is_empty(), "{:?}", rt.errors);
}

#[test]
fn f3_fwd_rev_mon_st() {
    fwd_rev(FWD_REV_MON_ST);
}

#[test]
fn f3_fwd_rev_mon_il() {
    fwd_rev(FWD_REV_MON_IL);
}

const ANALOG: &str = r#"
FUNCTION_BLOCK LAG1
VAR_INPUT RUN : BOOL ; XIN : REAL ; TAU : TIME ; CYCLE : TIME ; END_VAR
VAR_OUTPUT XOUT : REAL ; END_VAR
VAR K : REAL ; END_VAR
IF RUN THEN XOUT := XOUT + K * (XIN - XOUT) ;
ELSE XOUT := XIN ;
  K := TIME_TO_REAL(CYCLE) / TIME_TO_REAL(CYCLE + TAU) ;
END_IF ;
END_FUNCTION_BLOCK

FUNCTION_BLOCK DELAY (* N-sample delay *)
VAR_INPUT RUN : BOOL ; XIN : REAL ; N : INT ; END_VAR
VAR_OUTPUT XOUT : REAL; END_VAR
VAR X : ARRAY [0..127] OF REAL; I, IXIN, IXOUT : INT := 0; END_VAR
IF RUN THEN IXIN := MOD(IXIN + 1, 128) ; X[IXIN] := XIN ;
  IXOUT := MOD(IXOUT + 1, 128) ; XOUT := X[IXOUT];
ELSE XOUT := XIN ; IXIN := N ; IXOUT := 0;
  FOR I := 0 TO N DO X[I] := XIN; END_FOR;
END_IF ;
END_FUNCTION_BLOCK

FUNCTION_BLOCK AVERAGE
VAR_INPUT RUN : BOOL ; XIN : REAL ; N : INT ; END_VAR
VAR_OUTPUT XOUT : REAL ; END_VAR
VAR SUM : REAL := 0.0; FIFO : DELAY ; END_VAR
SUM := SUM - FIFO.XOUT ;
FIFO (RUN := RUN , XIN := XIN, N := N) ;
SUM := SUM + FIFO.XOUT ;
IF RUN THEN XOUT := SUM/N ;
ELSE SUM := N*XIN ; XOUT := XIN ;
END_IF ;
END_FUNCTION_BLOCK

FUNCTION_BLOCK INTEGRAL
VAR_INPUT RUN : BOOL ; R1 : BOOL ; XIN : REAL ; X0 : REAL ; CYCLE : TIME ; END_VAR
VAR_OUTPUT Q : BOOL ; XOUT : REAL ; END_VAR
Q := NOT R1 ;
IF R1 THEN XOUT := X0 ;
ELSIF RUN THEN XOUT := XOUT + XIN * TIME_TO_REAL(CYCLE);
END_IF ;
END_FUNCTION_BLOCK

FUNCTION_BLOCK DERIVATIVE
VAR_INPUT RUN : BOOL ; XIN : REAL ; CYCLE : TIME ; END_VAR
VAR_OUTPUT XOUT : REAL ; END_VAR
VAR X1, X2, X3 : REAL ; END_VAR
IF RUN THEN
  XOUT := (3.0 * (XIN - X3) + X1 - X2) / (10.0 * TIME_TO_REAL(CYCLE)) ;
  X3 := X2 ; X2 := X1 ; X1 := XIN ;
ELSE XOUT := 0.0; X1 := XIN ; X2 := XIN ; X3 := XIN ;
END_IF ;
END_FUNCTION_BLOCK

FUNCTION_BLOCK HYSTERESIS
VAR_INPUT XIN1, XIN2, EPS : REAL; END_VAR
VAR_OUTPUT Q : BOOL := 0; END_VAR
IF Q THEN IF XIN1 < (XIN2 - EPS) THEN Q := 0; END_IF ;
ELSIF XIN1 > (XIN2 + EPS) THEN Q := 1 ;
END_IF ;
END_FUNCTION_BLOCK

FUNCTION_BLOCK PID
VAR_INPUT AUTO : BOOL ; PV : REAL ; SP : REAL ; X0 : REAL ; KP : REAL ; TR : REAL ; TD : REAL ; CYCLE : TIME ; END_VAR
VAR_OUTPUT XOUT : REAL; END_VAR
VAR ERROR : REAL ; ITERM : INTEGRAL ; DTERM : DERIVATIVE ; END_VAR
ERROR := PV - SP ;
(*** Adjust ITERM so that XOUT := X0 when AUTO = 0 ***)
ITERM (RUN := AUTO, R1 := NOT AUTO, XIN := ERROR, X0 := TR * (X0 - ERROR), CYCLE := CYCLE) ;
DTERM (RUN := AUTO, XIN := ERROR, CYCLE := CYCLE) ;
XOUT := KP * (ERROR + ITERM.XOUT/TR + DTERM.XOUT*TD) ;
END_FUNCTION_BLOCK

FUNCTION_BLOCK DIFFEQ
VAR_INPUT RUN : BOOL ; XIN : REAL ; A : ARRAY[1..127] OF REAL ; M : INT ; B : ARRAY[0..127] OF REAL ; N : INT ; END_VAR
VAR_OUTPUT XOUT : REAL := 0.0 ; END_VAR
VAR XI : ARRAY [0..127] OF REAL ; XO : ARRAY [0..127] OF REAL ; I : INT ; END_VAR
XO[0] := XOUT ; XI[0] := XIN ;
XOUT := B[0] * XIN ;
IF RUN THEN
  FOR I := M TO 1 BY -1 DO XOUT := XOUT + A[I] * XO[I] ; XO[I] := XO[I-1]; END_FOR;
  FOR I := N TO 1 BY -1 DO XOUT := XOUT + B[I] * XI[I] ; XI[I] := XI[I-1]; END_FOR;
ELSE
  FOR I := 1 TO M DO XO[I] := 0.0; END_FOR;
  FOR I := 1 TO N DO XI[I] := 0.0; END_FOR;
END_IF ;
END_FUNCTION_BLOCK

FUNCTION_BLOCK RAMP
VAR_INPUT RUN : BOOL ; X0,X1 : REAL ; TR : TIME ; CYCLE : TIME ; END_VAR
VAR_OUTPUT BUSY : BOOL ; XOUT : REAL := 0.0 ; END_VAR
VAR XI : REAL ; T : TIME := T#0s; END_VAR
BUSY := RUN ;
IF RUN THEN
  IF T >= TR THEN BUSY := 0 ; XOUT := X1 ;
  ELSE XOUT := XI + (X1-XI) * TIME_TO_REAL(T) / TIME_TO_REAL(TR) ;
    T := T + CYCLE ;
  END_IF ;
ELSE XOUT := X0 ; XI := X0 ; T := t#0s ;
END_IF ;
END_FUNCTION_BLOCK

FUNCTION_BLOCK TRANSFER
VAR_INPUT AUTO : BOOL ; XIN : REAL ; FAST_RATE, SLOW_RATE : REAL ;
  FAST_UP, SLOW_UP, FAST_DOWN, SLOW_DOWN : BOOL; CYCLE : TIME ; END_VAR
VAR_OUTPUT XOUT : REAL ; END_VAR
VAR XFER_RAMP : INTEGRAL ; RAMP_RATE : REAL ; END_VAR
RAMP_RATE := 0.0 ;
IF NOT AUTO THEN
  IF FAST_UP THEN RAMP_RATE := FAST_RATE; END_IF;
  IF SLOW_UP THEN RAMP_RATE := RAMP_RATE + SLOW_RATE; END_IF;
  IF FAST_DOWN THEN RAMP_RATE := RAMP_RATE - FAST_RATE; END_IF;
  IF SLOW_DOWN THEN RAMP_RATE := RAMP_RATE - SLOW_RATE; END_IF;
END_IF ;
XFER_RAMP (RUN := 1, CYCLE := CYCLE, R1 := AUTO, XIN := RAMP_RATE, X0 := XIN) ;
XOUT := XFER_RAMP.XOUT;
END_FUNCTION_BLOCK

TYPE ANALOG_LIMITS :
STRUCT
  HS : REAL ; HM : REAL ; HA : REAL ; HW : REAL ; NV : REAL ;
  EPS : REAL ; LW : REAL ; LA : REAL ; LM : REAL ; LS : REAL ;
END_STRUCT ;
END_TYPE
"#;

#[test]
fn f6_analog_signal_processing() {
    let src = format!(
        r#"{ANALOG}
PROGRAM P
VAR LAG : LAG1; AVG : AVERAGE; R : RAMP; C : PID; H : HYSTERESIS; D : DIFFEQ; TR : TRANSFER;
    RUN : BOOL; X, Y, Z, W : REAL; HQ : BOOL; DOUT, TOUT : REAL; LIMITS : ANALOG_LIMITS;
    ACOEF : ARRAY[1..127] OF REAL; BCOEF : ARRAY[0..127] OF REAL := [1.0]; END_VAR
LAG(RUN := RUN, XIN := 100.0, TAU := T#90ms, CYCLE := T#10ms); X := LAG.XOUT;
AVG(RUN := RUN, XIN := 10.0, N := 4); Y := AVG.XOUT;
R(RUN := RUN, X0 := 0.0, X1 := 50.0, TR := T#100ms, CYCLE := T#10ms); Z := R.XOUT;
C(AUTO := RUN, PV := 1.0, SP := 0.0, X0 := 0.0, KP := 2.0, TR := 1.0, TD := 0.0, CYCLE := T#10ms); W := C.XOUT;
H(XIN1 := Z, XIN2 := 20.0, EPS := 1.0); HQ := H.Q;
D(RUN := RUN, XIN := 2.0, A := ACOEF, M := 0, B := BCOEF, N := 0); DOUT := D.XOUT;
TR(AUTO := FALSE, XIN := 5.0, FAST_RATE := 1000.0, SLOW_RATE := 1.0, FAST_UP := RUN, CYCLE := T#10ms); TOUT := TR.XOUT;
END_PROGRAM
CONFIGURATION C RESOURCE R ON VM TASK T(INTERVAL := T#10ms, PRIORITY := 1); PROGRAM P1 WITH T : P; END_RESOURCE END_CONFIGURATION
"#
    );
    let (sm, out) = structure_compiler::compile_str("f6.st", &src, structure_compiler::Mode::Build);
    let warnings = out.diagnostics.render(&sm);
    let mut rt = structure_runtime::Runtime::new(out.image.expect(&warnings), structure_runtime::Options::default());
    rt.cold_start().unwrap();
    // AVERAGE uses SUM/N (REAL/INT): an implicit conversion (extension)
    assert!(warnings.contains("implicit conversion from INT to REAL"), "{}", warnings);
    rt.run_for(10_000);
    assert_eq!(real(&rt, "R.P1.X"), 100.0);
    rt.write("R.P1.RUN", &Value::Bool(true)).unwrap();
    rt.run_for(200_000);
    assert!((real(&rt, "R.P1.Y") - 10.0).abs() < 1e-4);
    assert_eq!(real(&rt, "R.P1.Z"), 50.0);
    assert!(boolean(&rt, "R.P1.HQ"));
    assert_eq!(real(&rt, "R.P1.DOUT"), 2.0);
    assert!(real(&rt, "R.P1.W") > 2.0, "integral action grows the output");
    assert!(real(&rt, "R.P1.TOUT") > 5.0);
    assert!(rt.errors.is_empty(), "{:?}", rt.errors);
}

#[test]
fn f7_gravel_textual_sfc_and_configuration() {
    let src = r#"
PROGRAM GRAVEL (* Gravel measurement and loading system *)
VAR_INPUT
  OFF_PB : BOOL ; ON_PB : BOOL ; FILL_PB : BOOL ; SIREN_ACK : BOOL ;
  LOAD_PB : BOOL ; (* Load truck from bin *)
  JOG_PB : BOOL ; LAMP_TEST : BOOL ;
  TRUCK_ON_RAMP : BOOL ; (* Optical sensor *)
  SILO_EMPTY_LS : BOOL ; BIN_EMPTY_LS : BOOL ;
  SETPOINT : BYTE ; (* 2-digit BCD *)
END_VAR
VAR_OUTPUT
  CONTROL_LAMP : BOOL ; TRUCK_LAMP : BOOL ; SILO_EMPTY_LAMP : BOOL ; CONVEYOR_LAMP : BOOL ;
  CONVEYOR_MOTOR : BOOL ; SILO_VALVE : BOOL ; BIN_VALVE : BOOL ; SIREN : BOOL ;
  BIN_LEVEL : BYTE ;
END_VAR
VAR
  BLINK_TIME : TIME := T#200ms; (* BLINK ON/OFF time *)
  PULSE_TIME : TIME := T#50ms; (* LEVEL_CTR increment interval *)
  RUNOUT_TIME: TIME := T#300ms; (* Conveyor running time after loading *)
  RUN_IN_TIME: TIME := T#200ms; (* Conveyor running time before loading *)
  SILENT_TIME: TIME := T#1s; (* Siren silent time after SIREN_ACK *)
  OK_TO_RUN : BOOL; (* 1 = Conveyor is allowed to run *)
  BLINK: TON; BLANK: TON; PULSE: TON; SIREN_FF: RS; SILENCE_TMR: TP;
END_VAR
VAR RETAIN LEVEL_CTR : CTU ; END_VAR
(* Major operating states *)
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
TRANSITION FROM RUN_IN TO LOAD_WAIT := NOT OK_TO_RUN ;
END_TRANSITION
TRANSITION FROM RUN_IN TO DUMP_BIN
  := RUN_IN.T > RUN_IN_TIME;
END_TRANSITION
STEP DUMP_BIN: BIN_VALVE(N); END_STEP
TRANSITION FROM DUMP_BIN TO LOAD_WAIT := NOT OK_TO_RUN ;
END_TRANSITION
TRANSITION FROM DUMP_BIN TO RUNOUT := BIN_EMPTY_LS ;
END_TRANSITION
STEP RUNOUT : END_STEP
TRANSITION FROM RUNOUT TO LOAD_WAIT := NOT OK_TO_RUN ;
END_TRANSITION
TRANSITION FROM RUNOUT TO START
  := RUNOUT.T >= RUNOUT_TIME ; END_TRANSITION
(* Control state sequencing *)
INITIAL_STEP CONTROL_OFF: END_STEP
TRANSITION FROM CONTROL_OFF TO CONTROL
  := ON_PB & NOT OFF_PB ; END_TRANSITION
STEP CONTROL: CONTROL_ACTION(N); END_STEP
ACTION CONTROL_ACTION:
  BLINK(EN:=CONTROL.X & NOT BLANK.Q, IN := TRUE, PT := BLINK_TIME) ;
  BLANK(EN:=BLINK.Q, IN := TRUE, PT := BLINK_TIME) ;
  OK_TO_RUN := CONTROL.X & TRUCK_ON_RAMP ;
  CONVEYOR_MOTOR :=
    OK_TO_RUN & OR(JOG_PB, RUN_IN.X, DUMP_BIN.X, RUNOUT.X);
END_ACTION
TRANSITION FROM CONTROL TO CONTROL_OFF := OFF_PB ;
END_TRANSITION
(* Monitor Logic *)
INITIAL_STEP MONITOR: MONITOR_ACTION(N); END_STEP
ACTION MONITOR_ACTION:
  CONVEYOR_LAMP := CONVEYOR_MOTOR & BLINK.Q ;
  CONTROL_LAMP := CONTROL.X OR LAMP_TEST ;
  TRUCK_LAMP := TRUCK_ON_RAMP OR LAMP_TEST ;
  SILO_EMPTY_LAMP := BLINK.Q & SILO_EMPTY_LS OR LAMP_TEST ;
  SILENCE_TMR(IN:=SIREN_ACK, PT:=SILENT_TIME) ;
  SIREN_FF(S:=SILO_EMPTY_LS, R1:=SILENCE_TMR.Q) ;
  SIREN := SIREN_FF.Q1 ;
  PULSE(IN:=FILL_BIN.X & NOT PULSE.Q, PT:=PULSE_TIME) ;
  LEVEL_CTR(R := BIN_EMPTY_LS, CU := PULSE.Q,
    PV := BYTE_BCD_TO_INT(SETPOINT)) ;
  BIN_LEVEL := INT_TO_BCD_BYTE(LEVEL_CTR.CV) ;
END_ACTION
END_PROGRAM

CONFIGURATION GRAVEL_CONTROL
  RESOURCE PROC1 ON PROC_TYPE_Y
    TASK SCAN(INTERVAL := T#10ms, PRIORITY := 1);
    PROGRAM G WITH SCAN : GRAVEL
    (* Inputs *)
    (OFF_PB := %I0.0 ,
     ON_PB := %I0.1 ,
     FILL_PB := %I0.2 ,
     SIREN_ACK := %I0.3 ,
     LOAD_PB := %I0.4 ,
     JOG_PB := %I0.5 ,
     LAMP_TEST := %I0.7 ,
     TRUCK_ON_RAMP := %I1.4 ,
     SILO_EMPTY_LS := %I1.5 ,
     BIN_EMPTY_LS := %I1.6 ,
     SETPOINT := %IB2 ,
     (* Outputs *)
     CONTROL_LAMP => %Q4.0,
     TRUCK_LAMP => %Q4.2,
     SILO_EMPTY_LAMP => %Q4.3,
     CONVEYOR_LAMP => %Q5.3,
     CONVEYOR_MOTOR => %Q5.4,
     SILO_VALVE => %Q5.5,
     BIN_VALVE => %Q5.6,
     SIREN => %Q5.7,
     BIN_LEVEL => %QB6) ;
  END_RESOURCE
END_CONFIGURATION
"#;
    let mut rt = build(src);
    let bit = |rt: &mut structure_runtime::Runtime, byte: usize, bit: u8, v: bool| {
        let i = rt.io(0, 'I');
        if v {
            i[byte] |= 1 << bit;
        } else {
            i[byte] &= !(1 << bit);
        }
    };
    let q = |rt: &mut structure_runtime::Runtime, byte: usize, bit: u8| rt.io(0, 'Q')[byte] & (1 << bit) != 0;
    rt.io(0, 'I')[2] = 0x03; // setpoint 3 units (BCD)
    bit(&mut rt, 0, 1, true); // ON
    rt.run_for(50_000);
    bit(&mut rt, 0, 1, false);
    assert!(q(&mut rt, 4, 0), "control lamp");
    bit(&mut rt, 0, 2, true); // FILL
    rt.run_for(50_000);
    assert!(q(&mut rt, 5, 5), "silo valve open while filling");
    rt.run_for(300_000);
    assert_eq!(rt.io(0, 'Q')[6], 0x03, "bin level reached the set point");
    // directly represented variables by name
    assert_eq!(rt.read("PROC1.%QB6").unwrap().to_string(), "3");
    assert_eq!(rt.read("%IB2").unwrap().to_string(), "3");
    assert_eq!(rt.read("%QX4.0").unwrap(), Value::Bool(true));
    assert!(rt.read("%QW999").is_err());
    assert!(!q(&mut rt, 5, 5), "silo valve closed");
    assert!(boolean(&rt, "PROC1.G.LOAD_WAIT.X"));
    // truck on ramp, load
    bit(&mut rt, 0, 2, false);
    bit(&mut rt, 1, 4, true);
    bit(&mut rt, 0, 4, true);
    rt.run_for(50_000);
    assert!(q(&mut rt, 5, 4), "conveyor running in RUN_IN");
    rt.run_for(250_000);
    assert!(q(&mut rt, 5, 6), "bin valve open while dumping");
    bit(&mut rt, 1, 6, true); // bin empty
    rt.run_for(50_000);
    assert!(boolean(&rt, "PROC1.G.RUNOUT.X"));
    rt.run_for(400_000);
    assert!(boolean(&rt, "PROC1.G.START.X"));
    assert!(rt.errors.is_empty(), "{:?}", rt.errors);
}

#[test]
fn f11_function_with_outputs() {
    let src = r#"
FUNCTION ALRM_INT : BOOL
VAR_INPUT IN : INT ; THI : INT ; (* High threshold *) TLO : INT ; (* Low threshold *) END_VAR
VAR_OUTPUT HI: BOOL; (* High level alarm *) LO: BOOL; (* Low level alarm *) END_VAR
HI := IN > THI ;
LO := IN < TLO ;
ALRM_INT := HI OR LO ;
END_FUNCTION
PROGRAM P
VAR A, H, L : BOOL; B : BOOL; END_VAR
A := ALRM_INT(IN := 120, THI := 100, TLO := 0, HI => H, LO => L);
B := ALRM_INT(50, 100, 0);
END_PROGRAM
CONFIGURATION C RESOURCE R ON VM TASK T(INTERVAL := T#10ms, PRIORITY := 1); PROGRAM P1 WITH T : P; END_RESOURCE END_CONFIGURATION
"#;
    let mut rt = build(src);
    rt.run_for(10_000);
    assert!(boolean(&rt, "R.P1.A") && boolean(&rt, "R.P1.H") && !boolean(&rt, "R.P1.L"));
    assert!(!boolean(&rt, "R.P1.B"));
}

#[test]
fn f4_stack_int_st() {
    let src = r#"
FUNCTION_BLOCK STACK_INT
VAR_INPUT PUSH, POP: BOOL R_EDGE; (* Basic stack operations *)
  R1 : BOOL ; (* Over-riding reset *)
  IN : INT ; (* Input to be pushed *)
  N : INT ; (* Maximum depth after reset *)
END_VAR
VAR_OUTPUT EMPTY : BOOL := 1 ; (* Stack empty *)
  OFLO : BOOL := 0 ; (* Stack overflow *)
  OUT : INT := 0 ; (* Top of stack data *)
END_VAR
VAR STK : ARRAY[0..127] OF INT; (* Internal stack *)
  NI : INT :=128 ; (* Storage for N upon reset *)
  PTR : INT := -1 ; (* Stack pointer *)
END_VAR
IF R1 THEN
  OFLO := 0; EMPTY := 1; PTR := -1;
  NI := LIMIT (MN:=1,IN:=N,MX:=128); OUT := 0;
ELSIF POP & NOT EMPTY THEN
  OFLO := 0; PTR := PTR-1; EMPTY := PTR < 0;
  IF EMPTY THEN OUT := 0;
  ELSE OUT := STK[PTR];
  END_IF ;
ELSIF PUSH & NOT OFLO THEN
  EMPTY := 0; PTR := PTR+1; OFLO := (PTR = NI);
  IF NOT OFLO THEN OUT := IN ; STK[PTR] := IN;
  ELSE OUT := 0;
  END_IF ;
END_IF ;
END_FUNCTION_BLOCK
PROGRAM P
VAR S : STACK_INT; PUSH, POP, R1 : BOOL; IN : INT; OUT : INT; EMPTY, OFLO : BOOL; END_VAR
S(PUSH := PUSH, POP := POP, R1 := R1, IN := IN, N := 2);
OUT := S.OUT; EMPTY := S.EMPTY; OFLO := S.OFLO;
END_PROGRAM
CONFIGURATION C RESOURCE R ON VM TASK T(INTERVAL := T#10ms, PRIORITY := 1); PROGRAM P1 WITH T : P; END_RESOURCE END_CONFIGURATION
"#;
    let mut rt = build(src);
    let set = |rt: &mut structure_runtime::Runtime, k: &str, v: Value| rt.write(&format!("R.P1.{}", k), &v).unwrap();
    set(&mut rt, "R1", Value::Bool(true));
    rt.run_for(10_000);
    set(&mut rt, "R1", Value::Bool(false));
    for v in [10, 20, 30] {
        set(&mut rt, "IN", Value::Int(v));
        set(&mut rt, "PUSH", Value::Bool(true));
        rt.run_for(10_000);
        set(&mut rt, "PUSH", Value::Bool(false));
        rt.run_for(10_000);
    }
    // N = 2: the third push overflows (the pointer moves past the top)
    assert!(boolean(&rt, "R.P1.OFLO"));
    assert_eq!(int(&rt, "R.P1.OUT"), 0);
    for expected in [20, 10] {
        set(&mut rt, "POP", Value::Bool(true));
        rt.run_for(10_000);
        assert_eq!(int(&rt, "R.P1.OUT"), expected);
        assert!(!boolean(&rt, "R.P1.OFLO") && !boolean(&rt, "R.P1.EMPTY"));
        set(&mut rt, "POP", Value::Bool(false));
        rt.run_for(10_000);
    }
    set(&mut rt, "POP", Value::Bool(true));
    rt.run_for(10_000);
    assert!(boolean(&rt, "R.P1.EMPTY"));
    assert_eq!(int(&rt, "R.P1.OUT"), 0);
}

#[test]
fn f9_enumerated_types_in_st_and_il() {
    let src = r#"
TYPE SPEED: (SLOW, MEDIUM, FAST, VERY_FAST); END_TYPE
FUNCTION_BLOCK GOVERNOR
VAR_INPUT MOTOR_SPEED: SPEED; END_VAR
VAR_OUTPUT SPEED_OUT: SPEED; CHANGE : INT; END_VAR
CASE MOTOR_SPEED OF
SLOW: CHANGE := 1; (* speed it up *)
MEDIUM: CHANGE := 0; (* hold the current speed *)
FAST: CHANGE := -1; (* slow it down *)
ELSE CHANGE := -2; (* take special care *)
END_CASE;
SPEED_OUT := MOTOR_SPEED;
END_FUNCTION_BLOCK
FUNCTION_BLOCK RESET_SPEED
VAR_OUTPUT SPEED_OUT: SPEED; END_VAR
LD SPEED#SLOW (* enumerated value qualified by data type *)
ST SPEED_OUT
END_FUNCTION_BLOCK
PROGRAM P
VAR G : GOVERNOR; RSP : RESET_SPEED; A1, A2 : INT; S : SPEED := VERY_FAST; END_VAR
G(MOTOR_SPEED := FAST); A1 := G.CHANGE;
G(MOTOR_SPEED := S); A2 := G.CHANGE;
RSP(); S := RSP.SPEED_OUT;
END_PROGRAM
CONFIGURATION C RESOURCE R ON VM TASK T(INTERVAL := T#10ms, PRIORITY := 1); PROGRAM P1 WITH T : P; END_RESOURCE END_CONFIGURATION
"#;
    let mut rt = build(src);
    rt.run_for(5_000);
    assert_eq!(int(&rt, "R.P1.A1"), -1);
    assert_eq!(int(&rt, "R.P1.A2"), -2);
    assert_eq!(rt.read("R.P1.S").unwrap().to_string(), "SLOW");
}

#[test]
fn f10_real_time_clock() {
    let src = r#"
PROGRAM P
VAR CLOCK : RTC; RUN : BOOL; NOW : DT; VALID : BOOL; END_VAR
CLOCK(IN := RUN, PDT := DT#2003-01-15-08:00:00);
NOW := CLOCK.CDT; VALID := CLOCK.Q;
END_PROGRAM
CONFIGURATION C RESOURCE R ON VM TASK T(INTERVAL := T#10ms, PRIORITY := 1); PROGRAM P1 WITH T : P; END_RESOURCE END_CONFIGURATION
"#;
    let (sm, out) = structure_compiler::compile_str("rtc.st", src, structure_compiler::Mode::Build);
    assert!(out.diagnostics.render(&sm).contains("RTC"), "RTC use is reported as an extension");
    let mut rt = structure_runtime::Runtime::new(out.image.unwrap(), structure_runtime::Options::default());
    rt.cold_start().unwrap();
    rt.run_for(5_000);
    assert!(!boolean(&rt, "R.P1.VALID"));
    rt.write("R.P1.RUN", &Value::Bool(true)).unwrap();
    rt.run_for(10_000);
    assert_eq!(rt.read("R.P1.NOW").unwrap().to_string(), "DT#2003-01-15-08:00:00");
    rt.run_for(1_000_000);
    let now = rt.read("R.P1.NOW").unwrap().to_string();
    assert!(now.starts_with("DT#2003-01-15-08:00:01"), "{}", now);
}
