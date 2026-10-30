//! The `modbus-tcp` driver: a Modbus TCP client (master).
//!
//! Driver parameters:
//!
//! | Parameter | Default | |
//! |---|---|---|
//! | `host` | (required) | address of the server (device or gateway) |
//! | `port` | 502 | |
//! | `unit` | 1 | unit identifier (slave address behind a gateway) |
//! | `mode` | `"polled"` | `"polled"`: a thread of the driver polls the device every `poll_interval_ms` and the task cycles exchange the latest data; `"synchronous"`: the requests are made in the task cycle |
//! | `poll_interval_ms` | 10 | |
//! | `timeout_ms` | 1000 | timeout of a request (and of connecting) |
//! | `stop_outputs` | `"zero"` | outputs when the resource stops: `"zero"` or `"hold"` |
//!
//! Binding parameters:
//!
//! | Parameter | | |
//! |---|---|---|
//! | `table` | (required) | `coils`, `discrete_inputs`, `input_registers` or `holding_registers`; outputs can only be `coils` or `holding_registers` |
//! | `start` | 0 | first Modbus address of the binding |
//! | `count` | size of the binding | number of bits or registers |
//! | `unit` | the driver's | |
//!
//! Registers are mapped to the words of the process image (register
//! `start + n` is `%IWn` of the binding, or `%QWn`); bits (coils and
//! discrete inputs) to its bits (`start + n` is `%IXn`, i.e. bit n mod 8 of
//! byte n / 8). Outputs are written in every cycle, once the programs have
//! produced them.

use serde_json::Value as Json;
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream, ToSocketAddrs};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::Duration;
use structure_runtime::driver::{Binding, Direction, IoDriver};

/// Modbus tables.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Table {
    Coils,
    DiscreteInputs,
    InputRegisters,
    HoldingRegisters,
}

impl Table {
    fn parse(s: &str) -> Option<Table> {
        Some(match s {
            "coils" => Table::Coils,
            "discrete_inputs" => Table::DiscreteInputs,
            "input_registers" => Table::InputRegisters,
            "holding_registers" => Table::HoldingRegisters,
            _ => return None,
        })
    }

    fn is_bits(self) -> bool {
        matches!(self, Table::Coils | Table::DiscreteInputs)
    }
}

// ------------------------------------------------------------ protocol client

/// A Modbus TCP client connection.
pub struct Client {
    addr: SocketAddr,
    timeout: Duration,
    stream: Option<TcpStream>,
    transaction: u16,
}

const MAX_READ_BITS: usize = 2000;
const MAX_WRITE_BITS: usize = 1968;
const MAX_READ_REGISTERS: usize = 125;
const MAX_WRITE_REGISTERS: usize = 123;

impl Client {
    pub fn new(host: &str, port: u16, timeout: Duration) -> Result<Client, String> {
        let addr = (host, port).to_socket_addrs().map_err(|e| format!("cannot resolve {}: {}", host, e))?.next().ok_or_else(|| format!("cannot resolve {}", host))?;
        Ok(Client { addr, timeout, stream: None, transaction: 0 })
    }

    fn connect(&mut self) -> Result<&mut TcpStream, String> {
        if self.stream.is_none() {
            let s = TcpStream::connect_timeout(&self.addr, self.timeout).map_err(|e| format!("cannot connect to {}: {}", self.addr, e))?;
            s.set_read_timeout(Some(self.timeout)).map_err(|e| e.to_string())?;
            s.set_write_timeout(Some(self.timeout)).map_err(|e| e.to_string())?;
            let _ = s.set_nodelay(true);
            self.stream = Some(s);
        }
        Ok(self.stream.as_mut().unwrap())
    }

    /// Sends a request (function code and data) and returns the data of the
    /// response.
    pub fn request(&mut self, unit: u8, pdu: &[u8]) -> Result<Vec<u8>, String> {
        let r = self.transact(unit, pdu);
        if r.is_err() {
            // resynchronise on the next request
            self.stream = None;
        }
        r?
    }

