//! The modbus-tcp driver against a Modbus TCP server.
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use structure_drivers::modbus::{Client, Table};
use structure_runtime::{DriverConfig, Options, Runtime, Value};

#[derive(Default)]
struct Tables {
    coils: Vec<bool>,
    discrete: Vec<bool>,
    input_regs: Vec<u16>,
    holding: Vec<u16>,
}

/// A minimal Modbus TCP server (functions 1, 2, 3, 4, 15, 16).
struct Server {
    port: u16,
    tables: Arc<Mutex<Tables>>,
    requests: Arc<AtomicUsize>,
    online: Arc<AtomicBool>,
}

impl Server {
    fn start() -> Server {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let tables = Arc::new(Mutex::new(Tables { coils: vec![false; 100], discrete: vec![false; 100], input_regs: vec![0; 500], holding: vec![0; 500] }));
        let requests = Arc::new(AtomicUsize::new(0));
        let online = Arc::new(AtomicBool::new(true));
        let (t, r, o) = (tables.clone(), requests.clone(), online.clone());
        std::thread::spawn(move || {
            for stream in listener.incoming().flatten() {
                let (t, r, o) = (t.clone(), r.clone(), o.clone());
                std::thread::spawn(move || serve(stream, t, r, o));
            }
        });
        Server { port, tables, requests, online }
    }
}

fn serve(mut s: TcpStream, tables: Arc<Mutex<Tables>>, requests: Arc<AtomicUsize>, online: Arc<AtomicBool>) {
    loop {
        let mut header = [0u8; 7];
        if s.read_exact(&mut header).is_err() {
            return;
        }
        let len = u16::from_be_bytes([header[4], header[5]]) as usize;
        let mut pdu = vec![0u8; len - 1];
        if s.read_exact(&mut pdu).is_err() {
            return;
        }
        if !online.load(Ordering::SeqCst) {
            return; // drop the connection
        }
        requests.fetch_add(1, Ordering::SeqCst);
        let fc = pdu[0];
        let addr = u16::from_be_bytes([pdu[1], pdu[2]]) as usize;
        let qty = u16::from_be_bytes([pdu[3], pdu[4]]) as usize;
        let mut t = tables.lock().unwrap();
        let resp: Result<Vec<u8>, u8> = (|| {
            match fc {
                1 | 2 => {
                    let table = if fc == 1 { &t.coils } else { &t.discrete };
                    let bits = table.get(addr..addr + qty).ok_or(2u8)?;
                    let mut out = vec![fc, qty.div_ceil(8) as u8];
                    let mut packed = vec![0u8; qty.div_ceil(8)];
                    for (i, b) in bits.iter().enumerate() {
                        if *b {
                            packed[i / 8] |= 1 << (i % 8);
                        }
                    }
                    out.extend(packed);
                    Ok(out)
                }
                3 | 4 => {
                    if qty > 125 {
                        return Err(3);
                    }
                    let table = if fc == 3 { &t.holding } else { &t.input_regs };
                    let regs = table.get(addr..addr + qty).ok_or(2u8)?;
                    let mut out = vec![fc, (2 * qty) as u8];
                    for r in regs {
                        out.extend_from_slice(&r.to_be_bytes());
                    }
                    Ok(out)
                }
                15 => {
                    let data = &pdu[6..];
                    let dst = t.coils.get_mut(addr..addr + qty).ok_or(2u8)?;
                    for (i, c) in dst.iter_mut().enumerate() {
                        *c = data[i / 8] & (1 << (i % 8)) != 0;
                    }
                    Ok(pdu[..5].to_vec())
                }
                16 => {
                    let data = &pdu[6..];
                    let dst = t.holding.get_mut(addr..addr + qty).ok_or(2u8)?;
                    for (i, r) in dst.iter_mut().enumerate() {
                        *r = u16::from_be_bytes([data[2 * i], data[2 * i + 1]]);
                    }
                    Ok(pdu[..5].to_vec())
                }
                _ => Err(1),
            }
        })();
        drop(t);
        let body = match resp {
            Ok(b) => b,
            Err(code) => vec![fc | 0x80, code],
        };
        let mut frame = header[..4].to_vec();
        frame.extend_from_slice(&((body.len() + 1) as u16).to_be_bytes());
        frame.push(header[6]);
        frame.extend(body);
        if s.write_all(&frame).is_err() {
            return;
        }
    }
}

