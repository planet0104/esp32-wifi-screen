# TJpgDec-rs - Tiny JPEG Decoder

A modern Rust implementation of ChaN's TJpgDec library - a lightweight JPEG decoder designed for embedded systems.

[English Version](README.en.md) | [中文文档](README.md)

## Features

- **Lightweight**: Optimized for memory-constrained embedded systems
- **High Performance**: Multiple optimization levels available
- **Flexible**: Support for various output formats (RGB888, RGB565, Grayscale)
- **no_std Compatible**: Can run without the standard library
- **Safe Rust**: Modern Rust implementation with safety guarantees

## Supported Features

- Baseline JPEG (SOF0)
- Grayscale and YCbCr color spaces
- Sampling factors: 4:4:4, 4:2:0, 4:2:2
- Output scaling (1/1, 1/2, 1/4, 1/8)
- RGB888, RGB565, and Grayscale output formats

## Usage

### Basic Usage (Recommended for Embedded Systems)

```rust
use tjpg_decoder::{JpegDecoder, Result};

fn decode_jpeg(jpeg_data: &[u8]) -> Result<()> {
    let mut decoder = JpegDecoder::new();
    
    // Prepare decoder
    decoder.prepare(jpeg_data)?;
    
    // Calculate required buffer sizes
    let mcu_size = decoder.mcu_buffer_size();
    let work_size = decoder.work_buffer_size();
    
    // Allocate buffers (can be in static memory to save stack space)
    let mut mcu_buffer = vec![0i16; mcu_size];
    let mut work_buffer = vec![0u8; work_size];
    
    // Decompress using external buffers (memory-efficient)
    decoder.decompress_with_buffers(
        jpeg_data, 0, 
        &mut mcu_buffer, 
        &mut work_buffer,
        &mut |_decoder, bitmap, rect| {
            // Process decoded rectangular region
            println!("Decoded block: {}x{} at ({}, {})", 
                     rect.width(), rect.height(), rect.left, rect.top);
            Ok(true)
        }
    )?;
    
    Ok(())
}
```

### Auto-Allocated Buffers (Requires `alloc-buffers` feature)

```rust
// Available only with alloc-buffers feature enabled
use tjpg_decoder::{JpegDecoder, Result};

fn decode_jpeg_auto(jpeg_data: &[u8]) -> Result<()> {
    let mut decoder = JpegDecoder::new();
    decoder.prepare(jpeg_data)?;
    
    // Automatically allocates internal buffers (requires more stack space)
    decoder.decompress(jpeg_data, 0, &mut |_decoder, bitmap, rect| {
        println!("Decoded block: {}x{} at ({}, {})", 
                 rect.width(), rect.height(), rect.left, rect.top);
        Ok(true)
    })?;
    
    Ok(())
}
```

## Installation

Add to your `Cargo.toml`:

```toml
[dependencies]
tjpg-decoder = { path = "path/to/tjpg-decoder", features = ["fast-decode"] }
```

### Feature Flags

- `std` (default) - Enable standard library support
- `fast-decode` - Enable fast Huffman decoding (requires ~6KB extra RAM)
- `table-clip` - Use lookup table for value clipping (adds ~1KB code)
- `use-scale` - Enable output scaling support
- `alloc-buffers` - Enable auto-buffer allocation `decompress()` method (disabled by default, requires more stack space)

### Configuration for Different Platforms

**8/16-bit MCUs (Minimal Memory):**
```toml
[dependencies.tjpg-decoder]
path = "path/to/tjpg-decoder"
default-features = false
```

**32-bit MCUs (e.g., ESP32):**
```toml
[dependencies.tjpg-decoder]
path = "path/to/tjpg-decoder"
features = ["fast-decode", "table-clip"]
```

## Memory Requirements

### Using `decompress_with_buffers()` (Recommended)
- MCU buffer: 192-384 i16 elements (384-768 bytes)
  - 4:4:4 sampling: 192 elements
  - 4:2:0/4:2:2 sampling: 384 elements
- Work buffer:
  - Basic mode: 192-768 bytes
  - Fast decode mode (`fast-decode`): ~6KB
- **Total**: ~1-7KB (depends on configuration and sampling format)

### Using `decompress()` (Requires `alloc-buffers` feature)
- Automatically allocates all buffers on stack
- ESP32 default stack (3-4KB) may be insufficient, need to increase stack size
- **Not recommended for embedded systems**