    fn transact(&mut self, unit: u8, pdu: &[u8]) -> Result<Result<Vec<u8>, String>, String> {
        self.transaction = self.transaction.wrapping_add(1);
        let tid = self.transaction;
        let mut frame = Vec::with_capacity(7 + pdu.len());
        frame.extend_from_slice(&tid.to_be_bytes());
        frame.extend_from_slice(&0u16.to_be_bytes());
        frame.extend_from_slice(&((pdu.len() + 1) as u16).to_be_bytes());
        frame.push(unit);
        frame.extend_from_slice(pdu);
        let addr = self.addr;
        let s = self.connect()?;
        s.write_all(&frame).map_err(|e| format!("{}: {}", addr, e))?;
        let mut header = [0u8; 7];
        s.read_exact(&mut header).map_err(|e| format!("{}: {}", addr, e))?;
        let len = u16::from_be_bytes([header[4], header[5]]) as usize;
        if !(2..=254).contains(&len) {
            return Err(format!("{}: invalid response length {}", addr, len));
        }
        let mut body = vec![0u8; len - 1];
        s.read_exact(&mut body).map_err(|e| format!("{}: {}", addr, e))?;
        if u16::from_be_bytes([header[0], header[1]]) != tid || header[6] != unit {
            return Err(format!("{}: response does not match the request", addr));
        }
        let fc = pdu[0];
        if body[0] == fc | 0x80 {
            let code = body.get(1).copied().unwrap_or(0);
            return Ok(Err(format!("{}: Modbus exception {} ({}) for function {}", addr, code, exception_text(code), fc)));
        }
        if body[0] != fc {
            return Err(format!("{}: unexpected function code {} in response", addr, body[0]));
        }
        Ok(Ok(body[1..].to_vec()))
    }

    /// Reads coils (function 1) or discrete inputs (function 2), packed
    /// eight per byte, least significant bit first.
    pub fn read_bits(&mut self, unit: u8, table: Table, start: u16, count: usize) -> Result<Vec<u8>, String> {
        let fc = if table == Table::Coils { 1 } else { 2 };
        let mut out = vec![0u8; count.div_ceil(8)];
        let mut done = 0;
        while done < count {
            let n = (count - done).min(MAX_READ_BITS);
            let addr = start.checked_add(done as u16).ok_or("address beyond 65535")?;
            let mut pdu = vec![fc];
            pdu.extend_from_slice(&addr.to_be_bytes());
            pdu.extend_from_slice(&(n as u16).to_be_bytes());
            let data = self.request(unit, &pdu)?;
            if data.is_empty() || data[0] as usize != n.div_ceil(8) || data.len() < 1 + n.div_ceil(8) {
                return Err(format!("{}: malformed response to function {}", self.addr, fc));
            }
            for i in 0..n {
                if data[1 + i / 8] & (1 << (i % 8)) != 0 {
                    let k = done + i;
                    out[k / 8] |= 1 << (k % 8);
                }
            }
            done += n;
        }
        Ok(out)
    }

    /// Reads input registers (function 4) or holding registers (function 3).
    pub fn read_registers(&mut self, unit: u8, table: Table, start: u16, count: usize) -> Result<Vec<u16>, String> {
        let fc = if table == Table::InputRegisters { 4 } else { 3 };
        let mut out = Vec::with_capacity(count);
        while out.len() < count {
            let n = (count - out.len()).min(MAX_READ_REGISTERS);
            let addr = start.checked_add(out.len() as u16).ok_or("address beyond 65535")?;
            let mut pdu = vec![fc];
            pdu.extend_from_slice(&addr.to_be_bytes());
            pdu.extend_from_slice(&(n as u16).to_be_bytes());
            let data = self.request(unit, &pdu)?;
            if data.is_empty() || data[0] as usize != 2 * n || data.len() < 1 + 2 * n {
                return Err(format!("{}: malformed response to function {}", self.addr, fc));
            }
            out.extend((0..n).map(|i| u16::from_be_bytes([data[1 + 2 * i], data[2 + 2 * i]])));
        }
        Ok(out)
    }

