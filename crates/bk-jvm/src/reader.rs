use bk_core::{Error, Result};

/// Big-endian cursor over a class file.
///
/// Every read is bounds checked and reports the offset it failed at, so a malformed file
/// produces a usable error instead of a panic. Slices are handed out borrowed — nothing in
/// the parse path copies.
pub struct Reader<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    pub fn new(data: &'a [u8]) -> Self {
        Self { data, pos: 0 }
    }

    pub fn at(data: &'a [u8], pos: usize) -> Self {
        Self { data, pos }
    }

    pub fn pos(&self) -> usize {
        self.pos
    }

    pub fn remaining(&self) -> usize {
        self.data.len() - self.pos
    }

    pub fn is_empty(&self) -> bool {
        self.pos >= self.data.len()
    }

    fn need(&self, count: usize, what: &'static str) -> Result<()> {
        if self.remaining() < count {
            return Err(Error::Malformed { at: self.pos, what });
        }
        Ok(())
    }

    pub fn u1(&mut self) -> Result<u8> {
        self.need(1, "expected u1")?;
        let value = self.data[self.pos];
        self.pos += 1;
        Ok(value)
    }

    pub fn u2(&mut self) -> Result<u16> {
        self.need(2, "expected u2")?;
        let value = u16::from_be_bytes([self.data[self.pos], self.data[self.pos + 1]]);
        self.pos += 2;
        Ok(value)
    }

    pub fn u4(&mut self) -> Result<u32> {
        self.need(4, "expected u4")?;
        let bytes = &self.data[self.pos..self.pos + 4];
        self.pos += 4;
        Ok(u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
    }

    pub fn i4(&mut self) -> Result<i32> {
        Ok(self.u4()? as i32)
    }

    pub fn u8v(&mut self) -> Result<u64> {
        let high = self.u4()? as u64;
        let low = self.u4()? as u64;
        Ok(high << 32 | low)
    }

    pub fn bytes(&mut self, count: usize) -> Result<&'a [u8]> {
        self.need(count, "expected byte run")?;
        let slice = &self.data[self.pos..self.pos + count];
        self.pos += count;
        Ok(slice)
    }

    pub fn skip(&mut self, count: usize) -> Result<()> {
        self.need(count, "expected padding")?;
        self.pos += count;
        Ok(())
    }
}
