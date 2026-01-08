//! tjpg-decoder - Tiny JPEG Decompressor
//! 
//! A modern Rust implementation of TJpg_Decoder library.
//! This is a lightweight JPEG decoder optimized for embedded systems.
//! 
//! Based on: TJpg_Decoder (https://github.com/Bodmer/TJpg_Decoder)
//! Original: TJpgDec R0.03 (C)ChaN, 2021
//! Rust implementation: 2026

#![cfg_attr(not(feature = "std"), no_std)]

mod types;
mod tables;
mod huffman;
mod idct;
mod decoder;

pub use types::{Result, Error, OutputFormat, Rectangle};
pub use decoder::JpegDecoder;

/// Size of stream input buffer
pub const BUFFER_SIZE: usize = 512;

/// Minimum workspace size required (depends on optimization level)
#[cfg(feature = "fast-decode")]
pub const MIN_WORKSPACE_SIZE: usize = 9644;

#[cfg(not(feature = "fast-decode"))]
pub const MIN_WORKSPACE_SIZE: usize = 3500;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic() {
        // Basic sanity test
        assert_eq!(BUFFER_SIZE, 512);
    }
}
