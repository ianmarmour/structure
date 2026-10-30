mod common;
use common::*;

#[test]
fn counter_program() {
    let rt = run_prog("VAR X : INT; END_VAR", "X := X + 1;", 10);
    assert_eq!(int(&rt, "R.P1.X"), 10);
}

#[test]
fn arithmetic_precedence() {
    let rt = run_prog(
        "VAR A, B, C, D, R1, R2 : INT; END_VAR",
        "A := 1; B := 2; C := 3; D := 4; R1 := A+B-C*ABS(D); R2 := (A+B-C)*ABS(D);",
        1,
    );
    assert_eq!(int(&rt, "R.P1.R1"), -9);
    assert_eq!(int(&rt, "R.P1.R2"), 0);
}
