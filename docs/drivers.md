# I/O drivers

Drivers connect the process images of a configuration to the outside
world: fieldbuses and industrial networks (Modbus TCP, EtherCAT,
PROFINET, ...), local I/O hardware, or simulations. A driver can be built
into the runtime or loaded from a shared library, so open source and
proprietary drivers are used the same way.

## The process image

Every resource has an input area (`%I`), an output area (`%Q`) and a memory
area (`%M`), plus an area per *module* for hierarchical addresses: `%IW1.2`
and `%IX1.4.0` belong to module 1 of the inputs (`%I1`), `%QD2.1.0` to module
2.1 of the outputs (`%Q2.1`). The linker sizes each area to the
directly represented variables the configuration uses.

The bytes of an area are those of its directly represented variables:

| Variable | Bytes of the area |
|---|---|
| `%IXa.b` (`%Ia.b`) | bit `b` (LSB = 0) of byte `a` |
| `%IBn` | byte `n` |
| `%IWn` | bytes `2n`, `2n+1` (little-endian) |
| `%IDn` | bytes `4n` to `4n+3` (little-endian) |
| `%ILn` | bytes `8n` to `8n+7` (little-endian) |

For a module area the same applies to the last address fields: `%IW1.2` is
bytes 4 and 5 of the area of module 1; `%IX1.4.0` bit 0 of its byte 4. A
driver exchanges *bindings*: an input or output area, optionally restricted to
a byte range.

## When data is exchanged

The runtime calls the drivers of a resource at task boundaries (2.7.2):

* the **inputs** are read when a task is activated, before its programs and
  function blocks execute;
* the **outputs** are written when the last of them has completed.

The programs of a task therefore see consistent inputs for a whole cycle, and
the outputs of a cycle leave together. A driver associated with a task
(`"task"`) exchanges data only around the executions of that task, for a
fieldbus cycle synchronised with it; a driver without task exchanges around
the executions of every task of its resource and of programs without task.
With preemptive scheduling, a task that preempts another refreshes the
inputs of the drivers it shares with it.

Run with `--realtime` when drivers talk to real devices: in simulated time
(the default) the tasks run as fast as possible.

## Configuration

```
structure run plant.st --realtime --drivers drivers.json
```

```json
{
  "drivers": [
    {
      "name": "rack1",
      "type": "modbus-tcp",
      "resource": "PLC",
      "task": "FAST",
      "status": "RACK1_OK",
      "on_fault": "continue",
      "params": { "host": "192.168.0.20", "port": 502, "unit": 1 },
      "bindings": [
        { "area": "%I1", "size": 16, "params": { "table": "input_registers", "start": 0 } },
        { "area": "%I1", "offset": 16, "size": 2, "params": { "table": "discrete_inputs", "start": 0 } },
        { "area": "%Q1", "size": 8, "params": { "table": "holding_registers", "start": 100 } }
      ]
    },
    {
      "name": "ethercat",
      "library": "/opt/drivers/libmy_ethercat.so",
      "task": "FAST",
      "params": { "interface": "eth1" },
      "bindings": [ { "area": "%I2" }, { "area": "%Q2" } ]
    }
  ]
}
```

| Field | | |
|---|---|---|
| `name` | required | name of the driver instance (in messages) |
| `type` | one of `type`, `library` | a built-in driver (`structure drivers` lists them) |
| `library` | | path of a shared library implementing `structure_driver.h` |
| `resource` | when several | resource whose process image is exchanged |
| `task` | optional | task the exchange is synchronised with |
| `params` | optional | parameters of the driver (any JSON) |
| `bindings` | required | the areas exchanged: `area` (`%I`, `%Q`, optionally with a module path: `%I1`, `%Q2.1`), `offset` and `size` in bytes (default: the whole area), `params` of the binding (any JSON) |
| `status` | optional | a BOOL variable (`RES.PROG.VAR`, a global variable, or `%MX...`) set to TRUE while the driver works and FALSE when it fails |
| `on_fault` | `continue` | `continue`: report the fault and keep running, the inputs keep their last values; `stop`: report a run-time error and stop the resource |

A driver is healthy while the last reading of its inputs and the last
writing of its outputs succeeded. Changes are reported (`structure run`
prints them) and available to embedding programs as `Runtime::driver_events`
and `Runtime::drivers()`. When a resource stops (an error, a driver fault
with `on_fault: stop`, or the end of `structure run`), its drivers are asked
to bring their outputs into a safe state.

