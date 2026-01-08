//! Main JPEG decoder implementation

use crate::huffman::{BitStream, HuffmanTable};
use crate::idct::{block_idct, color};
use crate::types::{Error, OutputFormat, Rectangle, Result, SamplingFactor};

/// JPEG marker codes
mod markers {
    pub const SOI: u16 = 0xFFD8; // Start of Image
    pub const SOF0: u8 = 0xC0; // Start of Frame (Baseline)
    pub const DHT: u8 = 0xC4; // Define Huffman Table
    pub const DQT: u8 = 0xDB; // Define Quantization Table
    pub const DRI: u8 = 0xDD; // Define Restart Interval
    pub const SOS: u8 = 0xDA; // Start of Scan
    #[allow(dead_code)]
    pub const RST0: u8 = 0xD0; // Restart markers
    pub const EOI: u8 = 0xD9; // End of Image
}

/// Output callback function type
pub type OutputCallback<'a> = &'a mut dyn FnMut(&JpegDecoder, &[u8], &Rectangle) -> Result<bool>;

/// Input callback function type for streaming input
/// Returns the number of bytes actually read
pub type InputCallback<'a> = &'a mut dyn FnMut(&mut [u8]) -> usize;

/// Main JPEG decoder structure
pub struct JpegDecoder {
    // Image dimensions
    width: u16,
    height: u16,
    
    // Component information
    num_components: u8,
    sampling: SamplingFactor,
    
    // Huffman tables [component][dc/ac]
    huff_dc: [Option<HuffmanTable>; 2],
    huff_ac: [Option<HuffmanTable>; 2],
    
    // Quantization tables
    qtables: [Option<Box<[i32; 64]>>; 4],
    qtable_ids: [u8; 3], // For Y, Cb, Cr
    
    // DC coefficients for each component
    dc_values: [i16; 3],
    
    // Restart interval
    restart_interval: u16,
    
    // Output format
    _output_format: OutputFormat,
    scale: u8,
    
    // Byte swapping for RGB565
    swap_bytes: bool,
    
    // SOS位置,用于正确定位scan data
    sos_position: usize,
}

impl JpegDecoder {
    /// Create a new JPEG decoder
    pub fn new() -> Self {
        Self {
            width: 0,
            height: 0,
            num_components: 0,
            sampling: SamplingFactor::Yuv444,
            huff_dc: [None, None],
            huff_ac: [None, None],
            qtables: [None, None, None, None],
            qtable_ids: [0; 3],
            dc_values: [0; 3],
            restart_interval: 0,
            _output_format: OutputFormat::Rgb565,
            scale: 0,
            swap_bytes: false,
            sos_position: 0,
        }
    }

    /// Set byte swapping for RGB565 output
    pub fn set_swap_bytes(&mut self, swap: bool) {
        self.swap_bytes = swap;
    }

    /// Prepare decoder by parsing JPEG headers (requires full JPEG data in memory)
    /// For memory-constrained systems, use `prepare_with_callback` instead
    pub fn prepare(&mut self, data: &[u8]) -> Result<()> {
        let mut pos = 0;

        // Find SOI marker
        if data.len() < 2 {
            return Err(Error::Input);
        }

        let mut marker = u16::from_be_bytes([data[0], data[1]]);
        pos += 2;

        if marker != markers::SOI {
            return Err(Error::FormatError);
        }

        // Parse segments
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
                markers::SOF0 => {
                    self.parse_sof(segment)?;
                }
                markers::DHT => {
                    self.parse_dht(segment)?;
                }
                markers::DQT => {
                    self.parse_dqt(segment)?;
                }
                markers::DRI => {
                    self.parse_dri(segment)?;
                }
                markers::SOS => {
                    self.parse_sos(segment)?;
                    // 记录SOS位置,用于后续定位scan data
                    self.sos_position = pos;
                    return Ok(());
                }
                markers::EOI => {
                    return Err(Error::FormatError);
                }
                0xD8 => {
                    // 嵌入的SOI,可能是缩略图
                }
                _ if (marker & 0xFF) as u8 >= 0xC0 && (marker & 0xFF) as u8 <= 0xCF => {
                    // 不支持的SOF marker (如progressive等)
                    return Err(Error::UnsupportedStandard);
                }
                _ => {
                    // 跳过未知segment
                }
            }

