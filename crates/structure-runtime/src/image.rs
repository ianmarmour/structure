//! The executable image produced by the compiler: bytecode, routines,
//! memory layout, configuration (resources, tasks, jobs) and symbols.

use crate::error::{SourceLoc, TrapCode};
use crate::ty::Ty;
use serde::{Deserialize, Serialize};

/// Addressing modes. All offsets and addresses are in **bits**: BOOL
/// variables can be located on single bits of the I/O images, and arrays of
/// BOOL are bit-packed.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Addr {
    /// Absolute address in memory.
    Abs(u64),
    /// Relative to the frame of the executing routine (temporaries,
    /// function variables).
    Fp(u32),
    /// Relative to the instance of the executing function block or program.
    Ip(u32),
    /// Relative to the most recently pushed (pending) call frame.
    Nf(u32),
    /// Address popped from the operand stack.
    Ind,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum CmpOp {
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum MathFn {
    Sqrt,
    Ln,
    Log,
    Exp,
    Sin,
    Cos,
    Tan,
    Asin,
    Acos,
    Atan,
}

/// Kinds of value for conversions to and from character strings.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum TextKind {
    Bool,
    Int(Ty),
    Real(Ty),
    /// Bit strings (BYTE, WORD, ...): written as unsigned decimal numbers.
    Bits(Ty),
    Time,
    Date,
    TimeOfDay,
    DateAndTime,
}

