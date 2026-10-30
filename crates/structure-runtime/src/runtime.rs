//! Configurations, resources and tasks (1.4.1, 2.7): starting and stopping,
//! initialisation (2.4.2), task scheduling (2.7.2) and access to variables.

use crate::driver::{parse_area, Binding, Direction, DriverConfig, DriverEvent, DriverRegistry, DriverSpec, DriverStatus, FaultPolicy, IoDriver};
use crate::error::{RuntimeError, TrapCode};
use crate::image::{Area, Image, Source, TypeDesc};
use crate::memory::Memory;
use crate::value::{read_value, write_value, Value};
use crate::vm::{Context, Exit};
use std::time::Instant;

/// How the time of the task clock advances.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClockMode {
    /// Deterministic simulated time: every executed instruction takes
    /// `ns_per_instruction` nanoseconds; idle time is skipped.
    Virtual,
    /// Wall-clock time.
    RealTime,
}

/// What to do when an annex E error is detected during execution
/// (1.5.1 d 4).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ErrorPolicy {
    /// Stop the resource in which the error occurred.
    StopResource,
    /// Abandon the current execution of the program organisation unit and
    /// continue scheduling.
    AbortExecution,
}

#[derive(Clone, Debug)]
pub struct Options {
    /// Preemptive (table 50 feature 5b) or non-preemptive (5a) scheduling.
    pub preemptive: bool,
    pub clock: ClockMode,
    pub ns_per_instruction: u64,
    /// Maximum instructions per execution of a program organisation unit.
    pub watchdog: u64,
    pub on_error: ErrorPolicy,
    /// Report task overruns (2.7.2) as errors.
    pub report_overruns: bool,
}