## Built-in drivers

### `loopback`

The k-th input binding reads back the data last written to the k-th output
binding. For tests and simulations.

### `modbus-tcp`

A Modbus TCP client (master).

| Driver parameter | Default | |
|---|---|---|
| `host` | required | server (device or gateway) |
| `port` | 502 | |
| `unit` | 1 | unit identifier |
| `mode` | `polled` | `polled`: a thread polls the device every `poll_interval_ms` and the task cycles exchange the latest data, so a slow device never delays a task; `synchronous`: the requests are made in the task cycle |
| `poll_interval_ms` | 10 | |
| `timeout_ms` | 1000 | timeout of connecting and of each request |
| `stop_outputs` | `zero` | outputs when the resource stops: `zero` or `hold` |

| Binding parameter | Default | |
|---|---|---|
| `table` | required | `coils`, `discrete_inputs`, `input_registers`, `holding_registers` (outputs: `coils` or `holding_registers`) |
| `start` | 0 | first Modbus address |
| `count` | the size of the binding | number of bits or registers |
| `unit` | the driver's | |

Register `start + n` is word `n` of the binding (`%IWn` of the area for a
binding at offset 0), bit `start + n` is bit `n`. Requests larger than the
protocol allows are split. The client reconnects after errors; outputs are
only written once the programs have produced them.

## Writing a driver

### In Rust

Implement `structure_runtime::driver::IoDriver`:

```rust
use structure_runtime::driver::{Binding, IoDriver};

struct MyDriver { /* ... */ }

impl IoDriver for MyDriver {
    fn open(&mut self, bindings: &[Binding]) -> Result<(), String> { /* connect, check the bindings */ Ok(()) }
    fn read_inputs(&mut self, data: &mut [&mut [u8]]) -> Result<(), String> { /* fill the input bindings */ Ok(()) }
    fn write_outputs(&mut self, data: &[&[u8]]) -> Result<(), String> { /* send the output bindings */ Ok(()) }
    fn stop(&mut self) { /* safe state */ }
    fn close(&mut self) { /* disconnect */ }
}
```

`data[i]` is the process image of `bindings[i]` (empty for bindings of the
other direction); `structure_runtime::driver::image` has helpers for bits
and words. Either register it in a `DriverRegistry` of a program embedding
the runtime, or build it as a shared library with

```rust
fn create(params: &serde_json::Value) -> Result<Box<dyn IoDriver>, String> { /* ... */ }
structure_runtime::export_driver!(create);
```

in a crate with `crate-type = ["cdylib"]` (see `examples/drivers/rust`).
Panics are caught at the interface and reported as faults.

### In C, C++ or any other language

Implement the functions of `crates/structure-runtime/include/structure_driver.h`
and export `structure_driver_api_v1`. `examples/drivers/c/loopback.c` is a
complete driver:

```
cc -shared -fPIC -I crates/structure-runtime/include examples/drivers/c/loopback.c -o libloopback.so
```

The interface is versioned (`abi_version`); the runtime refuses libraries
of another version. All functions of an instance are called from the
scheduling thread, one at a time; parameters arrive as JSON text; errors are
returned as a nonzero result with a message.

### Guidelines

* `read_inputs` and `write_outputs` run in every cycle of the associated
  tasks: they should only take as long as the exchange itself. Devices that
  answer slowly are best polled from a thread of the driver (as `modbus-tcp`
  does in `polled` mode).
* Report problems as errors rather than hiding them: the runtime keeps the
  inputs of a failed exchange unchanged, reports the fault and applies the
  configured policy.
* Put outputs into a safe state in `stop`.

### Fieldbus stacks

Cyclic fieldbuses map naturally onto a driver associated with the task whose
interval is the bus cycle:

* **EtherCAT** (for example with SOEM or the IgH master): configure the
  master and the slaves in `open` and take the offsets of the process data
  objects from the binding parameters; in `read_inputs` receive the process
  data of the cycle and copy the inputs, in `write_outputs` copy the outputs
  and send the frame. Run with `--realtime`, on a real-time kernel for tight
  cycle times.
* **PROFINET** (for example with p-net as IO device, or a controller stack):
  the stack exchanges cyclic data from its own thread; the driver copies the
  latest input data in `read_inputs` and hands the outputs over in
  `write_outputs`, and reports the state of the application relation as its
  health.

These stacks have their own licence terms, which apply to the driver library
built with them.