## API Documentation

### JpegDecoder

Main decoder struct.

```rust
// Create new decoder
let mut decoder = JpegDecoder::new();

// Set byte swapping (for some displays)
decoder.set_swap_bytes(true);

// Prepare decoder (parse JPEG header)
decoder.prepare(jpeg_data)?;

// Get image information
let width = decoder.width();
let height = decoder.height();
let components = decoder.components();

// Calculate required buffer sizes
let mcu_size = decoder.mcu_buffer_size();
let work_size = decoder.work_buffer_size();

// Allocate buffers
let mut mcu_buffer = vec![0i16; mcu_size];
let mut work_buffer = vec![0u8; work_size];

// Decompress image (using external buffers)
decoder.decompress_with_buffers(
    jpeg_data,
    scale,  // 0=1/1, 1=1/2, 2=1/4, 3=1/8
    &mut mcu_buffer,
    &mut work_buffer,
    &mut |decoder, bitmap, rect| {
        // Process bitmap data
        Ok(true)  // Return true to continue, false to interrupt
    }
)?;
```

### Error Handling

All operations return `Result<T, Error>`:

- `Error::Ok` - Success
- `Error::Interrupted` - Interrupted by output function
- `Error::Input` - Input stream error
- `Error::InsufficientMemory` - Insufficient memory
- `Error::InsufficientBuffer` - Insufficient buffer
- `Error::Parameter` - Parameter error
- `Error::FormatError` - Format error
- `Error::UnsupportedFormat` - Unsupported format
- `Error::UnsupportedStandard` - Unsupported JPEG standard

## Examples

### Example 1: Decode to Memory Buffer

```rust
use tjpg_decoder::{JpegDecoder, Result};

fn decode_to_buffer(jpeg_data: &[u8]) -> Result<Vec<u8>> {
    let mut decoder = JpegDecoder::new();
    decoder.prepare(jpeg_data)?;
    
    let width = decoder.width() as usize;
    let height = decoder.height() as usize;
    let mut output = vec![0u8; width * height * 3]; // RGB888
    
    // Allocate decoding buffers
    let mcu_size = decoder.mcu_buffer_size();
    let work_size = decoder.work_buffer_size();
    let mut mcu_buffer = vec![0i16; mcu_size];
    let mut work_buffer = vec![0u8; work_size];
    
    decoder.decompress_with_buffers(
        jpeg_data, 0, 
        &mut mcu_buffer, 
        &mut work_buffer,
        &mut |_, bitmap, rect| {
            let x = rect.left as usize;
            let y = rect.top as usize;
            let w = rect.width() as usize;
            
            // Copy to output buffer
            for dy in 0..rect.height() as usize {
                let src_offset = dy * w * 3;
                let dst_offset = ((y + dy) * width + x) * 3;
                output[dst_offset..dst_offset + w * 3]
                    .copy_from_slice(&bitmap[src_offset..src_offset + w * 3]);
            }
            
            Ok(true)
        }
    )?;
    
    Ok(output)
}
```

### Example 2: Display on ESP32 (Memory Optimized)

```rust
use tjpg_decoder::{JpegDecoder, Result};

fn display_jpeg(jpeg_data: &[u8], display: &mut Display) -> Result<()> {
    let mut decoder = JpegDecoder::new();
    decoder.set_swap_bytes(true); // If display requires it
    
    decoder.prepare(jpeg_data)?;
    
    // Memory-optimized buffer allocation for ESP32
    let mcu_size = decoder.mcu_buffer_size();    // Usually 192-384 i16
    let work_size = decoder.work_buffer_size();  // Basic mode ~200 bytes
    
    let mut mcu_buffer = vec![0i16; mcu_size];
    let mut work_buffer = vec![0u8; work_size];
    
    decoder.decompress_with_buffers(
        jpeg_data, 0,
        &mut mcu_buffer,
        &mut work_buffer,
        &mut |_, bitmap, rect| {
            // Write RGB data to display
            display.draw_image(
                rect.left,
                rect.top,
                rect.width(),
                rect.height(),
                bitmap,
            )?;
            
            Ok(true)
        }
    )?;
    
    Ok(())
}
```

## Performance Comparison

| Platform | Optimization | Decode Time | Memory Usage |
|----------|-------------|-------------|--------------|
| ESP32 | fast-decode | ~40% faster | 9.6KB |
| ESP32 | Basic mode | Baseline | 3.5KB |
| STM32F4 | fast-decode | ~35% faster | 9.6KB |

