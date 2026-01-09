//! tjpg-decoder - Tiny JPEG Decompressor
//! 
//! Rust implementation of TJpgDec library, fully compatible with C version.
//! This is a lightweight JPEG decoder optimized for embedded systems.
//! 
//! Based on: TJpgDec R0.03 (C)ChaN, 2021
//! 
//! Key features:
//! - Memory pool based allocation (same as C version)
//! - Small decoder struct (~120 bytes, same as C JDEC)
//! - All internal data allocated from user-provided pool
//! - No heap allocation (Box/Vec) in core decoding

#![cfg_attr(not(feature = "std"), no_std)]

mod types;
mod tables;
mod huffman;
mod idct;
mod decoder;
mod pool;

pub use types::{Result, Error, OutputFormat, Rectangle};
pub use decoder::{JpegDecoder, OutputCallback, calculate_pool_size};
pub use huffman::{HuffmanTable, BitStream};
pub use pool::{MemoryPool, RECOMMENDED_POOL_SIZE, MINIMUM_POOL_SIZE};

/// Size of stream input buffer
pub const BUFFER_SIZE: usize = 512;

/// Minimum workspace size required (depends on JD_FASTDECODE level)
/// - Level 0: 3100 bytes (basic, for 8/16-bit MCUs)
/// - Level 1: 3500 bytes (32-bit barrel shifter)
/// - Level 2: 9644 bytes (+ huffman LUT)
#[cfg(feature = "fast-decode-2")]
pub const MIN_WORKSPACE_SIZE: usize = 9644;

#[cfg(all(feature = "fast-decode-1", not(feature = "fast-decode-2")))]
pub const MIN_WORKSPACE_SIZE: usize = 3500;

#[cfg(all(feature = "fast-decode-0", not(feature = "fast-decode-1"), not(feature = "fast-decode-2")))]
pub const MIN_WORKSPACE_SIZE: usize = 3100;

#[cfg(not(any(feature = "fast-decode-0", feature = "fast-decode-1", feature = "fast-decode-2")))]
pub const MIN_WORKSPACE_SIZE: usize = 3500; // Default to level 1

/// Re-export fastdecode level query function
pub use huffman::fastdecode_level;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic() {
        // Basic sanity test
        assert_eq!(BUFFER_SIZE, 512);
    }
}
