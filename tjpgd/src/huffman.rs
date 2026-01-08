//! Huffman decoding implementation

use crate::types::{Error, Result};

/// Huffman table for DC or AC component
#[derive(Debug)]
pub struct HuffmanTable {
    /// Number of codes for each bit length (1-16 bits)
    pub bits: [u8; 16],
    /// Huffman code words
    pub codes: heapless::Vec<u16, 256>,
    /// Decoded data corresponding to each code word
    pub data: heapless::Vec<u8, 256>,
    
    #[cfg(feature = "fast-decode")]
    /// Fast lookup table for short codes
    pub lut: Option<heapless::Vec<u16, 1024>>,
    
    #[cfg(feature = "fast-decode")]
    /// Offset for long codes in the table
    pub long_offset: usize,
}

impl HuffmanTable {
    pub fn new() -> Self {
        Self {
            bits: [0; 16],
            codes: heapless::Vec::new(),
            data: heapless::Vec::new(),
            
            #[cfg(feature = "fast-decode")]
            lut: None,
            
            #[cfg(feature = "fast-decode")]
            long_offset: 0,
        }
    }

    /// Create Huffman table from raw data
    pub fn create(&mut self, bits: &[u8], values: &[u8]) -> Result<()> {
        if bits.len() != 16 {
            return Err(Error::FormatError);
        }

        self.bits.copy_from_slice(bits);
        
        // Calculate total number of codes
        let total_codes: usize = bits.iter().map(|&b| b as usize).sum();
        
        if values.len() != total_codes {
            return Err(Error::FormatError);
        }

        // Build code word table
        self.codes.clear();
        let mut code = 0u16;
        
        for (_bit_len, &count) in bits.iter().enumerate() {
            for _ in 0..count {
                if self.codes.push(code).is_err() {
                    return Err(Error::InsufficientMemory);
                }
                code += 1;
            }
            code <<= 1;
        }

        // Store decoded data
        self.data.clear();
        for &val in values {
            if self.data.push(val).is_err() {
                return Err(Error::InsufficientMemory);
            }
        }

        #[cfg(feature = "fast-decode")]
        self.build_fast_lut()?;

        Ok(())
    }

    #[cfg(feature = "fast-decode")]
    fn build_fast_lut(&mut self) -> Result<()> {
        const HUFF_BIT: usize = 10;
        const HUFF_LEN: usize = 1 << HUFF_BIT;
        const HUFF_MASK: u16 = (HUFF_LEN - 1) as u16;

        let mut lut = heapless::Vec::new();
        lut.resize(HUFF_LEN, 0xFFFF).map_err(|_| Error::InsufficientMemory)?;

        let mut idx = 0;
        for bit_len in 0..HUFF_BIT {
            let count = self.bits[bit_len] as usize;
            
            for _ in 0..count {
                if idx >= self.codes.len() {
                    break;
                }
                
                let code = self.codes[idx];
                let data = self.data[idx];
                idx += 1;

                let table_idx = ((code << (HUFF_BIT - 1 - bit_len)) & HUFF_MASK) as usize;
                let entry = data as u16 | ((bit_len as u16 + 1) << 8);
                let span = 1 << (HUFF_BIT - 1 - bit_len);

                for i in 0..span {
                    if table_idx + i < HUFF_LEN {
                        lut[table_idx + i] = entry;
                    }
                }
            }
        }

        self.long_offset = idx;
        self.lut = Some(lut);
        Ok(())
    }

    /// Extract Huffman decoded value from bit stream
    pub fn decode(&self, bits: &mut BitStream) -> Result<u8> {
        #[cfg(feature = "fast-decode")]
        {
            if let Some(ref lut) = self.lut {
                return self.decode_fast(bits, lut);
            }
        }
        
        self.decode_slow(bits)
    }

