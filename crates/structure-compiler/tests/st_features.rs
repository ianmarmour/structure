mod common;
use common::*;
use structure_runtime::Value;

#[test]
fn literals_table_4() {
    let rt = run_prog(
        "VAR a, b, c, d, e : DINT; f : UINT; g, h : BOOL; r : LREAL; s : REAL; END_VAR",
        "a := -12; b := 123_456; c := 2#1111_1111; d := 8#377; e := 16#FF; f := UINT#16#9AF; g := BOOL#1; h := TRUE;
         r := -1.34E-12; s := 3.14159_26;",
        1,
    );
    assert_eq!(int(&rt, "R.P1.A"), -12);
    assert_eq!(int(&rt, "R.P1.B"), 123456);
    assert_eq!(int(&rt, "R.P1.C"), 255);
    assert_eq!(int(&rt, "R.P1.D"), 255);
    assert_eq!(int(&rt, "R.P1.E"), 255);
    assert_eq!(int(&rt, "R.P1.F"), 0x9AF);
    assert!(boolean(&rt, "R.P1.G"));
    assert!(boolean(&rt, "R.P1.H"));
    assert_eq!(real(&rt, "R.P1.R"), -1.34E-12);
    #[allow(clippy::approx_constant)] // the literal of the program under test
    let s = 3.1415926;
    assert!((real(&rt, "R.P1.S") - s).abs() < 1e-6);
}

#[test]
fn integer_division_and_mod() {
    let rt = run_prog(
        "VAR a, b, c, d : INT; END_VAR",
        "a := 7 / 3; b := -7 / 3; c := MOD(7, 3); d := 7 MOD 0;",
        1,
    );
    assert_eq!(int(&rt, "R.P1.A"), 2);
    assert_eq!(int(&rt, "R.P1.B"), -2);
    assert_eq!(int(&rt, "R.P1.C"), 1);
    assert_eq!(int(&rt, "R.P1.D"), 0);
}

#[test]
fn statements_table_56() {
    let rt = run_prog(
        "VAR A, B, C, D, X1, X2 : REAL; NROOTS, TW, DISPLAY, J, I, SUM : INT;
             WORDS : ARRAY[1..100] OF STRING[10]; FLAG : BOOL; END_VAR",
        "A := 1.0; B := -3.0; C := 2.0;
         D := B*B - 4.0*A*C ;
         IF D < 0.0 THEN NROOTS := 0 ;
         ELSIF D = 0.0 THEN NROOTS := 1 ; X1 := - B/(2.0*A) ;
         ELSE NROOTS := 2 ; X1 := (- B + SQRT(D))/(2.0*A) ; X2 := (- B - SQRT(D))/(2.0*A) ;
         END_IF ;
         TW := 7;
         CASE TW OF
           1,5: DISPLAY := 10;
           2: DISPLAY := 20;
           4,6..10: DISPLAY := 40;
         ELSE DISPLAY := 0 ;
         END_CASE;
         WORDS[51] := 'KEY';
         J := 101 ;
         FOR I := 1 TO 100 BY 2 DO
           IF WORDS[I] = 'KEY' THEN J := I ; EXIT ; END_IF ;
         END_FOR ;
         SUM := 0 ;
         FOR I := 1 TO 3 DO
           FOR J := 1 TO 2 DO
             IF FLAG THEN EXIT ; END_IF
             SUM := SUM + J ;
           END_FOR ;
           SUM := SUM + I ;
         END_FOR ;",
        1,
    );
    assert_eq!(int(&rt, "R.P1.NROOTS"), 2);
    assert_eq!(real(&rt, "R.P1.X1"), 2.0);
    assert_eq!(real(&rt, "R.P1.X2"), 1.0);
    assert_eq!(int(&rt, "R.P1.DISPLAY"), 40);
    assert_eq!(int(&rt, "R.P1.SUM"), 15);
}

#[test]
fn while_repeat() {
    let rt = run_prog(
        "VAR J, K : INT; WORDS : ARRAY[1..101] OF STRING[10]; END_VAR",
        "WORDS[9] := 'KEY';
         J := 1;
         WHILE J <= 100 & WORDS[J] <> 'KEY' DO J := J+2 ; END_WHILE ;
         K := -1 ;
         REPEAT K := K+2 ; UNTIL K = 101 OR WORDS[K] = 'KEY' END_REPEAT ;",
        1,
    );
    assert_eq!(int(&rt, "R.P1.J"), 9);
    assert_eq!(int(&rt, "R.P1.K"), 9);
}