#[test]
fn client_functions() {
    let server = Server::start();
    let mut c = Client::new("127.0.0.1", server.port, Duration::from_secs(1)).unwrap();
    {
        let mut t = server.tables.lock().unwrap();
        for i in 0..300 {
            t.input_regs[i] = i as u16 * 3;
        }
        t.discrete[1] = true;
        t.discrete[9] = true;
    }
    // more registers than one request can carry
    let regs = c.read_registers(1, Table::InputRegisters, 0, 300).unwrap();
    assert_eq!(regs.len(), 300);
    assert_eq!(regs[299], 897);
    assert_eq!(c.read_bits(1, Table::DiscreteInputs, 0, 10).unwrap(), [0b10, 0b10]);
    c.write_registers(1, 10, &[7, 8, 9]).unwrap();
    c.write_coils(1, 3, 2, &[0b11]).unwrap();
    {
        let t = server.tables.lock().unwrap();
        assert_eq!(&t.holding[10..13], &[7, 8, 9]);
        assert!(t.coils[3] && t.coils[4] && !t.coils[5]);
    }
    let e = c.read_registers(1, Table::HoldingRegisters, 499, 5).unwrap_err();
    assert!(e.contains("exception 2 (illegal data address)"), "{}", e);
    // the connection survives an exception
    assert_eq!(c.read_registers(1, Table::HoldingRegisters, 10, 1).unwrap(), [7]);
}

const PROGRAM: &str = r#"
PROGRAM P
VAR
  A AT %IW1.0 : INT; B AT %IW1.1 : INT; C AT %IX1.4.0 : BOOL;
  SUM AT %QW1.0 : INT; L AT %QX1.4.0 : BOOL;
END_VAR
SUM := A + B;
L := NOT C;
END_PROGRAM
CONFIGURATION CFG
  VAR_GLOBAL LINK : BOOL; END_VAR
  RESOURCE R ON VM
    TASK T(INTERVAL := T#10ms, PRIORITY := 1);
    PROGRAM P1 WITH T : P;
  END_RESOURCE
END_CONFIGURATION
"#;

fn runtime(port: u16, mode: &str) -> Runtime {
    let (sm, out) = structure_compiler::compile_str("p.st", PROGRAM, structure_compiler::Mode::Build);
    let image = out.image.unwrap_or_else(|| panic!("{}", out.diagnostics.render(&sm)));
    let mut rt = Runtime::new(image, Options::default());
    let config = format!(
        r#"{{"drivers": [{{
            "name": "rack", "type": "modbus-tcp", "status": "LINK",
            "params": {{"host": "127.0.0.1", "port": {port}, "mode": "{mode}", "poll_interval_ms": 2, "timeout_ms": 200}},
            "bindings": [
              {{"area": "%I1", "size": 4, "params": {{"table": "input_registers", "start": 100}}}},
              {{"area": "%I1", "offset": 4, "size": 1, "params": {{"table": "discrete_inputs", "start": 0, "count": 1}}}},
              {{"area": "%Q1", "size": 2, "params": {{"table": "holding_registers", "start": 10}}}},
              {{"area": "%Q1", "offset": 4, "size": 1, "params": {{"table": "coils", "start": 0, "count": 1}}}}
            ]}}]}}"#
    );
    rt.load_drivers(&DriverConfig::from_json(&config).unwrap(), &structure_drivers::registry()).unwrap();
    rt.cold_start().unwrap();
    rt
}

