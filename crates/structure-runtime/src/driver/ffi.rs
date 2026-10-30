//! The C interface of drivers (`include/structure_driver.h`, ABI version 1).
//!
//! A driver library exports the function `structure_driver_api_v1`, which
//! returns a table of functions. The runtime loads the library, checks the
//! ABI version, creates an instance from the JSON parameters of the driver
//! and calls the functions of the table from its scheduling thread.
//!
//! Drivers written in Rust can be built as such libraries (`cdylib`) with
//! [`export_driver!`](crate::export_driver).

use super::{Binding, Direction, IoDriver};
use serde_json::Value as Json;
use std::ffi::{c_char, c_void, CStr, CString};
use std::path::Path;

/// Version of the C interface.
pub const ABI_VERSION: u32 = 1;

/// Name of the function exported by driver libraries.
pub const ENTRY_POINT: &str = "structure_driver_api_v1";

pub const DIRECTION_INPUT: u32 = 0;
pub const DIRECTION_OUTPUT: u32 = 1;

/// `structure_binding`
#[repr(C)]
pub struct CBinding {
    pub area: *const c_char,
    pub direction: u32,
    pub path: *const u32,
    pub path_len: usize,
    pub offset: usize,
    pub size: usize,
    pub params_json: *const c_char,
}

/// `structure_buffer`
#[repr(C)]
pub struct CBuffer {
    pub data: *mut u8,
    pub size: usize,
}

/// `structure_driver_api`
#[repr(C)]
pub struct CDriverApi {
    pub abi_version: u32,
    pub create: unsafe extern "C" fn(params_json: *const c_char, error: *mut c_char, error_len: usize) -> *mut c_void,
    pub open: unsafe extern "C" fn(driver: *mut c_void, bindings: *const CBinding, count: usize, error: *mut c_char, error_len: usize) -> i32,
    pub read_inputs: unsafe extern "C" fn(driver: *mut c_void, buffers: *const CBuffer, count: usize, error: *mut c_char, error_len: usize) -> i32,
    pub write_outputs: unsafe extern "C" fn(driver: *mut c_void, buffers: *const CBuffer, count: usize, error: *mut c_char, error_len: usize) -> i32,
    pub stop: Option<unsafe extern "C" fn(driver: *mut c_void)>,
    pub close: Option<unsafe extern "C" fn(driver: *mut c_void)>,
    pub destroy: unsafe extern "C" fn(driver: *mut c_void),
}

const ERROR_LEN: usize = 1024;

fn error_text(buf: &[u8], fallback: &str) -> String {
    let end = buf.iter().position(|&b| b == 0).unwrap_or(buf.len());
    let s = String::from_utf8_lossy(&buf[..end]).into_owned();
    if s.is_empty() {
        fallback.to_string()
    } else {
        s
    }
}

/// A driver implemented behind the C interface.
pub struct DynamicDriver {
    api: *const CDriverApi,
    instance: *mut c_void,
    /// Strings and paths referenced by the bindings given to `open`.
    strings: Vec<CString>,
    paths: Vec<Vec<u32>>,
    // Dropped last, after the instance has been destroyed.
    _library: Option<libloading::Library>,
}

// The functions of a driver are called from one thread at a time.
unsafe impl Send for DynamicDriver {}

impl DynamicDriver {
    /// Loads a driver library and creates an instance.
    pub fn load(path: &Path, params: &Json) -> Result<DynamicDriver, String> {
        // SAFETY: loading a library runs its initialisers; driver libraries
        // are trusted code configured by the user.
        let library = unsafe { libloading::Library::new(path) }.map_err(|e| format!("cannot load {}: {}", path.display(), e))?;
        let api = {
            // SAFETY: the entry point has the signature of the C interface.
            let entry: libloading::Symbol<unsafe extern "C" fn() -> *const CDriverApi> =
                unsafe { library.get(format!("{}\0", ENTRY_POINT).as_bytes()) }.map_err(|_| format!("{} does not export {}", path.display(), ENTRY_POINT))?;
            unsafe { entry() }
        };
        // SAFETY: the table stays valid while the library is loaded.
        unsafe { DynamicDriver::from_api(api, params, Some(library)) }
    }

    /// Creates an instance of a driver whose C interface is linked into the
    /// program.
    ///
    /// # Safety
    /// `api` shall point to a valid table that outlives the driver (when
    /// `library` is given, while it is loaded).
    pub unsafe fn from_api(api: *const CDriverApi, params: &Json, library: Option<libloading::Library>) -> Result<DynamicDriver, String> {
        if api.is_null() {
            return Err("the driver returned no interface".into());
        }
        let version = (*api).abi_version;
        if version != ABI_VERSION {
            return Err(format!("the driver implements version {} of the driver interface; version {} is required", version, ABI_VERSION));
        }
        let params = CString::new(params.to_string()).map_err(|e| e.to_string())?;
        let mut err = vec![0u8; ERROR_LEN];
        let instance = ((*api).create)(params.as_ptr(), err.as_mut_ptr() as *mut c_char, ERROR_LEN);
        if instance.is_null() {
            return Err(error_text(&err, "the driver could not be created"));
        }
        Ok(DynamicDriver { api, instance, strings: Vec::new(), paths: Vec::new(), _library: library })
    }

