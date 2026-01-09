# TJpgDec-rs - Tiny JPEG Decoder

A Rust implementation of ChaN's TJpgDec library - a lightweight JPEG decoder designed for embedded systems.

[English Version](README.en.md) | [中文文档](README.md)

## Features

- **Lightweight**: Optimized for memory-constrained embedded systems
- **High Performance**: Three optimization levels matching the C version exactly
- **Flexible**: Support for various output formats (RGB888, RGB565, Grayscale)
- **no_std Compatible**: Can run without the standard library
- **C-Compatible**: Memory management matches the original C code exactly

## Supported Features

- Baseline JPEG (SOF0)
- Grayscale and YCbCr color spaces
- Sampling factors: 4:4:4, 4:2:0, 4:2:2
- Output scaling (1/1, 1/2, 1/4, 1/8)
- RGB888 output format

## JD_FASTDECODE Optimization Levels

Three optimization levels matching the C version exactly:

| Level | Feature | Description | Workspace Size | Target Platform |
|-------|---------|-------------|----------------|-----------------|
| 0 | `fast-decode-0` | Basic optimization | 3100 bytes | 8/16-bit MCUs |
| 1 | `fast-decode-1` | + 32-bit barrel shifter | 3500 bytes | 32-bit MCUs (ESP32 recommended) |
| 2 | `fast-decode-2` | + Huffman lookup table | 9644 bytes | Fastest, more memory |

## Usage

### Basic Usage

```rust
use tjpgd::{JpegDecoder, MemoryPool, RECOMMENDED_POOL_SIZE, Result};

fn decode_jpeg(jpeg_data: &[u8]) -> Result<()> {
    // Allocate memory pool (matches C API exactly)
    let mut pool_buffer = vec![0u8; RECOMMENDED_POOL_SIZE];
    let mut pool = MemoryPool::new(&mut pool_buffer);
    
    // Create decoder
    let mut decoder = JpegDecoder::new();
    
    // Prepare decoder (corresponds to C's jd_prepare)
    decoder.prepare(jpeg_data, &mut pool)?;
    
    // Get image info
    let width = decoder.width();
    let height = decoder.height();
    println!("Image size: {}x{}", width, height);
    
    // Calculate required buffer sizes
    let mcu_size = decoder.mcu_buffer_size();
    let work_size = decoder.work_buffer_size();
    
    // Allocate work buffers
    let mut mcu_buffer = vec![0i16; mcu_size];
    let mut work_buffer = vec![0u8; work_size];
    
    // Allocate output framebuffer
    let mut framebuffer = vec![0u8; (width as usize * height as usize * 3)];
    let fb_width = width as usize;
    
    // Decompress (corresponds to C's jd_decomp)
    decoder.decompress(
        jpeg_data,
        0,  // scale: 0=1/1, 1=1/2, 2=1/4, 3=1/8
        &mut mcu_buffer,
        &mut work_buffer,
        &mut |_decoder, bitmap, rect| {
            // bitmap is RGB888 format, 3 bytes per pixel
            let rect_width = (rect.right - rect.left + 1) as usize;
            let bytes_per_row = rect_width * 3;
            
            for y in rect.top..=rect.bottom {
                let src_offset = ((y - rect.top) as usize) * bytes_per_row;
                let dst_offset = (y as usize) * fb_width * 3 + (rect.left as usize) * 3;
                
                if src_offset + bytes_per_row <= bitmap.len()
                   && dst_offset + bytes_per_row <= framebuffer.len() {
                    framebuffer[dst_offset..dst_offset + bytes_per_row]
                        .copy_from_slice(&bitmap[src_offset..src_offset + bytes_per_row]);
                }
            }
            Ok(true)  // Continue decoding
        }
    )?;
    
    Ok(())
}
```

### ESP32 Example

