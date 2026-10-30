mod common;
use structure_compiler::{compile_str, Mode};
use structure_runtime::runtime::Event;
use structure_runtime::{Options, Runtime, Value};

fn runtime(src: &str, preemptive: bool) -> Runtime {
    let (sm, out) = compile_str("test.st", src, Mode::Build);
    if out.image.is_none() {
        panic!("{}", out.diagnostics.render(&sm));
    }
    let mut rt = Runtime::new(out.image.unwrap(), Options { preemptive, report_overruns: false, ..Options::default() });
    rt.trace = Some(Vec::new());
    rt.cold_start().unwrap();
    rt
}

const PROGRAMS: &str = r#"
PROGRAM BUSY
VAR_INPUT LOOPS : DINT; END_VAR
VAR_OUTPUT RUNS : DINT; END_VAR
VAR I, X : DINT; END_VAR
FOR I := 1 TO LOOPS DO X := X + 1; END_FOR;
RUNS := RUNS + 1;
END_PROGRAM
"#;

/// Start times of a job (by name) from the trace.
fn starts(rt: &Runtime, job: &str) -> Vec<i64> {
    let ji = rt.image.resources[0].jobs.iter().position(|j| j.name == job).unwrap();
    rt.trace.as_ref().unwrap().iter().filter_map(|e| match e {
        Event::Start { time, job, .. } if *job == ji => Some(*time),
        _ => None,
    }).collect()
}