    fn api(&self) -> &CDriverApi {
        // SAFETY: valid while self exists (see from_api).
        unsafe { &*self.api }
    }
}

impl IoDriver for DynamicDriver {
    fn open(&mut self, bindings: &[Binding]) -> Result<(), String> {
        self.strings.clear();
        self.paths = bindings.iter().map(|b| b.path.clone()).collect();
        for b in bindings {
            self.strings.push(CString::new(b.area.clone()).map_err(|e| e.to_string())?);
            self.strings.push(CString::new(b.params.to_string()).map_err(|e| e.to_string())?);
        }
        let cb: Vec<CBinding> = bindings
            .iter()
            .enumerate()
            .map(|(i, b)| CBinding {
                area: self.strings[2 * i].as_ptr(),
                direction: if b.direction == Direction::Input { DIRECTION_INPUT } else { DIRECTION_OUTPUT },
                path: self.paths[i].as_ptr(),
                path_len: self.paths[i].len(),
                offset: b.offset,
                size: b.size,
                params_json: self.strings[2 * i + 1].as_ptr(),
            })
            .collect();
        let mut err = vec![0u8; ERROR_LEN];
        // SAFETY: the bindings and their strings are valid during the call
        // (and kept until the next open).
        let rc = unsafe { (self.api().open)(self.instance, cb.as_ptr(), cb.len(), err.as_mut_ptr() as *mut c_char, ERROR_LEN) };
        if rc == 0 {
            Ok(())
        } else {
            Err(error_text(&err, "open failed"))
        }
    }

    fn read_inputs(&mut self, data: &mut [&mut [u8]]) -> Result<(), String> {
        let bufs: Vec<CBuffer> = data.iter_mut().map(|d| CBuffer { data: d.as_mut_ptr(), size: d.len() }).collect();
        let mut err = vec![0u8; ERROR_LEN];
        // SAFETY: the buffers are valid and writable during the call.
        let rc = unsafe { (self.api().read_inputs)(self.instance, bufs.as_ptr(), bufs.len(), err.as_mut_ptr() as *mut c_char, ERROR_LEN) };
        if rc == 0 {
            Ok(())
        } else {
            Err(error_text(&err, "reading the inputs failed"))
        }
    }

    fn write_outputs(&mut self, data: &[&[u8]]) -> Result<(), String> {
        let bufs: Vec<CBuffer> = data.iter().map(|d| CBuffer { data: d.as_ptr() as *mut u8, size: d.len() }).collect();
        let mut err = vec![0u8; ERROR_LEN];
        // SAFETY: the buffers are valid during the call; the interface
        // forbids writing to them.
        let rc = unsafe { (self.api().write_outputs)(self.instance, bufs.as_ptr(), bufs.len(), err.as_mut_ptr() as *mut c_char, ERROR_LEN) };
        if rc == 0 {
            Ok(())
        } else {
            Err(error_text(&err, "writing the outputs failed"))
        }
    }

    fn stop(&mut self) {
        if let Some(f) = self.api().stop {
            // SAFETY: valid instance.
            unsafe { f(self.instance) }
        }
    }

    fn close(&mut self) {
        if let Some(f) = self.api().close {
            // SAFETY: valid instance.
            unsafe { f(self.instance) }
        }
    }
}

impl Drop for DynamicDriver {
    fn drop(&mut self) {
        // SAFETY: the instance was created by this table and is destroyed once.
        unsafe { (self.api().destroy)(self.instance) };
    }
}

// ------------------------------------------------------------ Rust drivers as libraries

/// Exports a driver written in Rust with the C interface, so that it can be
/// built as a shared library (`crate-type = ["cdylib"]`) and loaded with
/// `"library"` in the driver configuration:
///
/// ```ignore
/// fn create(params: &serde_json::Value) -> Result<Box<dyn IoDriver>, String> { ... }
/// structure_runtime::export_driver!(create);
/// ```
#[macro_export]
macro_rules! export_driver {
    ($create:path) => {
        #[no_mangle]
        pub extern "C" fn structure_driver_api_v1() -> *const $crate::driver::ffi::CDriverApi {
            unsafe extern "C" fn __structure_driver_create(
                params: *const ::std::ffi::c_char,
                error: *mut ::std::ffi::c_char,
                error_len: usize,
            ) -> *mut ::std::ffi::c_void {
                $crate::driver::ffi::rust_create(params, error, error_len, $create)
            }
            static API: $crate::driver::ffi::CDriverApi = $crate::driver::ffi::CDriverApi {
                abi_version: $crate::driver::ffi::ABI_VERSION,
                create: __structure_driver_create,
                open: $crate::driver::ffi::rust_open,
                read_inputs: $crate::driver::ffi::rust_read_inputs,
                write_outputs: $crate::driver::ffi::rust_write_outputs,
                stop: Some($crate::driver::ffi::rust_stop),
                close: Some($crate::driver::ffi::rust_close),
                destroy: $crate::driver::ffi::rust_destroy,
            };
            &API
        }
    };
}