#[test]
fn synchronous_exchange_with_a_program() {
    let server = Server::start();
    {
        let mut t = server.tables.lock().unwrap();
        t.input_regs[100] = 20;
        t.input_regs[101] = 22;
    }
    let mut rt = runtime(server.port, "synchronous");
    rt.run_for(30_000);
    assert_eq!(rt.read("R.P1.SUM").unwrap(), Value::Int(42));
    {
        let t = server.tables.lock().unwrap();
        assert_eq!(t.holding[10], 42);
        assert!(t.coils[0], "L := NOT C");
    }
    server.tables.lock().unwrap().discrete[0] = true;
    rt.run_for(10_000);
    assert!(!server.tables.lock().unwrap().coils[0]);
    assert_eq!(rt.read("LINK").unwrap(), Value::Bool(true));
    // the device goes away: fault, inputs hold, then recovery
    server.online.store(false, Ordering::SeqCst);
    rt.run_for(10_000);
    assert_eq!(rt.read("LINK").unwrap(), Value::Bool(false));
    assert!(rt.driver_events.iter().any(|e| !e.healthy), "{:?}", rt.driver_events);
    assert_eq!(rt.read("R.P1.A").unwrap(), Value::Int(20));
    server.online.store(true, Ordering::SeqCst);
    rt.run_for(20_000);
    assert_eq!(rt.read("LINK").unwrap(), Value::Bool(true));
    assert!(rt.errors.is_empty(), "{:?}", rt.errors);
    // outputs go to zero when the resource stops
    rt.stop();
    assert_eq!(server.tables.lock().unwrap().holding[10], 0);
}

#[test]
fn polled_exchange_with_a_program() {
    let server = Server::start();
    {
        let mut t = server.tables.lock().unwrap();
        t.input_regs[100] = 1;
        t.input_regs[101] = 2;
    }
    let mut rt = runtime(server.port, "polled");
    let mut done = false;
    for _ in 0..200 {
        rt.run_for(10_000);
        std::thread::sleep(Duration::from_millis(5));
        if server.tables.lock().unwrap().holding[10] == 3 {
            done = true;
            break;
        }
    }
    assert!(done, "the outputs reached the device");
    assert!(server.requests.load(Ordering::SeqCst) > 0);
    assert_eq!(rt.read("LINK").unwrap(), Value::Bool(true));
    assert!(rt.drivers()[0].healthy);
}

#[test]
fn unreachable_device() {
    // nothing listens on this port
    let port = TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port();
    let mut rt = runtime(port, "synchronous");
    rt.run_for(20_000);
    assert_eq!(rt.read("LINK").unwrap(), Value::Bool(false));
    let status = &rt.drivers()[0];
    assert!(!status.healthy);
    assert!(status.message.contains("cannot connect"), "{}", status.message);
    assert!(rt.resource_running(0), "the default policy keeps the resource running");
}

#[test]
fn parameter_errors() {
    let reg = structure_drivers::registry();
    let spec = |params: &str, bindings: &str| {
        DriverConfig::from_json(&format!(r#"{{"drivers": [{{"name": "m", "type": "modbus-tcp", "params": {params}, "bindings": [{bindings}]}}]}}"#)).unwrap().drivers.remove(0)
    };
    assert!(reg.create(&spec("{}", "")).err().unwrap().contains("\"host\" is required"));
    assert!(reg.create(&spec(r#"{"host": "127.0.0.1", "mode": "fast"}"#, "")).err().unwrap().contains("unknown mode"));
    let mut d = reg.create(&spec(r#"{"host": "127.0.0.1"}"#, "")).unwrap();
    let b = |dir, table: &str, size| structure_runtime::driver::Binding {
        area: "%Q".into(),
        direction: dir,
        path: vec![],
        offset: 0,
        size,
        params: serde_json::json!({ "table": table }),
    };
    use structure_runtime::driver::Direction;
    assert!(d.open(&[b(Direction::Output, "input_registers", 2)]).unwrap_err().contains("coils or holding registers"));
    assert!(d.open(&[b(Direction::Input, "registers", 2)]).unwrap_err().contains("unknown table"));
    let mut big = b(Direction::Input, "holding_registers", 2);
    big.params = serde_json::json!({"table": "holding_registers", "count": 5});
    assert!(d.open(&[big]).unwrap_err().contains("do not fit"));
}