```rust
use tjpgd::{JpegDecoder, MemoryPool, RECOMMENDED_POOL_SIZE};

pub fn decode_jpeg_to_rgb565(jpeg_data: &[u8]) -> Result<(u16, u16, Vec<u16>)> {
    // Allocate memory pool
    let mut pool_buffer = vec![0u8; RECOMMENDED_POOL_SIZE];
    let mut pool = MemoryPool::new(&mut pool_buffer);
    
    let mut decoder = JpegDecoder::new();
    decoder.prepare(jpeg_data, &mut pool)?;
    
    let width = decoder.width();
    let height = decoder.height();
    
    let mut mcu_buffer = vec![0i16; decoder.mcu_buffer_size()];
    let mut work_buffer = vec![0u8; decoder.work_buffer_size()];
    let mut output = vec![0u16; width as usize * height as usize];
    let fb_width = width as usize;
    
    decoder.decompress(
        jpeg_data, 0,
        &mut mcu_buffer, &mut work_buffer,
        &mut |_decoder, bitmap, rect| {
            let rect_width = (rect.right - rect.left + 1) as usize;
            let bytes_per_row = rect_width * 3;
            
            for y in rect.top..=rect.bottom {
                let y_offset = (y - rect.top) as usize;
                let src_offset = y_offset * bytes_per_row;
                let dst_row = y as usize * fb_width + rect.left as usize;
                
                for x in 0..rect_width {
                    let byte_idx = src_offset + x * 3;
                    if byte_idx + 2 < bitmap.len() {
                        let r = bitmap[byte_idx];
                        let g = bitmap[byte_idx + 1];
                        let b = bitmap[byte_idx + 2];
                        // RGB888 to RGB565
                        let pixel = ((r as u16 & 0xF8) << 8)
                                  | ((g as u16 & 0xFC) << 3)
                                  | ((b as u16) >> 3);
                        output[dst_row + x] = pixel.swap_bytes();
                    }
                }
            }
            Ok(true)
        }
    )?;
    
    Ok((width, height, output))
}
```

## Installation

Add to your `Cargo.toml`:

```toml
[dependencies]
tjpgd = { path = "path/to/tjpgd", features = ["fast-decode-2"] }
```

### Feature Flags

| Feature | Description |
|---------|-------------|
| `std` (default) | Enable standard library support |
| `fast-decode-0` | JD_FASTDECODE=0: Basic optimization for 8/16-bit MCUs |
| `fast-decode-1` | JD_FASTDECODE=1: 32-bit barrel shifter (recommended for ESP32) |
| `fast-decode-2` | JD_FASTDECODE=2: + Huffman lookup table (fastest) |
| `fast-decode` | Alias for `fast-decode-2` |
| `table-clip` | Use lookup table for value clipping (adds ~1KB code) |
| `use-scale` | Enable output scaling support |
| `debug-huffman` | Enable Huffman decoding debug output |

### Configuration for Different Platforms

**ESP32 (Recommended):**
```toml
[dependencies.tjpgd]
path = "tjpgd"
default-features = false
features = ["fast-decode-2"]
```

**Memory-Constrained 32-bit MCUs:**
```toml
[dependencies.tjpgd]
path = "tjpgd"
default-features = false
features = ["fast-decode-1"]
```

**8/16-bit MCUs (Experimental):**
```toml
[dependencies.tjpgd]
path = "tjpgd"
default-features = false
features = ["fast-decode-0"]
```

## Memory Requirements

| Optimization Level | Decoder Struct | Workspace | Notes |
|-------------------|----------------|-----------|-------|
| Level 0 | ~120 bytes | 3100 bytes | Basic mode |
| Level 1 | ~120 bytes | 3500 bytes | + 32-bit register |
| Level 2 | ~120 bytes | 9644 bytes | + Huffman LUT |

### Buffer Requirements
- MCU buffer: 192-384 i16 elements (384-768 bytes)
- Work buffer: 192-768 bytes

## API Documentation

### JpegDecoder

