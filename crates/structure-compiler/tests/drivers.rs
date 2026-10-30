//! I/O drivers attached to the runtime: exchange of the process image at
//! task boundaries, bindings, status and fault handling.
use std::sync::{Arc, Mutex};
use structure_compiler::{compile_str, Mode};
use structure_runtime::driver::{Binding, Direction, DriverConfig, DriverSpec, IoDriver};
use structure_runtime::{Options, Runtime, Value};

#[derive(Default)]
struct Log {
    calls: Vec<String>,
    bindings: Vec<Binding>,
    outputs: Vec<Vec<u8>>,
    /// Fail the exchanges after this many successful reads.
    fail_after: Option<usize>,
    reads: usize,
}

/// A driver that sets input word 0 to the number of reads, and records the
/// outputs.
struct Mock(Arc<Mutex<Log>>);

impl IoDriver for Mock {
    fn open(&mut self, bindings: &[Binding]) -> Result<(), String> {
        let mut l = self.0.lock().unwrap();
        l.calls.push("open".into());
        l.bindings = bindings.to_vec();
        Ok(())
    }
    fn read_inputs(&mut self, data: &mut [&mut [u8]]) -> Result<(), String> {
        let mut l = self.0.lock().unwrap();
        if l.fail_after.map(|n| l.reads >= n).unwrap_or(false) {
            return Err("device unreachable".into());
        }
        l.reads += 1;
        let n = l.reads as u16;
        l.calls.push(format!("in {}", n));
        for (b, d) in l.bindings.clone().iter().zip(data.iter_mut()) {
            if b.direction == Direction::Input && d.len() >= 2 {
                d[..2].copy_from_slice(&n.to_le_bytes());
            }
        }
        Ok(())
    }
    fn write_outputs(&mut self, data: &[&[u8]]) -> Result<(), String> {
        let mut l = self.0.lock().unwrap();
        l.calls.push("out".into());
        l.outputs = data.iter().map(|d| d.to_vec()).collect();
        Ok(())
    }
    fn stop(&mut self) {
        self.0.lock().unwrap().calls.push("stop".into());
    }
    fn close(&mut self) {
        self.0.lock().unwrap().calls.push("close".into());
    }
}

const PROGRAM: &str = r#"
PROGRAM P
VAR IN0 AT %IW1.0 : INT; OUT0 AT %QW1.0 : INT; FLAG AT %QX0.3 : BOOL; RUNS : INT; END_VAR
OUT0 := IN0 * 10;
FLAG := TRUE;
RUNS := RUNS + 1;
END_PROGRAM
PROGRAM S VAR RUNS : DINT; END_VAR RUNS := RUNS + 1; END_PROGRAM
"#;

fn runtime(config: &str) -> Runtime {
    let src = format!("{}{}", PROGRAM, config);
    let (sm, out) = compile_str("drivers.st", &src, Mode::Build);
    let image = out.image.unwrap_or_else(|| panic!("{}", out.diagnostics.render(&sm)));
    Runtime::new(image, Options { report_overruns: false, ..Options::default() })
}

const ONE_TASK: &str = "CONFIGURATION C VAR_GLOBAL OK : BOOL; END_VAR RESOURCE R ON VM TASK T(INTERVAL := T#10ms, PRIORITY := 1); PROGRAM P1 WITH T : P; END_RESOURCE END_CONFIGURATION";

