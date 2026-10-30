#![allow(dead_code)]
use structure_compiler::{compile_str, Mode};
use structure_runtime::{Options, Runtime, Value};

pub fn build(src: &str) -> Runtime {
    let (sm, out) = compile_str("test.st", src, Mode::Build);
    if out.image.is_none() || out.diagnostics.has_errors() {
        panic!("compilation failed:\n{}", out.diagnostics.render(&sm));
    }
    let mut rt = Runtime::new(out.image.unwrap(), Options::default());
    rt.cold_start().expect("cold start");
    rt
}

pub fn errors(src: &str) -> String {
    let (sm, out) = compile_str("test.st", src, Mode::Build);
    assert!(out.diagnostics.has_errors(), "expected compilation errors");
    out.diagnostics.render(&sm)
}

pub fn warnings(src: &str) -> String {
    let (sm, out) = compile_str("test.st", src, Mode::Build);
    out.diagnostics.render(&sm)
}

/// Wraps a program body in a configuration with a 10 ms task.
pub fn prog(decls: &str, body: &str) -> String {
    format!(
        "PROGRAM P\n{decls}\n{body}\nEND_PROGRAM\nCONFIGURATION C RESOURCE R ON VM TASK T(INTERVAL := T#10ms, PRIORITY := 1); PROGRAM P1 WITH T : P; END_RESOURCE END_CONFIGURATION\n"
    )
}

pub fn run_prog(decls: &str, body: &str, scans: usize) -> Runtime {
    let mut rt = build(&prog(decls, body));
    rt.run_for(10_000 * scans as i64);
    rt
}

pub fn int(rt: &Runtime, path: &str) -> i128 {
    match rt.read(path) {
        Ok(Value::Int(i)) => i,
        Ok(v) => panic!("{} is not an integer: {:?}", path, v),
        Err(e) => panic!("{}: {}", path, e),
    }
}

pub fn real(rt: &Runtime, path: &str) -> f64 {
    match rt.read(path) {
        Ok(Value::Real(f)) => f,
        Ok(v) => panic!("{} is not a real: {:?}", path, v),
        Err(e) => panic!("{}: {}", path, e),
    }
}

pub fn boolean(rt: &Runtime, path: &str) -> bool {
    match rt.read(path) {
        Ok(Value::Bool(b)) => b,
        Ok(v) => panic!("{} is not a BOOL: {:?}", path, v),
        Err(e) => panic!("{}: {}", path, e),
    }
}

pub fn string(rt: &Runtime, path: &str) -> String {
    match rt.read(path) {
        Ok(Value::String(s)) | Ok(Value::WString(s)) => s,
        Ok(v) => panic!("{} is not a string: {:?}", path, v),
        Err(e) => panic!("{}: {}", path, e),
    }
}