#[test]
fn strings_table_29() {
    let rt = run_prog(
        "VAR a : INT; s1, s2, s3, s4, s5, s6, s7 : STRING; f : INT; w : WSTRING; END_VAR",
        "a := LEN('ASTRING');
         s1 := LEFT(IN:='ASTR',L:=3);
         s2 := RIGHT(IN:='ASTR',L:=3);
         s3 := MID(IN:='ASTR',L:=2,P:=2);
         s4 := CONCAT('AB','CD','E');
         s5 := INSERT(IN1:='ABC',IN2:='XY',P:=2);
         s6 := DELETE(IN:='ABXYC',L:=2,P:=3);
         s7 := REPLACE(IN1:='ABCDE',IN2:='X',L:=2,P:=3);
         f := FIND(IN1:='ABCBC',IN2:='BC');
         w := \"$00C4$00CB\";",
        1,
    );
    assert_eq!(int(&rt, "R.P1.A"), 7);
    assert_eq!(string(&rt, "R.P1.S1"), "AST");
    assert_eq!(string(&rt, "R.P1.S2"), "STR");
    assert_eq!(string(&rt, "R.P1.S3"), "ST");
    assert_eq!(string(&rt, "R.P1.S4"), "ABCDE");
    assert_eq!(string(&rt, "R.P1.S5"), "ABXYC");
    assert_eq!(string(&rt, "R.P1.S6"), "ABC");
    assert_eq!(string(&rt, "R.P1.S7"), "ABXE");
    assert_eq!(int(&rt, "R.P1.F"), 2);
    assert_eq!(string(&rt, "R.P1.W"), "ÄË");
}

#[test]
fn string_comparison() {
    let rt = run_prog(
        "VAR a, b, c : BOOL; END_VAR",
        "a := 'Z' > 'AZ'; b := 'AZ' > 'ABC'; c := 'AB' = 'AB';",
        1,
    );
    assert!(boolean(&rt, "R.P1.A"));
    assert!(boolean(&rt, "R.P1.B"));
    assert!(boolean(&rt, "R.P1.C"));
}

