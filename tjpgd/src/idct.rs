//! Inverse Discrete Cosine Transform (IDCT) implementation
//! 
//! Uses the Arai, Agui, and Nakajima algorithm for fast IDCT.
//! This implementation matches the original C code exactly.


// Arai algorithm rotation constants (scaled by 4096 for fixed-point math)
const M13: i32 = (1.41421 * 4096.0) as i32;  // sqrt(2) * 4096
const M2: i32 = (1.08239 * 4096.0) as i32;   // 1.08239 * 4096
const M4: i32 = (2.61313 * 4096.0) as i32;   // 2.61313 * 4096
const M5: i32 = (1.84776 * 4096.0) as i32;   // 1.84776 * 4096

/// Perform 8x8 IDCT on a block using Arai algorithm
/// Input: src - de-quantized and pre-scaled block data (already in raster order)
/// Output: dst - transformed block as byte array (0-255)
pub fn block_idct(src: &mut [i32; 64], dst: &mut [i16; 64]) {
    // Process columns
    for i in 0..8 {
        let base = i;
        
        // Get even elements
        let v0 = src[base + 8 * 0];
        let v1 = src[base + 8 * 2];
        let v2 = src[base + 8 * 4];
        let v3 = src[base + 8 * 6];

        // Process the even elements
        let t10 = v0 + v2;
        let t12 = v0 - v2;
        let mut t11 = ((v1 - v3) * M13) >> 12;
        let mut v3 = v3 + v1;
        t11 -= v3;
        let v0 = t10 + v3;
        v3 = t10 - v3;
        let v1 = t11 + t12;
        let v2 = t12 - t11;

        // Get odd elements
        let v4_odd = src[base + 8 * 7];
        let v5_odd = src[base + 8 * 1];
        let v6_odd = src[base + 8 * 5];
        let v7_odd = src[base + 8 * 3];

        // Process the odd elements
        let t10 = v5_odd - v4_odd;
        let t11 = v5_odd + v4_odd;
        let t12 = v6_odd - v7_odd;
        let mut v7 = v7_odd + v6_odd;
        let mut v5 = ((t11 - v7) * M13) >> 12;
        v7 += t11;
        let t13 = ((t10 + t12) * M5) >> 12;
        let mut v4 = t13 - ((t10 * M2) >> 12);
        let v6 = t13 - ((t12 * M4) >> 12) - v7;
        v5 -= v6;
        v4 -= v5;

        // Write-back transformed values
        src[base + 8 * 0] = v0 + v7;
        src[base + 8 * 7] = v0 - v7;
        src[base + 8 * 1] = v1 + v6;
        src[base + 8 * 6] = v1 - v6;
        src[base + 8 * 2] = v2 + v5;
        src[base + 8 * 5] = v2 - v5;
        src[base + 8 * 3] = v3 + v4;
        src[base + 8 * 4] = v3 - v4;
    }

    // Process rows
    for i in 0..8 {
        let base = i * 8;
        
        // Get even elements (add DC offset removal for row 0)
        let v0 = src[base + 0] + (128_i32 << 8);
        let v1 = src[base + 2];
        let v2 = src[base + 4];
        let v3 = src[base + 6];

        // Process the even elements
        let t10 = v0 + v2;
        let t12 = v0 - v2;
        let mut t11 = ((v1 - v3) * M13) >> 12;
        let mut v3 = v3 + v1;
        t11 -= v3;
        let v0 = t10 + v3;
        v3 = t10 - v3;
        let v1 = t11 + t12;
        let v2 = t12 - t11;

        // Get odd elements
        let v4_odd = src[base + 7];
        let v5_odd = src[base + 1];
        let v6_odd = src[base + 5];
        let v7_odd = src[base + 3];

        // Process the odd elements
        let t10 = v5_odd - v4_odd;
        let t11 = v5_odd + v4_odd;
        let t12 = v6_odd - v7_odd;
        let mut v7 = v7_odd + v6_odd;
        let mut v5 = ((t11 - v7) * M13) >> 12;
        v7 += t11;
        let t13 = ((t10 + t12) * M5) >> 12;
        let mut v4 = t13 - ((t10 * M2) >> 12);
        let v6 = t13 - ((t12 * M4) >> 12) - v7;
        v5 -= v6;
        v4 -= v5;

        // Descale the transformed values 8 bits and output
        dst[base + 0] = ((v0 + v7) >> 8) as i16;
        dst[base + 7] = ((v0 - v7) >> 8) as i16;
        dst[base + 1] = ((v1 + v6) >> 8) as i16;
        dst[base + 6] = ((v1 - v6) >> 8) as i16;
        dst[base + 2] = ((v2 + v5) >> 8) as i16;
        dst[base + 5] = ((v2 - v5) >> 8) as i16;
        dst[base + 3] = ((v3 + v4) >> 8) as i16;
        dst[base + 4] = ((v3 - v4) >> 8) as i16;
    }
}

