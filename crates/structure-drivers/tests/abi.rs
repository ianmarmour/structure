//! Drivers behind the C interface: a C library loaded at run time, and a
//! Rust driver exported with `export_driver!`.
use serde_json::{json, Value as Json};
use std::path::PathBuf;
use std::process::Command;
use structure_runtime::driver::ffi::DynamicDriver;
use structure_runtime::driver::{Binding, Direction, IoDriver};
use structure_runtime::{DriverConfig, Options, Runtime, Value};

const PROGRAM: &str = r#"
PROGRAM P
VAR N AT %IW1.0 : INT; NEXT AT %QW1.0 : INT; END_VAR
NEXT := N + 1;
END_PROGRAM
CONFIGURATION C RESOURCE R ON VM TASK T(INTERVAL := T#10ms, PRIORITY := 1); PROGRAM P1 WITH T : P; END_RESOURCE END_CONFIGURATION
"#;

fn runtime() -> Runtime {
    let (sm, out) = structure_compiler::compile_str("p.st", PROGRAM, structure_compiler::Mode::Build);
    let image = out.image.unwrap_or_else(|| panic!("{}", out.diagnostics.render(&sm)));
    Runtime::new(image, Options::default())
}

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

#[test]
fn c_driver_library() {
    let out = std::env::temp_dir().join(format!("structure-c-driver-{}", std::process::id()));
    std::fs::create_dir_all(&out).unwrap();
    let lib = out.join(if cfg!(target_os = "macos") { "libloopback.dylib" } else { "libloopback.so" });
    let include = root().join("crates/structure-runtime/include");
    let source = root().join("examples/drivers/c/loopback.c");
    let status = match Command::new("cc").args(["-shared", "-fPIC", "-std=c99", "-Wall", "-Wextra", "-Werror", "-I"]).arg(&include).arg(&source).arg("-o").arg(&lib).status() {
        Ok(s) => s,
        Err(_) => {
            eprintln!("no C compiler: skipped");
            return;
        }
    };
    assert!(status.success(), "the C example does not compile");
    // the header is also valid C++
    let cpp = Command::new("c++").args(["-fsyntax-only", "-x", "c++", "-I"]).arg(&include).arg("-").stdin(std::process::Stdio::piped()).spawn();
    if let Ok(mut child) = cpp {
        use std::io::Write;
        child.stdin.take().unwrap().write_all(b"#include \"structure_driver.h\"\nint main() { return structure_driver_api_v1 == nullptr; }\n").unwrap();
        assert!(child.wait().unwrap().success(), "structure_driver.h does not compile as C++");
    }

    let mut rt = runtime();
    let config = format!(
        r#"{{"drivers": [{{"name": "c", "library": {}, "bindings": [{{"area": "%I1"}}, {{"area": "%Q1"}}]}}]}}"#,
        serde_json::to_string(&lib.display().to_string()).unwrap()
    );
    rt.load_drivers(&DriverConfig::from_json(&config).unwrap(), &structure_drivers::registry()).unwrap();
    rt.cold_start().unwrap();
    rt.run_for(50_000);
    // each cycle reads back the output of the previous one
    assert_eq!(rt.read("R.P1.NEXT").unwrap(), Value::Int(5));
    assert!(rt.drivers()[0].healthy);

    let bad = DriverConfig::from_json(r#"{"drivers": [{"name": "x", "library": "/nonexistent/libx.so", "bindings": []}]}"#).unwrap();
    assert!(runtime().load_drivers(&bad, &structure_drivers::registry()).unwrap_err().contains("cannot load"));
    let _ = std::fs::remove_dir_all(&out);
}

// ------------------------------------------------------------ Rust driver exported with the C interface

/// Counts the cycles; fails or panics on request.
struct Counter {
    n: u16,
    panic_at: Option<u16>,
    bindings: Vec<Binding>,
}

impl IoDriver for Counter {
    fn open(&mut self, bindings: &[Binding]) -> Result<(), String> {
        if bindings.iter().any(|b| b.params.get("reject").is_some()) {
            return Err("rejected binding".into());
        }
        self.bindings = bindings.to_vec();
        Ok(())
    }
    fn read_inputs(&mut self, data: &mut [&mut [u8]]) -> Result<(), String> {
        self.n += 1;
        if Some(self.n) == self.panic_at {
            panic!("boom");
        }
        for (b, d) in self.bindings.iter().zip(data.iter_mut()) {
            if b.direction == Direction::Input {
                structure_runtime::driver::image::set_word(d, 0, self.n * 100);
            }
        }
        Ok(())
    }
    fn write_outputs(&mut self, _data: &[&[u8]]) -> Result<(), String> {
        Ok(())
    }
}

fn create(params: &Json) -> Result<Box<dyn IoDriver>, String> {
    if params.get("fail").is_some() {
        return Err("invalid parameters".into());
    }
    Ok(Box::new(Counter { n: 0, panic_at: params.get("panic_at").and_then(|v| v.as_u64()).map(|v| v as u16), bindings: vec![] }))
}

structure_runtime::export_driver!(create);

fn exported(params: Json) -> Result<DynamicDriver, String> {
    // SAFETY: the table is a static of this program.
    unsafe { DynamicDriver::from_api(structure_driver_api_v1(), &params, None) }
}

#[test]
fn rust_driver_through_the_c_interface() {
    let mut rt = runtime();
    let spec = DriverConfig::from_json(r#"{"drivers": [{"name": "rs", "bindings": [{"area": "%I1"}, {"area": "%Q1"}]}]}"#).unwrap().drivers.remove(0);
    rt.add_driver(&spec, Box::new(exported(json!({})).unwrap())).unwrap();
    rt.cold_start().unwrap();
    rt.run_for(30_000);
    assert_eq!(rt.read("R.P1.NEXT").unwrap(), Value::Int(301));

    // errors cross the interface as messages
    assert_eq!(exported(json!({"fail": true})).err().unwrap(), "invalid parameters");
    let mut d = exported(json!({})).unwrap();
    let b = Binding { area: "%I".into(), direction: Direction::Input, path: vec![], offset: 0, size: 2, params: json!({"reject": 1}) };
    assert_eq!(d.open(&[b]).unwrap_err(), "rejected binding");

    // a panic in the driver is contained and reported as a fault
    let mut rt = runtime();
    rt.add_driver(&spec, Box::new(exported(json!({"panic_at": 2})).unwrap())).unwrap();
    rt.cold_start().unwrap();
    rt.run_for(30_000);
    let status = &rt.drivers()[0];
    assert!(rt.driver_events.iter().any(|e| e.message == "the driver panicked"), "{:?}", rt.driver_events);
    assert!(status.healthy, "it recovers on the next cycle");
}
