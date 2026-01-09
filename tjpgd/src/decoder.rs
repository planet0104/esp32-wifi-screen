//! JPEG解码器
//! 
//! 与C版本tjpgd完全一致的实现：
//! - 用户提供工作内存池
//! - 所有内部数据从池中分配
//! - 解码器结构体本身很小（~120 bytes，与C版本JDEC一致）

use crate::huffman::{BitStream, HuffmanTable};
use crate::idct::{block_idct, color};
use crate::pool::MemoryPool;
use crate::types::{Error, OutputFormat, Rectangle, Result, SamplingFactor};

/// JPEG marker codes
mod markers {
    pub const SOI: u16 = 0xFFD8;
    pub const SOF0: u8 = 0xC0;
    pub const DHT: u8 = 0xC4;
    pub const DQT: u8 = 0xDB;
    pub const DRI: u8 = 0xDD;
    pub const SOS: u8 = 0xDA;
    pub const EOI: u8 = 0xD9;
}

/// Output callback
pub type OutputCallback<'a> = &'a mut dyn FnMut(&JpegDecoder, &[u8], &Rectangle) -> Result<bool>;

/// 计算所需的工作内存池大小
pub fn calculate_pool_size(_width: u16, _height: u16, fast_decode: bool) -> usize {
    let mut size = 0usize;
    
    // Huffman表（最大4个表）
    if fast_decode {
        size += 4 * (16 + 512 + 256 + 2048 + 64);  // 包括HuffmanTable结构体
    } else {
        size += 4 * (16 + 512 + 256 + 64);
    }
    
    // 量化表（最多4个）
    size += 4 * 256;
    
    // 对齐和余量
    size += 512;
    
    let c_min_size = if fast_decode { 9644 } else { 3500 };
    size.max(c_min_size)
}

