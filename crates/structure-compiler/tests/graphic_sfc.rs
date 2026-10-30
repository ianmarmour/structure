//! End-to-end tests of graphical SFC bodies (2.6, semigraphic form).
mod common;
use common::*;
use structure_runtime::Value;

const CFG: &str = "CONFIGURATION C RESOURCE R ON VM TASK T(INTERVAL := T#10ms, PRIORITY := 1); PROGRAM P1 WITH T : P; END_RESOURCE END_CONFIGURATION";

fn set(rt: &mut structure_runtime::Runtime, k: &str, v: bool) {
    rt.write(&format!("R.P1.{}", k), &Value::Bool(v)).unwrap();
}

// Drawings of annex F (as in the parser tests of structure-syntax).
const MIX_2_BRIX_SFC: &str = r#"
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
"#;

const GRAVEL_SEQUENCE_F4: &str = r#"
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
"#;

const GRAVEL_CONTROL_F5: &str = r#"
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
"#;

#[test]
fn f5_mix_2_brix_graphical() {
    let src = format!(
        r#"
FUNCTION_BLOCK MIX_2_BRIX
VAR_INPUT ST, d, S0, S1 : BOOL; WC : WORD; z, WA, WB : INT; t1 : TIME; END_VAR
VAR_OUTPUT DONE, VA, VB, VC, MT, MR, MP0, MP1 : BOOL; END_VAR
{MIX_2_BRIX_SFC}END_FUNCTION_BLOCK

PROGRAM P
VAR M : MIX_2_BRIX; ST, d, S0, S1 : BOOL; WC : WORD; DONE, VA, VB, VC, MT, MR, MP0, MP1 : BOOL; END_VAR
M(ST := ST, d := d, S0 := S0, S1 := S1, WC := WC, z := 0, WA := 10, WB := 5, t1 := T#40ms);
DONE := M.DONE; VA := M.VA; VB := M.VB; VC := M.VC; MT := M.MT; MR := M.MR; MP0 := M.MP0; MP1 := M.MP1;
END_PROGRAM
{CFG}
"#
    );
    let mut rt = build(&src);
    let w = |rt: &mut structure_runtime::Runtime, v: i128| rt.write("R.P1.WC", &Value::Int(v)).unwrap();
    rt.run_for(20_000);
    assert!(boolean(&rt, "R.P1.DONE"));
    set(&mut rt, "S0", true);
    set(&mut rt, "ST", true);
    rt.run_for(20_000);
    assert!(boolean(&rt, "R.P1.VA") && boolean(&rt, "R.P1.MT"), "simultaneous divergence");
    set(&mut rt, "ST", false);
    w(&mut rt, 0x10);
    rt.run_for(20_000);
    assert!(boolean(&rt, "R.P1.VB") && !boolean(&rt, "R.P1.VA"));
    w(&mut rt, 0x15);
    rt.run_for(20_000);
    assert!(boolean(&rt, "R.P1.VC"));
    set(&mut rt, "d", true);
    rt.run_for(20_000);
    set(&mut rt, "d", false);
    rt.run_for(20_000);
    set(&mut rt, "d", true);
    rt.run_for(20_000);
    assert!(!boolean(&rt, "R.P1.MT"));
    // simultaneous convergence
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
    assert!(boolean(&rt, "R.P1.DONE"), "the loop back to START");
    assert!(rt.errors.is_empty(), "{:?}", rt.errors);
}

#[test]
fn f7_gravel_graphical() {
    let src = format!(
        r#"
PROGRAM GRAVEL
VAR_INPUT
  OFF_PB, ON_PB, FILL_PB, SIREN_ACK, LOAD_PB, JOG_PB, LAMP_TEST, TRUCK_ON_RAMP, SILO_EMPTY_LS, BIN_EMPTY_LS : BOOL;
  SETPOINT : BYTE;
END_VAR
VAR_OUTPUT
  CONTROL_LAMP, TRUCK_LAMP, SILO_EMPTY_LAMP, CONVEYOR_LAMP, CONVEYOR_MOTOR, SILO_VALVE, BIN_VALVE, SIREN : BOOL;
  BIN_LEVEL : BYTE;
END_VAR
VAR
  BLINK_TIME : TIME := T#200ms; PULSE_TIME : TIME := T#50ms; RUNOUT_TIME: TIME := T#300ms;
  RUN_IN_TIME: TIME := T#200ms; SILENT_TIME: TIME := T#1s;
  OK_TO_RUN : BOOL; BLINK: TON; BLANK: TON; PULSE: TON; SIREN_FF: RS; SILENCE_TMR: TP;
END_VAR
VAR RETAIN LEVEL_CTR : CTU ; END_VAR
{GRAVEL_SEQUENCE_F4}
{GRAVEL_CONTROL_F5}
ACTION MONITOR_ACTION:
  CONVEYOR_LAMP := CONVEYOR_MOTOR & BLINK.Q ;
  CONTROL_LAMP := CONTROL.X OR LAMP_TEST ;
  TRUCK_LAMP := TRUCK_ON_RAMP OR LAMP_TEST ;
  SILO_EMPTY_LAMP := BLINK.Q & SILO_EMPTY_LS OR LAMP_TEST ;
  SILENCE_TMR(IN:=SIREN_ACK, PT:=SILENT_TIME) ;
  SIREN_FF(S:=SILO_EMPTY_LS, R1:=SILENCE_TMR.Q) ;
  SIREN := SIREN_FF.Q1 ;
  PULSE(IN:=FILL_BIN.X & NOT PULSE.Q, PT:=PULSE_TIME) ;
  LEVEL_CTR(R := BIN_EMPTY_LS, CU := PULSE.Q, PV := BYTE_BCD_TO_INT(SETPOINT)) ;
  BIN_LEVEL := INT_TO_BCD_BYTE(LEVEL_CTR.CV) ;
END_ACTION
END_PROGRAM

CONFIGURATION GRAVEL_CONTROL
  RESOURCE PROC1 ON PROC_TYPE_Y
    TASK SCAN(INTERVAL := T#10ms, PRIORITY := 1);
    PROGRAM G WITH SCAN : GRAVEL
    (OFF_PB := %I0.0, ON_PB := %I0.1, FILL_PB := %I0.2, SIREN_ACK := %I0.3, LOAD_PB := %I0.4,
     JOG_PB := %I0.5, LAMP_TEST := %I0.7, TRUCK_ON_RAMP := %I1.4, SILO_EMPTY_LS := %I1.5,
     BIN_EMPTY_LS := %I1.6, SETPOINT := %IB2,
     CONTROL_LAMP => %Q4.0, TRUCK_LAMP => %Q4.2, SILO_EMPTY_LAMP => %Q4.3, CONVEYOR_LAMP => %Q5.3,
     CONVEYOR_MOTOR => %Q5.4, SILO_VALVE => %Q5.5, BIN_VALVE => %Q5.6, SIREN => %Q5.7, BIN_LEVEL => %QB6) ;
  END_RESOURCE
END_CONFIGURATION
"#
    );
    let mut rt = build(&src);
    let bit = |rt: &mut structure_runtime::Runtime, byte: usize, bit: u8, v: bool| {
        let i = rt.io(0, 'I');
        if v {
            i[byte] |= 1 << bit;
        } else {
            i[byte] &= !(1 << bit);
        }
    };
    let q = |rt: &mut structure_runtime::Runtime, byte: usize, bit: u8| rt.io(0, 'Q')[byte] & (1 << bit) != 0;
    rt.io(0, 'I')[2] = 0x03;
    bit(&mut rt, 0, 1, true); // ON
    rt.run_for(50_000);
    bit(&mut rt, 0, 1, false);
    assert!(q(&mut rt, 4, 0), "control lamp");
    bit(&mut rt, 0, 2, true); // FILL
    rt.run_for(50_000);
    assert!(q(&mut rt, 5, 5), "silo valve open while filling");
    rt.run_for(300_000);
    assert_eq!(rt.io(0, 'Q')[6], 0x03, "bin level reached the set point");
    assert!(!q(&mut rt, 5, 5), "silo valve closed");
    assert!(boolean(&rt, "PROC1.G.LOAD_WAIT.X"));
    bit(&mut rt, 0, 2, false);
    bit(&mut rt, 1, 4, true);
    bit(&mut rt, 0, 4, true);
    rt.run_for(50_000);
    assert!(q(&mut rt, 5, 4), "conveyor running in RUN_IN (graphical action body)");
    rt.run_for(250_000);
    assert!(q(&mut rt, 5, 6), "bin valve open while dumping");
    bit(&mut rt, 1, 6, true);
    rt.run_for(50_000);
    assert!(boolean(&rt, "PROC1.G.RUNOUT.X"));
    rt.run_for(400_000);
    assert!(boolean(&rt, "PROC1.G.START.X"));
    // OFF: CONTROL -> CONTROL_OFF
    bit(&mut rt, 0, 0, true);
    rt.run_for(30_000);
    assert!(boolean(&rt, "PROC1.G.CONTROL_OFF.X") && !q(&mut rt, 4, 0));
    assert!(rt.errors.is_empty(), "{:?}", rt.errors);
}

fn divergence(drawing: &str) -> structure_runtime::Runtime {
    build(&format!("PROGRAM P\nVAR e, f : BOOL; END_VAR\n{drawing}END_PROGRAM\n{CFG}\n"))
}

#[test]
fn selection_divergence_table_46() {
    // 2a: left-to-right priority
    let mut rt = divergence(
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
    );
    set(&mut rt, "e", true);
    set(&mut rt, "f", true);
    rt.run_for(10_000);
    assert!(boolean(&rt, "R.P1.S6.X") && !boolean(&rt, "R.P1.S8.X"));
    // 2b: numbered branches, lowest number first
    let mut rt = divergence(
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
    );
    set(&mut rt, "e", true);
    set(&mut rt, "f", true);
    rt.run_for(10_000);
    assert!(!boolean(&rt, "R.P1.S6.X") && boolean(&rt, "R.P1.S8.X"));
    // 2c: the user assures mutual exclusion; simultaneously true
    // transitions are an error (annex E, 2.6.5)
    let c = r#"
     +===+===+
     || S5  ||
     +===+===+
         |
   +-----+------+
   |            |
   + e          + f
   |            |
+--+--+      +--+--+
| S6  |      | S8  |
+-----+      +-----+
"#;
    let mut rt = divergence(c);
    set(&mut rt, "f", true);
    rt.run_for(10_000);
    assert!(boolean(&rt, "R.P1.S8.X"));
    assert!(rt.errors.is_empty());
    let mut rt = divergence(c);
    set(&mut rt, "e", true);
    set(&mut rt, "f", true);
    rt.run_for(10_000);
    assert!(!rt.errors.is_empty(), "simultaneously true transitions must be reported");
}

#[test]
fn transition_conditions_table_41() {
    // features 2 (LD), 3 (FBD), 4 (connector) and 7 (transition name with
    // a textual definition in LD)
    let src = format!(
        r#"
PROGRAM P
VAR X, DN1 : BOOL; END_VAR
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
                        + TRAN910
                        |
                     +--+--+
                     | S10 |
                     +-----+

| %IX2.4  %IX2.3
+---| |-----| |---->TRANX>
|
TRANSITION TRAN910 FROM STEP9 TO S10 :
  |  X       DN1     TRAN910  |
  +--| |-----|/|-----( )------+
  |                           |
END_TRANSITION
END_PROGRAM
{CFG}
"#
    );
    let mut rt = build(&src);
    let x = |rt: &structure_runtime::Runtime, s: &str| boolean(rt, &format!("R.P1.{}.X", s));
    rt.run_for(10_000);
    assert!(x(&rt, "S6"));
    rt.io(0, 'I')[2] = 0b0001_0000; // %IX2.4 only
    rt.run_for(10_000);
    assert!(x(&rt, "S6"), "LD condition: both contacts are needed");
    rt.io(0, 'I')[2] = 0b0001_1000;
    rt.run_for(10_000);
    assert!(x(&rt, "STEP7"));
    rt.run_for(10_000);
    assert!(x(&rt, "STEP8"), "FBD condition");
    rt.run_for(10_000);
    assert!(x(&rt, "STEP9"), "condition through a connector");
    set(&mut rt, "X", true);
    set(&mut rt, "DN1", true);
    rt.run_for(10_000);
    assert!(x(&rt, "STEP9"), "named transition, LD definition");
    set(&mut rt, "DN1", false);
    rt.run_for(10_000);
    assert!(x(&rt, "S10"));
    assert!(rt.errors.is_empty(), "{:?}", rt.errors);
}

const ACTION_BLOCKS_43: &str = r#"
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
            | %MX10 := FF28.Q1;              |
            +--------------------------------+
"#;

const SFC_IN_ACTION_42: &str = r#"
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
  +-----+   |         + c         |
            |         |           |
            |    +----+-----+     |
            |    |V1_OPENING|     |
            |    +----------+     |
            +---------------------+
"#;

const SKIP_AND_LOOP_46: &str = r#"
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
"#;

#[test]
fn action_blocks_table_43_44() {
    let src = format!(
        r#"
PROGRAM P
VAR DN1, ACTION_1, ACTION_2, ACTION_3 : BOOL; C : INT := 1; D : INT := 2; FF28 : SR; END_VAR
{ACTION_BLOCKS_43}END_PROGRAM
{CFG}
"#
    );
    let mut rt = build(&src);
    let b = |rt: &structure_runtime::Runtime, k: &str| boolean(rt, &format!("R.P1.{}", k));
    rt.run_for(10_000);
    set(&mut rt, "DN1", true);
    rt.run_for(10_000); // S7 -> S8
    rt.run_for(10_000); // S8: ACTION_1 (L)
    assert!(b(&rt, "ACTION_1") && b(&rt, "S9.X"));
    rt.run_for(10_000); // S9: concatenated blocks
    assert!(b(&rt, "ACTION_1") && b(&rt, "ACTION_2") && b(&rt, "ACTION_3"));
    assert!(b(&rt, "S10.X"));
    rt.run_for(10_000); // S10: ACTION_4 ("d" field in ST)
    assert!(!b(&rt, "ACTION_1") && !b(&rt, "ACTION_2") && !b(&rt, "ACTION_3"));
    assert!(b(&rt, "FF28.Q1"));
    assert_eq!(rt.io(0, 'M')[1] & 0b100, 0b100, "%MX10 := FF28.Q1");
    assert!(rt.errors.is_empty(), "{:?}", rt.errors);
}

#[test]
fn chart_in_an_action_table_42_2s() {
    let src = format!("PROGRAM P\nVAR a, b, c : BOOL; END_VAR\n{SFC_IN_ACTION_42}END_PROGRAM\n{CFG}\n");
    let mut rt = build(&src);
    let x = |rt: &structure_runtime::Runtime, s: &str| boolean(rt, &format!("R.P1.{}.X", s));
    rt.run_for(10_000);
    assert!(x(&rt, "S1") && !x(&rt, "V1_READY"));
    set(&mut rt, "a", true);
    rt.run_for(20_000);
    assert!(x(&rt, "S2") && x(&rt, "V1_READY"));
    set(&mut rt, "c", true);
    rt.run_for(10_000);
    assert!(x(&rt, "V1_OPENING") && !x(&rt, "V1_READY"));
    set(&mut rt, "b", true);
    rt.run_for(20_000);
    assert!(x(&rt, "S3") && !x(&rt, "V1_OPENING"), "the included chart ends with its action");
    assert!(rt.errors.is_empty(), "{:?}", rt.errors);
}

#[test]
fn sequence_skip_and_loop_table_46() {
    let src = format!("PROGRAM P\nVAR a, b, c, d, e, f : BOOL; END_VAR\n{SKIP_AND_LOOP_46}END_PROGRAM\n{CFG}\n");
    let mut rt = build(&src);
    let x = |rt: &structure_runtime::Runtime, s: &str| boolean(rt, &format!("R.P1.{}.X", s));
    // sequence skip (feature 5)
    set(&mut rt, "d", true);
    rt.run_for(10_000);
    assert!(x(&rt, "S33") && !x(&rt, "S31"));
    set(&mut rt, "c", true);
    rt.run_for(10_000);
    assert!(x(&rt, "S34"));
    // sequence loop (feature 6): S34 is reactivated, its time restarts
    set(&mut rt, "f", true);
    rt.run_for(30_000);
    assert!(x(&rt, "S34"));
    assert!(rt.read("R.P1.S34.T").unwrap().to_string() == "T#0s", "{}", rt.read("R.P1.S34.T").unwrap());
    set(&mut rt, "f", false);
    set(&mut rt, "e", true);
    rt.run_for(10_000);
    assert!(x(&rt, "S35"));
    assert!(rt.errors.is_empty(), "{:?}", rt.errors);
}

#[test]
fn graphical_and_textual_action_and_transition_bodies() {
    // 41 7b: transition name with an FBD definition; 42 2l: action declared
    // in LD; 44 4: "d" field in IL
    let src = format!(
        r#"
PROGRAM P
VAR A, B, OUT1, OUT2 : BOOL; END_VAR
  +==+===+
  || S1 ||
  +==+===+
     |
     + T12
     |
  +--+--+   +---+-------+
  | S2  |---| N | LD_ACT|
  +--+--+   +---+-------+
     |
     + NOT A
     |
  +--+--+   +---+----------------+
  | S3  |---| N |     IL_ACT     |
  +-----+   +---+----------------+
            |   LD    B          |
            |   ST    OUT2       |
            +--------------------+
TRANSITION T12 FROM S1 TO S2 :
         +---+
A--------| & |---T12
B--------|   |
         +---+
END_TRANSITION
ACTION LD_ACT:
|   A        OUT1   |
+---| |------( )----+
|                   |
END_ACTION
END_PROGRAM
{CFG}
"#
    );
    let mut rt = build(&src);
    let x = |rt: &structure_runtime::Runtime, s: &str| boolean(rt, &format!("R.P1.{}", s));
    set(&mut rt, "A", true);
    rt.run_for(10_000);
    assert!(x(&rt, "S1.X"), "T12 needs A and B");
    set(&mut rt, "B", true);
    rt.run_for(20_000);
    assert!(x(&rt, "S2.X") && x(&rt, "OUT1"));
    set(&mut rt, "A", false);
    rt.run_for(20_000);
    assert!(x(&rt, "S3.X") && x(&rt, "OUT2"));
    assert!(!x(&rt, "OUT1"), "final scan of LD_ACT with A = FALSE");
    assert!(rt.errors.is_empty(), "{:?}", rt.errors);
}

const AGV_MAIN_F12: &str = r#"
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
"#;

const AGV_CYCLE_F12: &str = r#"
+-----------+
|           |
|      +====+====+
|      || READY ||
|      +====+====+
|           |
|           + CYCLE
|           |
|      +----+----+   +---+-----------+
|      | FORWARD |---| N | FWD_MOTOR |
|      +----+----+   +---+-----------+
|           |
|           + RIGHT_LS
|           |
|      +----+----+   +---+-----------+
|      | REVERSE |---| N | REV_MOTOR |
|      +----+----+   +---+-----------+
|           |
|           + LEFT_LS
|           |
|       +---+---+
|       | DONE  |
|       +---+---+
|           |
|           + NOT CYCLE
|           |
+-----------+
"#;

#[test]
fn f8_agv_graphical_program_and_configuration() {
    // figures F.10 (graphical declaration), F.11 (graphical configuration)
    // and F.12 (two charts)
    let src = format!(
        r#"
PROGRAM
       +----------------------+
       |         AGV          |
BOOL---|SINGLE_PB    FWD_MOTOR|---BOOL
BOOL---|DOUBLE_PB    REV_MOTOR|---BOOL
BOOL---|LEFT_LS               |
BOOL---|RIGHT_LS              |
       +----------------------+
VAR CYCLE : BOOL; END_VAR
{AGV_MAIN_F12}
{AGV_CYCLE_F12}END_PROGRAM

CONFIGURATION AGV_CONTROL
  RESOURCE AGV_PROC ON SMALL_PC
             AGV_1
          +----------------------+
          |         AGV          |
  %IX1----|SINGLE_PB    FWD_MOTOR|---%QX1
  %IX2----|DOUBLE_PB    REV_MOTOR|---%QX2
  %IX3----|LEFT_LS               |
  %IX4----|RIGHT_LS              |
          +----------------------+
  END_RESOURCE
END_CONFIGURATION
"#
    )
    .replace("%", "%");
    let mut rt = build(&src);
    let input = |rt: &mut structure_runtime::Runtime, bit: u8, v: bool| {
        let i = rt.io(0, 'I');
        if v {
            i[0] |= 1 << bit;
        } else {
            i[0] &= !(1 << bit);
        }
    };
    let fwd = |rt: &mut structure_runtime::Runtime| rt.io(0, 'Q')[0] & 0b10 != 0;
    let rev = |rt: &mut structure_runtime::Runtime| rt.io(0, 'Q')[0] & 0b100 != 0;
    let step = |rt: &mut structure_runtime::Runtime| rt.run_for(10_000);
    input(&mut rt, 3, true); // at the left
    for _ in 0..3 {
        step(&mut rt);
    }
    assert!(!fwd(&mut rt) && !rev(&mut rt));
    // one cycle
    input(&mut rt, 1, true);
    for _ in 0..4 {
        step(&mut rt);
    }
    input(&mut rt, 1, false);
    assert!(fwd(&mut rt), "moving right");
    input(&mut rt, 3, false);
    input(&mut rt, 4, true);
    for _ in 0..3 {
        step(&mut rt);
    }
    assert!(rev(&mut rt) && !fwd(&mut rt), "returning");
    input(&mut rt, 4, false);
    input(&mut rt, 3, true);
    for _ in 0..8 {
        step(&mut rt);
    }
    assert!(!fwd(&mut rt) && !rev(&mut rt), "a single cycle");
    assert!(boolean(&rt, "AGV_PROC.AGV_1.START.X"));
    // two cycles
    input(&mut rt, 2, true);
    for _ in 0..4 {
        step(&mut rt);
    }
    input(&mut rt, 2, false);
    let mut cycles = 0;
    for _ in 0..3 {
        // wait for the forward motion, then drive the limit switches
        let mut moved = false;
        for _ in 0..10 {
            if fwd(&mut rt) {
                moved = true;
                break;
            }
            step(&mut rt);
        }
        if !moved {
            break;
        }
        cycles += 1;
        input(&mut rt, 3, false);
        input(&mut rt, 4, true);
        for _ in 0..3 {
            step(&mut rt);
        }
        input(&mut rt, 4, false);
        input(&mut rt, 3, true);
        for _ in 0..4 {
            step(&mut rt);
        }
    }
    assert_eq!(cycles, 2, "a double cycle");
    assert!(rt.errors.is_empty(), "{:?}", rt.errors);
}
