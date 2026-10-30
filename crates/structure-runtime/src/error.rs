//! Run-time error conditions (annex E errors detected during execution).

use serde::{Deserialize, Serialize};
use std::fmt;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum TrapCode {
    /// 2.5.1.5.2 / 3.3.1: division by zero.
    DivisionByZero,
    /// 2.5.1.5.2 / 3.2.2 / 3.3.1: numerical result exceeds range for data type.
    Overflow,
    /// 2.5.1.5.1: type conversion error.
    ConversionError,
    /// 2.3.3.1: value of a variable exceeds the specified subrange.
    SubrangeViolation,
    /// Array subscript outside the declared range.
    IndexOutOfRange,
    /// 2.5.1.5.3: N input is less than zero in a bit-shift function.
    NegativeShift,
    /// 2.5.1.5.4: selector (K) out of range for the MUX function.
    MuxSelectorOutOfRange,
    /// 2.5.1.5.5: invalid character position specified.
    InvalidCharacterPosition,
    /// 2.5.1.5.5: result exceeds maximum string length.
    StringTooLong,
    /// 2.5.1.5.5: ANY_INT input is less than zero in a string function.
    NegativeStringArgument,
    /// 2.5.1.5.6: result exceeds range for (time) data type.
    TimeRange,
    /// 3.3.2.1: return from function without value assigned.
    NoValueAssigned,
    /// 2.5.2.2: no value specified for an in-out variable or a function
    /// block instance used as input variable.
    NoValueSpecified,
    /// 2.6.4.5: action control contention error.
    ActionControlContention,
    /// 2.6.5: simultaneously true, non-prioritized transitions in a
    /// selection divergence.
    SimultaneousTransitions,
    /// 2.6.5: unsafe SFC (more than one active token in a sequence).
    UnsafeSfc,
    /// 3.3.2.4: iteration fails to terminate (watchdog).
    Watchdog,
    /// The data stack of a task overflowed.
    StackOverflow,
    /// Internal: an address outside the memory was accessed.
    MemoryFault,
    /// Internal: invalid operation (corrupt image).
    InvalidOperation,
    /// 2.4.3: attempt to modify a constant (via an access path).
    ConstantModification,
}

impl TrapCode {
    /// Reference to the subclause of IEC 61131-3 defining the error.
    pub fn clause(self) -> &'static str {
        match self {
            TrapCode::DivisionByZero => "2.5.1.5.2",
            TrapCode::Overflow => "2.5.1.5.2",
            TrapCode::ConversionError => "2.5.1.5.1",
            TrapCode::SubrangeViolation => "2.3.3.1",
            TrapCode::IndexOutOfRange => "2.4.1.2",
            TrapCode::NegativeShift => "2.5.1.5.3",
            TrapCode::MuxSelectorOutOfRange => "2.5.1.5.4",
            TrapCode::InvalidCharacterPosition => "2.5.1.5.5",
            TrapCode::StringTooLong => "2.5.1.5.5",
            TrapCode::NegativeStringArgument => "2.5.1.5.5",
            TrapCode::TimeRange => "2.5.1.5.6",
            TrapCode::NoValueAssigned => "3.3.2.1",
            TrapCode::NoValueSpecified => "2.5.2.2",
            TrapCode::ActionControlContention => "2.6.4.5",
            TrapCode::SimultaneousTransitions => "2.6.5",
            TrapCode::UnsafeSfc => "2.6.5",
            TrapCode::Watchdog => "3.3.2.4",
            TrapCode::StackOverflow => "D",
            TrapCode::MemoryFault => "-",
            TrapCode::InvalidOperation => "-",
            TrapCode::ConstantModification => "2.4.3",
        }
    }

    pub fn message(self) -> &'static str {
        match self {
            TrapCode::DivisionByZero => "division by zero",
            TrapCode::Overflow => "numerical result exceeds range for data type",
            TrapCode::ConversionError => "type conversion error",
            TrapCode::SubrangeViolation => "value of a variable exceeds the specified subrange",
            TrapCode::IndexOutOfRange => "array subscript out of range",
            TrapCode::NegativeShift => "N input is less than zero in a bit-shift function",
            TrapCode::MuxSelectorOutOfRange => "selector (K) out of range for MUX function",
            TrapCode::InvalidCharacterPosition => "invalid character position specified",
            TrapCode::StringTooLong => "result exceeds maximum string length",
            TrapCode::NegativeStringArgument => "ANY_INT input is less than zero in a string function",
            TrapCode::TimeRange => "result exceeds range for data type",
            TrapCode::NoValueAssigned => "return from function without value assigned",
            TrapCode::NoValueSpecified => "no value specified for an in-out variable or function block instance input",
            TrapCode::ActionControlContention => "action control contention error",
            TrapCode::SimultaneousTransitions => "simultaneously true, non-prioritized transitions in a selection divergence",
            TrapCode::UnsafeSfc => "unsafe SFC: a step was activated while already active",
            TrapCode::Watchdog => "execution did not terminate within the watchdog limit (iteration fails to terminate)",
            TrapCode::StackOverflow => "data stack overflow",
            TrapCode::MemoryFault => "memory access out of bounds",
            TrapCode::InvalidOperation => "invalid operation",
            TrapCode::ConstantModification => "attempt to modify a variable declared CONSTANT",
        }
    }
}

impl fmt::Display for TrapCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} (IEC 61131-3 {})", self.message(), self.clause())
    }
}

/// Source location attached to code for error reporting.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceLoc {
    pub file: String,
    pub line: u32,
    pub column: u32,
}

impl fmt::Display for SourceLoc {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}:{}", self.file, self.line, self.column)
    }
}

/// An error raised during execution.
#[derive(Clone, Debug)]
pub struct RuntimeError {
    pub code: TrapCode,
    pub resource: String,
    pub job: String,
    pub routine: String,
    pub location: Option<SourceLoc>,
    /// Virtual or real time at which the error occurred (microseconds).
    pub time: i64,
}

impl fmt::Display for RuntimeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "run-time error: {}", self.code)?;
        if !self.resource.is_empty() {
            write!(f, " in resource {}", self.resource)?;
        }
        if !self.job.is_empty() {
            write!(f, ", {}", self.job)?;
        }
        if !self.routine.is_empty() {
            write!(f, " ({})", self.routine)?;
        }
        if let Some(loc) = &self.location {
            write!(f, " at {}", loc)?;
        }
        Ok(())
    }
}

impl std::error::Error for RuntimeError {}
