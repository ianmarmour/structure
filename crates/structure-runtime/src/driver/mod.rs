//! I/O drivers: the exchange of the process image of a resource with
//! devices and networks (Modbus TCP, EtherCAT, PROFINET, ...).
//!
//! A driver is bound to one or more *areas* of the process image of a
//! resource: the main input or output area (`%I`, `%Q`), or the area of a
//! module of hierarchical addresses (`%I1` holds `%IX1.0.3`, `%IW1.2`, ...),
//! optionally restricted to a byte range. The bytes of an area are those of
//! the directly represented variables: `%IXa.b` is bit `b` of byte `a`,
//! `%IWn` the bytes `2n` and `2n+1` (little-endian), `%IDn` the bytes `4n`
//! to `4n+3`, and so on.
//!
//! The runtime exchanges data with a driver at task boundaries (2.7.2):
//! the inputs are read when a task is activated, before its programs and
//! function blocks execute, and the outputs are written when the last of them
//! has completed. A driver associated with a task exchanges data only around
//! the executions of that task (for a fieldbus cycle synchronised to it);
//! otherwise it exchanges around the executions of every task of its
//! resource, and of programs without a task.
//!
//! Drivers are written in Rust by implementing [`IoDriver`], or in any
//! language as a shared library with the C interface of
//! `include/structure_driver.h` (see [`ffi`]).

pub mod ffi;

use serde::Deserialize;
use serde_json::Value as Json;
use std::collections::HashMap;
use std::path::PathBuf;

/// Direction of the data of a binding.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Direction {
    /// From the device to the process image (`%I`).
    Input,
    /// From the process image to the device (`%Q`).
    Output,
}

/// A part of the process image exchanged by a driver.
#[derive(Clone, Debug)]
pub struct Binding {
    /// The area as written in the configuration (`%I`, `%Q1`, `%I2.1`).
    pub area: String,
    pub direction: Direction,
    /// Module path of the area (empty for the main area).
    pub path: Vec<u32>,
    /// Byte offset of the binding in the area.
    pub offset: usize,
    /// Size of the binding in bytes. It is zero when no variable of the
    /// configuration uses the area.
    pub size: usize,
    /// Driver-specific parameters of the binding.
    pub params: Json,
}

/// A driver exchanging the process image with a device or a network.
///
/// The runtime calls the methods from its scheduling thread, one at a time.
/// `read_inputs` and `write_outputs` are called in every cycle of the tasks
/// the driver is associated with, so they shall not block for longer than
/// the exchange takes: a driver for slow devices typically polls them from a
/// thread of its own and only copies buffers here.
pub trait IoDriver: Send {
    /// Prepares the exchange of the given bindings; called when the
    /// configuration is started (cold or warm restart).
    fn open(&mut self, bindings: &[Binding]) -> Result<(), String>;

    /// Stores the current input values of the device into `data`: `data[i]`
    /// is the process image of `bindings[i]` (as given to [`IoDriver::open`]);
    /// it is empty for output bindings.
    fn read_inputs(&mut self, data: &mut [&mut [u8]]) -> Result<(), String>;

    /// Sends the output values of the process image to the device: `data[i]`
    /// is the process image of `bindings[i]`; it is empty for input bindings.
    fn write_outputs(&mut self, data: &[&[u8]]) -> Result<(), String>;

    /// The resource stopped (because of an error, or on request): the driver
    /// should bring the outputs into a safe state.
    fn stop(&mut self) {}

    /// The configuration is being shut down.
    fn close(&mut self) {}
}

/// What to do when a driver fails.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FaultPolicy {
    /// Report the fault and keep running; the inputs keep their last values.
    #[default]
    Continue,
    /// Report the fault and stop the resource.
    Stop,
}

/// Configuration of the drivers of a configuration (the `--drivers` file).
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DriverConfig {
    pub drivers: Vec<DriverSpec>,
}

/// Configuration of one driver instance.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DriverSpec {
    /// Name of the instance (used in messages).
    pub name: String,
    /// Type of a driver built into the runtime (`modbus-tcp`, `loopback`).
    #[serde(rename = "type", default)]
    pub kind: Option<String>,
    /// Shared library implementing the C interface of the driver.
    #[serde(default)]
    pub library: Option<PathBuf>,
    /// Resource whose process image is exchanged (optional when the
    /// configuration has a single resource).
    #[serde(default)]
    pub resource: Option<String>,
    /// Task whose executions the exchange is synchronised with.
    #[serde(default)]
    pub task: Option<String>,
    /// Driver-specific parameters.
    #[serde(default)]
    pub params: Json,
    pub bindings: Vec<BindingSpec>,
    /// A BOOL variable (path or directly represented variable) set to TRUE
    /// while the driver works and to FALSE when it fails.
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default)]
    pub on_fault: FaultPolicy,
}

/// Configuration of a binding.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BindingSpec {
    /// `%I` or `%Q`, optionally followed by a module path (`%I1`, `%Q2.1`).
    pub area: String,
    /// Byte offset in the area.
    #[serde(default)]
    pub offset: usize,
    /// Size in bytes (default: the rest of the area).
    #[serde(default)]
    pub size: Option<usize>,
    #[serde(default)]
    pub params: Json,
}