#[test]
fn conversions_table_22() {
    let rt = run_prog(
        "VAR a, b, c, d, e, f, g, h : INT; t1, t2 : INT; bcd : BYTE; u : UINT; s : STRING; i2 : DINT; END_VAR",
        "a := REAL_TO_INT(1.6); b := REAL_TO_INT(-1.6); c := REAL_TO_INT(1.5); d := REAL_TO_INT(-1.5);
         e := REAL_TO_INT(1.4); f := REAL_TO_INT(2.5); g := REAL_TO_INT(-2.5); h := REAL_TO_INT(-1.4);
         t1 := TRUNC(1.6); t2 := TRUNC(-1.6);
         bcd := USINT_TO_BCD_BYTE(25); u := WORD_BCD_TO_UINT(2#0011_0110_1001);
         s := INT_TO_STRING(-42); i2 := STRING_TO_DINT('123');",
        1,
    );
    assert_eq!(int(&rt, "R.P1.A"), 2);
    assert_eq!(int(&rt, "R.P1.B"), -2);
    assert_eq!(int(&rt, "R.P1.C"), 2);
    assert_eq!(int(&rt, "R.P1.D"), -2);
    assert_eq!(int(&rt, "R.P1.E"), 1);
    assert_eq!(int(&rt, "R.P1.F"), 2);
    assert_eq!(int(&rt, "R.P1.G"), -2);
    assert_eq!(int(&rt, "R.P1.H"), -1);
    assert_eq!(int(&rt, "R.P1.T1"), 1);
    assert_eq!(int(&rt, "R.P1.T2"), -1);
    assert_eq!(int(&rt, "R.P1.BCD"), 0b0010_0101);
    assert_eq!(int(&rt, "R.P1.U"), 369);
    assert_eq!(string(&rt, "R.P1.S"), "-42");
    assert_eq!(int(&rt, "R.P1.I2"), 123);
}

#[test]
fn selection_and_comparison() {
    let rt = run_prog(
        "VAR a, b, c, d, e, m : INT; g1, g2 : BOOL; END_VAR",
        "a := SEL(G:=FALSE, IN0:=10, IN1:=5); b := MAX(3, 9, 4); c := MIN(3, 9, 4);
         d := LIMIT(IN:=7, MN:=0, MX:=5); e := MUX(0, 11, 22, 33); m := MUX(K:=3, IN0 := 1, IN2 := 2, IN4 := 3);
         g1 := GT(9, 5, 3); g2 := GT(9, 5, 6);",
        1,
    );
    assert_eq!(int(&rt, "R.P1.A"), 10);
    assert_eq!(int(&rt, "R.P1.B"), 9);
    assert_eq!(int(&rt, "R.P1.C"), 3);
    assert_eq!(int(&rt, "R.P1.D"), 5);
    assert_eq!(int(&rt, "R.P1.E"), 11);
    assert_eq!(int(&rt, "R.P1.M"), 0);
    assert!(boolean(&rt, "R.P1.G1"));
    assert!(!boolean(&rt, "R.P1.G2"));
}

#[test]
fn bit_shift_and_bitwise() {
    let rt = run_prog(
        "VAR a, b, c, d, e : WORD; x : BOOL; END_VAR",
        "a := SHL(IN := WORD#1, N := 4); b := SHR(WORD#16#8000, 15); c := ROL(WORD#16#8001, 1); d := ROR(WORD#16#8001, 1);
         e := WORD#16#FF00 AND WORD#16#0FF0 OR WORD#1; x := NOT FALSE XOR FALSE;",
        1,
    );
    assert_eq!(int(&rt, "R.P1.A"), 16);
    assert_eq!(int(&rt, "R.P1.B"), 1);
    assert_eq!(int(&rt, "R.P1.C"), 0x0003);
    assert_eq!(int(&rt, "R.P1.D"), 0xC000);
    assert_eq!(int(&rt, "R.P1.E"), 0x0F01);
    assert!(boolean(&rt, "R.P1.X"));
}

#[test]
fn time_arithmetic_table_30() {
    let rt = run_prog(
        "VAR t1, t2, t3 : TIME; dt1 : DT; d : DATE; tod1 : TOD; dt2: DT; r : REAL; END_VAR",
        "t1 := T#1h + T#30m; t2 := MULTIME(T#10s, 3); t3 := T#10s * 1.5;
         dt1 := DT#1986-04-28-08:40:00; d := DT_TO_DATE(dt1); tod1 := DT_TO_TOD(dt1);
         dt2 := CONCAT_DATE_TOD(D#2000-01-01, TOD#12:00:00);
         r := TIME_TO_REAL(T#1.5s);",
        1,
    );
    assert_eq!(rt.read("R.P1.T1").unwrap(), Value::Time(5_400_000_000));
    assert_eq!(rt.read("R.P1.T2").unwrap(), Value::Time(30_000_000));
    assert_eq!(rt.read("R.P1.T3").unwrap(), Value::Time(15_000_000));
    assert_eq!(rt.read("R.P1.D").unwrap().to_string(), "D#1986-04-28");
    assert_eq!(rt.read("R.P1.TOD1").unwrap().to_string(), "TOD#08:40:00");
    assert_eq!(rt.read("R.P1.DT2").unwrap().to_string(), "DT#2000-01-01-12:00:00");
    assert_eq!(real(&rt, "R.P1.R"), 1500.0);
}

#[test]
fn user_function_and_fb() {
    let src = r#"
FUNCTION SIMPLE_FUN : REAL
VAR_INPUT A, B : REAL ; C : REAL := 1.0; END_VAR
VAR_IN_OUT COUNT : INT ; END_VAR
VAR COUNTP1 : INT ; END_VAR
COUNTP1 := ADD(COUNT,1);
COUNT := COUNTP1 ;
SIMPLE_FUN := A*B/C;
END_FUNCTION

FUNCTION_BLOCK DEBOUNCE
VAR_INPUT IN : BOOL ; DB_TIME : TIME := t#10ms ; END_VAR
VAR_OUTPUT OUT : BOOL ; ET_OFF : TIME ; END_VAR
VAR DB_ON : TON ; DB_OFF : TON ; DB_FF : SR ; END_VAR
DB_ON(IN := IN, PT := DB_TIME) ;
DB_OFF(IN := NOT IN, PT:=DB_TIME) ;
DB_FF(S1 :=DB_ON.Q, R := DB_OFF.Q) ;
OUT := DB_FF.Q1 ;
ET_OFF := DB_OFF.ET ;
END_FUNCTION_BLOCK

PROGRAM P
VAR X,Y,Z,RESULT, R2 : REAL; COUNT1,COUNT2 : INT; D : DEBOUNCE; I : BOOL; O : BOOL; END_VAR
X := 2.0; Y := 3.0; Z := 4.0;
RESULT := SIMPLE_FUN(A:=X,B:=Y,C:=Z,COUNT:=COUNT1);
R2 := SIMPLE_FUN(A:=X,B:=Y,COUNT:=COUNT1);
COUNT2 := COUNT1;
D(IN := I, DB_TIME := T#30ms);
O := D.OUT;
END_PROGRAM

CONFIGURATION C RESOURCE R ON VM TASK T(INTERVAL := T#10ms, PRIORITY := 1); PROGRAM P1 WITH T : P; END_RESOURCE END_CONFIGURATION
"#;
    let mut rt = build(src);
    rt.run_for(10_000);
    assert_eq!(real(&rt, "R.P1.RESULT"), 1.5);
    assert_eq!(real(&rt, "R.P1.R2"), 6.0);
    assert_eq!(int(&rt, "R.P1.COUNT2"), 2);
    rt.write("R.P1.I", &Value::Bool(true)).unwrap();
    rt.run_for(20_000);
    assert!(!boolean(&rt, "R.P1.O"));
    rt.run_for(30_000);
    assert!(boolean(&rt, "R.P1.O"));
}

#[test]
fn timers_ton_tof_tp() {
    let src = r#"
PROGRAM P
VAR IN : BOOL; T1 : TON; T2 : TOF; T3 : TP; Q1, Q2, Q3 : BOOL; E1 : TIME; END_VAR
T1(IN := IN, PT := T#50ms); Q1 := T1.Q; E1 := T1.ET;
T2(IN := IN, PT := T#50ms); Q2 := T2.Q;
T3(IN := IN, PT := T#50ms); Q3 := T3.Q;
END_PROGRAM
CONFIGURATION C RESOURCE R ON VM TASK T(INTERVAL := T#10ms, PRIORITY := 1); PROGRAM P1 WITH T : P; END_RESOURCE END_CONFIGURATION
"#;
    let mut rt = build(src);
    rt.run_for(5_000);
    rt.write("R.P1.IN", &Value::Bool(true)).unwrap();
    rt.run_for(10_000);
    assert!(!boolean(&rt, "R.P1.Q1"));
    assert!(boolean(&rt, "R.P1.Q2"));
    assert!(boolean(&rt, "R.P1.Q3"));
    rt.run_for(60_000);
    assert!(boolean(&rt, "R.P1.Q1"));
    assert_eq!(rt.read("R.P1.E1").unwrap(), Value::Time(50_000));
    assert!(!boolean(&rt, "R.P1.Q3"));
    rt.write("R.P1.IN", &Value::Bool(false)).unwrap();
    rt.run_for(20_000);
    assert!(!boolean(&rt, "R.P1.Q1"));
    assert!(boolean(&rt, "R.P1.Q2"));
    rt.run_for(60_000);
    assert!(!boolean(&rt, "R.P1.Q2"));
}

#[test]
fn counters_and_edges() {
    let src = r#"
FUNCTION_BLOCK AND_EDGE
VAR_INPUT X : BOOL R_EDGE; Y : BOOL F_EDGE; END_VAR
VAR_OUTPUT Z : BOOL ; END_VAR
Z := X AND Y ;
END_FUNCTION_BLOCK
PROGRAM P
VAR CU : BOOL; C : CTU; N : INT; D : CTD; CD : BOOL; E : AND_EDGE; X, Y, Z : BOOL; ZC : INT; END_VAR
C(CU := CU, PV := 3); N := C.CV;
D(CD := CD, LD := N = 0, PV := 5);
E(X := X, Y := Y); IF E.Z THEN ZC := ZC + 1; END_IF;
END_PROGRAM
CONFIGURATION C RESOURCE R ON VM TASK T(INTERVAL := T#10ms, PRIORITY := 1); PROGRAM P1 WITH T : P; END_RESOURCE END_CONFIGURATION
"#;
    let mut rt = build(src);
    for _ in 0..4 {
        rt.write("R.P1.CU", &Value::Bool(true)).unwrap();
        rt.run_for(10_000);
        rt.write("R.P1.CU", &Value::Bool(false)).unwrap();
        rt.run_for(10_000);
    }
    assert_eq!(int(&rt, "R.P1.N"), 4);
    assert!(boolean(&rt, "R.P1.C.Q"));
    // rising X and falling Y in the same scan
    rt.write("R.P1.Y", &Value::Bool(true)).unwrap();
    rt.run_for(10_000);
    rt.write("R.P1.X", &Value::Bool(true)).unwrap();
    rt.write("R.P1.Y", &Value::Bool(false)).unwrap();
    rt.run_for(10_000);
    assert_eq!(int(&rt, "R.P1.ZC"), 1);
    rt.run_for(30_000);
    assert_eq!(int(&rt, "R.P1.ZC"), 1);
}

#[test]
fn derived_types_and_initialisation() {
    let src = r#"
TYPE ANALOG_SIGNAL_TYPE : (SINGLE_ENDED, DIFFERENTIAL) ; END_TYPE
TYPE ANALOG_SIGNAL_RANGE :
  (BIPOLAR_10V, UNIPOLAR_10V, UNIPOLAR_1_5V, UNIPOLAR_0_5V, UNIPOLAR_4_20_MA, UNIPOLAR_0_20_MA) := UNIPOLAR_1_5V ;
END_TYPE
TYPE ANALOG_DATA : INT (-4095..4095) ; END_TYPE
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
TYPE ANALOG_16_INPUT_DATAI : ARRAY [1..16] OF ANALOG_DATA := [8(-4095), 8(4095)] ; END_TYPE
TYPE STR10 : STRING[10] := 'ABCDEF'; END_TYPE
TYPE FREQ : REAL := 50.0 ; END_TYPE
PROGRAM P
VAR
  CONF : ANALOG_16_INPUT_CONFIGURATION;
  M8 : ANALOG_16_INPUT_CONFIGURATION := (SIGNAL_TYPE := DIFFERENTIAL,
        CHANNEL := [4((RANGE := UNIPOLAR_1_5V)), (RANGE:= BIPOLAR_10V, MIN_SCALE := 0, MAX_SCALE := 500)]);
  INS : ANALOG_16_INPUT_DATAI;
  S : STR10;
  F : FREQ;
  TBT : ARRAY [1..2,1..3] OF INT := [1,2,3(4),6];
  BITS : ARRAY[0..7] OF BOOL := [1,1,0,0,0,1,0,0];
  V : INT;
  E : ANALOG_SIGNAL_TYPE;
END_VAR
CONF.CHANNEL[5].RANGE := BIPOLAR_10V;
V := CONF.CHANNEL[2].MIN_SCALE + INS[16];
E := ANALOG_SIGNAL_TYPE#DIFFERENTIAL;
END_PROGRAM
CONFIGURATION C RESOURCE R ON VM TASK T(INTERVAL := T#10ms, PRIORITY := 1); PROGRAM P1 WITH T : P; END_RESOURCE END_CONFIGURATION
"#;
    let mut rt = build(src);
    rt.run_for(10_000);
    assert_eq!(rt.read("R.P1.CONF.CHANNEL[1].RANGE").unwrap(), Value::Enum("UNIPOLAR_1_5V".into()));
    assert_eq!(rt.read("R.P1.CONF.CHANNEL[5].RANGE").unwrap(), Value::Enum("BIPOLAR_10V".into()));
    assert_eq!(int(&rt, "R.P1.CONF.CHANNEL[3].MIN_SCALE"), -4095);
    assert_eq!(rt.read("R.P1.M8.SIGNAL_TYPE").unwrap(), Value::Enum("DIFFERENTIAL".into()));
    assert_eq!(int(&rt, "R.P1.M8.CHANNEL[5].MAX_SCALE"), 500);
    assert_eq!(int(&rt, "R.P1.M8.CHANNEL[6].MAX_SCALE"), -4095);
    assert_eq!(int(&rt, "R.P1.INS[1]"), -4095);
    assert_eq!(int(&rt, "R.P1.INS[16]"), 4095);
    assert_eq!(string(&rt, "R.P1.S"), "ABCDEF");
    assert_eq!(real(&rt, "R.P1.F"), 50.0);
    assert_eq!(int(&rt, "R.P1.TBT[1,3]"), 4);
    assert_eq!(int(&rt, "R.P1.TBT[2,1]"), 4);
    assert_eq!(int(&rt, "R.P1.TBT[2,3]"), 6);
    assert!(boolean(&rt, "R.P1.BITS[5]"));
    assert!(!boolean(&rt, "R.P1.BITS[4]"));
    assert_eq!(int(&rt, "R.P1.V"), -4095 + 4095);
    assert_eq!(rt.read("R.P1.E").unwrap(), Value::Enum("DIFFERENTIAL".into()));
}

#[test]
fn subrange_violation_is_runtime_error() {
    let mut rt = run_prog("VAR X : INT (0..10); I : INT; END_VAR", "I := I + 6; X := I;", 1);
    assert!(rt.errors.is_empty());
    rt.run_for(10_000);
    assert_eq!(rt.errors.len(), 1);
    assert_eq!(rt.errors[0].code, structure_runtime::TrapCode::SubrangeViolation);
}

#[test]
fn division_by_zero_and_overflow() {
    let rt = run_prog("VAR A, B : INT; END_VAR", "A := 1 / B;", 1);
    assert_eq!(rt.errors[0].code, structure_runtime::TrapCode::DivisionByZero);
    let rt = run_prog("VAR A : SINT := 120; END_VAR", "A := A + 10;", 1);
    assert_eq!(rt.errors[0].code, structure_runtime::TrapCode::Overflow);
}

#[test]
fn eno_reset_on_error() {
    let rt = run_prog(
        "VAR A, B, C : INT; OK : BOOL := TRUE; OK2 : BOOL; END_VAR",
        "C := DIV(EN := TRUE, IN1 := 10, IN2 := B, ENO => OK); A := ADD(EN := TRUE, IN1 := 1, IN2 := 2, ENO => OK2);",
        1,
    );
    assert!(rt.errors.is_empty(), "{:?}", rt.errors);
    assert!(!boolean(&rt, "R.P1.OK"));
    assert!(boolean(&rt, "R.P1.OK2"));
    assert_eq!(int(&rt, "R.P1.A"), 3);
}

#[test]
fn globals_externals_and_access_paths() {
    let src = r#"
PROGRAM F
VAR_INPUT x1 : BOOL ; x2 : UINT ; END_VAR
VAR_OUTPUT y1 : BYTE ; END_VAR
VAR_EXTERNAL w : UINT; END_VAR
VAR COUNT : INT; END_VAR
COUNT := COUNT + 1;
w := w + x2;
IF x1 THEN y1 := 7; END_IF;
END_PROGRAM
CONFIGURATION CELL_1
  VAR_GLOBAL w : UINT; END_VAR
  RESOURCE STATION_1 ON PROCESSOR_TYPE_1
    VAR_GLOBAL z1 : BYTE; END_VAR
    TASK SLOW_1(INTERVAL := t#20ms, PRIORITY := 2) ;
    PROGRAM P1 WITH SLOW_1 : F(x1 := %IX1.1, x2 := UINT#2, y1 => z1) ;
  END_RESOURCE
  VAR_ACCESS
    ABLE  : STATION_1.%IX1.1 : BOOL READ_ONLY ;
    BAKER : STATION_1.P1.x2 : UINT READ_WRITE ;
    CHARLIE : STATION_1.z1 : BYTE ;
    DOG : w : UINT READ_ONLY ;
    S1_COUNT : STATION_1.P1.COUNT : INT;
  END_VAR
  VAR_CONFIG
    STATION_1.P1.COUNT : INT := 100;
  END_VAR
END_CONFIGURATION
"#;
    let mut rt = build(src);
    assert_eq!(int(&rt, "STATION_1.P1.COUNT"), 100);
    rt.run_for(100_000);
    assert_eq!(int(&rt, "STATION_1.P1.COUNT"), 105);
    assert_eq!(int(&rt, "W"), 10);
    assert_eq!(rt.read_access("DOG").unwrap(), Value::Int(10));
    assert_eq!(rt.read_access("S1_COUNT").unwrap(), Value::Int(105));
    assert_eq!(rt.read_access("CHARLIE").unwrap(), Value::Int(0));
    assert!(rt.write_access("DOG", &Value::Int(1)).is_err());
    // drive the input bit
    rt.io(0, 'I')[1] |= 2;
    rt.run_for(20_000);
    assert_eq!(rt.read_access("ABLE").unwrap(), Value::Bool(true));
    assert_eq!(rt.read_access("CHARLIE").unwrap(), Value::Int(7));
}

#[test]
fn retain_warm_restart() {
    let src = prog("VAR RETAIN R1 : INT; END_VAR VAR N1 : INT := 5; END_VAR", "R1 := R1 + 1; N1 := N1 + 1;");
    let mut rt = build(&src);
    rt.run_for(50_000);
    assert_eq!(int(&rt, "R.P1.R1"), 5);
    let snap = rt.retain_snapshot();
    rt.warm_start(&snap).unwrap();
    assert_eq!(int(&rt, "R.P1.R1"), 5);
    assert_eq!(int(&rt, "R.P1.N1"), 5);
    rt.cold_start().unwrap();
    assert_eq!(int(&rt, "R.P1.R1"), 0);
}
