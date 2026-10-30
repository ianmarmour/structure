mod common;
use common::*;
use structure_runtime::Value;

const CFG: &str = "CONFIGURATION C RESOURCE R ON VM TASK T(INTERVAL := T#10ms, PRIORITY := 1); PROGRAM P1 WITH T : P; END_RESOURCE END_CONFIGURATION";

fn set(rt: &mut structure_runtime::Runtime, k: &str, v: bool) {
    rt.write(&format!("R.P1.{}", k), &Value::Bool(v)).unwrap();
}

#[test]
fn simple_sequence_and_step_flags() {
    let src = format!(r#"
PROGRAM P
VAR GO, BACK : BOOL; LAMP : BOOL; CNT : INT; ELAPSED : TIME; END_VAR
INITIAL_STEP IDLE : END_STEP
TRANSITION FROM IDLE TO RUN := GO; END_TRANSITION
STEP RUN : LAMP(N); COUNT(N); END_STEP
ACTION COUNT : CNT := CNT + 1; ELAPSED := RUN.T; END_ACTION
TRANSITION FROM RUN TO IDLE := BACK; END_TRANSITION
END_PROGRAM
{CFG}
"#);
    let mut rt = build(&src);
    rt.run_for(30_000);
    assert!(boolean(&rt, "R.P1.IDLE.X"));
    assert!(!boolean(&rt, "R.P1.LAMP"));
    set(&mut rt, "GO", true);
    rt.run_for(10_000);
    assert!(boolean(&rt, "R.P1.RUN.X"));
    set(&mut rt, "GO", false);
    rt.run_for(40_000);
    assert!(boolean(&rt, "R.P1.LAMP"));
    // 4 executions while active
    assert_eq!(int(&rt, "R.P1.CNT"), 4);
    set(&mut rt, "BACK", true);
    rt.run_for(10_000);
    assert!(boolean(&rt, "R.P1.IDLE.X"));
    // final scan of the action (Q falling edge keeps A for one more execution)
    rt.run_for(10_000);
    assert_eq!(int(&rt, "R.P1.CNT"), 6);
    assert!(!boolean(&rt, "R.P1.LAMP"));
    assert!(rt.errors.is_empty(), "{:?}", rt.errors);
}

#[test]
fn qualifiers_s_r_l_d_p() {
    let src = format!(r#"
PROGRAM P
VAR NEXT : BOOL; A_S, A_L, A_D, A_P1 : BOOL; PULSES : INT; END_VAR
INITIAL_STEP S1 : A_S(S); A_L(L, T#30ms); A_D(D, T#30ms); PULSE(P); END_STEP
ACTION PULSE : PULSES := PULSES + 1; END_ACTION
TRANSITION FROM S1 TO S2 := NEXT; END_TRANSITION
STEP S2 : END_STEP
TRANSITION FROM S2 TO S3 := NEXT; END_TRANSITION
STEP S3 : A_S(R); END_STEP
TRANSITION FROM S3 TO S1 := FALSE; END_TRANSITION
END_PROGRAM
{CFG}
"#);
    let mut rt = build(&src);
    rt.run_for(10_000);
    assert!(boolean(&rt, "R.P1.A_S"));
    assert!(boolean(&rt, "R.P1.A_L"));
    assert!(!boolean(&rt, "R.P1.A_D"));
    rt.run_for(40_000);
    assert!(!boolean(&rt, "R.P1.A_L"));
    assert!(boolean(&rt, "R.P1.A_D"));
    // P executes the action once (plus final scan)
    assert_eq!(int(&rt, "R.P1.PULSES"), 2);
    set(&mut rt, "NEXT", true);
    rt.run_for(10_000);
    assert!(boolean(&rt, "R.P1.S2.X"));
    rt.run_for(10_000);
    // stored action stays set until reset
    assert!(boolean(&rt, "R.P1.A_S"));
    assert!(!boolean(&rt, "R.P1.A_D"));
    rt.run_for(20_000);
    assert!(boolean(&rt, "R.P1.S3.X"));
    assert!(!boolean(&rt, "R.P1.A_S"));
}

#[test]
fn mix_2_brix_simultaneous_sequences() {
    let src = format!(r#"
FUNCTION_BLOCK MIX_2_BRIX
VAR_INPUT ST : BOOL; d : BOOL; S0 : BOOL; S1 : BOOL; WC : WORD; z : INT; WA : INT; WB : INT; t1 : TIME; END_VAR
VAR_OUTPUT DONE, VA, VB, VC, MT, MR, MP0, MP1 : BOOL; END_VAR
INITIAL_STEP START: DONE(N); END_STEP
TRANSITION FROM START TO (WEIGH_A, BRICK1)
:= ST & S0 & WORD_BCD_TO_INT(WC) <= z;
END_TRANSITION
STEP WEIGH_A: VA(N); END_STEP
TRANSITION FROM WEIGH_A TO WEIGH_B := WORD_BCD_TO_INT(WC) >= WA+z ;
END_TRANSITION
STEP WEIGH_B: VB(N); END_STEP
TRANSITION FROM WEIGH_B TO FILL := WORD_BCD_TO_INT(WC) >= WA+WB+z ;
END_TRANSITION
STEP FILL: VC(N); END_STEP
STEP BRICK1: MT(S); END_STEP
TRANSITION FROM BRICK1 TO DROP_1 := d ; END_TRANSITION
STEP DROP_1: END_STEP
TRANSITION FROM DROP_1 TO BRICK2 := NOT d ; END_TRANSITION
STEP BRICK2: END_STEP
TRANSITION FROM BRICK2 TO DROP_2 := d ; END_TRANSITION
STEP DROP_2: MT(R); END_STEP
TRANSITION FROM (FILL,DROP_2) TO MIX
:= WORD_BCD_TO_INT(WC) <= z & NOT d ;
END_TRANSITION
STEP MIX: MR(S); END_STEP
TRANSITION FROM MIX TO TIP := MIX.T >= t1 ; END_TRANSITION
STEP TIP: MP1(N); END_STEP
TRANSITION FROM TIP TO RAISE := S1 ; END_TRANSITION
STEP RAISE: MR(R); MP0(N); END_STEP
TRANSITION FROM RAISE TO START := S0 ; END_TRANSITION
END_FUNCTION_BLOCK

PROGRAM P
VAR M : MIX_2_BRIX; ST, d, S0, S1 : BOOL; WC : WORD; DONE, VA, VB, VC, MT, MR, MP0, MP1 : BOOL; END_VAR
M(ST := ST, d := d, S0 := S0, S1 := S1, WC := WC, z := 0, WA := 10, WB := 5, t1 := T#40ms);
DONE := M.DONE; VA := M.VA; VB := M.VB; VC := M.VC; MT := M.MT; MR := M.MR; MP0 := M.MP0; MP1 := M.MP1;
END_PROGRAM
{CFG}
"#);
    let mut rt = build(&src);
    let w = |rt: &mut structure_runtime::Runtime, v: i128| rt.write("R.P1.WC", &Value::Int(v)).unwrap();
    rt.run_for(20_000);
    assert!(boolean(&rt, "R.P1.DONE"));
    set(&mut rt, "S0", true);
    set(&mut rt, "ST", true);
    rt.run_for(20_000);
    assert!(boolean(&rt, "R.P1.VA") && boolean(&rt, "R.P1.MT"));
    set(&mut rt, "ST", false);
    w(&mut rt, 0x10);
    rt.run_for(20_000);
    assert!(boolean(&rt, "R.P1.VB") && !boolean(&rt, "R.P1.VA"));
    w(&mut rt, 0x15);
    rt.run_for(20_000);
    assert!(boolean(&rt, "R.P1.VC"));
    // bricks
    set(&mut rt, "d", true);
    rt.run_for(20_000);
    set(&mut rt, "d", false);
    rt.run_for(20_000);
    set(&mut rt, "d", true);
    rt.run_for(20_000);
    assert!(!boolean(&rt, "R.P1.MT"));
    // both branches done: empty scale and no detector -> MIX
    set(&mut rt, "d", false);
    w(&mut rt, 0);
    rt.run_for(20_000);
    assert!(boolean(&rt, "R.P1.MR"));
    rt.run_for(50_000);
    assert!(boolean(&rt, "R.P1.MP1"));
    set(&mut rt, "S1", true);
    rt.run_for(20_000);
    assert!(boolean(&rt, "R.P1.MP0") && !boolean(&rt, "R.P1.MR"));
    rt.run_for(20_000);
    assert!(boolean(&rt, "R.P1.DONE"));
    assert!(rt.errors.is_empty(), "{:?}", rt.errors);
}

#[test]
fn transition_priority_in_declaration_order() {
    let src = format!(r#"
PROGRAM P
VAR A, B : BOOL; END_VAR
INITIAL_STEP S0 : END_STEP
TRANSITION FROM S0 TO S1 := A; END_TRANSITION
TRANSITION FROM S0 TO S2 := B; END_TRANSITION
STEP S1 : END_STEP
STEP S2 : END_STEP
TRANSITION FROM S1 TO S0 := FALSE; END_TRANSITION
TRANSITION FROM S2 TO S0 := FALSE; END_TRANSITION
END_PROGRAM
{CFG}
"#);
    let mut rt = build(&src);
    set(&mut rt, "A", true);
    set(&mut rt, "B", true);
    rt.run_for(20_000);
    assert!(boolean(&rt, "R.P1.S1.X"));
    assert!(!boolean(&rt, "R.P1.S2.X"));
}

#[test]
fn il_transition_and_errors() {
    let src = format!(r#"
PROGRAM P
VAR A, B : BOOL; END_VAR
INITIAL_STEP S0 : END_STEP
TRANSITION FROM S0 TO S1 :
  LD A
  AND B
END_TRANSITION
STEP S1 : END_STEP
TRANSITION FROM S1 TO S0 := NOT A; END_TRANSITION
END_PROGRAM
{CFG}
"#);
    let mut rt = build(&src);
    set(&mut rt, "A", true);
    rt.run_for(20_000);
    assert!(boolean(&rt, "R.P1.S0.X"));
    set(&mut rt, "B", true);
    rt.run_for(20_000);
    assert!(boolean(&rt, "R.P1.S1.X"));
    // two initial steps in one network
    let e = errors(&format!("PROGRAM P\nINITIAL_STEP A: END_STEP\nINITIAL_STEP B: END_STEP\nTRANSITION FROM A TO B := TRUE; END_TRANSITION\nTRANSITION FROM B TO A := TRUE; END_TRANSITION\nEND_PROGRAM\n{CFG}"));
    assert!(e.contains("exactly one initial step"), "{}", e);
    // assignment to a step flag
    let e = errors(&format!("PROGRAM P\nINITIAL_STEP A: X1(N); END_STEP\nACTION X1: A.X := FALSE; END_ACTION\nTRANSITION FROM A TO B := TRUE; END_TRANSITION\nSTEP B: END_STEP\nTRANSITION FROM B TO A := TRUE; END_TRANSITION\nEND_PROGRAM\n{CFG}"));
    assert!(e.contains("cannot be assigned"), "{}", e);
    // unsafe SFC (figure 18 a)
    let e = errors(&format!("PROGRAM P\nINITIAL_STEP A: END_STEP\nTRANSITION FROM A TO (B, C) := TRUE; END_TRANSITION\nSTEP B: END_STEP\nSTEP C: END_STEP\nTRANSITION FROM B TO A := TRUE; END_TRANSITION\nTRANSITION FROM C TO D := TRUE; END_TRANSITION\nSTEP D: END_STEP\nTRANSITION FROM D TO A := TRUE; END_TRANSITION\nEND_PROGRAM\n{CFG}"));
    assert!(e.contains("unsafe"), "{}", e);
}

#[test]
fn chart_included_in_action_table_42_2s() {
    let src = format!(r#"
PROGRAM P
VAR GO, STOP, NEXT : BOOL; FWD : BOOL; TICKS : INT; END_VAR
INITIAL_STEP IDLE : END_STEP
TRANSITION FROM IDLE TO OPENING := GO; END_TRANSITION
STEP OPENING : OPEN_VALVE(N); END_STEP
TRANSITION FROM OPENING TO IDLE := STOP; END_TRANSITION
ACTION OPEN_VALVE:
  INITIAL_STEP READY : END_STEP
  TRANSITION FROM READY TO MOVING := NEXT & OPENING.X; END_TRANSITION
  STEP MOVING : FWD(N); COUNT(N); END_STEP
  ACTION COUNT : TICKS := TICKS + 1; END_ACTION
  TRANSITION FROM MOVING TO READY := NOT NEXT; END_TRANSITION
END_ACTION
END_PROGRAM
{CFG}
"#);
    let mut rt = build(&src);
    let x = |rt: &structure_runtime::Runtime, s: &str| boolean(rt, &format!("R.P1.{}.X", s));
    rt.run_for(10_000);
    // the included chart is inactive while its action is
    assert!(x(&rt, "IDLE") && !x(&rt, "READY") && !x(&rt, "MOVING"));
    set(&mut rt, "GO", true);
    rt.run_for(10_000);
    set(&mut rt, "GO", false);
    assert!(x(&rt, "OPENING") && !x(&rt, "READY"));
    rt.run_for(10_000);
    // activating the action activates the initial step of its chart
    assert!(x(&rt, "READY"));
    set(&mut rt, "NEXT", true);
    rt.run_for(10_000);
    assert!(x(&rt, "MOVING") && !x(&rt, "READY"));
    rt.run_for(10_000);
    assert!(boolean(&rt, "R.P1.FWD"));
    assert_eq!(int(&rt, "R.P1.TICKS"), 1);
    set(&mut rt, "STOP", true);
    rt.run_for(10_000);
    assert!(x(&rt, "IDLE") && x(&rt, "MOVING"));
    assert_eq!(int(&rt, "R.P1.TICKS"), 2);
    // final scan of OPEN_VALVE: the included steps are deactivated and
    // their actions get their own final scan
    rt.run_for(10_000);
    assert!(!x(&rt, "MOVING") && !x(&rt, "READY"));
    assert!(!boolean(&rt, "R.P1.FWD"));
    assert_eq!(int(&rt, "R.P1.TICKS"), 3);
    rt.run_for(30_000);
    assert_eq!(int(&rt, "R.P1.TICKS"), 3);
    // reactivation restarts the included chart at its initial step
    set(&mut rt, "STOP", false);
    set(&mut rt, "NEXT", false);
    set(&mut rt, "GO", true);
    rt.run_for(10_000);
    set(&mut rt, "GO", false);
    rt.run_for(10_000);
    assert!(x(&rt, "READY") && !x(&rt, "MOVING"));
    assert!(rt.errors.is_empty(), "{:?}", rt.errors);
}

#[test]
fn included_chart_errors() {
    let src = format!(r#"
PROGRAM P
VAR A, B : BOOL; END_VAR
INITIAL_STEP S0 : B(N); END_STEP
TRANSITION FROM S0 TO S1 := A; END_TRANSITION
STEP S1 : OUTER(N); END_STEP
TRANSITION FROM S1 TO S0 := NOT A; END_TRANSITION
ACTION OUTER:
  INITIAL_STEP N0 : B(N); END_STEP
  TRANSITION FROM N0 TO S1 := A; END_TRANSITION
  STEP N1 : END_STEP
END_ACTION
END_PROGRAM
{CFG}
"#);
    let e = errors(&src);
    assert!(e.contains("the action B is associated with steps of different charts"), "{}", e);
    assert!(e.contains("the step S1 belongs to another chart"), "{}", e);
    assert!(e.contains("the step N1 can never become active"), "{}", e);
}