fn spec(json: &str) -> DriverSpec {
    DriverConfig::from_json(&format!(r#"{{"drivers": [{}]}}"#, json)).unwrap().drivers.remove(0)
}

fn attach(rt: &mut Runtime, json: &str) -> Arc<Mutex<Log>> {
    let log = Arc::new(Mutex::new(Log::default()));
    rt.add_driver(&spec(json), Box::new(Mock(log.clone()))).unwrap();
    log
}

#[test]
fn inputs_before_and_outputs_after_each_task_execution() {
    let mut rt = runtime(ONE_TASK);
    let log = attach(&mut rt, r#"{"name": "io", "type": "mock", "bindings": [{"area": "%I1"}, {"area": "%Q1"}, {"area": "%Q", "size": 1}]}"#);
    rt.cold_start().unwrap();
    rt.run_for(30_000);
    let l = log.lock().unwrap();
    assert_eq!(l.calls, ["open", "in 1", "out", "in 2", "out", "in 3", "out"]);
    // the outputs of a cycle are computed from the inputs of the same cycle
    assert_eq!(&l.outputs[1][..2], &30i16.to_le_bytes());
    assert_eq!(l.outputs[0].len(), 0, "input bindings have empty output buffers");
    assert_eq!(l.outputs[2], [0b1000], "%QX0.3");
    // bindings: module areas, explicit sizes
    assert_eq!(l.bindings[0].path, [1]);
    assert_eq!(l.bindings[0].size, 8);
    assert_eq!(l.bindings[2].size, 1);
    drop(l);
    assert_eq!(rt.read("%IW1.0").unwrap(), Value::Int(3));
    assert_eq!(rt.drivers()[0].exchanges, 6);
    assert!(rt.drivers()[0].healthy);
}

#[test]
fn drivers_synchronised_with_a_task() {
    let mut rt = runtime(
        "CONFIGURATION C RESOURCE R ON VM
           TASK FAST(INTERVAL := T#10ms, PRIORITY := 1);
           TASK SLOW(INTERVAL := T#30ms, PRIORITY := 2);
           PROGRAM P1 WITH FAST : P; PROGRAM P2 WITH SLOW : S;
         END_RESOURCE END_CONFIGURATION",
    );
    let slow = attach(&mut rt, r#"{"name": "slow", "type": "mock", "task": "slow", "bindings": [{"area": "%I1"}, {"area": "%Q1"}]}"#);
    let all = attach(&mut rt, r#"{"name": "all", "type": "mock", "bindings": [{"area": "%I1"}]}"#);
    rt.cold_start().unwrap();
    rt.run_for(60_000);
    let s = slow.lock().unwrap().calls.clone();
    assert_eq!(s.iter().filter(|c| c.starts_with("in")).count(), 2, "{:?}", s);
    assert_eq!(s.iter().filter(|c| *c == "out").count(), 2);
    // a driver without task exchanges around every task
    let a = all.lock().unwrap().calls.clone();
    assert_eq!(a.iter().filter(|c| c.starts_with("in")).count(), 8, "{:?}", a);
    // and around every execution of a program without task
    let mut rt = runtime("CONFIGURATION C RESOURCE R ON VM PROGRAM P3 : S; END_RESOURCE END_CONFIGURATION");
    let bg = attach(&mut rt, r#"{"name": "bg", "type": "mock", "bindings": [{"area": "%I"}]}"#);
    rt.cold_start().unwrap();
    rt.run_for(100);
    let runs = match rt.read("R.P3.RUNS").unwrap() {
        Value::Int(n) => n as usize,
        _ => 0,
    };
    assert!(runs > 2);
    assert!(bg.lock().unwrap().reads >= runs);
}

#[test]
fn status_variable_and_fault_policies() {
    // continue: the inputs keep their last values, the status is FALSE
    let mut rt = runtime(ONE_TASK);
    let log = attach(&mut rt, r#"{"name": "io", "type": "mock", "status": "OK", "bindings": [{"area": "%I1"}, {"area": "%Q1"}]}"#);
    log.lock().unwrap().fail_after = Some(2);
    rt.cold_start().unwrap();
    rt.run_for(20_000);
    assert_eq!(rt.read("OK").unwrap(), Value::Bool(true));
    rt.run_for(30_000);
    assert_eq!(rt.read("OK").unwrap(), Value::Bool(false));
    assert_eq!(rt.read("%IW1.0").unwrap(), Value::Int(2));
    assert!(rt.resource_running(0));
    assert!(rt.errors.is_empty());
    assert_eq!(rt.driver_events.len(), 2, "{:?}", rt.driver_events);
    assert_eq!(rt.driver_events[1].to_string(), "driver io: fault: device unreachable");
    assert!(!rt.drivers()[0].healthy);
    // stop: the resource stops and the driver is asked for a safe state
    let mut rt = runtime(ONE_TASK);
    let log = attach(&mut rt, r#"{"name": "io", "type": "mock", "on_fault": "stop", "bindings": [{"area": "%I1"}, {"area": "%Q1"}]}"#);
    log.lock().unwrap().fail_after = Some(1);
    rt.cold_start().unwrap();
    rt.run_for(50_000);
    assert!(!rt.resource_running(0));
    assert_eq!(rt.errors.len(), 1);
    assert!(rt.errors[0].to_string().contains("I/O driver fault"), "{}", rt.errors[0]);
    assert!(rt.errors[0].to_string().contains("device unreachable"));
    assert!(log.lock().unwrap().calls.contains(&"stop".to_string()));
    // the driver is closed with the runtime
    drop(rt);
    assert_eq!(log.lock().unwrap().calls.last().unwrap(), "close");
}

#[test]
fn configuration_errors() {
    let mut rt = runtime(ONE_TASK);
    let mut add = |json: &str| rt.add_driver(&spec(json), Box::new(Mock(Arc::default()))).unwrap_err();
    assert!(add(r#"{"name": "a", "resource": "X", "bindings": []}"#).contains("unknown resource 'X'"));
    assert!(add(r#"{"name": "a", "task": "SLOW", "bindings": []}"#).contains("has no task 'SLOW'"));
    assert!(add(r#"{"name": "a", "bindings": [{"area": "%M0"}]}"#).contains("invalid area"));
    assert!(add(r#"{"name": "a", "bindings": [{"area": "%Q1", "offset": 100}]}"#).contains("beyond the area"));
    assert!(add(r#"{"name": "a", "status": "R.P1.RUNS", "bindings": []}"#).contains("BOOL"));
    assert!(DriverConfig::from_json(r#"{"drivers": [{"name": "a", "bindings": [], "colour": 1}]}"#).unwrap_err().contains("unknown field"));
    // an area no variable uses gives an empty binding
    let log = attach(&mut rt, r#"{"name": "io", "type": "mock", "bindings": [{"area": "%I7"}]}"#);
    rt.cold_start().unwrap();
    assert_eq!(log.lock().unwrap().bindings[0].size, 0);
}
