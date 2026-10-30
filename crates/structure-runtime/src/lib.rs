//! Runtime for compiled IEC 61131-3 programs: a bytecode virtual machine
//! with bit-addressed memory, the native implementation of standard
//! functions, and the configuration/resource/task execution model.

pub mod calendar;
pub mod error;
pub mod image;
pub mod memory;
pub mod natives;
pub mod runtime;
pub mod strings;
pub mod text;
pub mod ty;
pub mod value;
pub mod vm;

pub use error::{RuntimeError, SourceLoc, TrapCode};
pub use image::Image;
pub use runtime::{ClockMode, ErrorPolicy, Options, Runtime};
pub use ty::Ty;
pub use value::Value;