    #[cfg(feature = "fast-decode")]
    fn decode_fast(&self, bits: &mut BitStream, lut: &heapless::Vec<u16, 1024>) -> Result<u8> {
        const HUFF_BIT: usize = 10;
        
        // Ensure we have at least HUFF_BIT bits available
        bits.ensure_bits(HUFF_BIT)?;
        
        // Peek at the next HUFF_BIT bits
        let peek = bits.peek(HUFF_BIT)? as usize;
        
        if peek < lut.len() {
            let entry = lut[peek];
            
            if entry != 0xFFFF {
                let code_len = (entry >> 8) as usize;
                let value = (entry & 0xFF) as u8;
                bits.skip(code_len)?;
                return Ok(value);
            }
        }

        // Fall back to slow decode for long codes
        bits.skip(HUFF_BIT)?;
        self.decode_slow_from(bits, self.long_offset)
    }

    #[cfg(feature = "fast-decode")]
    fn decode_slow_from(&self, bits: &mut BitStream, start_idx: usize) -> Result<u8> {
        let mut code = 0u16;
        
        for bit_len in 0..16 {
            let bit = bits.read_bit()?;
            code = (code << 1) | bit as u16;
            
            let count = self.bits[bit_len] as usize;
            let base_idx = start_idx + 
                self.bits[..bit_len].iter().map(|&b| b as usize).sum::<usize>();
            
            for i in 0..count {
                let idx = base_idx + i;
                if idx < self.codes.len() && self.codes[idx] == code {
                    return Ok(self.data[idx]);
                }
            }
        }
        
        Err(Error::FormatError)
    }

    fn decode_slow(&self, bits: &mut BitStream) -> Result<u8> {
        // 使用与C代码相同的策略: 先确保有足够的位,然后从buffer中提取而不是边读边消费
        // C代码: d = w >> (wbit - bl) 只是读取,不修改w
        // 只有匹配成功时才更新dbit
        
        // 确保有足够的位用于最长的code (16 bits)
        bits.ensure_bits(16).or_else(|_| {
            while bits.bits_in_buffer < 16 && bits.pos < bits.data.len() {
                let _ = bits.refill();
            }
            Ok(())
        })?;

        // 获取当前buffer状态用于解码
        let buffer = bits.bit_buffer;
        let total_bits = bits.bits_in_buffer;
        
        let mut data_idx = 0;

        for bit_len in 0..16 {
            let code_len = bit_len + 1;
            
            // 从buffer的高位提取code_len位 (与C代码的 d = w >> (wbit - bl) 对应)
            if total_bits < code_len {
                break;
            }
            let shift = total_bits - code_len;
            let code = ((buffer >> shift) & ((1 << code_len) - 1)) as u16;
            
            let count = self.bits[bit_len] as usize;
            
            // 搜索匹配的code
            for _ in 0..count {
                if data_idx < self.codes.len() && self.codes[data_idx] == code {
                    // 找到匹配,现在消费这些位
                    bits.bits_in_buffer -= code_len;
                    return Ok(self.data[data_idx]);
                }
                data_idx += 1;
            }
        }

        Err(Error::FormatError)
    }
}

/// Bit stream reader with byte stuffing handling
pub struct BitStream<'a> {
    data: &'a [u8],
    pos: usize,
    bit_buffer: u32,
    bits_in_buffer: usize,
    marker_found: Option<u8>,
}

impl<'a> BitStream<'a> {
    pub fn new(data: &'a [u8]) -> Self {
        Self {
            data,
            pos: 0,
            bit_buffer: 0,
            bits_in_buffer: 0,
            marker_found: None,
        }
    }

    /// Read a single bit
    pub fn read_bit(&mut self) -> Result<u8> {
        if self.bits_in_buffer == 0 {
            self.refill()?;
        }

        self.bits_in_buffer -= 1;
        let bit = ((self.bit_buffer >> self.bits_in_buffer) & 1) as u8;
        
        // Debug: log every bit read
        // eprintln!("DEBUG read_bit: bit={}, bits_left={}, buffer={:08X}", bit, self.bits_in_buffer, self.bit_buffer);
        
        Ok(bit)
    }

