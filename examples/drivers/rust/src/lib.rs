//! An I/O driver written in Rust and built as a shared library: a signal
//! generator. Each input word of the bindings follows a sawtooth from 0 to
//! `amplitude` over `period_ms` milliseconds, word n being shifted by n/8 of
//! a period. Output bindings are accepted and ignored.
//!
//! ```text
//! cargo build --release -p structure-driver-example
//! ```
//!
//! and in the driver configuration:
//!
//! ```json
//! { "name": "gen", "library": "target/release/libsignal_generator.so",
//!   "params": { "period_ms": 2000, "amplitude": 1000 },
//!   "bindings": [ { "area": "%I3" } ] }
//! ```

use serde_json::Value;
use std::time::Instant;
use structure_runtime::driver::{image, Binding, Direction, IoDriver};

struct SignalGenerator {
    period_ms: u64,
    amplitude: u64,
    start: Instant,
    inputs: Vec<bool>,
}

fn create(params: &Value) -> Result<Box<dyn IoDriver>, String> {
    let get = |key: &str, default: u64| match params.get(key) {
        None => Ok(default),
        Some(v) => v.as_u64().filter(|v| *v > 0).ok_or_else(|| format!("{} shall be a positive integer", key)),
    };
    Ok(Box::new(SignalGenerator { period_ms: get("period_ms", 1000)?, amplitude: get("amplitude", 32767)?.min(65535), start: Instant::now(), inputs: vec![] }))
}

impl IoDriver for SignalGenerator {
    fn open(&mut self, bindings: &[Binding]) -> Result<(), String> {
        self.inputs = bindings.iter().map(|b| b.direction == Direction::Input).collect();
        self.start = Instant::now();
        Ok(())
    }

    fn read_inputs(&mut self, data: &mut [&mut [u8]]) -> Result<(), String> {
        let t = self.start.elapsed().as_millis() as u64;
        for (d, input) in data.iter_mut().zip(&self.inputs) {
            if !input {
                continue;
            }
            for n in 0..d.len() / 2 {
                let phase = (t + n as u64 * self.period_ms / 8) % self.period_ms;
                image::set_word(d, n, (phase * self.amplitude / self.period_ms) as u16);
            }
        }
        Ok(())
    }

    fn write_outputs(&mut self, _data: &[&[u8]]) -> Result<(), String> {
        Ok(())
    }
}

structure_runtime::export_driver!(create);