impl DriverConfig {
    pub fn from_json(text: &str) -> Result<DriverConfig, String> {
        serde_json::from_str(text).map_err(|e| format!("invalid driver configuration: {}", e))
    }
}

/// Creates a driver from its parameters.
pub type DriverFactory = fn(params: &Json) -> Result<Box<dyn IoDriver>, String>;

/// The driver types available by name.
#[derive(Clone, Default)]
pub struct DriverRegistry {
    factories: HashMap<String, DriverFactory>,
}

impl DriverRegistry {
    pub fn new() -> DriverRegistry {
        DriverRegistry::default()
    }

    pub fn register(&mut self, kind: &str, factory: DriverFactory) {
        self.factories.insert(kind.to_string(), factory);
    }

    pub fn kinds(&self) -> Vec<String> {
        let mut k: Vec<String> = self.factories.keys().cloned().collect();
        k.sort();
        k
    }

    /// Creates the driver of a specification: a registered type, or a
    /// shared library.
    pub fn create(&self, spec: &DriverSpec) -> Result<Box<dyn IoDriver>, String> {
        match (&spec.kind, &spec.library) {
            (Some(_), Some(_)) => Err(format!("driver {}: give either a type or a library, not both", spec.name)),
            (Some(kind), None) => {
                let f = self.factories.get(kind).ok_or_else(|| format!("driver {}: unknown driver type '{}' (available: {})", spec.name, kind, self.kinds().join(", ")))?;
                f(&spec.params).map_err(|e| format!("driver {}: {}", spec.name, e))
            }
            (None, Some(lib)) => ffi::DynamicDriver::load(lib, &spec.params).map(|d| Box::new(d) as Box<dyn IoDriver>).map_err(|e| format!("driver {}: {}", spec.name, e)),
            (None, None) => Err(format!("driver {}: a type or a library is required", spec.name)),
        }
    }
}

/// Parses an area: `%I` or `%Q` followed by an optional module path.
pub fn parse_area(area: &str) -> Result<(Direction, Vec<u32>), String> {
    let invalid = || format!("invalid area '{}' (expected %I or %Q, optionally followed by a module path such as %I1 or %Q2.1)", area);
    let text = area.trim().to_ascii_uppercase();
    let rest = text.strip_prefix('%').ok_or_else(invalid)?;
    let mut chars = rest.chars();
    let dir = match chars.next() {
        Some('I') => Direction::Input,
        Some('Q') => Direction::Output,
        _ => return Err(invalid()),
    };
    let path = chars.as_str();
    if path.is_empty() {
        return Ok((dir, vec![]));
    }
    let path = path.split('.').map(|p| p.parse::<u32>().map_err(|_| invalid())).collect::<Result<Vec<_>, _>>()?;
    Ok((dir, path))
}

/// Status of a driver instance.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DriverStatus {
    pub name: String,
    pub resource: String,
    pub healthy: bool,
    /// The last error, when the driver is not healthy.
    pub message: String,
    pub exchanges: u64,
}

/// A change of the status of a driver.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DriverEvent {
    pub time: i64,
    pub driver: String,
    pub healthy: bool,
    pub message: String,
}

impl std::fmt::Display for DriverEvent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.healthy {
            write!(f, "driver {}: {}", self.driver, if self.message.is_empty() { "ok" } else { &self.message })
        } else {
            write!(f, "driver {}: fault: {}", self.driver, self.message)
        }
    }
}

/// Helpers for drivers: reading and writing values of the process image
/// with the layout of directly represented variables.
pub mod image {
    /// Bit `n` of an area (`%IXa.b` is bit `8a+b`).
    pub fn bit(data: &[u8], n: usize) -> bool {
        data.get(n / 8).map(|b| b & (1 << (n % 8)) != 0).unwrap_or(false)
    }

    pub fn set_bit(data: &mut [u8], n: usize, v: bool) {
        if let Some(b) = data.get_mut(n / 8) {
            if v {
                *b |= 1 << (n % 8);
            } else {
                *b &= !(1 << (n % 8));
            }
        }
    }

    /// Word `n` of an area (`%IWn`).
    pub fn word(data: &[u8], n: usize) -> u16 {
        match data.get(2 * n..2 * n + 2) {
            Some(b) => u16::from_le_bytes([b[0], b[1]]),
            None => 0,
        }
    }

    pub fn set_word(data: &mut [u8], n: usize, v: u16) {
        if let Some(b) = data.get_mut(2 * n..2 * n + 2) {
            b.copy_from_slice(&v.to_le_bytes());
        }
    }

    /// Double word `n` of an area (`%IDn`).
    pub fn dword(data: &[u8], n: usize) -> u32 {
        match data.get(4 * n..4 * n + 4) {
            Some(b) => u32::from_le_bytes([b[0], b[1], b[2], b[3]]),
            None => 0,
        }
    }

    pub fn set_dword(data: &mut [u8], n: usize, v: u32) {
        if let Some(b) = data.get_mut(4 * n..4 * n + 4) {
            b.copy_from_slice(&v.to_le_bytes());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn areas() {
        assert_eq!(parse_area("%I").unwrap(), (Direction::Input, vec![]));
        assert_eq!(parse_area("%q2.1").unwrap(), (Direction::Output, vec![2, 1]));
        assert!(parse_area("%M1").is_err());
        assert!(parse_area("%IW1").is_err());
        assert!(parse_area("I1").is_err());
    }
}