## C Version Correspondence

| C Function/Type | Rust Equivalent | Notes |
|----------------|-----------------|-------|
| `jd_prepare()` | `decoder.prepare()` | Parse JPEG header |
| `jd_decomp()` | `decoder.decompress_with_buffers()` | Decompress with external buffers |
| `jd_decomp()` (auto-alloc) | `decoder.decompress()` | Requires `alloc-buffers` feature |
| `JDEC` | `JpegDecoder` | Decoder object |
| `JRESULT` | `Result<T>` | Error type |
| `JRECT` | `Rectangle` | Rectangle region |

**Note**: The C version uses externally provided work buffers, corresponding to Rust's `decompress_with_buffers()` method.

## Development and Testing

```bash
# Check compilation
cargo check

# Run tests
cargo test

# Run tests with alloc-buffers feature
cargo test --features alloc-buffers

# Build release version
cargo build --release

# Build with all features
cargo build --all-features

# Build in no_std mode
cargo build --no-default-features

# Run examples
cargo run --example basic
cargo run --example jpg2bmp
cargo run --example test_suite
```

## Quick Start Guide

### Project Structure

```
tjpgd/
├── Cargo.toml          # Project configuration
├── README.md           # Project documentation
├── LICENSE             # License
├── CHANGELOG.md        # Changelog
├── DEVELOPMENT.md      # Development guide
├── src/
│   ├── lib.rs         # Library entry point
│   ├── types.rs       # Type definitions
│   ├── tables.rs      # Constant tables
│   ├── huffman.rs     # Huffman decoding
│   ├── idct.rs        # IDCT and color conversion
│   └── decoder.rs     # Main decoder
└── examples/
    ├── basic.rs       # Basic usage example
    ├── jpg2bmp.rs     # JPEG to BMP converter
    ├── test_info.rs   # Display image info
    └── test_suite.rs  # Comprehensive test suite
```

### Performance Optimization for Different Platforms

**8/16-bit MCUs (Minimal Memory):**
```toml
[dependencies.tjpgd]
path = "tjpgd"
default-features = false
```

**32-bit MCUs (e.g., ESP32):**
```toml
[dependencies.tjpgd]
path = "tjpgd"
features = ["fast-decode", "table-clip"]
```

**Desktop/Server (Performance Priority):**
```toml
[dependencies.tjpgd]
path = "tjpgd"
features = ["std", "fast-decode", "table-clip", "use-scale", "alloc-buffers"]
```

### Integration with Your Project

Example usage in your main project:

```rust
// In src/main.rs
mod tjpgd_wrapper {
    use tjpgd::{JpegDecoder, Rectangle, Result};
    
    pub fn decode_jpeg_to_rgb565(
        jpeg_data: &[u8],
        output: &mut [u16],
    ) -> Result<(u16, u16)> {
        let mut decoder = JpegDecoder::new();
        decoder.set_swap_bytes(true);
        
        decoder.prepare(jpeg_data)?;
        let (width, height) = (decoder.width(), decoder.height());
        
        // Allocate decoding buffers
        let mcu_size = decoder.mcu_buffer_size();
        let work_size = decoder.work_buffer_size();
        let mut mcu_buffer = vec![0i16; mcu_size];
        let mut work_buffer = vec![0u8; work_size];
        
        decoder.decompress_with_buffers(
            jpeg_data, 0,
            &mut mcu_buffer,
            &mut work_buffer,
            &mut |_, bitmap, rect| {
                // Convert and write to output buffer
                // ...
                Ok(true)
            }
        )?;
        
        Ok((width, height))
    }
}
```

## FAQ

### Q: Stack overflow on ESP32?
A: Use `decompress_with_buffers()` method, which allows allocating buffers in static memory or heap instead of stack.

### Q: How to reduce memory usage?
A: 
1. Don't enable `fast-decode` feature (saves ~6KB)
2. Use `decompress_with_buffers()` and allocate buffers in static memory
3. Consider using smaller scaling factors

### Q: Why is output slightly different from C version?
A: Rounding errors in IDCT calculations, typically <2% difference, doesn't affect visual quality.

## API Updates (v0.3.1)

### New APIs

