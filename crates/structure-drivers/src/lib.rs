//! I/O drivers built into the runtime.
//!
//! * `loopback`: the data written to the k-th output binding is read back by
//!   the k-th input binding (simulation and tests);
//! * `modbus-tcp`: Modbus TCP client (master) for the four Modbus tables.
//!
//! Further drivers (EtherCAT, PROFINET, proprietary fieldbuses) are loaded
//! from shared libraries implementing `structure_driver.h`.

pub mod loopback;
pub mod modbus;

use structure_runtime::DriverRegistry;

/// A registry with the built-in drivers.
pub fn registry() -> DriverRegistry {
    let mut r = DriverRegistry::new();
    r.register("loopback", loopback::create);
    r.register("modbus-tcp", modbus::create);
    r
}
