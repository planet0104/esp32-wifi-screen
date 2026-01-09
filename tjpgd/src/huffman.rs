//! Huffman解码实现
//! 
//! 与C版本tjpgd完全一致的实现，支持三种优化级别：
//! - fast-decode-0: 基础优化，适合 8/16 位 MCU
//! - fast-decode-1: + 32 位桶移位器，适合 32 位 MCU
//! - fast-decode-2: + Huffman 快速查找表
//!
//! 所有数据从用户提供的工作内存池中分配

use crate::types::{Error, Result};
use crate::pool::MemoryPool;

// 确定当前使用的优化级别
#[cfg(feature = "fast-decode-2")]
const FASTDECODE_LEVEL: u8 = 2;
#[cfg(all(feature = "fast-decode-1", not(feature = "fast-decode-2")))]
const FASTDECODE_LEVEL: u8 = 1;
#[cfg(all(feature = "fast-decode-0", not(feature = "fast-decode-1"), not(feature = "fast-decode-2")))]
const FASTDECODE_LEVEL: u8 = 0;
#[cfg(not(any(feature = "fast-decode-0", feature = "fast-decode-1", feature = "fast-decode-2")))]
const FASTDECODE_LEVEL: u8 = 1; // 默认使用 level 1

/// Huffman 快速查找表配置 (JD_FASTDECODE == 2)
#[cfg(feature = "fast-decode-2")]
pub const HUFF_BIT: usize = 10;
#[cfg(feature = "fast-decode-2")]
pub const HUFF_LEN: usize = 1 << HUFF_BIT;

/// Huffman表
/// 
/// 与C版本的内存布局完全一致：
/// - bits: 16 bytes (固定)
/// - codes: 动态分配 (num_codes * 2 bytes)
/// - data: 动态分配 (num_codes bytes)
/// - lut: 可选的快速查找表 (JD_FASTDECODE == 2 时使用)
#[derive(Debug)]
pub struct HuffmanTable<'a> {
    /// 每个位长度的码字数量 (1-16位)
    pub bits: [u8; 16],
    /// Huffman码字 - 从池中分配
    pub codes: &'a mut [u16],
    /// 解码数据 - 从池中分配  
    pub data: &'a mut [u8],
    /// 码字总数
    pub num_codes: usize,
    
    /// 快速查找表 - 从池中分配 (JD_FASTDECODE == 2)
    #[cfg(feature = "fast-decode-2")]
    pub lut: Option<&'a mut [u16]>,
    
    /// 长码字的起始偏移 (JD_FASTDECODE == 2)
    #[cfg(feature = "fast-decode-2")]
    pub long_offset: usize,
}

impl<'a> HuffmanTable<'a> {
    /// 从内存池中创建Huffman表
    pub fn create_in_pool(
        pool: &mut MemoryPool<'a>,
        bits: &[u8],
        values: &[u8],
    ) -> Result<Self> {
        if bits.len() != 16 {
            return Err(Error::FormatError);
        }

        // 计算码字总数
        let num_codes: usize = bits.iter().map(|&b| b as usize).sum();
        
        if values.len() != num_codes {
            return Err(Error::FormatError);
        }

        // 从池中分配codes数组
        let codes = pool.alloc_u16(num_codes).ok_or(Error::InsufficientMemory)?;
        
        // 从池中分配data数组  
        let data = pool.alloc_u8(num_codes).ok_or(Error::InsufficientMemory)?;

        // 复制bits
        let mut bits_arr = [0u8; 16];
        bits_arr.copy_from_slice(bits);

        // 构建码字表 - 与C版本逻辑一致
        let mut code = 0u16;
        let mut idx = 0;
        
        for (_bit_len, &count) in bits.iter().enumerate() {
            for _ in 0..count {
                codes[idx] = code;
                idx += 1;
                code += 1;
            }
            code <<= 1;
        }

        // 复制解码数据
        data.copy_from_slice(values);

        let mut table = Self {
            bits: bits_arr,
            codes,
            data,
            num_codes,
            #[cfg(feature = "fast-decode-2")]
            lut: None,
            #[cfg(feature = "fast-decode-2")]
            long_offset: 0,
        };

        #[cfg(feature = "fast-decode-2")]
        table.build_fast_lut(pool)?;

        Ok(table)
    }