1. **`mcu_buffer_size()`** - Calculate required MCU buffer size
2. **`work_buffer_size()`** - Calculate required work buffer size
3. **`decompress_with_buffers()`** - Decompress using external buffers (recommended)

### API Changes

- `decompress()` method now requires `alloc-buffers` feature (disabled by default)
- Recommended to use `decompress_with_buffers()` for better memory control

## Developer Guide

### Architecture

The library is organized into several modules:

#### Core Modules

1. **types.rs** - Type definitions and error handling
2. **tables.rs** - Constant lookup tables and conversion functions
3. **huffman.rs** - Huffman decoding implementation
4. **idct.rs** - Inverse DCT and color conversion
5. **decoder.rs** - Main JPEG decoder implementation

### Memory Management

The library is designed for embedded systems with limited memory:

- Uses stack allocation where possible
- `heapless` crate for fixed-size collections in `no_std` environments
- Configurable workspace size based on optimization level

### Feature Flags Details

#### `std` (default)
Enables standard library support. Disable for `no_std` environments.

#### `fast-decode`
Enables fast Huffman decoding using lookup tables (LUT):
- Increases memory usage by ~6KB
- Significantly faster decoding
- Recommended for 32-bit MCUs with sufficient RAM

#### `table-clip`
Uses a 1KB lookup table for value clipping:
- Faster than conditional clipping
- Minimal memory overhead

#### `use-scale`
Enables output scaling support (1/2, 1/4, 1/8):
- Useful for generating thumbnails
- Minimal code size increase

#### `alloc-buffers`
Enables automatic buffer allocation in `decompress()` method:
- **Disabled by default** - may cause stack overflow on embedded systems
- Automatically allocates MCU and work buffers on the stack
- Only use on platforms with sufficient stack space (>4KB)
- For embedded systems, use `decompress_with_buffers()` instead

### Implementation Details

#### Memory-Efficient API (v0.3.1+)

The library now provides two decoding APIs:

1. **`decompress_with_buffers()`** (Recommended for embedded)
   - Accepts external MCU and work buffers
   - Buffers can be allocated in static memory or heap
   - No stack overflow risk
   - Use `mcu_buffer_size()` and `work_buffer_size()` to calculate sizes

2. **`decompress()`** (Requires `alloc-buffers` feature)
   - Automatically allocates buffers on stack
   - Convenient but may cause stack overflow on ESP32
   - Only suitable for systems with >4KB stack

#### Huffman Decoding

The Huffman decoder supports two modes:
1. **Incremental search** - Slower but uses less memory
2. **Fast LUT** - Uses 10-bit lookup table for common codes

#### IDCT

Uses the Arai, Agui, and Nakajima fast DCT algorithm:
- Fixed-point arithmetic for efficiency
- Optimized for 8x8 blocks
- In-place computation where possible

#### Color Conversion

YCbCr to RGB conversion using fixed-point arithmetic:
- Avoids floating-point operations
- Efficient on integer-only processors
- Handles chroma subsampling (4:2:0, 4:2:2)

### Safety

The library prioritizes memory safety:
- Minimal unsafe code (only for BMP header serialization in examples)
- Bounds checking on all buffer accesses
- Proper error propagation
- No panics in release builds (returns `Result`)
- Buffer size validation before decompression

### Contributing Guidelines

When contributing, please ensure:
1. Code follows Rust idioms and best practices
2. All tests pass
3. `no_std` compatibility is maintained
4. Performance-critical code is benchmarked
5. Documentation is updated

## Contributing

Contributions welcome! Please ensure:
1. Code follows Rust idioms
2. All tests pass
3. Maintain no_std compatibility
4. Update relevant documentation

## License

Based on [TJpg_Decoder](https://github.com/Bodmer/TJpg_Decoder) (Original author: ChaN)  
Rust implementation: MIT License

Original TJpgDec License:
```
TJpg_Decoder based on TJpgDec - Tiny JPEG Decompressor R0.03 (C)ChaN, 2021
```

Original TJpgDec License:
```
The TJpgDec module is a free software and there is NO WARRANTY.
No restriction on use. You can use, modify and redistribute it for
personal, non-profit or commercial products UNDER YOUR RESPONSIBILITY.
Redistributions of source code must retain the above copyright notice.
```

## Related Links

- [Changelog](CHANGELOG.md)
- [Chinese Documentation](README.md)

## Acknowledgements

Thanks to ChaN for creating the original TJpgDec library