            pos = seg_start + seg_len;
        }
    }

    /// Parse Start of Frame
    fn parse_sof(&mut self, data: &[u8]) -> Result<()> {
        if data.len() < 6 {
            return Err(Error::FormatError);
        }

        let precision = data[0];
        if precision != 8 {
            return Err(Error::UnsupportedFormat);
        }

        self.height = u16::from_be_bytes([data[1], data[2]]);
        self.width = u16::from_be_bytes([data[3], data[4]]);
        self.num_components = data[5];

        if self.num_components != 1 && self.num_components != 3 {
            return Err(Error::UnsupportedStandard);
        }

        // Parse component information
        let expected_len = 6 + self.num_components as usize * 3;
        if data.len() < expected_len {
            return Err(Error::FormatError);
        }

        for i in 0..self.num_components as usize {
            let comp_start = 6 + i * 3;
            let sampling_factor = data[comp_start + 1];
            let qtable_id = data[comp_start + 2];

            if i == 0 {
                // Y component - determines MCU size
                let h = sampling_factor >> 4;
                let v = sampling_factor & 0x0F;
                self.sampling = SamplingFactor::from_factor(h, v)
                    .ok_or(Error::UnsupportedFormat)?;
            } else {
                // Cb/Cr must be 1x1
                if sampling_factor != 0x11 {
                    return Err(Error::UnsupportedFormat);
                }
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

    /// Parse Define Huffman Table
    fn parse_dht(&mut self, mut data: &[u8]) -> Result<()> {
        while !data.is_empty() {
            if data.len() < 17 {
                return Err(Error::FormatError);
            }

            let table_info = data[0];
            let class = (table_info >> 4) & 0x01; // 0=DC, 1=AC
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

            let mut table = HuffmanTable::new();
            table.create(bits, values)?;

            if class == 0 {
                self.huff_dc[id as usize] = Some(table);
            } else {
                self.huff_ac[id as usize] = Some(table);
            }

            data = &data[17 + num_codes..];
        }

        Ok(())
    }

    /// Parse Define Quantization Table
    fn parse_dqt(&mut self, mut data: &[u8]) -> Result<()> {
        use crate::tables::{ZIGZAG, ARAI_SCALE_FACTOR};
        
        while !data.is_empty() {
            if data.is_empty() {
                break;
            }

            let table_info = data[0];
            let precision = (table_info >> 4) & 0x0F;
            let id = table_info & 0x0F;

            if id > 3 {
                return Err(Error::FormatError);
            }

            let qtable = if precision == 0 {
                // 8-bit precision
                if data.len() < 65 {
                    return Err(Error::FormatError);
                }
                let mut table = Box::new([0i32; 64]);
                // Load in zigzag order, convert to raster order, apply Arai scale factor
                for i in 0..64 {
                    let zi = ZIGZAG[i] as usize;  // Raster order index
                    let q_value = data[1 + i] as u32;  // Quantization value in zigzag order
                    let ipsf = ARAI_SCALE_FACTOR[zi] as u32;  // Arai scale factor
                    table[zi] = (q_value * ipsf) as i32;  // Apply Arai scale factor
                }
                data = &data[65..];
                table
            } else {
                // 16-bit precision
                if data.len() < 129 {
                    return Err(Error::FormatError);
                }
                let mut table = Box::new([0i32; 64]);
                // Load in zigzag order, convert to raster order, apply Arai scale factor
                for i in 0..64 {
                    let zi = ZIGZAG[i] as usize;  // Raster order index
                    let q_value = u16::from_be_bytes([data[1 + i * 2], data[2 + i * 2]]) as u32;
                    let ipsf = ARAI_SCALE_FACTOR[zi] as u32;  // Arai scale factor
                    table[zi] = (q_value * ipsf) as i32;  // Apply Arai scale factor
                }
                data = &data[129..];
                table
            };

            self.qtables[id as usize] = Some(qtable);
        }

        Ok(())
    }

    /// Parse Define Restart Interval
    fn parse_dri(&mut self, data: &[u8]) -> Result<()> {
        if data.len() < 2 {
            return Err(Error::FormatError);
        }

        self.restart_interval = u16::from_be_bytes([data[0], data[1]]);
        Ok(())
    }

    /// Parse Start of Scan
    fn parse_sos(&mut self, data: &[u8]) -> Result<()> {
        if data.len() < 1 {
            return Err(Error::FormatError);
        }

        let num_components = data[0];
        if num_components != self.num_components {
            return Err(Error::FormatError);
        }

        // Validate that required tables are loaded
        for i in 0..self.num_components as usize {
            let table_id = if i == 0 { 0 } else { 1 };
            
            if self.huff_dc[table_id].is_none() || self.huff_ac[table_id].is_none() {
                return Err(Error::FormatError);
            }

            if self.qtables[self.qtable_ids[i] as usize].is_none() {
                return Err(Error::FormatError);
            }
        }

        Ok(())
    }

    /// Decompress JPEG image with automatic buffer allocation
    /// 
    /// ⚠️ **WARNING**: This method allocates large buffers internally and may cause 
    /// stack overflow on memory-constrained systems like ESP32!
    /// 
    /// This method is only available when the `alloc-buffers` feature is enabled.
    /// For embedded systems, use `decompress_with_buffers()` instead to have explicit
    /// control over memory allocation.
    /// 
    /// # Enable this feature
    /// ```toml
    /// [dependencies]
    /// tjpgd = { version = "0.3", features = ["alloc-buffers"] }
    /// ```
    /// 
    /// # Arguments
    /// * `data` - Complete JPEG data
    /// * `scale` - Scale factor (0-3)
    /// * `callback` - Output callback function
    #[cfg(feature = "alloc-buffers")]
    pub fn decompress(
        &mut self,
        data: &[u8],
        scale: u8,
        callback: OutputCallback,
    ) -> Result<()> {
        if scale > 3 {
            return Err(Error::Parameter);
        }

        self.scale = scale;
        self.dc_values = [0; 3];

        let mcu_width = self.sampling.mcu_width() as usize;
        let mcu_height = self.sampling.mcu_height() as usize;
        let mcu_pixel_width = mcu_width * 8;
        let mcu_pixel_height = mcu_height * 8;

        // Find SOS marker and get scan data
        let scan_data = self.find_scan_data(data)?;
        let mut bitstream = BitStream::new(scan_data);

        let mut restart_counter = 0u16;
        let mut restart_marker = 0u8;

        // ⚠️ Allocate MCU buffers internally - THIS CAN CAUSE STACK OVERFLOW ON ESP32!
        let mcu_blocks = (mcu_width * mcu_height + 2) * 64;
        let mut mcu_buffer = vec![0i16; mcu_blocks];
        let mut work_buffer = vec![0u8; mcu_pixel_width * mcu_pixel_height * 3];

        self.decompress_internal(
            &mut bitstream,
            &mut mcu_buffer,
            &mut work_buffer,
            mcu_width,
            mcu_height,
            &mut restart_counter,
            &mut restart_marker,
            callback
        )
    }

    /// Decompress JPEG image with external buffers (memory-efficient version for embedded systems)
    /// 
    /// # Arguments
    /// * `data` - Complete JPEG data
    /// * `scale` - Scale factor (0-3)
    /// * `mcu_buffer` - Working buffer for MCU data (must be at least `mcu_buffer_size()` bytes)
    /// * `work_buffer` - Working buffer for RGB conversion (must be at least `work_buffer_size()` bytes)
    /// * `callback` - Output callback function
    /// 
    /// # Example
    /// ```ignore
    /// let mut decoder = JpegDecoder::new();
    /// decoder.prepare(&jpeg_data)?;
    /// 
    /// let mcu_size = decoder.mcu_buffer_size();
    /// let work_size = decoder.work_buffer_size();
    /// 
    /// // Allocate buffers on heap or use static buffers
    /// let mut mcu_buf = vec![0i16; mcu_size];
    /// let mut work_buf = vec![0u8; work_size];
    /// 
    /// decoder.decompress_with_buffers(
    ///     &jpeg_data, 
    ///     0, 
    ///     &mut mcu_buf, 
    ///     &mut work_buf,
    ///     &mut |decoder, rgb, rect| { /* output */ Ok(true) }
    /// )?;
    /// ```
    pub fn decompress_with_buffers(
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

        // Validate buffer sizes
        let required_mcu_size = self.mcu_buffer_size();
        let required_work_size = self.work_buffer_size();

        if mcu_buffer.len() < required_mcu_size {
            return Err(Error::InsufficientMemory);
        }
        if work_buffer.len() < required_work_size {
            return Err(Error::InsufficientMemory);
        }

        self.scale = scale;
        self.dc_values = [0; 3];

        let mcu_width = self.sampling.mcu_width() as usize;
        let mcu_height = self.sampling.mcu_height() as usize;

        // Find SOS marker and get scan data
        let scan_data = self.find_scan_data(data)?;
        let mut bitstream = BitStream::new(scan_data);

        let mut restart_counter = 0u16;
        let mut restart_marker = 0u8;

        self.decompress_internal(
            &mut bitstream,
            mcu_buffer,
            work_buffer,
            mcu_width,
            mcu_height,
            &mut restart_counter,
            &mut restart_marker,
            callback
        )
    }

    /// Internal decompression logic shared by both decompress methods
    fn decompress_internal(
        &mut self,
        bitstream: &mut BitStream,
        mcu_buffer: &mut [i16],
        work_buffer: &mut [u8],
        mcu_width: usize,
        mcu_height: usize,
        restart_counter: &mut u16,
        restart_marker: &mut u8,
        callback: OutputCallback,
    ) -> Result<()> {
        let mcu_pixel_width = mcu_width * 8;
        let mcu_pixel_height = mcu_height * 8;

        for mcu_y in (0..self.height).step_by(mcu_pixel_height) {
            for mcu_x in (0..self.width).step_by(mcu_pixel_width) {
                // Handle restart interval (counter-based restart)
                if self.restart_interval > 0 && *restart_counter >= self.restart_interval {
                    self.process_restart(bitstream, *restart_marker)?;
                    *restart_counter = 0;
                    *restart_marker = (*restart_marker + 1) & 0x07;
                }

                // Decode MCU
                self.decode_mcu(bitstream, mcu_buffer, mcu_width, mcu_height)?;

                // 检查bitstream是否在MCU解码过程中检测到marker (RST)
                if let Some(marker) = bitstream.get_marker() {
                    if marker >= 0xD0 && marker <= 0xD7 {
                        // RST marker,重置bitstream和DC预测器
                        bitstream.reset_for_restart();
                        self.dc_values = [0; 3];
                        *restart_marker = ((marker - 0xD0) + 1) & 0x07;
                    }
                }

                // Convert and output MCU
                self.output_mcu(
                    mcu_buffer,
                    work_buffer,
                    mcu_x,
                    mcu_y,
                    mcu_width,
                    mcu_height,
                    callback,
                )?;

                *restart_counter += 1;
            }
        }

        Ok(())
    }

    /// Calculate required MCU buffer size (in i16 elements)
    pub fn mcu_buffer_size(&self) -> usize {
        let mcu_width = self.sampling.mcu_width() as usize;
        let mcu_height = self.sampling.mcu_height() as usize;
        (mcu_width * mcu_height + 2) * 64
    }

    /// Calculate required work buffer size (in bytes)
    pub fn work_buffer_size(&self) -> usize {
        let mcu_width = self.sampling.mcu_width() as usize;
        let mcu_height = self.sampling.mcu_height() as usize;
        mcu_width * 8 * mcu_height * 8 * 3
    }

    fn find_scan_data<'a>(&self, data: &'a [u8]) -> Result<&'a [u8]> {
        // 使用prepare中保存的SOS位置
        let i = self.sos_position;
        
        if i + 4 > data.len() {
            return Err(Error::Input);
        }
        
        // 验证这确实是SOS marker
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

    fn process_restart(&mut self, bitstream: &mut BitStream, _marker: u8) -> Result<()> {
        bitstream.reset_for_restart();
        self.dc_values = [0; 3];
        Ok(())
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
            
            self.decode_and_dequantize_block_with_id(bitstream, &mut tmp, qtable_id, 0)?;
            block_idct(&mut tmp, block);
        }

        if self.num_components == 3 {
            // 解码Cb block
            let cb_offset = num_y_blocks * 64;
            let cb_slice = &mut buffer[cb_offset..cb_offset + 64];
            let cb_block: &mut [i16; 64] = cb_slice.try_into().map_err(|_| Error::FormatError)?;
            let qtable_id = self.qtable_ids[1];
            
            self.decode_and_dequantize_block_with_id(bitstream, &mut tmp, qtable_id, 1)?;
            block_idct(&mut tmp, cb_block);

            // 解码Cr block
            let cr_offset = cb_offset + 64;
            let cr_slice = &mut buffer[cr_offset..cr_offset + 64];
            let cr_block: &mut [i16; 64] = cr_slice.try_into().map_err(|_| Error::FormatError)?;
            let qtable_id = self.qtable_ids[2];
            
            self.decode_and_dequantize_block_with_id(bitstream, &mut tmp, qtable_id, 2)?;
            block_idct(&mut tmp, cr_block);
        }

        Ok(())
    }

    /// 解码并反量化block
    fn decode_and_dequantize_block_with_id(
        &mut self,
        bitstream: &mut BitStream,
        tmp: &mut [i32; 64],
        qtable_id: u8,
        component: usize,
    ) -> Result<()> {
        use crate::tables::ZIGZAG;
        
        let qtable = self.qtables[qtable_id as usize].as_ref().ok_or(Error::FormatError)?;
        let table_id = if component == 0 { 0 } else { 1 };

        // 解码DC系数
        let dc_table = self.huff_dc[table_id].as_ref().ok_or(Error::FormatError)?;
        let dc_len = dc_table.decode(bitstream)? as usize;
        
        let dc_diff = if dc_len > 0 {
            let bits = bitstream.read_bits(dc_len)?;
            Self::extend(bits, dc_len) as i32
        } else {
            0
        };

        self.dc_values[component] = self.dc_values[component].wrapping_add(dc_diff as i16);
        let dc = self.dc_values[component] as i32;
        
        // 反量化DC (应用scale factor并descale 8位)
        tmp[0] = (dc * qtable[0]) >> 8;

        // 初始化所有AC元素为0
        tmp[1..].fill(0);

        // 解码AC系数
        let ac_table = self.huff_ac[table_id].as_ref().ok_or(Error::FormatError)?;
        let mut z = 1;

        loop {
            let symbol = ac_table.decode(bitstream)?;
            
            if symbol == 0 {
                // EOB - 剩余系数为零
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
                
                // 将zigzag索引转换为光栅索引
                let i = ZIGZAG[z] as usize;
                
                // 反量化并存储
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

        // Calculate actual output size (may be clipped at image edges)
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

        // Convert YCbCr to RGB or grayscale
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

        // Squeeze pixel table if MCU is at right/bottom edge (like C code)
        // This removes truncated pixels so the output buffer has correct stride
        let rx = scaled_width as usize;
        let ry = scaled_height as usize;
        let mx = (mcu_pixel_width >> self.scale) as usize;
        
        if rx < mx {
            // MCU spans right edge, need to squeeze
            let mut s = 0usize; // source index
            let mut d = 0usize; // destination index
            for _y in 0..ry {
                // Copy rx pixels (effective pixels)
                for _x in 0..rx {
                    work_buffer[d] = work_buffer[s];
                    work_buffer[d + 1] = work_buffer[s + 1];
                    work_buffer[d + 2] = work_buffer[s + 2];
                    s += 3;
                    d += 3;
                }
                // Skip truncated pixels at end of row
                s += (mx - rx) * 3;
            }
        }

        let continue_processing = callback(self, work_buffer, &rect)?;
        
        if !continue_processing {
            return Err(Error::Interrupted);
        }

        Ok(())
    }

    /// Get image width
    pub fn width(&self) -> u16 {
        self.width >> self.scale
    }

    /// Get image height
    pub fn height(&self) -> u16 {
        self.height >> self.scale
    }

    /// Get number of components
    pub fn components(&self) -> u8 {
        self.num_components
    }
}

impl Default for JpegDecoder {
    fn default() -> Self {
        Self::new()
    }
}