    /// Read multiple bits (up to 16)
    pub fn read_bits(&mut self, count: usize) -> Result<u16> {
        if count > 16 {
            return Err(Error::Parameter);
        }

        let mut result = 0u16;
        for _ in 0..count {
            result = (result << 1) | self.read_bit()? as u16;
        }
        Ok(result)
    }

    /// Peek at next bits without consuming
    #[allow(dead_code)]
    pub fn peek(&mut self, count: usize) -> Result<u16> {
        self.ensure_bits(count)?;
        let shift = self.bits_in_buffer - count;
        Ok(((self.bit_buffer >> shift) & ((1 << count) - 1)) as u16)
    }

    /// Skip bits
    #[allow(dead_code)]
    pub fn skip(&mut self, count: usize) -> Result<()> {
        if count <= self.bits_in_buffer {
            self.bits_in_buffer -= count;
        } else {
            let mut remaining = count - self.bits_in_buffer;
            self.bits_in_buffer = 0;
            
            while remaining > 0 {
                self.refill()?;
                let to_skip = remaining.min(self.bits_in_buffer);
                self.bits_in_buffer -= to_skip;
                remaining -= to_skip;
            }
        }
        Ok(())
    }

    /// Ensure at least `count` bits are available in buffer
    #[allow(dead_code)]
    pub fn ensure_bits(&mut self, count: usize) -> Result<()> {
        // Keep refilling until we have enough bits or can't refill anymore
        // This matches C code: while (wbit < 16)
        while self.bits_in_buffer < count {
            if self.pos >= self.data.len() && self.marker_found.is_none() {
                break;
            }
            self.refill()?;
        }
        
        if self.bits_in_buffer < count {
            Err(Error::Input)
        } else {
            Ok(())
        }
    }

    fn refill(&mut self) -> Result<()> {
        // 关键: 在左移前清除无效的高位,防止垃圾数据被带入有效区域
        // 这与C代码的 w = jd->wreg & ((1UL << wbit) - 1) 对应
        if self.bits_in_buffer > 0 && self.bits_in_buffer < 32 {
            let mask = (1u32 << self.bits_in_buffer) - 1;
            self.bit_buffer &= mask;
        }
        
        // 如果有marker,生成stuff bits
        if self.marker_found.is_some() {
            self.bit_buffer = (self.bit_buffer << 8) | 0xFF;
            self.bits_in_buffer += 8;
            return Ok(());
        }

        if self.pos >= self.data.len() {
            return Err(Error::Input);
        }

        let byte = self.data[self.pos];
        self.pos += 1;

        // 处理byte stuffing (0xFF转义)
        if byte == 0xFF {
            if self.pos >= self.data.len() {
                return Err(Error::Input);
            }
            
            let next = self.data[self.pos];
            self.pos += 1;

            if next == 0x00 {
                // 转义的0xFF,作为数据使用
                self.bit_buffer = (self.bit_buffer << 8) | 0xFF;
                self.bits_in_buffer += 8;
            } else {
                // 找到marker
                self.marker_found = Some(next);
                // 生成stuff byte
                self.bit_buffer = (self.bit_buffer << 8) | 0xFF;
                self.bits_in_buffer += 8;
            }
        } else {
            self.bit_buffer = (self.bit_buffer << 8) | byte as u32;
            self.bits_in_buffer += 8;
        }

        Ok(())
    }

    #[allow(dead_code)]
    pub fn marker(&self) -> Option<u8> {
        self.marker_found
    }
    
    pub fn reset_for_restart(&mut self) {
        self.bit_buffer = 0;
        self.bits_in_buffer = 0;
        self.marker_found = None;
    }

    /// Get and consume the marker if one has been found
    pub fn get_marker(&mut self) -> Option<u8> {
        self.marker_found.take()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bitstream() {
        let data = [0b10110010, 0b01101100];
        let mut bs = BitStream::new(&data);

        assert_eq!(bs.read_bit().unwrap(), 1);
        assert_eq!(bs.read_bit().unwrap(), 0);
        assert_eq!(bs.read_bits(3).unwrap(), 0b110);
    }
}