type Instance = Box<dyn IoDriver>;

fn set_error(error: *mut c_char, error_len: usize, msg: &str) {
    if error.is_null() || error_len == 0 {
        return;
    }
    let bytes = msg.as_bytes();
    let n = bytes.len().min(error_len - 1);
    // SAFETY: the caller provides `error_len` writable bytes.
    unsafe {
        std::ptr::copy_nonoverlapping(bytes.as_ptr(), error as *mut u8, n);
        *error.add(n) = 0;
    }
}

fn guarded<T>(error: *mut c_char, error_len: usize, failed: T, f: impl FnOnce() -> Result<T, String>) -> T {
    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(f)) {
        Ok(Ok(v)) => v,
        Ok(Err(e)) => {
            set_error(error, error_len, &e);
            failed
        }
        Err(_) => {
            set_error(error, error_len, "the driver panicked");
            failed
        }
    }
}

/// # Safety
/// C interface (see `structure_driver.h`).
pub unsafe fn rust_create(params: *const c_char, error: *mut c_char, error_len: usize, create: fn(&Json) -> Result<Box<dyn IoDriver>, String>) -> *mut c_void {
    guarded(error, error_len, std::ptr::null_mut(), || {
        let text = if params.is_null() { "null".into() } else { CStr::from_ptr(params).to_string_lossy().into_owned() };
        let params: Json = serde_json::from_str(&text).map_err(|e| e.to_string())?;
        let d: Instance = create(&params)?;
        Ok(Box::into_raw(Box::new(d)) as *mut c_void)
    })
}

/// # Safety
/// C interface (see `structure_driver.h`).
pub unsafe extern "C" fn rust_open(driver: *mut c_void, bindings: *const CBinding, count: usize, error: *mut c_char, error_len: usize) -> i32 {
    guarded(error, error_len, -1, || {
        let d = &mut *(driver as *mut Instance);
        let raw = if count == 0 { &[][..] } else { std::slice::from_raw_parts(bindings, count) };
        let mut list = Vec::with_capacity(count);
        for b in raw {
            let params = if b.params_json.is_null() { Json::Null } else { serde_json::from_str(&CStr::from_ptr(b.params_json).to_string_lossy()).map_err(|e| e.to_string())? };
            list.push(Binding {
                area: if b.area.is_null() { String::new() } else { CStr::from_ptr(b.area).to_string_lossy().into_owned() },
                direction: if b.direction == DIRECTION_INPUT { Direction::Input } else { Direction::Output },
                path: if b.path_len == 0 { vec![] } else { std::slice::from_raw_parts(b.path, b.path_len).to_vec() },
                offset: b.offset,
                size: b.size,
                params,
            });
        }
        d.open(&list)?;
        Ok(0)
    })
}

unsafe fn slices<'a>(buffers: *const CBuffer, count: usize) -> Vec<&'a mut [u8]> {
    if count == 0 {
        return vec![];
    }
    std::slice::from_raw_parts(buffers, count)
        .iter()
        .map(|b| if b.size == 0 || b.data.is_null() { &mut [][..] } else { std::slice::from_raw_parts_mut(b.data, b.size) })
        .collect()
}

/// # Safety
/// C interface (see `structure_driver.h`).
pub unsafe extern "C" fn rust_read_inputs(driver: *mut c_void, buffers: *const CBuffer, count: usize, error: *mut c_char, error_len: usize) -> i32 {
    guarded(error, error_len, -1, || {
        let d = &mut *(driver as *mut Instance);
        let mut data = slices(buffers, count);
        d.read_inputs(&mut data)?;
        Ok(0)
    })
}

/// # Safety
/// C interface (see `structure_driver.h`).
pub unsafe extern "C" fn rust_write_outputs(driver: *mut c_void, buffers: *const CBuffer, count: usize, error: *mut c_char, error_len: usize) -> i32 {
    guarded(error, error_len, -1, || {
        let d = &mut *(driver as *mut Instance);
        let data = slices(buffers, count);
        let data: Vec<&[u8]> = data.into_iter().map(|s| &*s).collect();
        d.write_outputs(&data)?;
        Ok(0)
    })
}

/// # Safety
/// C interface (see `structure_driver.h`).
pub unsafe extern "C" fn rust_stop(driver: *mut c_void) {
    let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| (*(driver as *mut Instance)).stop()));
}

/// # Safety
/// C interface (see `structure_driver.h`).
pub unsafe extern "C" fn rust_close(driver: *mut c_void) {
    let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| (*(driver as *mut Instance)).close()));
}

/// # Safety
/// C interface (see `structure_driver.h`).
pub unsafe extern "C" fn rust_destroy(driver: *mut c_void) {
    if !driver.is_null() {
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| drop(Box::from_raw(driver as *mut Instance))));
    }
}