    /// Writes coils (function 15) from packed bits.
    pub fn write_coils(&mut self, unit: u8, start: u16, count: usize, bits: &[u8]) -> Result<(), String> {
        let mut done = 0;
        while done < count {
            let n = (count - done).min(MAX_WRITE_BITS);
            let addr = start.checked_add(done as u16).ok_or("address beyond 65535")?;
            let mut packed = vec![0u8; n.div_ceil(8)];
            for i in 0..n {
                let k = done + i;
                if bits.get(k / 8).map(|b| b & (1 << (k % 8)) != 0).unwrap_or(false) {
                    packed[i / 8] |= 1 << (i % 8);
                }
            }
            let mut pdu = vec![15];
            pdu.extend_from_slice(&addr.to_be_bytes());
            pdu.extend_from_slice(&(n as u16).to_be_bytes());
            pdu.push(packed.len() as u8);
            pdu.extend_from_slice(&packed);
            self.request(unit, &pdu)?;
            done += n;
        }
        Ok(())
    }

    /// Writes holding registers (function 16).
    pub fn write_registers(&mut self, unit: u8, start: u16, values: &[u16]) -> Result<(), String> {
        let mut done = 0;
        while done < values.len() {
            let n = (values.len() - done).min(MAX_WRITE_REGISTERS);
            let addr = start.checked_add(done as u16).ok_or("address beyond 65535")?;
            let mut pdu = vec![16];
            pdu.extend_from_slice(&addr.to_be_bytes());
            pdu.extend_from_slice(&(n as u16).to_be_bytes());
            pdu.push((2 * n) as u8);
            for v in &values[done..done + n] {
                pdu.extend_from_slice(&v.to_be_bytes());
            }
            self.request(unit, &pdu)?;
            done += n;
        }
        Ok(())
    }
}

fn exception_text(code: u8) -> &'static str {
    match code {
        1 => "illegal function",
        2 => "illegal data address",
        3 => "illegal data value",
        4 => "server device failure",
        5 => "acknowledge",
        6 => "server device busy",
        10 => "gateway path unavailable",
        11 => "gateway target device failed to respond",
        _ => "unknown exception",
    }
}

// ------------------------------------------------------------ driver

