//! Bit-addressed memory. All addresses are bit addresses; values other than
//! BOOL must be byte aligned. Multi-byte values are little-endian.

use crate::error::TrapCode;
use crate::ty::Ty;

#[derive(Clone)]
pub struct Memory {
    pub bytes: Vec<u8>,
}

type R<T> = Result<T, TrapCode>;

impl Memory {
    pub fn new(size: usize) -> Memory {
        Memory { bytes: vec![0; size] }
    }

    #[inline]
    fn range(&self, byte: u64, len: u64) -> R<std::ops::Range<usize>> {
        let end = byte.checked_add(len).ok_or(TrapCode::MemoryFault)?;
        if end > self.bytes.len() as u64 {
            return Err(TrapCode::MemoryFault);
        }
        Ok(byte as usize..end as usize)
    }

    #[inline]
    pub fn load(&self, ty: Ty, addr: u64) -> R<u64> {
        if ty == Ty::Bool {
            let b = *self.bytes.get((addr >> 3) as usize).ok_or(TrapCode::MemoryFault)?;
            return Ok(((b >> (addr & 7)) & 1) as u64);
        }
        if addr & 7 != 0 {
            return Err(TrapCode::MemoryFault);
        }
        let r = self.range(addr >> 3, ty.bytes())?;
        let s = &self.bytes[r];
        Ok(match ty {
            Ty::I8 => s[0] as i8 as i64 as u64,
            Ty::U8 => s[0] as u64,
            Ty::I16 => i16::from_le_bytes([s[0], s[1]]) as i64 as u64,
            Ty::U16 => u16::from_le_bytes([s[0], s[1]]) as u64,
            Ty::I32 => i32::from_le_bytes([s[0], s[1], s[2], s[3]]) as i64 as u64,
            Ty::U32 | Ty::F32 => u32::from_le_bytes([s[0], s[1], s[2], s[3]]) as u64,
            Ty::I64 | Ty::U64 | Ty::F64 => u64::from_le_bytes(s.try_into().unwrap()),
            Ty::Bool => unreachable!(),
        })
    }

    #[inline]
    pub fn store(&mut self, ty: Ty, addr: u64, v: u64) -> R<()> {
        if ty == Ty::Bool {
            let b = self.bytes.get_mut((addr >> 3) as usize).ok_or(TrapCode::MemoryFault)?;
            let bit = 1u8 << (addr & 7);
            if v & 1 != 0 {
                *b |= bit;
            } else {
                *b &= !bit;
            }
            return Ok(());
        }
        if addr & 7 != 0 {
            return Err(TrapCode::MemoryFault);
        }
        let n = ty.bytes();
        let r = self.range(addr >> 3, n)?;
        let le = v.to_le_bytes();
        self.bytes[r].copy_from_slice(&le[..n as usize]);
        Ok(())
    }

    fn get_bit(&self, a: u64) -> R<bool> {
        let b = *self.bytes.get((a >> 3) as usize).ok_or(TrapCode::MemoryFault)?;
        Ok((b >> (a & 7)) & 1 != 0)
    }

    fn set_bit(&mut self, a: u64, v: bool) -> R<()> {
        let b = self.bytes.get_mut((a >> 3) as usize).ok_or(TrapCode::MemoryFault)?;
        if v {
            *b |= 1 << (a & 7);
        } else {
            *b &= !(1 << (a & 7));
        }
        Ok(())
    }

    /// Copies `bits` bits from `src` to `dst` (regions may overlap).
    pub fn copy(&mut self, dst: u64, src: u64, bits: u64) -> R<()> {
        if bits == 0 || dst == src {
            return Ok(());
        }
        if dst & 7 == 0 && src & 7 == 0 && bits & 7 == 0 {
            let s = self.range(src >> 3, bits >> 3)?;
            let d = self.range(dst >> 3, bits >> 3)?;
            self.bytes.copy_within(s, d.start);
            return Ok(());
        }
        // General bitwise copy through a temporary.
        let mut tmp = Vec::with_capacity(bits as usize);
        for i in 0..bits {
            tmp.push(self.get_bit(src + i)?);
        }
        for (i, v) in tmp.into_iter().enumerate() {
            self.set_bit(dst + i as u64, v)?;
        }
        Ok(())
    }

    pub fn zero(&mut self, dst: u64, bits: u64) -> R<()> {
        if dst & 7 == 0 && bits & 7 == 0 {
            let r = self.range(dst >> 3, bits >> 3)?;
            self.bytes[r].fill(0);
            return Ok(());
        }
        for i in 0..bits {
            self.set_bit(dst + i, false)?;
        }
        Ok(())
    }

    pub fn read_bytes(&self, byte: u64, len: u64) -> R<&[u8]> {
        let r = self.range(byte, len)?;
        Ok(&self.bytes[r])
    }

    pub fn write_bytes(&mut self, byte: u64, data: &[u8]) -> R<()> {
        let r = self.range(byte, data.len() as u64)?;
        self.bytes[r].copy_from_slice(data);
        Ok(())
    }

    pub fn len(&self) -> usize {
        self.bytes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.bytes.is_empty()
    }
}