/// YCbCr to RGB color space conversion
pub mod color {
    use crate::tables::{byte_clip, CB_TO_B, CB_TO_G, CR_TO_G, CR_TO_R, CVACC};

    /// Convert YCbCr to RGB888
    #[inline]
    pub fn ycbcr_to_rgb(y: i32, cb: i32, cr: i32) -> [u8; 3] {
        let r = y + (CR_TO_R * cr) / CVACC;
        let g = y - (CB_TO_G * cb + CR_TO_G * cr) / CVACC;
        let b = y + (CB_TO_B * cb) / CVACC;

        [byte_clip(r), byte_clip(g), byte_clip(b)]
    }

    /// Convert RGB888 to RGB565
    #[inline]
    #[allow(dead_code)]
    pub fn rgb888_to_rgb565(r: u8, g: u8, b: u8) -> u16 {
        let r5 = (r & 0xF8) as u16;
        let g6 = (g & 0xFC) as u16;
        let b5 = (b & 0xF8) as u16;
        
        (r5 << 8) | (g6 << 3) | (b5 >> 3)
    }

    /// Convert RGB565 to swapped byte order (for displays)
    #[inline]
    #[allow(dead_code)]
    pub fn swap_rgb565(color: u16) -> u16 {
        (color << 8) | (color >> 8)
    }

    /// Process MCU block for RGB output
    pub fn mcu_to_rgb(
        y_block: &[i16],
        cb_block: &[i16],
        cr_block: &[i16],
        output: &mut [u8],
        mcu_width: usize,
        mcu_height: usize,
        sampling_h: usize,
        sampling_v: usize,
    ) {
        let mut out_idx = 0;

        for block_y in 0..mcu_height {
            for y in 0..8 {
                let abs_y = block_y * 8 + y;
                
                for block_x in 0..mcu_width {
                    for x in 0..8 {
                        let abs_x = block_x * 8 + x;
                        
                        // Get Y component
                        let y_idx = (block_y * mcu_width + block_x) * 64 + y * 8 + x;
                        let yy = y_block[y_idx] as i32;

                        // Get Cb/Cr components (subsampled)
                        let cb_x = abs_x / sampling_h;
                        let cb_y = abs_y / sampling_v;
                        let cb_idx = cb_y * 8 + cb_x;
                        
                        let cb = cb_block[cb_idx] as i32 - 128;
                        let cr = cr_block[cb_idx] as i32 - 128;

                        // Convert to RGB
                        let rgb = ycbcr_to_rgb(yy, cb, cr);
                        
                        output[out_idx] = rgb[0];
                        output[out_idx + 1] = rgb[1];
                        output[out_idx + 2] = rgb[2];
                        out_idx += 3;
                    }
                }
            }
        }
    }

    /// Process MCU block for grayscale output
    pub fn mcu_to_grayscale(
        y_block: &[i16],
        output: &mut [u8],
        mcu_width: usize,
        mcu_height: usize,
    ) {
        let mut out_idx = 0;

        for block_y in 0..mcu_height {
            for y in 0..8 {
                for block_x in 0..mcu_width {
                    for x in 0..8 {
                        let y_idx = (block_y * mcu_width + block_x) * 64 + y * 8 + x;
                        output[out_idx] = byte_clip(y_block[y_idx] as i32);
                        out_idx += 1;
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_idct_dc_only() {
        // Create test data with DC only
        // In C code: tmp[0] = d * dqf[0] >> 8
        // For a DC value of 0, and quantization table[0] = 8192 (1.0 * 8192),
        // tmp[0] would be (0 * 8192) >> 8 = 0
        let mut src = [0i32; 64];
        src[0] = 0; // DC component = 0 after dequantization
        
        let mut dst = [0i16; 64];
        block_idct(&mut src, &mut dst);

        // After IDCT with DC=0, all values should be around 128 (the DC offset added in row processing)
        // Row processing adds (128 << 8) to v0
        for &val in &dst {
            assert!((val - 128).abs() < 5, "Expected ~128, got {}", val);
        }
    }

    #[test]
    fn test_color_conversion() {
        use color::*;
        
        // Test white (Y=255, Cb=0, Cr=0)
        let rgb = ycbcr_to_rgb(255, 0, 0);
        assert_eq!(rgb, [255, 255, 255]);

        // Test RGB565 conversion
        let rgb565 = rgb888_to_rgb565(255, 255, 255);
        assert_eq!(rgb565, 0xFFFF);
    }
}