#[test]
fn preemptive_vs_non_preemptive() {
    let src = format!(
        "{PROGRAMS}
CONFIGURATION C RESOURCE R ON VM
  TASK SLOW(INTERVAL := T#20ms, PRIORITY := 2);
  TASK FAST(INTERVAL := T#5ms, PRIORITY := 1);
  PROGRAM LONG WITH SLOW : BUSY(LOOPS := 50000);
  PROGRAM SHORT WITH FAST : BUSY(LOOPS := 10);
END_RESOURCE END_CONFIGURATION"
    );
    // Preemptive: the fast task always starts on time.
    let mut rt = runtime(&src, true);
    rt.run_for(40_000);
    let fast = starts(&rt, "R.SHORT");
    assert_eq!(fast[..8], [0, 5_000, 10_000, 15_000, 20_000, 25_000, 30_000, 35_000]);
    // Non-preemptive: the fast task waits for the long program to finish.
    let mut rt = runtime(&src, false);
    rt.run_for(40_000);
    let fast = starts(&rt, "R.SHORT");
    let long = starts(&rt, "R.LONG");
    // The fast program ran first (same start, higher priority), then had to
    // wait until the long program completed (about 32 ms of execution);
    // activations in between were overruns.
    assert_eq!(fast[0], 0);
    assert!(long[0] > 0);
    assert!(fast[1] > 30_000, "{:?}", fast);
    assert_eq!(rt.read("R.SHORT.RUNS").unwrap(), Value::Int(fast.len() as i128));
}

#[test]
fn single_triggered_task() {
    let src = format!(
        "{PROGRAMS}
CONFIGURATION C
  VAR_GLOBAL TRIG : BOOL; END_VAR
  RESOURCE R ON VM
  TASK EVT(SINGLE := TRIG, PRIORITY := 1);
  PROGRAM ONCE WITH EVT : BUSY(LOOPS := 1);
END_RESOURCE END_CONFIGURATION"
    );
    let mut rt = runtime(&src, true);
    rt.run_for(10_000);
    assert_eq!(rt.read("R.ONCE.RUNS").unwrap(), Value::Int(0));
    rt.write("TRIG", &Value::Bool(true)).unwrap();
    rt.run_for(10_000);
    assert_eq!(rt.read("R.ONCE.RUNS").unwrap(), Value::Int(1));
    // stays TRUE: no new rising edge
    rt.run_for(10_000);
    assert_eq!(rt.read("R.ONCE.RUNS").unwrap(), Value::Int(1));
    rt.write("TRIG", &Value::Bool(false)).unwrap();
    rt.run_for(1_000);
    rt.write("TRIG", &Value::Bool(true)).unwrap();
    rt.run_for(1_000);
    assert_eq!(rt.read("R.ONCE.RUNS").unwrap(), Value::Int(2));
}

#[test]
fn program_without_task_runs_continuously() {
    let src = format!(
        "{PROGRAMS}
CONFIGURATION C RESOURCE R ON VM
  TASK T(INTERVAL := T#10ms, PRIORITY := 1);
  PROGRAM BG : BUSY(LOOPS := 100);
  PROGRAM CYC WITH T : BUSY(LOOPS := 1);
END_RESOURCE END_CONFIGURATION"
    );
    let mut rt = runtime(&src, true);
    rt.run_for(20_000);
    let bg = match rt.read("R.BG.RUNS").unwrap() {
        Value::Int(i) => i,
        _ => 0,
    };
    assert!(bg > 10, "background program ran {} times", bg);
    assert_eq!(rt.read("R.CYC.RUNS").unwrap(), Value::Int(2));
}

#[test]
fn function_block_task_association() {
    let src = r#"
FUNCTION_BLOCK COUNTER
VAR_INPUT INC : DINT; END_VAR
VAR_OUTPUT N : DINT; END_VAR
N := N + INC;
END_FUNCTION_BLOCK
PROGRAM G
VAR_OUTPUT OUT1 : DINT; END_VAR
VAR FB1 : COUNTER; FB2 : COUNTER; END_VAR
FB1(INC := 1);
FB2(INC := 1);
OUT1 := FB1.N;
END_PROGRAM
CONFIGURATION C
  VAR_GLOBAL W : DINT; END_VAR
  RESOURCE R ON VM
  TASK SLOW_1(INTERVAL := T#20ms, PRIORITY := 2);
  TASK FAST_1(INTERVAL := T#10ms, PRIORITY := 1);
  PROGRAM P2 WITH FAST_1 : G(OUT1 => W, FB1 WITH SLOW_1);
END_RESOURCE END_CONFIGURATION
"#;
    let mut rt = runtime(src, true);
    rt.run_for(100_000);
    // P2 runs 10 times, FB2 with it; FB1 only under control of SLOW_1 (5 times)
    assert_eq!(rt.read("R.P2.FB2.N").unwrap(), Value::Int(10));
    assert_eq!(rt.read("R.P2.FB1.N").unwrap(), Value::Int(5));
    assert!(matches!(rt.read("W").unwrap(), Value::Int(4 | 5)));
}

#[test]
fn task_overrun_is_reported() {
    let src = format!(
        "{PROGRAMS}
CONFIGURATION C RESOURCE R ON VM
  TASK T(INTERVAL := T#1ms, PRIORITY := 1);
  PROGRAM LONG WITH T : BUSY(LOOPS := 50000);
END_RESOURCE END_CONFIGURATION"
    );
    let (_, out) = compile_str("t.st", &src, Mode::Build);
    let mut rt = Runtime::new(out.image.unwrap(), Options::default());
    rt.cold_start().unwrap();
    rt.run_for(20_000);
    assert!(rt.task_overruns(0, 0) > 0);
    assert!(!rt.errors.is_empty());
}

#[test]
fn graphical_tasks_and_associations_table_50() {
    // features 2a, 2b (TASK blocks), 4a (programs) and 4b (function blocks)
    let src = r#"
FUNCTION_BLOCK COUNTER
VAR_OUTPUT N : DINT; END_VAR
N := N + 1;
END_FUNCTION_BLOCK
PROGRAM F VAR_OUTPUT N : DINT; END_VAR N := N + 1; END_PROGRAM
PROGRAM H VAR_OUTPUT N : DINT; END_VAR N := N + 1; END_PROGRAM
PROGRAM G
VAR_OUTPUT OUT1 : DINT; END_VAR
VAR FB1 : COUNTER; FB2 : COUNTER; END_VAR
FB1();
FB2();
OUT1 := FB1.N;
END_PROGRAM
CONFIGURATION C
  VAR_GLOBAL W : DINT; TRIG : BOOL; END_VAR
  RESOURCE R ON VM
             SLOW_1                  FAST_1                  INT_2
           +---------+             +---------+             +---------+
           |  TASK   |             |  TASK   |             |  TASK   |
           |SINGLE   |             |SINGLE   |       TRIG--|SINGLE   |
  t#20ms---|INTERVAL |    t#10ms---|INTERVAL |             |INTERVAL |
       2---|PRIORITY |         1---|PRIORITY |          3--|PRIORITY |
           +---------+             +---------+             +---------+

        P1                     P2
     +-------+      +------------------------------+
     |   F   |      |              G               |
     |       |      |                          OUT1|---W
     +-------+      |    FB1           FB2         |
     |SLOW_1 |      |  +-------+     +-------+     |
     +-------+      |  |COUNTER|     |COUNTER|     |
                    |  +-------+     +-------+     |
                    |  |SLOW_1 |     |FAST_1 |     |
                    |  +-------+     +-------+     |
                    +------------------------------+
                    |            FAST_1            |
                    +------------------------------+

        P3
     +-------+
     |   H   |
     +-------+
     | INT_2 |
     +-------+
  END_RESOURCE
END_CONFIGURATION
"#;
    let mut rt = runtime(src, true);
    rt.run_for(100_000);
    assert_eq!(rt.read("R.P1.N").unwrap(), Value::Int(5));
    assert_eq!(rt.read("R.P2.FB2.N").unwrap(), Value::Int(10));
    assert_eq!(rt.read("R.P2.FB1.N").unwrap(), Value::Int(5));
    assert!(matches!(rt.read("W").unwrap(), Value::Int(4 | 5)));
    assert_eq!(rt.read("R.P3.N").unwrap(), Value::Int(0));
    rt.write("TRIG", &Value::Bool(true)).unwrap();
    rt.run_for(20_000);
    assert_eq!(rt.read("R.P3.N").unwrap(), Value::Int(1), "single (event) task");
}