```rust
use tjpgd::{JpegDecoder, MemoryPool, RECOMMENDED_POOL_SIZE};

// Create memory pool
let mut pool_buffer = vec![0u8; RECOMMENDED_POOL_SIZE];
let mut pool = MemoryPool::new(&mut pool_buffer);

// Create decoder
let mut decoder = JpegDecoder::new();

// Prepare decoder
decoder.prepare(jpeg_data, &mut pool)?;

// Get image info
let width = decoder.width();      // Output width (after scaling)
let height = decoder.height();    // Output height (after scaling)
let raw_width = decoder.raw_width();   // Original width
let raw_height = decoder.raw_height(); // Original height
let components = decoder.components(); // Color components (1=grayscale, 3=color)

// Calculate buffer sizes
let mcu_size = decoder.mcu_buffer_size();
let work_size = decoder.work_buffer_size();

// Decompress
decoder.decompress(jpeg_data, scale, &mut mcu_buf, &mut work_buf, callback)?;
```

### Query Optimization Level

```rust
use tjpgd::fastdecode_level;

let level = fastdecode_level();
println!("Current JD_FASTDECODE level: {}", level);
```

## C Version Correspondence

| C Function/Type | Rust Equivalent |
|----------------|-----------------|
| `jd_prepare()` | `decoder.prepare()` |
| `jd_decomp()` | `decoder.decompress()` |
| `JDEC` | `JpegDecoder` |
| `JRESULT` | `Result<T>` |
| `JRECT` | `Rectangle` |
| `alloc_pool()` | `MemoryPool` |
| `JD_FASTDECODE` | `fast-decode-0/1/2` features |

## Project Structure

```
tjpgd/
├── Cargo.toml
├── README.md / README.en.md
├── src/
│   ├── lib.rs           # Library entry point
│   ├── types.rs         # Type definitions
│   ├── tables.rs        # Constant tables
│   ├── huffman.rs       # Huffman decoding (supports 3 optimization levels)
│   ├── idct.rs          # IDCT and color conversion
│   ├── decoder.rs       # Main decoder
│   └── pool.rs          # Memory pool implementation
└── examples/
    ├── basic.rs         # Basic usage example
    ├── jpg2bmp.rs       # JPEG to BMP converter
    └── compare_outputs.ps1  # C/Rust output comparison script
```

## Development and Testing

```bash
# Run tests
cargo test

# Run example (using Level 2)
cargo run --example jpg2bmp -- input.jpg output.bmp

# Use specific optimization level
cargo run --example jpg2bmp --no-default-features --features fast-decode-1 -- input.jpg

# Compare C and Rust outputs (test all modes)
cd examples
powershell -ExecutionPolicy Bypass -File compare_outputs.ps1 -Mode all

# Test specific mode only
powershell -ExecutionPolicy Bypass -File compare_outputs.ps1 -Mode 2
```

## FAQ

### Q: Stack overflow on ESP32?
A: Make sure to use `MemoryPool` for memory allocation. The decoder struct itself is only ~120 bytes and won't cause stack overflow. Work buffers should be allocated on heap (using `vec![]`).

### Q: How to choose optimization level?
A: 
- **ESP32**: Recommend `fast-decode-2` (fastest) or `fast-decode-1` (saves memory)
- **Memory-constrained**: Use `fast-decode-1`, workspace only needs 3500 bytes
- **8/16-bit MCUs**: Use `fast-decode-0` (experimental)

### Q: Why is output slightly different from C version?
A: Rounding errors in IDCT calculations, typically ≤3 per pixel, doesn't affect visual quality.

### Q: How to reduce memory usage?
A: 
1. Use `fast-decode-1` instead of `fast-decode-2` (saves ~6KB)
2. Use smaller scale factors (1/2, 1/4, 1/8)
3. Process large images in chunks

## License

Based on [TJpg_Decoder](https://github.com/Bodmer/TJpg_Decoder) (Original author: ChaN)  
Rust implementation: MIT License

```
The TJpgDec module is a free software and there is NO WARRANTY.
No restriction on use. You can use, modify and redistribute it for
personal, non-profit or commercial products UNDER YOUR RESPONSIBILITY.
```

## Related Links

- [Changelog](CHANGELOG.md)
- [Chinese Documentation](README.md)

## Acknowledgements

Thanks to ChaN for creating the original TJpgDec library.
