//! Constant tables for JPEG decompression

/// Zigzag-order to raster-order conversion table
pub const ZIGZAG: [u8; 64] = [
    0, 1, 8, 16, 9, 2, 3, 10,
    17, 24, 32, 25, 18, 11, 4, 5,
    12, 19, 26, 33, 40, 48, 41, 34,
    27, 20, 13, 6, 7, 14, 21, 28,
    35, 42, 49, 56, 57, 50, 43, 36,
    29, 22, 15, 23, 30, 37, 44, 51,
    58, 59, 52, 45, 38, 31, 39, 46,
    53, 60, 61, 54, 47, 55, 62, 63,
];

/// Input scale factor of Arai algorithm
/// (scaled up 16 bits for fixed point operations)
pub const ARAI_SCALE_FACTOR: [u16; 64] = [
    8192, 11363, 10703, 9633, 8192, 6436, 4433, 2260,
    11363, 15746, 14852, 13363, 11363, 8930, 6149, 3135,
    10703, 14852, 13983, 12583, 10703, 8410, 5793, 2953,
    9633, 13363, 12583, 11327, 9633, 7568, 5212, 2657,
    8192, 11363, 10703, 9633, 8192, 6436, 4433, 2260,
    6436, 8930, 8410, 7568, 6436, 5057, 3484, 1776,
    4433, 6149, 5793, 5212, 4433, 3484, 2400, 1224,
    2260, 3135, 2953, 2657, 2260, 1776, 1224, 623,
];

/// Clipping table for fast saturation
#[cfg(feature = "table-clip")]
pub const CLIP_TABLE: [u8; 1024] = {
    let mut table = [0u8; 1024];
    let mut i = 0;
    
    // 0..255
    while i < 256 {
        table[i] = i as u8;
        i += 1;
    }
    
    // 256..511 (all 255)
    while i < 512 {
        table[i] = 255;
        i += 1;
    }
    
    // 512..767 (all 0)
    while i < 768 {
        table[i] = 0;
        i += 1;
    }
    
    // 768..1023
    while i < 1024 {
        table[i] = (i - 768) as u8;
        i += 1;
    }
    
    table
};

/// Fast clipping using table lookup
#[cfg(feature = "table-clip")]
#[inline]
pub fn byte_clip(val: i32) -> u8 {
    CLIP_TABLE[(val as usize) & 0x3FF]
}

/// Clipping without table
#[cfg(not(feature = "table-clip"))]
#[inline]
pub fn byte_clip(val: i32) -> u8 {
    if val < 0 {
        0
    } else if val > 255 {
        255
    } else {
        val as u8
    }
}

/// YCbCr to RGB conversion constants (fixed point with CVACC scaling)
pub const CVACC: i32 = 1024;

/// Conversion factor for Cr to R
pub const CR_TO_R: i32 = (1.402 * CVACC as f64) as i32;

/// Conversion factor for Cb to G
pub const CB_TO_G: i32 = (0.344 * CVACC as f64) as i32;

/// Conversion factor for Cr to G
pub const CR_TO_G: i32 = (0.714 * CVACC as f64) as i32;

/// Conversion factor for Cb to B
pub const CB_TO_B: i32 = (1.772 * CVACC as f64) as i32;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_zigzag_table() {
        assert_eq!(ZIGZAG[0], 0);
        assert_eq!(ZIGZAG[63], 63);
        assert_eq!(ZIGZAG.len(), 64);
    }

    #[test]
    fn test_byte_clip() {
        assert_eq!(byte_clip(-10), 0);
        assert_eq!(byte_clip(0), 0);
        assert_eq!(byte_clip(128), 128);
        assert_eq!(byte_clip(255), 255);
        assert_eq!(byte_clip(300), 255);
    }
}
