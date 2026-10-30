//! Error conditions of annex E detected while preparing the program for
//! execution (1.5.1 d 3) or during execution (1.5.1 d 4).
mod common;
use common::*;
use structure_runtime::TrapCode;

const CFG: &str = "CONFIGURATION C RESOURCE R ON VM TASK T(INTERVAL := T#10ms, PRIORITY := 1); PROGRAM P1 WITH T : P; END_RESOURCE END_CONFIGURATION";

fn err(src: &str, needle: &str) {
    let e = errors(&format!("{}\n{}", src, CFG));
    assert!(e.contains(needle), "expected '{}' in:\n{}", needle, e);
}

fn trap(decls: &str, body: &str, code: TrapCode) {
    let rt = run_prog(decls, body, 1);
    assert_eq!(rt.errors.first().map(|e| e.code), Some(code), "{:?}", rt.errors);
}

#[test]
fn e_2_1_5_nested_comments() {
    err("PROGRAM P (* a (* nested *) comment *) END_PROGRAM", "nested comments");
}

#[test]
fn e_2_3_3_1_ambiguous_enumerated_value() {
    err("TYPE A : (X, Y); B : (Y, Z); END_TYPE PROGRAM P VAR I : INT; END_VAR IF Y = Y THEN I := 1; END_IF; END_PROGRAM", "ambiguous enumerated value");
}

#[test]
fn e_2_3_3_1_subrange() {
    err("PROGRAM P VAR X : INT (0..10) := 11; END_VAR END_PROGRAM", "subrange");
    err("PROGRAM P VAR X : INT (0..10); END_VAR X := 20; END_PROGRAM", "subrange");
    trap("VAR X : INT (0..10); Y : INT := 11; END_VAR", "X := Y;", TrapCode::SubrangeViolation);
}

#[test]
fn e_2_4_1_1_missing_var_config() {
    let e = errors("PROGRAM P VAR C2 AT %Q* : BYTE; END_VAR C2 := 1; END_PROGRAM\nCONFIGURATION C RESOURCE R ON VM TASK T(INTERVAL := T#10ms, PRIORITY := 1); PROGRAM P1 WITH T : P; END_RESOURCE END_CONFIGURATION");
    assert!(e.contains("missing VAR_CONFIG"), "{}", e);
}

#[test]
fn e_2_4_3_constant_modification() {
    err("PROGRAM P VAR CONSTANT PI : REAL := 3.14; END_VAR PI := 3.0; END_PROGRAM", "CONSTANT");
}

#[test]
fn e_2_5_1_external_and_direct_variables_in_functions() {
    err("FUNCTION F : INT VAR_INPUT A : INT; END_VAR VAR_EXTERNAL G : INT; END_VAR F := A; END_FUNCTION", "functions");
    err("FUNCTION F : INT VAR_INPUT A : INT; END_VAR F := %IW1; END_FUNCTION", "functions");
}

#[test]
fn e_2_5_1_1_in_out_not_properly_mapped() {
    let fb = "FUNCTION_BLOCK ACC VAR_IN_OUT A : INT; END_VAR VAR_INPUT X : INT; END_VAR A := A + X; END_FUNCTION_BLOCK\n";
    err(&format!("{}PROGRAM P VAR F : ACC; I : INT; END_VAR F(X := 1); END_PROGRAM", fb), "no value specified");
    err(&format!("{}PROGRAM P VAR F : ACC; I : INT; END_VAR F(A := I + 1, X := 1); END_PROGRAM", fb), "not properly mapped");
}

#[test]
fn e_2_5_1_5_runtime_errors() {
    trap("VAR I : INT := 300; S : SINT; END_VAR", "S := INT_TO_SINT(I);", TrapCode::ConversionError);
    trap("VAR A : DINT := 2147483647; END_VAR", "A := A + 1;", TrapCode::Overflow);
    trap("VAR R : REAL; END_VAR", "R := 1.0 / R;", TrapCode::DivisionByZero);
    trap("VAR W : WORD; N : INT := -1; END_VAR", "W := SHL(W, N);", TrapCode::NegativeShift);
    trap("VAR K : INT := 5; X : INT; END_VAR", "X := MUX(K, 1, 2, 3);", TrapCode::MuxSelectorOutOfRange);
    trap("VAR S : STRING; END_VAR", "S := MID(IN := 'ABC', L := 2, P := 3);", TrapCode::InvalidCharacterPosition);
    trap("VAR S : STRING; N : INT := -1; END_VAR", "S := LEFT('ABC', N);", TrapCode::NegativeStringArgument);
    trap("VAR T : TOD := TOD#23:00:00; END_VAR", "T := ADD_TOD_TIME(T, T#2h);", TrapCode::TimeRange);
    trap("VAR S : STRING; X : INT; END_VAR", "S := 'X1'; X := STRING_TO_INT(S);", TrapCode::ConversionError);
}