/// Operations implemented natively by the VM. String operands are passed as
/// addresses of string storage (see [`crate::strings`]); integer operands as
/// 64-bit signed values.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Native {
    /// `[s] -> [len]`
    StrLen { wide: bool },
    /// `[dst, s, l] -> []`
    StrLeft { wide: bool },
    /// `[dst, s, l] -> []`
    StrRight { wide: bool },
    /// `[dst, s, l, p] -> []`
    StrMid { wide: bool },
    /// `[dst, s1 .. sn] -> []`
    StrConcat { wide: bool, n: u8 },
    /// `[dst, s1, s2, p] -> []`
    StrInsert { wide: bool },
    /// `[dst, s, l, p] -> []`
    StrDelete { wide: bool },
    /// `[dst, s1, s2, l, p] -> []`
    StrReplace { wide: bool },
    /// `[s1, s2] -> [pos]`
    StrFind { wide: bool },
    /// `[a, b] -> [cmp]` where cmp is -1, 0 or 1.
    StrCmp { wide: bool },
    /// `[dst, src] -> []`, truncating to the capacity of `dst`.
    StrAssign { wide: bool },
    /// `[dst] -> []` initialise an empty string of the given capacity.
    StrInit { wide: bool, cap: u32 },
    /// `[dst, value] -> []`
    ToText { kind: TextKind, wide: bool },
    /// `[src] -> [value]`
    FromText { kind: TextKind, wide: bool },
    /// `[dst, src] -> []` STRING to WSTRING.
    StrToWstr,
    /// `[dst, src] -> []` WSTRING to STRING.
    WstrToStr,
    /// `[v] -> [v]` BCD-encoded bit string to integer.
    BcdToInt { from: Ty, to: Ty },
    /// `[v] -> [v]` integer to BCD-encoded bit string.
    IntToBcd { from: Ty, to: Ty },
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub enum Op {
    Nop,
    /// Push a constant slot.
    Const(u64),
    Dup,
    /// Duplicate the second slot from the top: `[a, b] -> [a, b, a]`.
    Over,
    Pop,
    Swap,
    /// `[a, b, c] -> [b, c, a]`
    Rot,
    Load(Ty, Addr),
    /// Store the top slot; for `Addr::Ind` the stack is `[addr, value]`.
    Store(Ty, Addr),
    /// Push an address (`Addr::Ind` is not allowed).
    Address(Addr),
    /// Add a constant bit offset to the address on top of the stack.
    Offset(i64),
    /// `[addr, index] -> [addr']` with bounds checking.
    Index { lo: i64, hi: i64, stride: u64 },
    /// `[dst, src] -> []` copy the given number of bits.
    Copy(u64),
    /// `[dst] -> []` clear the given number of bits.
    Zero(u64),
    Add(Ty),
    Sub(Ty),
    Mul(Ty),
    Div(Ty),
    /// Modulo per table 24: `IN2 = 0` yields 0.
    Mod(Ty),
    Neg(Ty),
    Abs(Ty),
    /// `[base, exponent] -> [result]`; base type and exponent type.
    Expt(Ty, Ty),
    And(Ty),
    Or(Ty),
    Xor(Ty),
    Not(Ty),
    /// `[in, n] -> [out]`, n as 64-bit signed integer.
    Shl(Ty),
    Shr(Ty),
    Rol(Ty),
    Ror(Ty),
    Cmp(CmpOp, Ty),
    /// Value-preserving conversion; traps with `ConversionError` when the
    /// value cannot be represented. Real to integer rounds half to even.
    Conv(Ty, Ty),
    /// Truncating real-to-integer conversion (TRUNC).
    Trunc(Ty, Ty),
    Math(MathFn, Ty),
    /// Trap with `code` unless `lo <= top <= hi` (top keeps its value).
    CheckRange { ty: Ty, lo: i64, hi: i64, code: TrapCode },
    /// Trap with `code` unless the top slot is non-zero (pops it).
    Assert(TrapCode),
    Jmp(u32),
    /// Jump if the popped slot is zero.
    Jz(u32),
    /// Jump if the popped slot is non-zero.
    Jnz(u32),
    /// Reserve a call frame of the given size (bytes) on the data stack and
    /// make it the pending frame (`Addr::Nf`). The frame is zero-filled.
    PushFrame(u32),
    /// Release the most recent pending frame.
    PopFrame,
    /// Call a function: the pending frame becomes the callee frame.
    CallFunc(u32),
    /// `[instance] -> []` call a function block or program body.
    CallFb(u32),
    Ret,
    /// Begin a region in which traps transfer control to the handler (used
    /// to reset ENO of standard functions, 2.5.1.2 rule 3).
    TryBegin(u32),
    TryEnd,
    Trap(TrapCode),
    Native(Native),
    /// Push the current time (TIME, microseconds of the task clock).
    Now,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Routine {
    pub name: String,
    pub entry: u32,
    pub end: u32,
    /// Size in bytes of the frame (VAR_TEMP, function variables and
    /// temporaries). Function block routines allocate it on call.
    pub frame_size: u32,
}

/// A memory area: absolute base address and size, in bytes.
#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize)]
pub struct Area {
    pub base: u64,
    pub size: u64,
}

/// Source of a task input (SINGLE, INTERVAL).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum Source {
    Const(i64),
    Var { addr: u64, ty: Ty },
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TaskInfo {
    pub name: String,
    pub priority: u32,
    pub single: Option<Source>,
    /// Interval in microseconds.
    pub interval: Option<Source>,
}

/// A program organisation unit scheduled by the resource: a program
/// instance or a function block instance with an explicit task association.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct JobInfo {
    pub name: String,
    /// Entry routine (takes the instance as its instance pointer).
    pub routine: u32,
    /// Instance address (bits).
    pub instance: u64,
    /// Associated task; `None` for programs without task association, which
    /// run continuously at the lowest priority (2.7.2 rule 4).
    pub task: Option<usize>,
    /// Data stack (bytes).
    pub stack: Area,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ResourceInfo {
    pub name: String,
    pub type_name: String,
    pub input: Area,
    pub output: Area,
    pub memory: Area,
    /// Initialises the variables of the resource (cold start values).
    pub init_routine: u32,
    pub tasks: Vec<TaskInfo>,
    pub jobs: Vec<JobInfo>,
    /// Stack used for the initialisation routine.
    pub init_stack: Area,
    /// Process image areas of hierarchical addresses with more levels
    /// (`%IW2.5.7.1`): location prefix, module path and area.
    #[serde(default)]
    pub io_modules: Vec<IoModule>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct IoModule {
    pub location: char,
    pub path: Vec<u32>,
    pub area: Area,
}

/// Run-time type description, used to read and write variables by name.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum TypeDesc {
    Bool,
    SInt,
    Int,
    DInt,
    LInt,
    USInt,
    UInt,
    UDInt,
    ULInt,
    Real,
    LReal,
    Byte,
    Word,
    DWord,
    LWord,
    Time,
    Date,
    TimeOfDay,
    DateAndTime,
    String { cap: u32 },
    WString { cap: u32 },
    Enum { name: String, values: Vec<String> },
    /// Subrange or directly derived type of an elementary type.
    Derived { name: String, base: Box<TypeDesc> },
    Array { dims: Vec<(i64, i64)>, elem: Box<TypeDesc>, stride: u64 },
    Struct { name: String, fields: Vec<FieldDesc> },
    /// Function block or program instance.
    Instance { name: String, fields: Vec<FieldDesc> },
    /// Hidden reference (VAR_IN_OUT, VAR_EXTERNAL, located variable).
    Reference(Box<TypeDesc>),
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct FieldDesc {
    pub name: String,
    /// Bit offset from the start of the structure or instance.
    pub offset: u64,
    pub ty: TypeDesc,
}

impl TypeDesc {
    pub fn scalar(&self) -> Option<Ty> {
        Some(match self {
            TypeDesc::Bool => Ty::Bool,
            TypeDesc::SInt => Ty::I8,
            TypeDesc::Int => Ty::I16,
            TypeDesc::DInt => Ty::I32,
            TypeDesc::LInt => Ty::I64,
            TypeDesc::USInt | TypeDesc::Byte => Ty::U8,
            TypeDesc::UInt | TypeDesc::Word => Ty::U16,
            TypeDesc::UDInt | TypeDesc::DWord => Ty::U32,
            TypeDesc::ULInt | TypeDesc::LWord => Ty::U64,
            TypeDesc::Real => Ty::F32,
            TypeDesc::LReal => Ty::F64,
            TypeDesc::Time | TypeDesc::Date | TypeDesc::TimeOfDay | TypeDesc::DateAndTime => Ty::I64,
            TypeDesc::Enum { .. } => Ty::I32,
            TypeDesc::Derived { base, .. } => return base.scalar(),
            TypeDesc::Reference(_) => Ty::U64,
            _ => return None,
        })
    }
}

/// A named variable in the image (globals, program instances, access paths).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Symbol {
    /// Fully qualified, upper-case path, e.g. `STATION_1.P1.COUNT`.
    pub path: String,
    /// Bit address.
    pub addr: u64,
    pub ty: TypeDesc,
    pub retain: bool,
    pub constant: bool,
}

/// Access path declared by VAR_ACCESS (2.7.1).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AccessPathInfo {
    pub name: String,
    pub addr: u64,
    pub ty: TypeDesc,
    pub read_only: bool,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Image {
    pub code: Vec<Op>,
    pub routines: Vec<Routine>,
    /// Total size of memory in bytes (static data, I/O images and stacks).
    pub memory_size: u64,
    /// Constant data written at load time: (byte address, bytes).
    pub data: Vec<(u64, Vec<u8>)>,
    pub configuration: String,
    /// Initialises configuration-level globals.
    pub config_init: Option<u32>,
    pub config_init_stack: Area,
    pub resources: Vec<ResourceInfo>,
    pub symbols: Vec<Symbol>,
    pub access_paths: Vec<AccessPathInfo>,
    /// Retentive regions (bit address, bit length).
    pub retain: Vec<(u64, u64)>,
    /// (first pc, location) sorted by pc.
    pub lines: Vec<(u32, SourceLoc)>,
}

impl Image {
    pub fn location(&self, pc: u32) -> Option<&SourceLoc> {
        match self.lines.binary_search_by_key(&pc, |(p, _)| *p) {
            Ok(i) => Some(&self.lines[i].1),
            Err(0) => None,
            Err(i) => Some(&self.lines[i - 1].1),
        }
    }

    pub fn routine_at(&self, pc: u32) -> Option<&Routine> {
        self.routines.iter().find(|r| r.entry <= pc && pc < r.end)
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string(self).expect("image serialisation")
    }

    pub fn from_json(s: &str) -> Result<Image, String> {
        serde_json::from_str(s).map_err(|e| e.to_string())
    }
}
