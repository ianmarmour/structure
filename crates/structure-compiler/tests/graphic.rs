//! End-to-end tests of LD and FBD bodies (clause 4): parsed, lowered,
//! linked and executed.
mod common;
use common::*;
use structure_runtime::Value;

const CFG: &str = "CONFIGURATION C RESOURCE R ON VM TASK T(INTERVAL := T#10ms, PRIORITY := 1); PROGRAM P1 WITH T : P; END_RESOURCE END_CONFIGURATION";

fn set(rt: &mut structure_runtime::Runtime, k: &str, v: Value) {
    rt.write(&format!("R.P1.{}", k), &v).unwrap();
}

// Drawings of annex F (as in the parser tests of structure-syntax).
const WEIGH_LD: &str = r#"
|                 +--------+      +-------+      +--------+             |
|                 |  BCD_  |      |  SUB  |      |  INT_  |             |
| weigh_command   | TO_INT |      |       |      | TO_BCD |    ENO      |
+------| |--------|EN   ENO|------|EN  ENO|------|EN   ENO|----( )------+
|                 |        |      |       |      |        |             |
| gross_weight----|        |------|       |------|        |---WEIGH     |
|                 +--------+      |       |      +--------+             |
| tare_weight---------------------|       |                             |
|                                 +-------+                             |
"#;

const WEIGH_FBD: &str = r#"
                  +--------+      +-------+      +--------+
                  |  BCD_  |      |  SUB  |      |  INT_  |
                  | TO_INT |      |       |      | TO_BCD |
weigh_command-----|EN   ENO|------|EN  ENO|------|EN   ENO|---ENO
gross_weight------|        |------|       |------|        |---WEIGH
                  +--------+      |       |      +--------+
tare_weight-----------------------|       |
                                  +-------+
"#;

const CMD_MONITOR_LD: &str = r#"
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
"#;

const CMD_MONITOR_FBD: &str = r#"
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
"#;

const STACK_INT_LD: &str = r#"
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
"#;


const WEIGH_DECL: &str = "VAR_INPUT weigh_command : BOOL; gross_weight : WORD; tare_weight : INT; END_VAR";