#[test]
fn e_2_5_1_5_4_mixed_selection_types() {
    err("PROGRAM P VAR A : INT; B : TIME; C : INT; END_VAR C := MAX(A, B); END_PROGRAM", "same data type");
}

#[test]
fn e_2_5_2_2_no_value_for_instance_input() {
    let src = "FUNCTION_BLOCK INSIDE VAR_INPUT I_TMR : TON; END_VAR VAR_OUTPUT E : BOOL; END_VAR E := I_TMR.Q; END_FUNCTION_BLOCK\n\
               PROGRAM P VAR B : INSIDE; END_VAR B(); END_PROGRAM\n";
    let rt = run_prog_raw(src);
    assert_eq!(rt.errors.first().map(|e| e.code), Some(TrapCode::NoValueSpecified));
}

fn run_prog_raw(src: &str) -> structure_runtime::Runtime {
    let mut rt = build(&format!("{}\n{}", src, CFG));
    rt.run_for(10_000);
    rt
}

#[test]
fn e_3_3_1_invalid_data_type() {
    err("PROGRAM P VAR A : BOOL; B : INT; END_VAR B := A + B; END_PROGRAM", "");
    err("PROGRAM P VAR A : INT; B : STRING; END_VAR A := B; END_PROGRAM", "type mismatch");
    err("PROGRAM P VAR A : INT; END_VAR A := 7 / 0; END_PROGRAM", "division by zero");
}

#[test]
fn e_3_3_2_1_return_without_value() {
    let src = "FUNCTION F : INT VAR_INPUT A : BOOL; END_VAR IF A THEN F := 1; END_IF; END_FUNCTION\n\
               PROGRAM P VAR X : INT; END_VAR X := F(FALSE); END_PROGRAM\n";
    let rt = run_prog_raw(src);
    assert_eq!(rt.errors.first().map(|e| e.code), Some(TrapCode::NoValueAssigned));
}

#[test]
fn e_3_3_2_4_iteration_fails_to_terminate() {
    let mut rt = build(&prog("VAR X : INT; END_VAR", "WHILE TRUE DO X := 1; END_WHILE;"));
    rt.options.watchdog = 100_000;
    rt.run_for(10_000);
    assert_eq!(rt.errors.first().map(|e| e.code), Some(TrapCode::Watchdog));
}

#[test]
fn recursion_is_rejected() {
    err("FUNCTION F : INT VAR_INPUT A : INT; END_VAR F := G(A); END_FUNCTION FUNCTION G : INT VAR_INPUT A : INT; END_VAR G := F(A); END_FUNCTION PROGRAM P VAR X : INT; END_VAR X := F(1); END_PROGRAM", "recursive");
}

#[test]
fn keywords_and_reserved_names() {
    err("PROGRAM P VAR IF : INT; END_VAR END_PROGRAM", "keyword");
    err("PROGRAM P VAR TON : INT; END_VAR END_PROGRAM", "");
    err("FUNCTION_BLOCK TON VAR_INPUT IN : BOOL; END_VAR END_FUNCTION_BLOCK", "standard function block");
    err("PROGRAM P VAR A__B : INT; END_VAR END_PROGRAM", "underline");
}

#[test]
fn function_block_rules() {
    // assignment to an output from outside (table 32)
    err("PROGRAM P VAR T1 : TON; END_VAR T1.Q := TRUE; END_PROGRAM", "can only be assigned from within");
    // assignment to an input outside an invocation
    err("PROGRAM P VAR T1 : TON; END_VAR T1.IN := TRUE; END_PROGRAM", "part of its invocation");
    // internal variables are hidden
    err("FUNCTION_BLOCK F VAR X : INT; END_VAR END_FUNCTION_BLOCK PROGRAM P VAR A : F; I : INT; END_VAR I := A.X; END_PROGRAM", "not accessible");
    // inputs cannot be modified inside the POU
    err("FUNCTION_BLOCK F VAR_INPUT X : INT; END_VAR X := 1; END_FUNCTION_BLOCK", "cannot be modified");
}