/// JPEG解码器
/// 
/// 与C版本JDEC结构体完全一致的内存布局（约120 bytes）
pub struct JpegDecoder<'a> {
    pub(crate) width: u16,
    pub(crate) height: u16,
    num_components: u8,
    sampling: SamplingFactor,
    
    // Huffman表指针（存储原始指针以避免生命周期问题）
    huff_dc: [*const HuffmanTable<'a>; 2],
    huff_ac: [*const HuffmanTable<'a>; 2],
    
    // 量化表指针
    qtables: [*const [i32; 64]; 4],
    qtable_ids: [u8; 3],
    
    dc_values: [i16; 3],
    restart_interval: u16,
    _output_format: OutputFormat,
    scale: u8,
    sos_position: usize,
    
    // 生命周期标记
    _marker: core::marker::PhantomData<&'a ()>,
}

impl<'a> JpegDecoder<'a> {
    pub fn new() -> Self {
        Self {
            width: 0,
            height: 0,
            num_components: 0,
            sampling: SamplingFactor::Yuv444,
            huff_dc: [core::ptr::null(); 2],
            huff_ac: [core::ptr::null(); 2],
            qtables: [core::ptr::null(); 4],
            qtable_ids: [0; 3],
            dc_values: [0; 3],
            restart_interval: 0,
            _output_format: OutputFormat::Rgb565,
            scale: 0,
            sos_position: 0,
            _marker: core::marker::PhantomData,
        }
    }

    /// 准备解码（解析JPEG头部）
    pub fn prepare(&mut self, data: &[u8], pool: &mut MemoryPool<'a>) -> Result<()> {
        let mut pos = 0;

        if data.len() < 2 {
            return Err(Error::Input);
        }

        let mut marker = u16::from_be_bytes([data[0], data[1]]);
        pos += 2;

        if marker != markers::SOI {
            return Err(Error::FormatError);
        }

        loop {
            if pos + 4 > data.len() {
                return Err(Error::Input);
            }

            marker = u16::from_be_bytes([data[pos], data[pos + 1]]);
            let length = u16::from_be_bytes([data[pos + 2], data[pos + 3]]);
            
            if length < 2 || (marker >> 8) != 0xFF {
                return Err(Error::FormatError);
            }

            let seg_start = pos + 4;
            let seg_len = (length - 2) as usize;
            
            if seg_start + seg_len > data.len() {
                return Err(Error::Input);
            }

            let segment = &data[seg_start..seg_start + seg_len];
            
            match (marker & 0xFF) as u8 {
                markers::SOF0 => self.parse_sof(segment)?,
                markers::DHT => self.parse_dht(segment, pool)?,
                markers::DQT => self.parse_dqt(segment, pool)?,
                markers::DRI => self.parse_dri(segment)?,
                markers::SOS => {
                    self.parse_sos(segment)?;
                    self.sos_position = pos;
                    return Ok(());
                }
                markers::EOI => return Err(Error::FormatError),
                _ if (marker & 0xFF) as u8 >= 0xC0 && (marker & 0xFF) as u8 <= 0xCF => {
                    return Err(Error::UnsupportedStandard);
                }
                _ => {}
            }

            pos = seg_start + seg_len;
        }
    }

    fn parse_sof(&mut self, data: &[u8]) -> Result<()> {
        if data.len() < 6 {
            return Err(Error::FormatError);
        }

        if data[0] != 8 {
            return Err(Error::UnsupportedFormat);
        }

        self.height = u16::from_be_bytes([data[1], data[2]]);
        self.width = u16::from_be_bytes([data[3], data[4]]);
        self.num_components = data[5];

        if self.num_components != 1 && self.num_components != 3 {
            return Err(Error::UnsupportedStandard);
        }

        let expected_len = 6 + self.num_components as usize * 3;
        if data.len() < expected_len {
            return Err(Error::FormatError);
        }

        for i in 0..self.num_components as usize {
            let comp_start = 6 + i * 3;
            let sampling_factor = data[comp_start + 1];
            let qtable_id = data[comp_start + 2];

            if i == 0 {
                let h = sampling_factor >> 4;
                let v = sampling_factor & 0x0F;
                self.sampling = SamplingFactor::from_factor(h, v)
                    .ok_or(Error::UnsupportedFormat)?;
            } else if sampling_factor != 0x11 {
                return Err(Error::UnsupportedFormat);
            }

            if i < 3 {
                self.qtable_ids[i] = qtable_id;
            }

            if qtable_id > 3 {
                return Err(Error::FormatError);
            }
        }

        Ok(())
    }

    fn parse_dht(&mut self, mut data: &[u8], pool: &mut MemoryPool<'a>) -> Result<()> {
        while !data.is_empty() {
            if data.len() < 17 {
                return Err(Error::FormatError);
            }

            let table_info = data[0];
            let class = (table_info >> 4) & 0x01;
            let id = table_info & 0x0F;

            if id > 1 {
                return Err(Error::FormatError);
            }

            let bits = &data[1..17];
            let num_codes: usize = bits.iter().map(|&b| b as usize).sum();

            if data.len() < 17 + num_codes {
                return Err(Error::FormatError);
            }

            let values = &data[17..17 + num_codes];

            // 从池中创建Huffman表
            let table = HuffmanTable::create_in_pool(pool, bits, values)?;
            
            // 分配结构体存储空间
            let table_size = core::mem::size_of::<HuffmanTable>();
            let table_mem = pool.alloc(table_size).ok_or(Error::InsufficientMemory)?;
            
            unsafe {
                let table_ptr = table_mem.as_mut_ptr() as *mut HuffmanTable<'a>;
                core::ptr::write(table_ptr, table);
                
                if class == 0 {
                    self.huff_dc[id as usize] = table_ptr;
                } else {
                    self.huff_ac[id as usize] = table_ptr;
                }
            }

            data = &data[17 + num_codes..];
        }

        Ok(())
    }

    fn parse_dqt(&mut self, mut data: &[u8], pool: &mut MemoryPool<'a>) -> Result<()> {
        use crate::tables::{ZIGZAG, ARAI_SCALE_FACTOR};
        
        while !data.is_empty() {
            let table_info = data[0];
            let precision = (table_info >> 4) & 0x0F;
            let id = table_info & 0x0F;

            if id > 3 {
                return Err(Error::FormatError);
            }

            // 分配量化表存储空间
            let qtable_mem = pool.alloc(64 * 4).ok_or(Error::InsufficientMemory)?;
            let qtable_ptr = qtable_mem.as_mut_ptr() as *mut i32;
            
            unsafe {
                let qtable = core::slice::from_raw_parts_mut(qtable_ptr, 64);
                
                if precision == 0 {
                    if data.len() < 65 {
                        return Err(Error::FormatError);
                    }
                    for i in 0..64 {
                        let zi = ZIGZAG[i] as usize;
                        let q_value = data[1 + i] as u32;
                        let ipsf = ARAI_SCALE_FACTOR[zi] as u32;
                        qtable[zi] = (q_value * ipsf) as i32;
                    }
                    data = &data[65..];
                } else {
                    if data.len() < 129 {
                        return Err(Error::FormatError);
                    }
                    for i in 0..64 {
                        let zi = ZIGZAG[i] as usize;
                        let q_value = u16::from_be_bytes([data[1 + i * 2], data[2 + i * 2]]) as u32;
                        let ipsf = ARAI_SCALE_FACTOR[zi] as u32;
                        qtable[zi] = (q_value * ipsf) as i32;
                    }
                    data = &data[129..];
                }
                
                self.qtables[id as usize] = qtable_ptr as *const [i32; 64];
            }
        }

        Ok(())
    }

    fn parse_dri(&mut self, data: &[u8]) -> Result<()> {
        if data.len() < 2 {
            return Err(Error::FormatError);
        }
        self.restart_interval = u16::from_be_bytes([data[0], data[1]]);
        Ok(())
    }

    fn parse_sos(&self, data: &[u8]) -> Result<()> {
        if data.is_empty() {
            return Err(Error::FormatError);
        }

        let num_components = data[0];
        if num_components != self.num_components {
            return Err(Error::FormatError);
        }

        for i in 0..self.num_components as usize {
            let table_id = if i == 0 { 0 } else { 1 };
            
            if self.huff_dc[table_id].is_null() || self.huff_ac[table_id].is_null() {
                return Err(Error::FormatError);
            }

            if self.qtables[self.qtable_ids[i] as usize].is_null() {
                return Err(Error::FormatError);
            }
        }

        Ok(())
    }

    /// 解压JPEG图像
    /// 
    /// # Arguments
    /// * `data` - 完整的JPEG数据
    /// * `scale` - 缩放因子 (0-3)
    /// * `mcu_buffer` - MCU工作缓冲区（由用户提供）
    /// * `work_buffer` - RGB转换工作缓冲区（由用户提供）
    /// * `callback` - 输出回调函数
    pub fn decompress(
        &mut self,
        data: &[u8],
        scale: u8,
        mcu_buffer: &mut [i16],
        work_buffer: &mut [u8],
        callback: OutputCallback,
    ) -> Result<()> {
        if scale > 3 {
            return Err(Error::Parameter);
        }

        // 验证缓冲区大小
        let mcu_size = self.mcu_buffer_size();
        let work_size = self.work_buffer_size();
        
        if mcu_buffer.len() < mcu_size {
            return Err(Error::InsufficientMemory);
        }
        if work_buffer.len() < work_size {
            return Err(Error::InsufficientMemory);
        }

        self.scale = scale;
        self.dc_values = [0; 3];

        let mcu_width = self.sampling.mcu_width() as usize;
        let mcu_height = self.sampling.mcu_height() as usize;
        let mcu_pixel_width = mcu_width * 8;
        let mcu_pixel_height = mcu_height * 8;

        let scan_data = self.find_scan_data(data)?;
        let mut bitstream = BitStream::new(scan_data);

        let mut restart_counter = 0u16;
        let mut restart_marker = 0u8;

        for mcu_y in (0..self.height).step_by(mcu_pixel_height) {
            for mcu_x in (0..self.width).step_by(mcu_pixel_width) {
                if self.restart_interval > 0 && restart_counter >= self.restart_interval {
                    bitstream.reset_for_restart();
                    self.dc_values = [0; 3];
                    restart_counter = 0;
                    restart_marker = (restart_marker + 1) & 0x07;
                }

                self.decode_mcu(&mut bitstream, mcu_buffer, mcu_width, mcu_height)?;

                if let Some(marker) = bitstream.get_marker() {
                    if marker >= 0xD0 && marker <= 0xD7 {
                        bitstream.reset_for_restart();
                        self.dc_values = [0; 3];
                        restart_marker = ((marker - 0xD0) + 1) & 0x07;
                    }
                }

                self.output_mcu(
                    mcu_buffer,
                    work_buffer,
                    mcu_x,
                    mcu_y,
                    mcu_width,
                    mcu_height,
                    callback,
                )?;

                restart_counter += 1;
            }
        }

        Ok(())
    }

    /// 获取MCU缓冲区所需大小
    pub fn mcu_buffer_size(&self) -> usize {
        let mcu_width = self.sampling.mcu_width() as usize;
        let mcu_height = self.sampling.mcu_height() as usize;
        (mcu_width * mcu_height + 2) * 64
    }

    /// 获取工作缓冲区所需大小
    pub fn work_buffer_size(&self) -> usize {
        let mcu_width = self.sampling.mcu_width() as usize;
        let mcu_height = self.sampling.mcu_height() as usize;
        mcu_width * 8 * mcu_height * 8 * 3
    }

    fn find_scan_data<'b>(&self, data: &'b [u8]) -> Result<&'b [u8]> {
        let i = self.sos_position;
        
        if i + 4 > data.len() {
            return Err(Error::Input);
        }
        
        if data[i] != 0xFF || data[i + 1] != markers::SOS {
            return Err(Error::FormatError);
        }
        
        let seg_len = u16::from_be_bytes([data[i + 2], data[i + 3]]) as usize;
        let scan_start = i + 2 + seg_len;
        
        if scan_start < data.len() {
            Ok(&data[scan_start..])
        } else {
            Err(Error::Input)
        }
    }

    fn decode_mcu(
        &mut self,
        bitstream: &mut BitStream,
        buffer: &mut [i16],
        mcu_width: usize,
        mcu_height: usize,
    ) -> Result<()> {
        let num_y_blocks = mcu_width * mcu_height;
        let mut tmp = [0i32; 64];

        // 解码Y blocks
        for i in 0..num_y_blocks {
            let block_slice = &mut buffer[i * 64..(i + 1) * 64];
            let block: &mut [i16; 64] = block_slice.try_into().map_err(|_| Error::FormatError)?;
            let qtable_id = self.qtable_ids[0];
            
            self.decode_and_dequantize_block(bitstream, &mut tmp, qtable_id, 0)?;
            block_idct(&mut tmp, block);
        }

        if self.num_components == 3 {
            // Cb block
            let cb_offset = num_y_blocks * 64;
            let cb_slice = &mut buffer[cb_offset..cb_offset + 64];
            let cb_block: &mut [i16; 64] = cb_slice.try_into().map_err(|_| Error::FormatError)?;
            self.decode_and_dequantize_block(bitstream, &mut tmp, self.qtable_ids[1], 1)?;
            block_idct(&mut tmp, cb_block);

            // Cr block
            let cr_offset = cb_offset + 64;
            let cr_slice = &mut buffer[cr_offset..cr_offset + 64];
            let cr_block: &mut [i16; 64] = cr_slice.try_into().map_err(|_| Error::FormatError)?;
            self.decode_and_dequantize_block(bitstream, &mut tmp, self.qtable_ids[2], 2)?;
            block_idct(&mut tmp, cr_block);
        }

        Ok(())
    }

    fn decode_and_dequantize_block(
        &mut self,
        bitstream: &mut BitStream,
        tmp: &mut [i32; 64],
        qtable_id: u8,
        component: usize,
    ) -> Result<()> {
        use crate::tables::ZIGZAG;
        
        let qtable = unsafe {
            let ptr = self.qtables[qtable_id as usize];
            if ptr.is_null() {
                return Err(Error::FormatError);
            }
            &*ptr
        };
        
        let table_id = if component == 0 { 0 } else { 1 };

        let dc_table = unsafe {
            let ptr = self.huff_dc[table_id];
            if ptr.is_null() {
                return Err(Error::FormatError);
            }
            &*ptr
        };
        
        let dc_len = dc_table.decode(bitstream)? as usize;
        
        let dc_diff = if dc_len > 0 {
            let bits = bitstream.read_bits(dc_len)?;
            Self::extend(bits, dc_len) as i32
        } else {
            0
        };

        self.dc_values[component] = self.dc_values[component].wrapping_add(dc_diff as i16);
        let dc = self.dc_values[component] as i32;
        
        tmp[0] = (dc * qtable[0]) >> 8;
        tmp[1..].fill(0);

        let ac_table = unsafe {
            let ptr = self.huff_ac[table_id];
            if ptr.is_null() {
                return Err(Error::FormatError);
            }
            &*ptr
        };
        
        let mut z = 1;

        loop {
            let symbol = ac_table.decode(bitstream)?;
            
            if symbol == 0 {
                break;
            }

            let zero_run = (symbol >> 4) as usize;
            let ac_len = (symbol & 0x0F) as usize;

            z += zero_run;
            
            if z >= 64 {
                return Err(Error::FormatError);
            }

            if ac_len > 0 {
                let bits = bitstream.read_bits(ac_len)?;
                let ac_value = Self::extend(bits, ac_len) as i32;
                let i = ZIGZAG[z] as usize;
                tmp[i] = (ac_value * qtable[i]) >> 8;
            }

            z += 1;
            
            if z >= 64 {
                break;
            }
        }
        
        Ok(())
    }

    fn extend(v: u16, t: usize) -> i16 {
        let vt = 1 << (t - 1);
        if (v as i16) < vt {
            v as i16 + ((-1i16) << t) + 1
        } else {
            v as i16
        }
    }

    fn output_mcu(
        &self,
        mcu_buffer: &[i16],
        work_buffer: &mut [u8],
        x: u16,
        y: u16,
        mcu_width: usize,
        mcu_height: usize,
        callback: OutputCallback,
    ) -> Result<()> {
        let mcu_pixel_width = (mcu_width * 8) as u16;
        let mcu_pixel_height = (mcu_height * 8) as u16;

        let out_width = mcu_pixel_width.min(self.width - x);
        let out_height = mcu_pixel_height.min(self.height - y);

        let scaled_width = out_width >> self.scale;
        let scaled_height = out_height >> self.scale;

        if scaled_width == 0 || scaled_height == 0 {
            return Ok(());
        }

        let rect = Rectangle::new(
            x >> self.scale,
            (x >> self.scale) + scaled_width - 1,
            y >> self.scale,
            (y >> self.scale) + scaled_height - 1,
        );

        if self.num_components == 3 {
            let num_y_blocks = mcu_width * mcu_height;
            let y_data = &mcu_buffer[0..num_y_blocks * 64];
            let cb_data = &mcu_buffer[num_y_blocks * 64..(num_y_blocks + 1) * 64];
            let cr_data = &mcu_buffer[(num_y_blocks + 1) * 64..(num_y_blocks + 2) * 64];

            color::mcu_to_rgb(
                y_data,
                cb_data,
                cr_data,
                work_buffer,
                mcu_width,
                mcu_height,
                self.sampling.mcu_width() as usize,
                self.sampling.mcu_height() as usize,
            );
        } else {
            color::mcu_to_grayscale(mcu_buffer, work_buffer, mcu_width, mcu_height);
        }

        let rx = scaled_width as usize;
        let ry = scaled_height as usize;
        let mx = (mcu_pixel_width >> self.scale) as usize;
        
        if rx < mx {
            let mut s = 0usize;
            let mut d = 0usize;
            for _y in 0..ry {
                for _x in 0..rx {
                    work_buffer[d] = work_buffer[s];
                    work_buffer[d + 1] = work_buffer[s + 1];
                    work_buffer[d + 2] = work_buffer[s + 2];
                    s += 3;
                    d += 3;
                }
                s += (mx - rx) * 3;
            }
        }

        let continue_processing = callback(self, work_buffer, &rect)?;
        
        if !continue_processing {
            return Err(Error::Interrupted);
        }

        Ok(())
    }

    /// 获取输出宽度（已应用缩放）
    pub fn width(&self) -> u16 {
        self.width >> self.scale
    }

    /// 获取输出高度（已应用缩放）
    pub fn height(&self) -> u16 {
        self.height >> self.scale
    }

    /// 获取原始图像宽度
    pub fn raw_width(&self) -> u16 {
        self.width
    }

    /// 获取原始图像高度
    pub fn raw_height(&self) -> u16 {
        self.height
    }

    pub fn components(&self) -> u8 {
        self.num_components
    }
}

impl Default for JpegDecoder<'_> {
    fn default() -> Self {
        Self::new()
    }
}