#[derive(Clone, Debug)]
struct Point {
    binding: usize,
    direction: Direction,
    table: Table,
    unit: u8,
    start: u16,
    count: usize,
    size: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Mode {
    Polled,
    Synchronous,
}

struct Shared {
    inputs: Vec<Vec<u8>>,
    outputs: Vec<Vec<u8>>,
    /// The programs have produced outputs (nothing is written before).
    outputs_valid: bool,
    status: Result<(), String>,
    /// At least one complete poll succeeded since the driver was opened.
    polled: bool,
    stop: bool,
}

pub struct ModbusDriver {
    host: String,
    port: u16,
    unit: u8,
    mode: Mode,
    poll_interval: Duration,
    timeout: Duration,
    zero_on_stop: bool,
    points: Vec<Point>,
    client: Option<Client>,
    shared: Arc<Mutex<Shared>>,
    thread: Option<JoinHandle<()>>,
}

fn param_u64(params: &Json, key: &str, default: u64) -> Result<u64, String> {
    match params.get(key) {
        None | Some(Json::Null) => Ok(default),
        Some(v) => v.as_u64().ok_or_else(|| format!("parameter {} shall be a non-negative integer", key)),
    }
}

pub fn create(params: &Json) -> Result<Box<dyn IoDriver>, String> {
    let host = params.get("host").and_then(|h| h.as_str()).ok_or("the parameter \"host\" is required")?.to_string();
    let port = param_u64(params, "port", 502)?;
    let unit = param_u64(params, "unit", 1)?;
    if port > 65535 || unit > 255 {
        return Err("port or unit out of range".into());
    }
    let mode = match params.get("mode").and_then(|m| m.as_str()) {
        None | Some("polled") => Mode::Polled,
        Some("synchronous") => Mode::Synchronous,
        Some(m) => return Err(format!("unknown mode '{}' (polled or synchronous)", m)),
    };
    let zero_on_stop = match params.get("stop_outputs").and_then(|m| m.as_str()) {
        None | Some("zero") => true,
        Some("hold") => false,
        Some(m) => return Err(format!("unknown stop_outputs '{}' (zero or hold)", m)),
    };
    Ok(Box::new(ModbusDriver {
        host,
        port: port as u16,
        unit: unit as u8,
        mode,
        poll_interval: Duration::from_millis(param_u64(params, "poll_interval_ms", 10)?),
        timeout: Duration::from_millis(param_u64(params, "timeout_ms", 1000)?.max(1)),
        zero_on_stop,
        points: Vec::new(),
        client: None,
        shared: Arc::new(Mutex::new(Shared { inputs: vec![], outputs: vec![], outputs_valid: false, status: Ok(()), polled: false, stop: false })),
        thread: None,
    }))
}

/// One exchange with the device: writes the outputs (when valid), then
/// reads the inputs.
fn cycle(client: &mut Client, points: &[Point], outputs: Option<&[Vec<u8>]>, inputs: &mut [Vec<u8>]) -> Result<(), String> {
    if let Some(outputs) = outputs {
        for p in points.iter().filter(|p| p.direction == Direction::Output) {
            let data = &outputs[p.binding];
            if p.table == Table::Coils {
                client.write_coils(p.unit, p.start, p.count, data)?;
            } else {
                let regs: Vec<u16> = (0..p.count).map(|i| structure_runtime::driver::image::word(data, i)).collect();
                client.write_registers(p.unit, p.start, &regs)?;
            }
        }
    }
    for p in points.iter().filter(|p| p.direction == Direction::Input) {
        let buf = &mut inputs[p.binding];
        if p.table.is_bits() {
            let bits = client.read_bits(p.unit, p.table, p.start, p.count)?;
            let n = bits.len().min(buf.len());
            buf[..n].copy_from_slice(&bits[..n]);
        } else {
            let regs = client.read_registers(p.unit, p.table, p.start, p.count)?;
            for (i, r) in regs.iter().enumerate() {
                structure_runtime::driver::image::set_word(buf, i, *r);
            }
        }
    }
    Ok(())
}

impl ModbusDriver {
    fn shutdown_thread(&mut self) {
        if let Some(t) = self.thread.take() {
            self.shared.lock().unwrap().stop = true;
            let _ = t.join();
        }
    }
}

impl IoDriver for ModbusDriver {
    fn open(&mut self, bindings: &[Binding]) -> Result<(), String> {
        self.shutdown_thread();
        self.points.clear();
        for (i, b) in bindings.iter().enumerate() {
            let table = b.params.get("table").and_then(|t| t.as_str()).ok_or_else(|| format!("binding {}: the parameter \"table\" is required", b.area))?;
            let table = Table::parse(table).ok_or_else(|| format!("binding {}: unknown table '{}'", b.area, table))?;
            if b.direction == Direction::Output && !matches!(table, Table::Coils | Table::HoldingRegisters) {
                return Err(format!("binding {}: outputs can only be written to coils or holding registers", b.area));
            }
            let start = param_u64(&b.params, "start", 0)?;
            let unit = param_u64(&b.params, "unit", self.unit as u64)?;
            let capacity = if table.is_bits() { b.size * 8 } else { b.size / 2 };
            let count = param_u64(&b.params, "count", capacity as u64)? as usize;
            if count > capacity {
                return Err(format!("binding {}: {} {} do not fit in the {} bytes of the binding", b.area, count, if table.is_bits() { "bits" } else { "registers" }, b.size));
            }
            if start + count as u64 > 65536 || unit > 255 {
                return Err(format!("binding {}: Modbus address or unit out of range", b.area));
            }
            if count > 0 {
                self.points.push(Point { binding: i, direction: b.direction, table, unit: unit as u8, start: start as u16, count, size: b.size });
            }
        }
        let client = Client::new(&self.host, self.port, self.timeout)?;
        {
            let mut sh = self.shared.lock().unwrap();
            sh.inputs = bindings.iter().map(|b| vec![0; if b.direction == Direction::Input { b.size } else { 0 }]).collect();
            sh.outputs = bindings.iter().map(|b| vec![0; if b.direction == Direction::Output { b.size } else { 0 }]).collect();
            sh.outputs_valid = false;
            sh.status = Ok(());
            sh.polled = false;
            sh.stop = false;
        }
        match self.mode {
            Mode::Synchronous => self.client = Some(client),
            Mode::Polled => {
                let shared = self.shared.clone();
                let points = self.points.clone();
                let interval = self.poll_interval;
                let mut client = client;
                self.thread = Some(
                    std::thread::Builder::new()
                        .name(format!("modbus {}:{}", self.host, self.port))
                        .spawn(move || loop {
                            let (outputs, mut inputs) = {
                                let sh = shared.lock().unwrap();
                                if sh.stop {
                                    break;
                                }
                                (if sh.outputs_valid { Some(sh.outputs.clone()) } else { None }, sh.inputs.clone())
                            };
                            let r = cycle(&mut client, &points, outputs.as_deref(), &mut inputs);
                            {
                                let mut sh = shared.lock().unwrap();
                                if r.is_ok() {
                                    sh.inputs = inputs;
                                    sh.polled = true;
                                }
                                sh.status = r;
                                if sh.stop {
                                    break;
                                }
                            }
                            std::thread::sleep(interval);
                        })
                        .map_err(|e| e.to_string())?,
                );
            }
        }
        Ok(())
    }