fn weigh(body: &str) {
    let src = format!(
        r#"FUNCTION WEIGH : WORD
{WEIGH_DECL}
{body}END_FUNCTION
PROGRAM P
VAR W1, W2 : WORD; OK1, OK2 : BOOL; END_VAR
W1 := WEIGH(weigh_command := TRUE, gross_weight := WORD#16#0123, tare_weight := 23, ENO => OK1);
W2 := WEIGH(weigh_command := FALSE, gross_weight := WORD#16#0123, tare_weight := 23, ENO => OK2);
END_PROGRAM
{CFG}
"#
    );
    let mut rt = build(&src);
    rt.run_for(10_000);
    assert_eq!(int(&rt, "R.P1.W1"), 0x100);
    assert!(boolean(&rt, "R.P1.OK1"));
    assert!(!boolean(&rt, "R.P1.OK2"));
    assert!(rt.errors.is_empty(), "{:?}", rt.errors);
}

#[test]
fn f1_weigh_ld() {
    weigh(WEIGH_LD);
}

#[test]
fn f1_weigh_fbd() {
    weigh(WEIGH_FBD);
}

const CMD_MONITOR_DECL: &str = "VAR_INPUT AUTO_CMD, AUTO_MODE, MAN_CMD, MAN_CMD_CHK : BOOL; T_CMD_MAX : TIME; FDBK, ACK : BOOL; END_VAR
VAR_OUTPUT CMD, ALRM : BOOL; END_VAR
VAR CMD_TMR : TON; ALRM_FF : SR; END_VAR";

fn cmd_monitor(body: &str) {
    let src = format!(
        r#"FUNCTION_BLOCK CMD_MONITOR
{CMD_MONITOR_DECL}
{body}END_FUNCTION_BLOCK
PROGRAM P
VAR M : CMD_MONITOR; AUTO, MAN, MAN_CHK, FDBK, ACK, CMD, ALRM : BOOL; END_VAR
M(AUTO_CMD := AUTO, AUTO_MODE := TRUE, MAN_CMD := MAN, MAN_CMD_CHK := MAN_CHK, T_CMD_MAX := T#50ms, FDBK := FDBK, ACK := ACK);
CMD := M.CMD; ALRM := M.ALRM;
END_PROGRAM
{CFG}
"#
    );
    let mut rt = build(&src);
    rt.run_for(10_000);
    assert!(!boolean(&rt, "R.P1.CMD"));
    set(&mut rt, "MAN", Value::Bool(true));
    rt.run_for(10_000);
    assert!(!boolean(&rt, "R.P1.CMD"), "manual commands are ignored in automatic mode");
    set(&mut rt, "AUTO", Value::Bool(true));
    rt.run_for(20_000);
    assert!(boolean(&rt, "R.P1.CMD"));
    assert!(!boolean(&rt, "R.P1.ALRM"));
    rt.run_for(60_000);
    assert!(boolean(&rt, "R.P1.ALRM"), "no feedback within T_CMD_MAX");
    set(&mut rt, "ACK", Value::Bool(true));
    rt.run_for(10_000);
    assert!(boolean(&rt, "R.P1.ALRM"), "the command must be cancelled before ACK can cancel the alarm");
    set(&mut rt, "AUTO", Value::Bool(false));
    rt.run_for(10_000);
    assert!(!boolean(&rt, "R.P1.ALRM"));
    assert!(rt.errors.is_empty(), "{:?}", rt.errors);
}

#[test]
fn f2_cmd_monitor_ld() {
    cmd_monitor(CMD_MONITOR_LD);
}

#[test]
fn f2_cmd_monitor_fbd() {
    cmd_monitor(CMD_MONITOR_FBD);
}

#[test]
fn f4_stack_int_ld() {
    let src = format!(
        r#"FUNCTION_BLOCK STACK_INT
VAR_INPUT PUSH, POP : BOOL R_EDGE; R1 : BOOL; IN : INT; N : INT; END_VAR
VAR_OUTPUT EMPTY : BOOL := 1; OFLO : BOOL := 0; OUT : INT := 0; END_VAR
VAR STK : ARRAY[0..127] OF INT; NI : INT := 128; PTR : INT := -1; END_VAR
{STACK_INT_LD}END_FUNCTION_BLOCK
PROGRAM P
VAR S : STACK_INT; PUSH, POP, R1 : BOOL; IN : INT; OUT : INT; EMPTY, OFLO : BOOL; END_VAR
S(PUSH := PUSH, POP := POP, R1 := R1, IN := IN, N := 2);
OUT := S.OUT; EMPTY := S.EMPTY; OFLO := S.OFLO;
END_PROGRAM
{CFG}
"#
    );
    let mut rt = build(&src);
    set(&mut rt, "R1", Value::Bool(true));
    rt.run_for(10_000);
    assert!(boolean(&rt, "R.P1.EMPTY") && !boolean(&rt, "R.P1.OFLO"));
    assert_eq!(int(&rt, "R.P1.S.NI"), 2);
    set(&mut rt, "R1", Value::Bool(false));
    for v in [10, 20, 30] {
        set(&mut rt, "IN", Value::Int(v));
        set(&mut rt, "PUSH", Value::Bool(true));
        rt.run_for(10_000);
        set(&mut rt, "PUSH", Value::Bool(false));
        rt.run_for(10_000);
        if v == 20 {
            assert_eq!(int(&rt, "R.P1.OUT"), 20);
            assert!(!boolean(&rt, "R.P1.EMPTY"));
        }
    }
    assert!(boolean(&rt, "R.P1.OFLO"), "the third push overflows");
    set(&mut rt, "POP", Value::Bool(true));
    rt.run_for(10_000);
    assert_eq!(int(&rt, "R.P1.OUT"), 20);
    assert!(!boolean(&rt, "R.P1.OFLO"));
    set(&mut rt, "POP", Value::Bool(false));
    rt.run_for(10_000);
    set(&mut rt, "POP", Value::Bool(true));
    rt.run_for(10_000);
    assert_eq!(int(&rt, "R.P1.OUT"), 10);
    assert!(rt.errors.is_empty(), "{:?}", rt.errors);
}

fn prog(decls: &str, body: &str) -> structure_runtime::Runtime {
    build(&format!("PROGRAM P\nVAR {decls} END_VAR\n{body}END_PROGRAM\n{CFG}\n"))
}

fn b(rt: &structure_runtime::Runtime, k: &str) -> bool {
    boolean(rt, &format!("R.P1.{}", k))
}

#[test]
fn ld_contacts_and_coils_tables_61_62() {
    let mut rt = prog(
        "a, bb, c, d, o1, o2, o3, o4, o5, o6, o7, o8 : BOOL;",
        r#"
|   a     bb      o1     o2     o3     o4    |
+--| |---|/|-----( )----(/)----(S)----(R)----+
|   c                    o5                  |
+--|P|------------------( )------------------+
|   d                    o6                  |
+--|N|------------------( )------------------+
|   a                    o7                  |
+--| |------------------(P)------------------+
|   a                    o8                  |
+--| |------------------(N)------------------+
|                                            |
"#,
    );
    rt.run_for(10_000);
    assert!(!b(&rt, "o1") && b(&rt, "o2") && !b(&rt, "o3") && !b(&rt, "o4"));
    set(&mut rt, "a", Value::Bool(true));
    rt.run_for(10_000);
    assert!(b(&rt, "o1") && !b(&rt, "o2") && b(&rt, "o3") && !b(&rt, "o4"));
    assert!(b(&rt, "o7"), "positive transition-sensing coil");
    rt.run_for(10_000);
    assert!(!b(&rt, "o7"));
    set(&mut rt, "bb", Value::Bool(true));
    rt.run_for(10_000);
    assert!(!b(&rt, "o1") && b(&rt, "o2") && b(&rt, "o3"), "S coil keeps its state");
    set(&mut rt, "a", Value::Bool(false));
    rt.run_for(10_000);
    assert!(b(&rt, "o8"), "negative transition-sensing coil");
    rt.run_for(10_000);
    assert!(!b(&rt, "o8"));
    set(&mut rt, "c", Value::Bool(true));
    rt.run_for(10_000);
    assert!(b(&rt, "o5"), "positive transition-sensing contact");
    rt.run_for(10_000);
    assert!(!b(&rt, "o5"));
    set(&mut rt, "d", Value::Bool(true));
    rt.run_for(10_000);
    assert!(!b(&rt, "o6"));
    set(&mut rt, "d", Value::Bool(false));
    rt.run_for(10_000);
    assert!(b(&rt, "o6"), "negative transition-sensing contact");
    rt.run_for(10_000);
    assert!(!b(&rt, "o6"));
}

fn latch(rt: &mut structure_runtime::Runtime) {
    set(rt, "ENABLE", Value::Bool(true));
    rt.run_for(10_000);
    assert!(!b(rt, "RUN"));
    set(rt, "START1", Value::Bool(true));
    rt.run_for(10_000);
    assert!(b(rt, "RUN"));
    set(rt, "START1", Value::Bool(false));
    rt.run_for(20_000);
    assert!(b(rt, "RUN"), "the feedback keeps RUN set");
    set(rt, "ENABLE", Value::Bool(false));
    rt.run_for(10_000);
    assert!(!b(rt, "RUN"));
    set(rt, "ENABLE", Value::Bool(true));
    rt.run_for(10_000);
    assert!(!b(rt, "RUN"));
}

#[test]
fn feedback_figure_23() {
    const DECLS: &str = "ENABLE, START1, START2, RUN : BOOL;";
    // a) explicit loop through the variable RUN
    latch(&mut prog(
        DECLS,
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
    ));
    // b) implicit loop
    latch(&mut prog(
        DECLS,
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
    ));
    // c) LD
    latch(&mut prog(
        DECLS,
        r#"
| START1     ENABLE     RUN   |
+---| |----+---| |------( )---+
| START2   |                  |
+---| |----+                  |
|   RUN    |                  |
+---| |----+                  |
|                             |
"#,
    ));
}

#[test]
fn fbd_jumps_return_and_connectors() {
    let mut rt = prog(
        "X, R, A, B, Y, Z, C : BOOL;",
        r#"
X---->>SKIP

R---<RETURN>

        +---+
A-------|>=1|---Y
B-------|   |
        +---+
SKIP:
        +---+
A-------| & |----->OTTO>
B-------|   |
        +---+

>OTTO>-------Z
"#,
    );
    set(&mut rt, "A", Value::Bool(true));
    rt.run_for(10_000);
    assert!(b(&rt, "Y") && !b(&rt, "Z"));
    set(&mut rt, "B", Value::Bool(true));
    set(&mut rt, "X", Value::Bool(true));
    set(&mut rt, "A", Value::Bool(false));
    rt.run_for(10_000);
    assert!(b(&rt, "Y"), "the OR network is skipped");
    assert!(!b(&rt, "Z"));
    set(&mut rt, "A", Value::Bool(true));
    rt.run_for(10_000);
    assert!(b(&rt, "Z"), "connector");
    set(&mut rt, "X", Value::Bool(false));
    set(&mut rt, "R", Value::Bool(true));
    set(&mut rt, "A", Value::Bool(false));
    rt.run_for(10_000);
    assert!(b(&rt, "Z"), "RETURN ends the execution");
}

#[test]
fn fbd_negation_and_en_eno_tables_19_20() {
    let mut rt = prog(
        "A, B, Y, EN_IN, OK : BOOL; I1 : INT := 2; I2 : INT := 3; SUM : INT;",
        r#"
        +---+
A------O| & |O---Y
B-------|   |
        +---+

        +-------+
        |  ADD  |
EN_IN---|EN  ENO|---OK
I1------|       |---SUM
I2------|       |
        +-------+
"#,
    );
    rt.run_for(10_000);
    assert!(b(&rt, "Y"), "NOT(NOT A AND B) with B = FALSE");
    assert_eq!(int(&rt, "R.P1.SUM"), 0);
    assert!(!b(&rt, "OK"));
    set(&mut rt, "B", Value::Bool(true));
    set(&mut rt, "EN_IN", Value::Bool(true));
    rt.run_for(10_000);
    assert!(!b(&rt, "Y"));
    assert_eq!(int(&rt, "R.P1.SUM"), 5);
    assert!(b(&rt, "OK"));
}

#[test]
fn figure_11_graphical_declarations_and_bodies() {
    let src = format!(
        r#"
FUNCTION_BLOCK
    +--------------+   (* External interface *)
    |   INSIDE_A   |
TON-|I_TMR  EXPIRED|---BOOL
    +--------------+

        +------+   (* Function Block body *)
        | MOVE |
I_TMR.Q-|      |---EXPIRED
        +------+
END_FUNCTION_BLOCK

FUNCTION_BLOCK
       +--------------+   (* External interface *)
       |   INSIDE_B   |
TON----|I_TMR----I_TMR|---TON
BOOL---|TMR_GO EXPIRED|---BOOL
       +--------------+

        I_TMR   (* Function Block body *)
        +-----+
        | TON |
TMR_GO--|IN  Q|---EXPIRED
        |PT ET|
        +-----+
END_FUNCTION_BLOCK

PROGRAM
       +--------------+   (* External interface *)
       |      P       |
BOOL---|GO        DONE|---BOOL
       +--------------+
VAR E_TMR : TON; I_BLK : INSIDE_A; E_TMR2 : TON; I_BLK2 : INSIDE_B; DONE2 : BOOL; END_VAR

           E_TMR
           +-----+                 I_BLK
           | TON |                 +--------------+
GO---------|IN  Q|                 |   INSIDE_A   |
t#100ms----|PT ET|       E_TMR-----|I_TMR  EXPIRED|---DONE
           +-----+                 +--------------+

           E_TMR2                  I_BLK2
           +-----+                 +---------------+
           | TON |                 |   INSIDE_B    |
           |IN  Q|      E_TMR2-----|I_TMR-----I_TMR|
t#100ms----|PT ET|          GO-----|TMR_GO  EXPIRED|---DONE2
           +-----+                 +---------------+
END_PROGRAM
{CFG}
"#
    );
    let mut rt = build(&src);
    set(&mut rt, "GO", Value::Bool(true));
    rt.run_for(50_000);
    assert!(!b(&rt, "DONE") && !b(&rt, "DONE2"));
    rt.run_for(80_000);
    assert!(b(&rt, "DONE") && b(&rt, "DONE2"));
}

const ACCUM: &str = r#"
FUNCTION_BLOCK
      +-------+
      | ACCUM |
INT---|A-----A|---INT
INT---|X      |
      +-------+
    +---+
A---| + |---A
X---|   |
    +---+
END_FUNCTION_BLOCK
"#;

#[test]
fn figure_12_in_out_variables() {
    let src = format!(
        r#"{ACCUM}
PROGRAM P
VAR ACC : INT; X1 : INT := 2; X2 : INT := 3; X3 : INT := 1; X4 : INT; ACC1, ACC2 : ACCUM; END_VAR
             ACC1
             +-------+
             | ACCUM |
ACC----------|A-----A|---ACC
     +---+   |       |
X1---| * |---|X      |
X2---|   |   +-------+
     +---+

             ACC2
             +-------+
             | ACCUM |
X3-----------|A-----A|---X4
     +---+   |       |
X1---| * |---|X      |
X2---|   |   +-------+
     +---+
END_PROGRAM
{CFG}
"#
    );
    let mut rt = build(&src);
    rt.run_for(10_000);
    assert_eq!(int(&rt, "R.P1.ACC"), 6);
    assert_eq!(int(&rt, "R.P1.X3"), 7, "12 d): X3 := X3 + X1*X2");
    assert_eq!(int(&rt, "R.P1.X4"), 7, "12 d): X4 := X3");
    rt.run_for(10_000);
    assert_eq!(int(&rt, "R.P1.ACC"), 12);
}

#[test]
fn figure_12e_illegal_in_out_connection() {
    let e = errors(&format!(
        r#"{ACCUM}
PROGRAM P
VAR ACC : INT; X1, X2, X3 : INT; ACC1 : ACCUM; END_VAR
             ACC1
     +---+   +-------+
X1---| * |   | ACCUM |
X2---|   |---|A-----A|---ACC
     +---+   |       |
X3-----------|X      |
             +-------+
END_PROGRAM
"#
    ));
    assert!(e.contains("in-out"), "{}", e);
}

#[test]
fn action_blocks_in_networks_table_44() {
    // features 8 and 9: an LD and an FBD network controlling Boolean
    // variable actions, in a POU without SFC
    let mut rt = prog(
        "S8, IN1, IN2, ACT1, ACT2, DN1, OK1, OK2 : BOOL;",
        r#"
|  S8    IN1      +---+------+---+      OK1    |
+--| |---| |------| N | ACT1 |DN1|------( )----+
|                 +---+------+---+             |
|                                              |

           +---+     +-------+------+
S8---------| & |-----| L     | ACT2 |---OK2
IN2--------|   |     |T#30ms |      |
           +---+     +-------+------+
"#,
    );
    rt.run_for(10_000);
    assert!(!b(&rt, "ACT1") && !b(&rt, "ACT2"));
    set(&mut rt, "S8", Value::Bool(true));
    set(&mut rt, "IN1", Value::Bool(true));
    set(&mut rt, "IN2", Value::Bool(true));
    set(&mut rt, "DN1", Value::Bool(true));
    rt.run_for(10_000);
    assert!(b(&rt, "ACT1") && b(&rt, "ACT2"));
    assert!(b(&rt, "OK1"), "output of an action block: its indicator");
    rt.run_for(40_000);
    assert!(b(&rt, "ACT1"));
    assert!(!b(&rt, "ACT2"), "time limited");
    assert!(!b(&rt, "OK2"));
    set(&mut rt, "IN1", Value::Bool(false));
    rt.run_for(10_000);
    assert!(!b(&rt, "ACT1"));
}