    /// 构建快速查找表 (JD_FASTDECODE == 2)
    #[cfg(feature = "fast-decode-2")]
    fn build_fast_lut(&mut self, pool: &mut MemoryPool<'a>) -> Result<()> {
        // 从池中分配LUT (2048 entries * 2 bytes = 4096 bytes)
        let lut = pool.alloc_u16(HUFF_LEN).ok_or(Error::InsufficientMemory)?;
        
        // 初始化为0xFFFF (无效标记)
        for entry in lut.iter_mut() {
            *entry = 0xFFFF;
        }

        let mut idx = 0;
        for bit_len in 0..HUFF_BIT {
            let count = self.bits[bit_len] as usize;
            
            for _ in 0..count {
                if idx >= self.num_codes {
                    break;
                }
                
                let code = self.codes[idx];
                let data = self.data[idx];
                idx += 1;

                // 计算表索引和填充跨度
                let shift = HUFF_BIT - 1 - bit_len;
                let table_idx = ((code << shift) & (HUFF_LEN as u16 - 1)) as usize;
                let entry = data as u16 | ((bit_len as u16 + 1) << 8);
                let span = 1 << shift;

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

    /// 从位流解码Huffman值
    pub fn decode(&self, bits: &mut BitStream) -> Result<u8> {
        // JD_FASTDECODE == 2: 使用 LUT 快速查找
        #[cfg(feature = "fast-decode-2")]
        {
            if let Some(ref lut) = self.lut {
                return self.decode_fastdecode2(bits, lut);
            }
        }
        
        // JD_FASTDECODE >= 1: 使用 32 位寄存器
        #[cfg(any(feature = "fast-decode-1", feature = "fast-decode-2"))]
        {
            return self.decode_fastdecode1(bits);
        }
        
        // JD_FASTDECODE == 0: 基础逐位解码
        #[cfg(all(feature = "fast-decode-0", not(feature = "fast-decode-1"), not(feature = "fast-decode-2")))]
        {
            return self.decode_fastdecode0(bits);
        }
        
        // 默认使用 level 1
        #[cfg(not(any(feature = "fast-decode-0", feature = "fast-decode-1", feature = "fast-decode-2")))]
        {
            self.decode_fastdecode1(bits)
        }
    }

    /// JD_FASTDECODE == 0: 基础逐位解码
    /// 适合 8/16 位 MCU，与 C 版本完全一致
    #[cfg(any(feature = "fast-decode-0", not(any(feature = "fast-decode-1", feature = "fast-decode-2"))))]
    #[allow(dead_code)]
    fn decode_fastdecode0(&self, bits: &mut BitStream) -> Result<u8> {
        let mut d = 0u16;
        let mut data_idx = 0usize;
        
        // 搜索 1-16 位长度的码字
        for bit_len in 0..16 {
            // 读取一位
            let bit = bits.read_bit_level0()?;
            d = (d << 1) | bit as u16;
            
            // 在当前位长度搜索码字
            let count = self.bits[bit_len] as usize;
            for _ in 0..count {
                if data_idx < self.num_codes && self.codes[data_idx] == d {
                    return Ok(self.data[data_idx]);
                }
                data_idx += 1;
            }
        }
        
        Err(Error::FormatError)
    }

    /// JD_FASTDECODE >= 1: 使用 32 位寄存器
    /// 适合 32 位 MCU，与 C 版本 huffext() 函数严格对齐
    #[cfg(any(feature = "fast-decode-1", feature = "fast-decode-2", not(feature = "fast-decode-0")))]
    fn decode_fastdecode1(&self, bits: &mut BitStream) -> Result<u8> {
        // 获取当前寄存器状态
        let wbit = bits.bits_in_buffer % 32;
        let mut w = if wbit > 0 && wbit < 32 {
            bits.bit_buffer & ((1u32 << wbit) - 1)
        } else if wbit == 0 {
            0
        } else {
            bits.bit_buffer
        };
        let mut wbit = wbit;
        
        let mut dc = bits.data.len() - bits.pos;
        let mut flg = false;
        
        // 填充到至少 16 位 - 与 C 版本完全一致
        while wbit < 16 {
            let d: u8;
            
            if bits.marker_found.is_some() {
                d = 0xFF; // 生成填充位
            } else {
                if dc == 0 {
                    return Err(Error::Input);
                }
                
                let byte = bits.data[bits.pos];
                bits.pos += 1;
                dc -= 1;
                
                if flg {
                    flg = false;
                    if byte != 0 {
                        bits.marker_found = Some(byte);
                    }
                    d = 0xFF;
                } else {
                    if byte == 0xFF {
                        flg = true;
                        continue;
                    }
                    d = byte;
                }
            }
            
            w = (w << 8) | d as u32;
            wbit += 8;
        }
        
        // 更新位流状态
        bits.bit_buffer = w;
        
        // 增量搜索所有码字 - 与 C 版本一致
        let mut data_idx = 0;

        for bit_len in 0..16 {
            let bl = bit_len + 1;
            let count = self.bits[bit_len] as usize;
            
            if count > 0 {
                let d = (w >> (wbit - bl)) as u16;
                
                for _ in 0..count {
                    if data_idx < self.num_codes && self.codes[data_idx] == d {
                        bits.bits_in_buffer = wbit - bl;
                        return Ok(self.data[data_idx]);
                    }
                    data_idx += 1;
                }
            }
        }

        Err(Error::FormatError)
    }

    /// JD_FASTDECODE == 2: LUT 快速查找 + 增量搜索
    /// 最高性能，需要更多内存
    #[cfg(feature = "fast-decode-2")]
    fn decode_fastdecode2(&self, bits: &mut BitStream, lut: &[u16]) -> Result<u8> {
        // 获取当前寄存器状态
        let wbit = bits.bits_in_buffer % 32;
        let mut w = if wbit > 0 && wbit < 32 {
            bits.bit_buffer & ((1u32 << wbit) - 1)
        } else if wbit == 0 {
            0
        } else {
            bits.bit_buffer
        };
        let mut wbit = wbit;
        
        let mut dc = bits.data.len() - bits.pos;
        let mut flg = false;
        
        // 填充到至少 16 位
        while wbit < 16 {
            let d: u8;
            
            if bits.marker_found.is_some() {
                d = 0xFF;
            } else {
                if dc == 0 {
                    return Err(Error::Input);
                }
                
                let byte = bits.data[bits.pos];
                bits.pos += 1;
                dc -= 1;
                
                if flg {
                    flg = false;
                    if byte != 0 {
                        bits.marker_found = Some(byte);
                    }
                    d = 0xFF;
                } else {
                    if byte == 0xFF {
                        flg = true;
                        continue;
                    }
                    d = byte;
                }
            }
            
            w = (w << 8) | d as u32;
            wbit += 8;
        }
        
        // 更新位流状态
        bits.bit_buffer = w;
        
        // LUT 快速查找 - 与 C 版本一致
        let d = (w >> (wbit - HUFF_BIT)) as usize;
        if d < lut.len() {
            let entry = lut[d];
            if entry != 0xFFFF {
                let code_len = (entry >> 8) as usize;
                let value = (entry & 0xFF) as u8;
                bits.bits_in_buffer = wbit - code_len;
                return Ok(value);
            }
        }
        
        // LUT 没命中，增量搜索长码字 (从 HUFF_BIT + 1 开始)
        // 与 C 版本完全一致
        let mut data_idx = self.long_offset;
        
        for bit_len in HUFF_BIT..16 {
            let bl = bit_len + 1;
            let count = self.bits[bit_len] as usize;
            
            if count > 0 {
                let d = (w >> (wbit - bl)) as u16;
                
                for _ in 0..count {
                    if data_idx < self.num_codes && self.codes[data_idx] == d {
                        bits.bits_in_buffer = wbit - bl;
                        return Ok(self.data[data_idx]);
                    }
                    data_idx += 1;
                }
            }
        }

        Err(Error::FormatError)
    }
}

/// 位流读取器
/// 
/// 与C版本的位流处理完全一致
pub struct BitStream<'a> {
    pub(crate) data: &'a [u8],
    pub(crate) pos: usize,
    pub bit_buffer: u32,
    pub bits_in_buffer: usize,
    pub(crate) marker_found: Option<u8>,
    
    /// JD_FASTDECODE == 0 使用的位掩码
    #[cfg(feature = "fast-decode-0")]
    pub(crate) bit_mask: u8,
}

impl<'a> BitStream<'a> {
    pub fn new(data: &'a [u8]) -> Self {
        Self {
            data,
            pos: 0,
            bit_buffer: 0,
            bits_in_buffer: 0,
            marker_found: None,
            #[cfg(feature = "fast-decode-0")]
            bit_mask: 0,
        }
    }

    /// JD_FASTDECODE == 0: 逐位读取，与 C 版本完全一致
    #[cfg(any(feature = "fast-decode-0", not(any(feature = "fast-decode-1", feature = "fast-decode-2"))))]
    #[allow(dead_code)]
    pub fn read_bit_level0(&mut self) -> Result<u8> {
        // 检查是否需要新字节
        if self.bit_mask == 0 {
            loop {
                if self.pos >= self.data.len() {
                    return Err(Error::Input);
                }
                
                let byte = self.data[self.pos];
                self.pos += 1;
                
                // 处理 0xFF escape 序列
                if self.marker_found.is_some() {
                    // 在 marker 后生成填充位
                    self.bit_buffer = 0xFF;
                    self.bit_mask = 0x80;
                    break;
                } else if byte == 0xFF {
                    // 检查下一个字节
                    if self.pos >= self.data.len() {
                        return Err(Error::Input);
                    }
                    let next = self.data[self.pos];
                    self.pos += 1;
                    
                    if next != 0 {
                        // 这是一个 marker，不是 escape
                        self.marker_found = Some(next);
                    }
                    // 0xFF 0x00 -> 数据 0xFF
                    self.bit_buffer = 0xFF;
                    self.bit_mask = 0x80;
                    break;
                } else {
                    self.bit_buffer = byte as u32;
                    self.bit_mask = 0x80;
                    break;
                }
            }
        }
        
        let bit = if (self.bit_buffer as u8) & self.bit_mask != 0 { 1 } else { 0 };
        self.bit_mask >>= 1;
        Ok(bit)
    }

    /// 读取单个位 (JD_FASTDECODE >= 1)
    pub fn read_bit(&mut self) -> Result<u8> {
        if self.bits_in_buffer == 0 {
            self.refill()?;
        }

        self.bits_in_buffer -= 1;
        let bit = ((self.bit_buffer >> self.bits_in_buffer) & 1) as u8;
        Ok(bit)
    }

    /// 读取多个位 (JD_FASTDECODE == 0)
    #[cfg(any(feature = "fast-decode-0", not(any(feature = "fast-decode-1", feature = "fast-decode-2"))))]
    #[allow(dead_code)]
    pub fn read_bits_level0(&mut self, nbit: usize) -> Result<u16> {
        let mut d = 0u16;
        for _ in 0..nbit {
            let bit = self.read_bit_level0()?;
            d = (d << 1) | bit as u16;
        }
        Ok(d)
    }

    /// 读取多个位 - 与 C 版本 bitext() 完全一致
    pub fn read_bits(&mut self, nbit: usize) -> Result<u16> {
        if nbit == 0 {
            return Ok(0);
        }
        if nbit > 16 {
            return Err(Error::Parameter);
        }

        // JD_FASTDECODE == 0: 使用逐位读取
        #[cfg(all(feature = "fast-decode-0", not(feature = "fast-decode-1"), not(feature = "fast-decode-2")))]
        {
            return self.read_bits_level0(nbit);
        }

        // JD_FASTDECODE >= 1: 使用 32 位寄存器
        #[cfg(any(feature = "fast-decode-1", feature = "fast-decode-2", not(feature = "fast-decode-0")))]
        {
            let mut wbit = self.bits_in_buffer % 32;
            let mut w = if wbit > 0 && wbit < 32 {
                self.bit_buffer & ((1u32 << wbit) - 1)
            } else if wbit == 0 {
                0
            } else {
                self.bit_buffer
            };
            
            let mut dc = self.data.len() - self.pos;
            let mut flg = false;
            
            while wbit < nbit {
                let d: u8;
                
                if self.marker_found.is_some() {
                    d = 0xFF;
                } else {
                    if dc == 0 {
                        return Err(Error::Input);
                    }
                    
                    let byte = self.data[self.pos];
                    self.pos += 1;
                    dc -= 1;
                    
                    if flg {
                        flg = false;
                        if byte != 0 {
                            self.marker_found = Some(byte);
                        }
                        d = 0xFF;
                    } else {
                        if byte == 0xFF {
                            flg = true;
                            continue;
                        }
                        d = byte;
                    }
                }
                
                w = (w << 8) | d as u32;
                wbit += 8;
            }
            
            self.bit_buffer = w;
            self.bits_in_buffer = wbit - nbit;
            
            let shift = (wbit - nbit) % 32;
            let result = (w >> shift) & ((1u32 << nbit) - 1);
            Ok(result as u16)
        }
    }

    #[allow(dead_code)]
    pub fn peek(&mut self, count: usize) -> Result<u16> {
        self.ensure_bits(count)?;
        let shift = self.bits_in_buffer - count;
        Ok(((self.bit_buffer >> shift) & ((1 << count) - 1)) as u16)
    }

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

    #[allow(dead_code)]
    pub fn ensure_bits(&mut self, count: usize) -> Result<()> {
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
        if self.bits_in_buffer > 0 && self.bits_in_buffer < 32 {
            let mask = (1u32 << self.bits_in_buffer) - 1;
            self.bit_buffer &= mask;
        }
        
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

        if byte == 0xFF {
            if self.pos >= self.data.len() {
                return Err(Error::Input);
            }
            
            let next = self.data[self.pos];
            self.pos += 1;

            if next == 0x00 {
                self.bit_buffer = (self.bit_buffer << 8) | 0xFF;
                self.bits_in_buffer += 8;
            } else {
                self.marker_found = Some(next);
                self.bit_buffer = (self.bit_buffer << 8) | 0xFF;
                self.bits_in_buffer += 8;
            }
        } else {
            self.bit_buffer = (self.bit_buffer << 8) | byte as u32;
            self.bits_in_buffer += 8;
        }

        Ok(())
    }

    pub fn reset_for_restart(&mut self) {
        self.bit_buffer = 0;
        self.bits_in_buffer = 0;
        self.marker_found = None;
        #[cfg(feature = "fast-decode-0")]
        {
            self.bit_mask = 0;
        }
    }

    pub fn get_marker(&mut self) -> Option<u8> {
        self.marker_found.take()
    }
}

/// 返回当前使用的 FASTDECODE 级别
pub fn fastdecode_level() -> u8 {
    FASTDECODE_LEVEL
}