    fn read_inputs(&mut self, data: &mut [&mut [u8]]) -> Result<(), String> {
        match self.mode {
            Mode::Synchronous => {
                let mut inputs: Vec<Vec<u8>> = data.iter().map(|d| d.to_vec()).collect();
                let client = self.client.as_mut().ok_or("the driver is not open")?;
                let inputs_only: Vec<Point> = self.points.iter().filter(|p| p.direction == Direction::Input).cloned().collect();
                cycle(client, &inputs_only, None, &mut inputs)?;
                for (d, i) in data.iter_mut().zip(inputs) {
                    d.copy_from_slice(&i);
                }
                Ok(())
            }
            Mode::Polled => {
                let sh = self.shared.lock().unwrap();
                sh.status.clone()?;
                if sh.polled {
                    for (d, i) in data.iter_mut().zip(&sh.inputs) {
                        if d.len() == i.len() {
                            d.copy_from_slice(i);
                        }
                    }
                }
                Ok(())
            }
        }
    }

    fn write_outputs(&mut self, data: &[&[u8]]) -> Result<(), String> {
        match self.mode {
            Mode::Synchronous => {
                let outputs: Vec<Vec<u8>> = data.iter().map(|d| d.to_vec()).collect();
                let client = self.client.as_mut().ok_or("the driver is not open")?;
                let outputs_only: Vec<Point> = self.points.iter().filter(|p| p.direction == Direction::Output).cloned().collect();
                cycle(client, &outputs_only, Some(&outputs), &mut [])
            }
            Mode::Polled => {
                let mut sh = self.shared.lock().unwrap();
                for (o, d) in sh.outputs.iter_mut().zip(data) {
                    if o.len() == d.len() {
                        o.copy_from_slice(d);
                    }
                }
                sh.outputs_valid = true;
                sh.status.clone()
            }
        }
    }

    fn stop(&mut self) {
        if !self.zero_on_stop {
            return;
        }
        match self.mode {
            Mode::Synchronous => {
                let zeros: Vec<Vec<u8>> = self.points.iter().fold(vec![], |mut v, p| {
                    if v.len() <= p.binding {
                        v.resize(p.binding + 1, vec![]);
                    }
                    v[p.binding] = vec![0; p.size];
                    v
                });
                if let Some(client) = self.client.as_mut() {
                    let outputs_only: Vec<Point> = self.points.iter().filter(|p| p.direction == Direction::Output).cloned().collect();
                    let _ = cycle(client, &outputs_only, Some(&zeros), &mut []);
                }
            }
            Mode::Polled => {
                let mut sh = self.shared.lock().unwrap();
                for o in sh.outputs.iter_mut() {
                    o.fill(0);
                }
                sh.outputs_valid = true;
            }
        }
    }

    fn close(&mut self) {
        self.shutdown_thread();
        self.client = None;
    }
}

impl Drop for ModbusDriver {
    fn drop(&mut self) {
        self.shutdown_thread();
    }
}
