//! Character string storage.
//!
//! A STRING of capacity `n` occupies `4 + n` bytes: a little-endian `u16`
//! current length, a `u16` capacity, then `n` bytes. A WSTRING occupies
//! `4 + 2n` bytes with 16-bit little-endian character codes.

use crate::error::TrapCode;
use crate::memory::Memory;

/// Maximum length of STRING and WSTRING values (annex D).
pub const MAX_STRING_LENGTH: u32 = 65535;
/// Default maximum length of STRING and WSTRING declared without a length.
pub const DEFAULT_STRING_LENGTH: u32 = 254;
pub const HEADER_BYTES: u64 = 4;

type R<T> = Result<T, TrapCode>;

pub fn storage_bytes(cap: u32, wide: bool) -> u64 {
    HEADER_BYTES + cap as u64 * if wide { 2 } else { 1 }
}

fn byte_addr(addr_bits: u64) -> R<u64> {
    if addr_bits & 7 != 0 {
        return Err(TrapCode::MemoryFault);
    }
    Ok(addr_bits >> 3)
}

pub fn capacity(mem: &Memory, addr_bits: u64) -> R<u32> {
    let a = byte_addr(addr_bits)?;
    let h = mem.read_bytes(a + 2, 2)?;
    Ok(u16::from_le_bytes([h[0], h[1]]) as u32)
}

/// Reads a string as character codes.
pub fn read(mem: &Memory, addr_bits: u64, wide: bool) -> R<Vec<u16>> {
    let a = byte_addr(addr_bits)?;
    let h = mem.read_bytes(a, 4)?;
    let len = u16::from_le_bytes([h[0], h[1]]) as u64;
    let cap = u16::from_le_bytes([h[2], h[3]]) as u64;
    let len = len.min(cap);
    if wide {
        let d = mem.read_bytes(a + 4, len * 2)?;
        Ok(d.chunks(2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect())
    } else {
        let d = mem.read_bytes(a + 4, len)?;
        Ok(d.iter().map(|b| *b as u16).collect())
    }
}

/// Writes a string. If `truncate` the value is cut to the capacity of the
/// destination, otherwise exceeding the capacity is an error.
pub fn write(mem: &mut Memory, addr_bits: u64, wide: bool, chars: &[u16], truncate: bool) -> R<()> {
    let a = byte_addr(addr_bits)?;
    let cap = capacity(mem, addr_bits)? as usize;
    let n = if chars.len() > cap {
        if !truncate {
            return Err(TrapCode::StringTooLong);
        }
        cap
    } else {
        chars.len()
    };
    mem.write_bytes(a, &(n as u16).to_le_bytes())?;
    if wide {
        let mut buf = Vec::with_capacity(n * 2);
        for c in &chars[..n] {
            buf.extend_from_slice(&c.to_le_bytes());
        }
        mem.write_bytes(a + 4, &buf)?;
    } else {
        let mut buf = Vec::with_capacity(n);
        for c in &chars[..n] {
            if *c > 0xFF {
                return Err(TrapCode::ConversionError);
            }
            buf.push(*c as u8);
        }
        mem.write_bytes(a + 4, &buf)?;
    }
    Ok(())
}

/// Initialises an empty string of the given capacity.
pub fn init(mem: &mut Memory, addr_bits: u64, cap: u32) -> R<()> {
    let a = byte_addr(addr_bits)?;
    mem.write_bytes(a, &0u16.to_le_bytes())?;
    mem.write_bytes(a + 2, &(cap as u16).to_le_bytes())?;
    Ok(())
}

/// Encodes a constant string image (header and data) for the data segment.
pub fn encode_literal(chars: &[u32], wide: bool) -> Vec<u8> {
    let mut v = Vec::new();
    let n = chars.len() as u16;
    v.extend_from_slice(&n.to_le_bytes());
    v.extend_from_slice(&n.to_le_bytes());
    for c in chars {
        if wide {
            v.extend_from_slice(&(*c as u16).to_le_bytes());
        } else {
            v.push(*c as u8);
        }
    }
    v
}

pub fn to_rust(chars: &[u16]) -> String {
    char::decode_utf16(chars.iter().copied()).map(|c| c.unwrap_or('\u{FFFD}')).collect()
}

pub fn from_rust(s: &str, wide: bool) -> Vec<u16> {
    if wide {
        s.encode_utf16().collect()
    } else {
        s.chars().map(|c| if (c as u32) < 256 { c as u16 } else { b'?' as u16 }).collect()
    }
}