impl Default for Options {
    fn default() -> Self {
        Options {
            preemptive: true,
            clock: ClockMode::Virtual,
            ns_per_instruction: 20,
            watchdog: 50_000_000,
            on_error: ErrorPolicy::StopResource,
            report_overruns: true,
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct JobStats {
    pub executions: u64,
    pub last_start: i64,
    pub last_end: i64,
    pub max_steps: u64,
}

#[derive(Clone, Debug)]
struct TaskState {
    next_due: Option<i64>,
    last_single: bool,
    pub activations: u64,
    pub overruns: u64,
    /// Jobs of the task activated and not yet completed; the outputs of the
    /// drivers are written when it drops to zero.
    pending: usize,
}

#[derive(Clone, Debug)]
struct JobState {
    ready: bool,
    ready_since: i64,
    priority: u32,
    stats: JobStats,
}

struct ResourceState {
    running: bool,
    tasks: Vec<TaskState>,
    jobs: Vec<JobState>,
    /// Stack of executing jobs (the last one runs; earlier ones were
    /// preempted).
    active: Vec<(usize, Context)>,
}

/// Lowest scheduling priority, used for programs without task association.
pub const BACKGROUND_PRIORITY: u32 = u32::MAX;

/// A driver attached to a resource.
struct DriverHost {
    name: String,
    driver: Box<dyn IoDriver>,
    resource: usize,
    task: Option<usize>,
    bindings: Vec<Binding>,
    /// Byte address in memory of each binding.
    addrs: Vec<usize>,
    /// Bit address of the BOOL status variable.
    status: Option<u64>,
    on_fault: FaultPolicy,
    healthy: Option<bool>,
    /// Result of the last exchange of each direction (open counts for both).
    inputs_ok: bool,
    outputs_ok: bool,
    message: String,
    exchanges: u64,
    opened: bool,
}

impl Drop for DriverHost {
    fn drop(&mut self) {
        if self.opened {
            self.driver.close();
        }
    }
}

/// An execution event, for tracing.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Event {
    Start { time: i64, resource: usize, job: usize },
    End { time: i64, resource: usize, job: usize },
    Preempt { time: i64, resource: usize, job: usize },
}

pub struct Runtime {
    pub image: Image,
    pub mem: Memory,
    pub options: Options,
    /// Current time in microseconds since start.
    now_us: i64,
    ns_accum: u64,
    resources: Vec<ResourceState>,
    pub errors: Vec<RuntimeError>,
    started: bool,
    epoch: Instant,
    pub trace: Option<Vec<Event>>,
    drivers: Vec<DriverHost>,
    /// Changes of the status of the drivers.
    pub driver_events: Vec<DriverEvent>,
}

impl Runtime {
    pub fn new(image: Image, options: Options) -> Runtime {
        let mut mem = Memory::new(image.memory_size as usize);
        for (addr, bytes) in &image.data {
            let _ = mem.write_bytes(*addr, bytes);
        }
        let resources = image
            .resources
            .iter()
            .map(|r| ResourceState {
                running: false,
                tasks: r.tasks.iter().map(|_| TaskState { next_due: None, last_single: false, activations: 0, overruns: 0, pending: 0 }).collect(),
                jobs: r
                    .jobs
                    .iter()
                    .map(|j| JobState {
                        ready: false,
                        ready_since: 0,
                        priority: j.task.map(|t| r.tasks[t].priority).unwrap_or(BACKGROUND_PRIORITY),
                        stats: JobStats::default(),
                    })
                    .collect(),
                active: Vec::new(),
            })
            .collect();
        Runtime {
            image,
            mem,
            options,
            now_us: 0,
            ns_accum: 0,
            resources,
            errors: Vec::new(),
            started: false,
            epoch: Instant::now(),
            trace: None,
            drivers: Vec::new(),
            driver_events: Vec::new(),
        }
    }

    pub fn now(&self) -> i64 {
        self.now_us
    }

    fn run_init(&mut self, routine: u32, stack: Area, resource: &str) -> Result<(), RuntimeError> {
        let mut ctx = Context::new(&self.image, routine, 0, stack, self.now_us);
        let _ = self.mem.zero(ctx.fp, self.image.routines[routine as usize].frame_size as u64 * 8);
        match ctx.run(&self.image, &mut self.mem, u64::MAX) {
            Exit::Done => Ok(()),
            Exit::Yield => unreachable!(),
            Exit::Trap(code, pc) => Err(self.make_error(code, pc, resource, "initialization")),
        }
    }

    fn make_error(&self, code: TrapCode, pc: u32, resource: &str, job: &str) -> RuntimeError {
        RuntimeError {
            code,
            resource: resource.to_string(),
            job: job.to_string(),
            routine: self.image.routine_at(pc).map(|r| r.name.clone()).unwrap_or_default(),
            location: self.image.location(pc).cloned(),
            time: self.now_us,
        }
    }

    /// Cold restart (2.4.2 rule 2): all variables take their initial values.
    pub fn cold_start(&mut self) -> Result<(), RuntimeError> {
        self.start(None)
    }

    /// Warm restart (2.4.2 rule 1): retentive variables keep the values in
    /// `retained` (as produced by [`Runtime::retain_snapshot`]); if the data
    /// does not match the image, a cold restart is performed.
    pub fn warm_start(&mut self, retained: &[u8]) -> Result<(), RuntimeError> {
        self.start(Some(retained))
    }

    fn start(&mut self, retained: Option<&[u8]>) -> Result<(), RuntimeError> {
        // Reset memory (except constant data) for a clean start.
        self.mem.bytes.fill(0);
        for (addr, bytes) in &self.image.data {
            let _ = self.mem.write_bytes(*addr, bytes);
        }
        self.errors.clear();
        self.now_us = 0;
        self.ns_accum = 0;
        self.epoch = Instant::now();
        if let Some(r) = self.image.config_init {
            let stack = self.image.config_init_stack;
            self.run_init(r, stack, "")?;
        }
        for i in 0..self.image.resources.len() {
            let r = self.image.resources[i].init_routine;
            let stack = self.image.resources[i].init_stack;
            let name = self.image.resources[i].name.clone();
            self.run_init(r, stack, &name)?;
        }
        if let Some(data) = retained {
            if data.len() == self.retain_size() + 8 && data[..8] == self.retain_signature().to_le_bytes() {
                self.restore_retained(&data[8..]);
            }
        }
        for (ri, rs) in self.resources.iter_mut().enumerate() {
            rs.running = true;
            rs.active.clear();
            for t in &mut rs.tasks {
                t.next_due = Some(0);
                t.last_single = false;
                t.activations = 0;
                t.overruns = 0;
                t.pending = 0;
            }
            for (ji, j) in rs.jobs.iter_mut().enumerate() {
                j.stats = JobStats::default();
                // Programs without a task are scheduled upon starting (2.7.2 rule 4).
                j.ready = self.image.resources[ri].jobs[ji].task.is_none();
                j.ready_since = 0;
            }
        }
        self.started = true;
        self.driver_events.clear();
        for di in 0..self.drivers.len() {
            let h = &mut self.drivers[di];
            h.healthy = None;
            h.exchanges = 0;
            if h.opened {
                h.driver.close();
            }
            let r = h.driver.open(&h.bindings);
            h.opened = r.is_ok();
            self.driver_result(di, None, r);
        }
        Ok(())
    }

    /// Stops all resources (their tasks are disabled).
    pub fn stop(&mut self) {
        for ri in 0..self.resources.len() {
            self.stop_resource(ri);
        }
        self.started = false;
    }

    fn stop_resource(&mut self, ri: usize) {
        let r = &mut self.resources[ri];
        let was_running = r.running;
        r.running = false;
        r.active.clear();
        for j in &mut r.jobs {
            j.ready = false;
        }
        if was_running {
            for h in self.drivers.iter_mut().filter(|h| h.resource == ri && h.opened) {
                h.driver.stop();
            }
        }
    }

    // ------------------------------------------------------------ drivers

    /// Attaches an I/O driver to a resource, as described by `spec` (its
    /// type and parameters are not used here). Drivers are opened when the
    /// configuration starts.
    pub fn add_driver(&mut self, spec: &DriverSpec, driver: Box<dyn IoDriver>) -> Result<(), String> {
        let name = &spec.name;
        let ri = match &spec.resource {
            Some(r) => self.image.resources.iter().position(|x| x.name.eq_ignore_ascii_case(r)).ok_or_else(|| format!("driver {}: unknown resource '{}'", name, r))?,
            None if self.image.resources.len() == 1 => 0,
            None => return Err(format!("driver {}: the configuration has several resources; give the \"resource\" of the driver", name)),
        };
        let res = &self.image.resources[ri];
        let task = match &spec.task {
            Some(t) => Some(res.tasks.iter().position(|x| x.name.eq_ignore_ascii_case(t)).ok_or_else(|| format!("driver {}: resource {} has no task '{}'", name, res.name, t))?),
            None => None,
        };
        let mut bindings = Vec::new();
        let mut addrs = Vec::new();
        for b in &spec.bindings {
            let (direction, path) = parse_area(&b.area).map_err(|e| format!("driver {}: {}", name, e))?;
            let loc = if direction == Direction::Input { 'I' } else { 'Q' };
            let area = if path.is_empty() {
                Some(if direction == Direction::Input { res.input } else { res.output })
            } else {
                res.io_modules.iter().find(|m| m.location == loc && m.path == path).map(|m| m.area)
            };
            let (base, avail) = area.map(|a| (a.base as usize, a.size as usize)).unwrap_or((0, 0));
            if area.is_some() && b.offset > avail {
                return Err(format!("driver {}: offset {} is beyond the area {} ({} bytes)", name, b.offset, b.area, avail));
            }
            let rest = avail.saturating_sub(b.offset);
            let size = b.size.map(|s| s.min(rest)).unwrap_or(rest);
            bindings.push(Binding { area: b.area.clone(), direction, path, offset: b.offset, size, params: b.params.clone() });
            addrs.push(base + b.offset);
        }
        let status = match &spec.status {
            Some(path) => match self.resolve(path) {
                Ok((addr, TypeDesc::Bool)) => Some(addr),
                Ok(_) => return Err(format!("driver {}: the status variable {} shall be of type BOOL", name, path)),
                Err(e) => return Err(format!("driver {}: status variable: {}", name, e)),
            },
            None => None,
        };
        self.drivers.push(DriverHost {
            name: name.clone(),
            driver,
            resource: ri,
            task,
            bindings,
            addrs,
            status,
            on_fault: spec.on_fault,
            healthy: None,
            inputs_ok: true,
            outputs_ok: true,
            message: String::new(),
            exchanges: 0,
            opened: false,
        });
        Ok(())
    }

    /// Creates and attaches the drivers of a driver configuration.
    pub fn load_drivers(&mut self, config: &DriverConfig, registry: &DriverRegistry) -> Result<(), String> {
        for spec in &config.drivers {
            if self.drivers.iter().any(|d| d.name.eq_ignore_ascii_case(&spec.name)) {
                return Err(format!("duplicate driver name {}", spec.name));
            }
            let driver = registry.create(spec)?;
            self.add_driver(spec, driver)?;
        }
        Ok(())
    }

    /// The status of the drivers.
    pub fn drivers(&self) -> Vec<DriverStatus> {
        self.drivers
            .iter()
            .map(|h| DriverStatus {
                name: h.name.clone(),
                resource: self.image.resources[h.resource].name.clone(),
                healthy: h.healthy.unwrap_or(false),
                message: h.message.clone(),
                exchanges: h.exchanges,
            })
            .collect()
    }

    /// The bindings of a driver, as given to it when it was opened.
    pub fn driver_bindings(&self, name: &str) -> Option<&[Binding]> {
        self.drivers.iter().find(|h| h.name.eq_ignore_ascii_case(name)).map(|h| h.bindings.as_slice())
    }

    fn driver_applies(h: &DriverHost, ri: usize, task: Option<usize>) -> bool {
        h.resource == ri && h.opened && (h.task.is_none() || h.task == task)
    }

    /// Reads the inputs of the drivers of a resource before the execution
    /// of a task (`None`: of a program without task).
    fn exchange_inputs(&mut self, ri: usize, task: Option<usize>) {
        for di in 0..self.drivers.len() {
            let h = &mut self.drivers[di];
            if !Self::driver_applies(h, ri, task) || !h.bindings.iter().any(|b| b.direction == Direction::Input) {
                continue;
            }
            let mut bufs: Vec<Vec<u8>> = h
                .bindings
                .iter()
                .zip(&h.addrs)
                .map(|(b, &a)| if b.direction == Direction::Input { self.mem.bytes[a..a + b.size].to_vec() } else { Vec::new() })
                .collect();
            let mut refs: Vec<&mut [u8]> = bufs.iter_mut().map(|b| b.as_mut_slice()).collect();
            let r = h.driver.read_inputs(&mut refs);
            if r.is_ok() {
                for ((b, &a), buf) in h.bindings.iter().zip(&h.addrs).zip(&bufs) {
                    if b.direction == Direction::Input {
                        self.mem.bytes[a..a + b.size].copy_from_slice(buf);
                    }
                }
            }
            self.driver_result(di, Some(Direction::Input), r);
        }
    }

    /// Writes the outputs of the drivers of a resource after the execution
    /// of a task.
    fn exchange_outputs(&mut self, ri: usize, task: Option<usize>) {
        for di in 0..self.drivers.len() {
            let h = &mut self.drivers[di];
            if !Self::driver_applies(h, ri, task) || !h.bindings.iter().any(|b| b.direction == Direction::Output) {
                continue;
            }
            let data: Vec<&[u8]> =
                h.bindings.iter().zip(&h.addrs).map(|(b, &a)| if b.direction == Direction::Output { &self.mem.bytes[a..a + b.size] } else { &[][..] }).collect();
            let r = h.driver.write_outputs(&data);
            self.driver_result(di, Some(Direction::Output), r);
        }
    }

    /// Records the result of opening a driver (`exchange` = None) or of an
    /// exchange. A driver is healthy while the last exchange of each
    /// direction succeeded.
    fn driver_result(&mut self, di: usize, exchange: Option<Direction>, r: Result<(), String>) {
        let now = self.now_us;
        let h = &mut self.drivers[di];
        let success = r.is_ok();
        match exchange {
            None => {
                h.inputs_ok = success;
                h.outputs_ok = success;
            }
            Some(Direction::Input) => h.inputs_ok = success,
            Some(Direction::Output) => h.outputs_ok = success,
        }
        if success && exchange.is_some() {
            h.exchanges += 1;
        }
        let ok = h.inputs_ok && h.outputs_ok;
        let message = match r {
            Err(e) => e,
            Ok(()) if ok => String::new(),
            Ok(()) => h.message.clone(),
        };
        let changed = h.healthy != Some(ok) || (!ok && message != h.message);
        h.healthy = Some(ok);
        h.message = message.clone();
        if let Some(addr) = h.status {
            let _ = self.mem.store(crate::ty::Ty::Bool, addr, ok as u64);
        }
        if !changed {
            return;
        }
        let (name, ri, policy) = (h.name.clone(), h.resource, h.on_fault);
        self.driver_events.push(DriverEvent { time: now, driver: name.clone(), healthy: ok, message: message.clone() });
        if !ok && policy == FaultPolicy::Stop && self.resources[ri].running {
            self.errors.push(RuntimeError {
                code: TrapCode::DriverFault,
                resource: self.image.resources[ri].name.clone(),
                job: format!("driver {}: {}", name, message),
                routine: String::new(),
                location: None,
                time: now,
            });
            self.stop_resource(ri);
        }
    }

    pub fn resource_running(&self, i: usize) -> bool {
        self.resources[i].running
    }

    fn retain_size(&self) -> usize {
        self.image.retain.iter().map(|(_, bits)| bits.div_ceil(8) as usize).sum()
    }

    fn retain_signature(&self) -> u64 {
        let mut h: u64 = 0xcbf29ce484222325;
        for (a, b) in &self.image.retain {
            for x in [*a, *b] {
                h ^= x;
                h = h.wrapping_mul(0x100000001b3);
            }
        }
        h
    }

    /// Captures the values of all retentive variables.
    pub fn retain_snapshot(&self) -> Vec<u8> {
        let mut out = self.retain_signature().to_le_bytes().to_vec();
        for (addr, bits) in &self.image.retain {
            let n = bits.div_ceil(8);
            let mut tmp = Memory::new(n as usize);
            let _ = copy_between(&self.mem, *addr, &mut tmp, 0, *bits);
            out.extend_from_slice(&tmp.bytes);
        }
        out
    }

    fn restore_retained(&mut self, data: &[u8]) {
        let mut off = 0usize;
        for (addr, bits) in self.image.retain.clone() {
            let n = bits.div_ceil(8) as usize;
            let src = Memory { bytes: data[off..off + n].to_vec() };
            let _ = copy_between(&src, 0, &mut self.mem, addr, bits);
            off += n;
        }
    }

    // ------------------------------------------------------------ scheduling

    fn read_source(&self, s: &Source) -> i64 {
        match s {
            Source::Const(v) => *v,
            Source::Var { addr, ty } => {
                let raw = self.mem.load(*ty, *addr).unwrap_or(0);
                crate::ty::to_i128(*ty, raw) as i64
            }
        }
    }

    /// Evaluates task triggers at the current time (2.7.2 rules 1 and 2).
    fn check_triggers(&mut self) {
        let now = self.now_us;
        for ri in 0..self.resources.len() {
            if !self.resources[ri].running {
                continue;
            }
            for ti in 0..self.image.resources[ri].tasks.len() {
                let info = &self.image.resources[ri].tasks[ti];
                let single = info.single.as_ref().map(|s| self.read_source(s) != 0).unwrap_or(false);
                let interval = info.interval.as_ref().map(|s| self.read_source(s)).unwrap_or(0);
                let mut activate = false;
                let ts = &mut self.resources[ri].tasks[ti];
                if single && !ts.last_single {
                    activate = true;
                }
                ts.last_single = single;
                if interval > 0 && !single {
                    match ts.next_due {
                        Some(due) if now >= due => {
                            activate = true;
                            let mut next = due + interval;
                            if next <= now {
                                // Missed activations.
                                let missed = (now - next) / interval + 1;
                                next += missed * interval;
                                ts.overruns += missed as u64;
                            }
                            ts.next_due = Some(next);
                        }
                        None => ts.next_due = Some(now + interval),
                        _ => {}
                    }
                } else if interval > 0 && single {
                    // Periodic scheduling is suspended while SINGLE is 1.
                    ts.next_due = Some(now + interval);
                }
                if activate {
                    self.activate_task(ri, ti);
                }
            }
        }
    }

    fn activate_task(&mut self, ri: usize, ti: usize) {
        let now = self.now_us;
        let mut readied = 0;
        self.resources[ri].tasks[ti].activations += 1;
        for ji in 0..self.image.resources[ri].jobs.len() {
            if self.image.resources[ri].jobs[ji].task != Some(ti) {
                continue;
            }
            let busy = self.resources[ri].jobs[ji].ready || self.resources[ri].active.iter().any(|(j, _)| *j == ji);
            if busy {
                // 2.7.2: a task fails to be scheduled or to meet its deadline.
                self.resources[ri].tasks[ti].overruns += 1;
                if self.options.report_overruns {
                    let task = self.image.resources[ri].tasks[ti].name.clone();
                    let job = self.image.resources[ri].jobs[ji].name.clone();
                    self.errors.push(RuntimeError {
                        code: TrapCode::Watchdog,
                        resource: self.image.resources[ri].name.clone(),
                        job: format!("{} (task {} missed its deadline)", job, task),
                        routine: String::new(),
                        location: None,
                        time: now,
                    });
                }
                continue;
            }
            let js = &mut self.resources[ri].jobs[ji];
            js.ready = true;
            js.ready_since = now;
            readied += 1;
        }
        if readied > 0 {
            // the inputs are read before the programs of the task execute
            let first = self.resources[ri].tasks[ti].pending == 0;
            self.resources[ri].tasks[ti].pending += readied;
            if first {
                self.exchange_inputs(ri, Some(ti));
            }
        }
    }

    /// Chooses the job to execute next on a resource, starting it if needed.
    fn select(&mut self, ri: usize) -> bool {
        let rs = &self.resources[ri];
        // Highest priority ready job; ties broken by longest waiting time.
        let mut best: Option<usize> = None;
        for (ji, j) in rs.jobs.iter().enumerate() {
            if !j.ready {
                continue;
            }
            best = match best {
                None => Some(ji),
                Some(b) => {
                    let bj = &rs.jobs[b];
                    if (j.priority, j.ready_since) < (bj.priority, bj.ready_since) {
                        Some(ji)
                    } else {
                        Some(b)
                    }
                }
            };
        }
        let start = match (best, rs.active.last()) {
            (None, None) => return false,
            (None, Some(_)) => return true,
            (Some(b), None) => Some(b),
            (Some(b), Some((cur, _))) => {
                if self.options.preemptive && rs.jobs[b].priority < rs.jobs[*cur].priority {
                    if let Some(t) = &mut self.trace {
                        t.push(Event::Preempt { time: self.now_us, resource: ri, job: *cur });
                    }
                    Some(b)
                } else {
                    None
                }
            }
        };
        if let Some(ji) = start {
            if self.image.resources[ri].jobs[ji].task.is_none() {
                self.exchange_inputs(ri, None);
            }
            let info = &self.image.resources[ri].jobs[ji];
            let mut ctx = Context::new(&self.image, info.routine, info.instance, info.stack, self.now_us);
            if !self.resources[ri].active.is_empty() {
                // The preempted job keeps its stack; give the new job a stack
                // beyond it. Each job has its own stack area, so nothing to do.
            }
            let fsize = self.image.routines[info.routine as usize].frame_size as u64;
            let _ = self.mem.zero(ctx.fp, fsize * 8);
            ctx.now = self.now_us;
            let rs = &mut self.resources[ri];
            rs.jobs[ji].ready = false;
            rs.jobs[ji].stats.last_start = self.now_us;
            rs.active.push((ji, ctx));
            if let Some(t) = &mut self.trace {
                t.push(Event::Start { time: self.now_us, resource: ri, job: ji });
            }
        }
        true
    }

    fn next_event(&self) -> Option<i64> {
        let mut next: Option<i64> = None;
        for (ri, rs) in self.resources.iter().enumerate() {
            if !rs.running {
                continue;
            }
            for (ti, ts) in rs.tasks.iter().enumerate() {
                if let Some(d) = ts.next_due {
                    let info = &self.image.resources[ri].tasks[ti];
                    if info.interval.is_some() {
                        next = Some(next.map_or(d, |n| n.min(d)));
                    }
                }
            }
        }
        next
    }

    /// Advances execution until `until_us` (virtual or real time).
    pub fn run_until(&mut self, until_us: i64) {
        if !self.started {
            return;
        }
        loop {
            if self.options.clock == ClockMode::RealTime {
                self.now_us = self.epoch.elapsed().as_micros() as i64;
            }
            if self.now_us >= until_us {
                break;
            }
            self.check_triggers();
            let mut busy = false;
            for ri in 0..self.resources.len() {
                if self.resources[ri].running && self.select(ri) {
                    busy = true;
                }
            }
            if !busy {
                let next = self.next_event().unwrap_or(until_us).min(until_us);
                match self.options.clock {
                    ClockMode::Virtual => {
                        self.now_us = if next > self.now_us { next } else { until_us };
                    }
                    ClockMode::RealTime => {
                        let wait = (next - self.now_us).clamp(0, 1000) as u64;
                        std::thread::sleep(std::time::Duration::from_micros(wait));
                    }
                }
                continue;
            }
            let quantum = self.quantum(until_us);
            let mut max_steps = 0;
            for ri in 0..self.resources.len() {
                max_steps = max_steps.max(self.execute(ri, quantum));
            }
            if self.options.clock == ClockMode::Virtual {
                self.advance(max_steps.max(1));
            }
        }
    }

    /// Runs for the given duration (microseconds).
    pub fn run_for(&mut self, duration_us: i64) {
        let until = self.now_us + duration_us;
        self.run_until(until);
    }

    fn quantum(&self, until_us: i64) -> u64 {
        if self.options.clock == ClockMode::RealTime {
            return 10_000;
        }
        let horizon = self.next_event().unwrap_or(until_us).min(until_us.max(self.now_us + 1));
        let dt_ns = ((horizon - self.now_us).max(1) as u64) * 1000;
        (dt_ns / self.options.ns_per_instruction.max(1)).clamp(1, 100_000)
    }

    fn advance(&mut self, steps: u64) {
        let ns = steps * self.options.ns_per_instruction.max(1) + self.ns_accum;
        self.now_us += (ns / 1000) as i64;
        self.ns_accum = ns % 1000;
    }

    /// Executes up to `budget` instructions of the current job of a resource.
    fn execute(&mut self, ri: usize, budget: u64) -> u64 {
        let rs = &mut self.resources[ri];
        if !rs.running {
            return 0;
        }
        let Some((ji, mut ctx)) = rs.active.pop() else { return 0 };
        let before = ctx.steps;
        ctx.now = self.now_us;
        let exit = ctx.run(&self.image, &mut self.mem, budget);
        let steps = ctx.steps - before;
        let watchdog_hit = ctx.steps > self.options.watchdog;
        match exit {
            Exit::Done => {
                let rs = &mut self.resources[ri];
                let st = &mut rs.jobs[ji].stats;
                st.executions += 1;
                st.last_end = self.now_us;
                st.max_steps = st.max_steps.max(ctx.steps);
                if let Some(t) = &mut self.trace {
                    t.push(Event::End { time: self.now_us, resource: ri, job: ji });
                }
                self.job_completed(ri, ji);
                if self.image.resources[ri].jobs[ji].task.is_none() && self.resources[ri].running {
                    // Re-scheduled as soon as its execution terminates (2.7.2 rule 4).
                    let js = &mut self.resources[ri].jobs[ji];
                    js.ready = true;
                    js.ready_since = self.now_us;
                }
            }
            Exit::Yield if !watchdog_hit => {
                self.resources[ri].active.push((ji, ctx));
            }
            Exit::Yield | Exit::Trap(..) => {
                let (code, pc) = match exit {
                    Exit::Trap(c, pc) => (c, pc),
                    _ => (TrapCode::Watchdog, ctx.pc),
                };
                let name = self.image.resources[ri].name.clone();
                let job = self.image.resources[ri].jobs[ji].name.clone();
                let err = self.make_error(code, pc, &name, &job);
                self.errors.push(err);
                match self.options.on_error {
                    ErrorPolicy::StopResource => self.stop_resource(ri),
                    ErrorPolicy::AbortExecution => {
                        self.job_completed(ri, ji);
                        if self.image.resources[ri].jobs[ji].task.is_none() {
                            let js = &mut self.resources[ri].jobs[ji];
                            js.ready = true;
                            js.ready_since = self.now_us;
                        }
                    }
                }
            }
        }
        steps
    }

    /// A job has completed (or was abandoned): the outputs of the drivers are
    /// written once all the jobs of its task activation have completed.
    fn job_completed(&mut self, ri: usize, ji: usize) {
        match self.image.resources[ri].jobs[ji].task {
            Some(ti) => {
                let ts = &mut self.resources[ri].tasks[ti];
                ts.pending = ts.pending.saturating_sub(1);
                if ts.pending == 0 {
                    self.exchange_outputs(ri, Some(ti));
                }
            }
            None => self.exchange_outputs(ri, None),
        }
    }

    /// Executes each ready job once, in priority order, without regard to
    /// task timing ("single scan"), then returns. Useful for tests.
    pub fn scan(&mut self) {
        for ri in 0..self.resources.len() {
            if !self.resources[ri].running {
                continue;
            }
            let n = self.image.resources[ri].jobs.len();
            let mut order: Vec<usize> = (0..n).collect();
            order.sort_by_key(|j| self.resources[ri].jobs[*j].priority);
            for ji in order {
                self.resources[ri].jobs[ji].ready = true;
                self.resources[ri].jobs[ji].ready_since = self.now_us;
                match self.image.resources[ri].jobs[ji].task {
                    Some(ti) => {
                        let first = self.resources[ri].tasks[ti].pending == 0;
                        self.resources[ri].tasks[ti].pending += 1;
                        if first {
                            self.exchange_inputs(ri, Some(ti));
                        }
                    }
                    None => self.exchange_inputs(ri, None),
                }
                // run this job to completion
                let info = &self.image.resources[ri].jobs[ji];
                let ctx = Context::new(&self.image, info.routine, info.instance, info.stack, self.now_us);
                let fsize = self.image.routines[info.routine as usize].frame_size as u64;
                let _ = self.mem.zero(ctx.fp, fsize * 8);
                self.resources[ri].jobs[ji].ready = false;
                self.resources[ri].active.push((ji, ctx));
                while !self.resources[ri].active.is_empty() && self.resources[ri].running {
                    let steps = self.execute(ri, 100_000);
                    if self.options.clock == ClockMode::Virtual {
                        self.advance(steps.max(1));
                    }
                    // A background job re-marks itself ready on completion; clear it.
                }
                self.resources[ri].jobs[ji].ready = self.image.resources[ri].jobs[ji].task.is_none() && self.resources[ri].running;
                if !self.resources[ri].running {
                    break;
                }
            }
        }
    }

    pub fn job_stats(&self, resource: usize, job: usize) -> &JobStats {
        &self.resources[resource].jobs[job].stats
    }

    pub fn task_overruns(&self, resource: usize, task: usize) -> u64 {
        self.resources[resource].tasks[task].overruns
    }

    pub fn task_activations(&self, resource: usize, task: usize) -> u64 {
        self.resources[resource].tasks[task].activations
    }

    // ------------------------------------------------------------ variables

    /// Resolves a hierarchical variable path to an address and type.
    pub fn resolve(&self, path: &str) -> Result<(u64, TypeDesc), String> {
        resolve_path(&self.image, &self.mem, path)
    }

    pub fn read(&self, path: &str) -> Result<Value, String> {
        let (addr, ty) = self.resolve(path)?;
        read_value(&self.mem, addr, &ty).map_err(|e| e.message().to_string())
    }

    pub fn write(&mut self, path: &str, v: &Value) -> Result<(), String> {
        let (addr, ty) = self.resolve(path)?;
        write_value(&mut self.mem, addr, &ty, v)
    }

    /// Reads a variable through an access path (VAR_ACCESS).
    pub fn read_access(&self, name: &str) -> Result<Value, String> {
        let a = self
            .image
            .access_paths
            .iter()
            .find(|a| a.name.eq_ignore_ascii_case(name))
            .ok_or_else(|| format!("no access path named {}", name))?;
        read_value(&self.mem, a.addr, &a.ty).map_err(|e| e.message().to_string())
    }

    /// Writes a variable through an access path; READ_ONLY paths reject writes.
    pub fn write_access(&mut self, name: &str, v: &Value) -> Result<(), String> {
        let a = self
            .image
            .access_paths
            .iter()
            .find(|a| a.name.eq_ignore_ascii_case(name))
            .ok_or_else(|| format!("no access path named {}", name))?
            .clone();
        if a.read_only {
            return Err(format!("access path {} is READ_ONLY", a.name));
        }
        write_value(&mut self.mem, a.addr, &a.ty, v)
    }

    /// The process image of a resource: `kind` is 'I', 'Q' or 'M'.
    pub fn io(&mut self, resource: usize, kind: char) -> &mut [u8] {
        let r = &self.image.resources[resource];
        let area = match kind {
            'I' => r.input,
            'Q' => r.output,
            _ => r.memory,
        };
        &mut self.mem.bytes[area.base as usize..(area.base + area.size) as usize]
    }

    pub fn resource_index(&self, name: &str) -> Option<usize> {
        self.image.resources.iter().position(|r| r.name.eq_ignore_ascii_case(name))
    }
}

fn copy_between(src: &Memory, src_addr: u64, dst: &mut Memory, dst_addr: u64, bits: u64) -> Result<(), TrapCode> {
    for i in 0..bits {
        let v = src.load(crate::ty::Ty::Bool, src_addr + i)?;
        dst.store(crate::ty::Ty::Bool, dst_addr + i, v)?;
    }
    Ok(())
}

/// Splits a path like `A.B[1,2].C` into segments.
/// A directly represented variable (`%IX1.2`, `%QW4`, `%MD8`) of a
/// resource, located as by the linker (2.4.1.1).
fn resolve_direct(image: &Image, resource: &str, dv: &str) -> Result<(u64, TypeDesc), String> {
    let r = if resource.is_empty() {
        match image.resources.as_slice() {
            [r] => r,
            _ => return Err(format!("name the resource of {} (RESOURCE.{})", dv, dv)),
        }
    } else {
        image.resources.iter().find(|r| r.name.eq_ignore_ascii_case(resource)).ok_or_else(|| format!("unknown resource '{}'", resource))?
    };
    let invalid = || format!("invalid directly represented variable '{}'", dv);
    let text = dv.trim().to_uppercase();
    let mut chars = text.chars().skip(1).peekable();
    let loc = chars.next().filter(|c| matches!(c, 'I' | 'Q' | 'M')).ok_or_else(invalid)?;
    let size = match chars.peek() {
        Some(c @ ('X' | 'B' | 'W' | 'D' | 'L')) => {
            let c = *c;
            chars.next();
            c
        }
        _ => 'X',
    };
    let fields: Vec<u64> = chars.collect::<String>().split('.').map(|f| f.parse::<u64>()).collect::<Result<_, _>>().map_err(|_| invalid())?;
    if fields.is_empty() {
        return Err(invalid());
    }
    let n = fields.len();
    let (path, off, ty, bits) = match size {
        'X' if n == 1 => (vec![], fields[0], TypeDesc::Bool, 1),
        'X' => (fields[..n - 2].to_vec(), fields[n - 2] * 8 + fields[n - 1], TypeDesc::Bool, 1),
        s => {
            let (bytes, ty) = match s {
                'B' => (1, TypeDesc::Byte),
                'W' => (2, TypeDesc::Word),
                'D' => (4, TypeDesc::DWord),
                _ => (8, TypeDesc::LWord),
            };
            (fields[..n - 1].to_vec(), fields[n - 1] * bytes * 8, ty, bytes * 8)
        }
    };
    let area = if path.is_empty() {
        match loc {
            'I' => r.input,
            'Q' => r.output,
            _ => r.memory,
        }
    } else {
        let path: Vec<u32> = path.iter().map(|&p| p as u32).collect();
        r.io_modules.iter().find(|m| m.location == loc && m.path == path).map(|m| m.area).ok_or_else(|| format!("{} is not used by resource {}", dv, r.name))?
    };
    if off + bits > area.size * 8 {
        return Err(format!("{} is outside the process image of resource {}", dv, r.name));
    }
    Ok((area.base * 8 + off, ty))
}

fn split_path(path: &str) -> Result<Vec<(String, Vec<i64>)>, String> {
    let mut out = Vec::new();
    for seg in path.split('.') {
        let (name, idx) = match seg.find('[') {
            Some(i) => {
                let inner = seg[i + 1..].trim_end_matches(']');
                let idx = inner
                    .split(',')
                    .map(|s| s.trim().parse::<i64>().map_err(|_| format!("invalid index in '{}'", seg)))
                    .collect::<Result<Vec<_>, _>>()?;
                (seg[..i].to_string(), idx)
            }
            None => (seg.to_string(), Vec::new()),
        };
        out.push((name.to_uppercase(), idx));
    }
    Ok(out)
}

pub fn resolve_path(image: &Image, mem: &Memory, path: &str) -> Result<(u64, TypeDesc), String> {
    if let Some(i) = path.find('%') {
        return resolve_direct(image, path[..i].trim().trim_end_matches('.'), &path[i..]);
    }
    let segs = split_path(path.trim())?;
    // longest symbol prefix
    let mut found = None;
    for n in (1..=segs.len()).rev() {
        let prefix: Vec<&str> = segs[..n].iter().map(|(s, _)| s.as_str()).collect();
        let key = prefix.join(".");
        if let Some(sym) = image.symbols.iter().find(|s| s.path == key) {
            found = Some((n, sym));
            break;
        }
    }
    let (n, sym) = found.ok_or_else(|| format!("unknown variable '{}'", path))?;
    let mut addr = sym.addr;
    let mut ty = sym.ty.clone();
    let mut pending_idx = segs[n - 1].1.clone();
    let mut rest = segs[n..].iter();
    loop {
        // dereference
        while let TypeDesc::Reference(inner) = ty {
            addr = mem.load(crate::ty::Ty::U64, addr).map_err(|e| e.message().to_string())?;
            ty = *inner;
        }
        if !pending_idx.is_empty() {
            while let TypeDesc::Derived { base, .. } = ty {
                ty = *base;
            }
            match ty {
                TypeDesc::Array { dims, elem, stride } => {
                    if dims.len() != pending_idx.len() {
                        return Err("wrong number of subscripts".into());
                    }
                    let mut lin = 0i64;
                    for ((lo, hi), i) in dims.iter().zip(&pending_idx) {
                        if i < lo || i > hi {
                            return Err(format!("subscript {} out of range {}..{}", i, lo, hi));
                        }
                        lin = lin * (hi - lo + 1) + (i - lo);
                    }
                    addr += lin as u64 * stride;
                    ty = *elem;
                }
                _ => return Err("subscript applied to a non-array".into()),
            }
            pending_idx.clear();
            continue;
        }
        let Some((name, idx)) = rest.next() else { break };
        let fields = match &ty {
            TypeDesc::Struct { fields, .. } | TypeDesc::Instance { fields, .. } => fields,
            TypeDesc::Derived { base, .. } => match base.as_ref() {
                TypeDesc::Struct { fields, .. } => fields,
                _ => return Err(format!("'{}' has no element {}", path, name)),
            },
            _ => return Err(format!("'{}' has no element {}", path, name)),
        };
        let f = fields.iter().find(|f| f.name.eq_ignore_ascii_case(name)).ok_or_else(|| format!("no element named {}", name))?;
        addr += f.offset;
        ty = f.ty.clone();
        pending_idx = idx.clone();
    }
    Ok((addr, ty))
}
