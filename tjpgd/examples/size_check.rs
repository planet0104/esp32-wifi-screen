//! Check struct sizes for stack analysis
//! 
//! 用于分析Rust JPEG解码器的栈内存使用情况

use std::mem::size_of;
use tjpgd::JpegDecoder;

fn main() {
    println!("=== Rust JPEG Decoder Stack Analysis ===\n");
    
    println!("JpegDecoder struct: {} bytes", size_of::<JpegDecoder>());
    println!("  (Huffman tables now use Box, data on heap)");
    
    println!("\n=== Heap-allocated data (via Box) ===");
    println!("Box<[i32; 64]> (pointer): {} bytes", size_of::<Box<[i32; 64]>>());
    println!("  Actual qtable data: 256 bytes each, 4 tables = 1024 bytes on heap");
    
    // heapless::Vec sizes (inside HuffmanTable)
    println!("\n=== HuffmanTable internal sizes ===");
    println!("heapless::Vec<u16, 256> (codes): {} bytes", size_of::<heapless::Vec<u16, 256>>());
    println!("heapless::Vec<u8, 256> (data): {} bytes", size_of::<heapless::Vec<u8, 256>>());
    println!("heapless::Vec<u16, 1024> (LUT): {} bytes", size_of::<heapless::Vec<u16, 1024>>());
    
    let huff_without_lut = 16 + size_of::<heapless::Vec<u16, 256>>() + size_of::<heapless::Vec<u8, 256>>() + 8;
    let huff_with_lut = huff_without_lut + size_of::<Option<heapless::Vec<u16, 1024>>>();
    println!("\nHuffmanTable per table (on heap via Box):");
    println!("  Without fast-decode: ~{} bytes", huff_without_lut);
    println!("  With fast-decode: ~{} bytes", huff_with_lut);
    println!("  4 tables total: ~{} bytes on heap", huff_with_lut * 4);
    
    println!("\n=== Stack usage during decode_mcu ===");
    println!("tmp array [i32; 64]: {} bytes (on stack)", size_of::<[i32; 64]>());
    println!("  Note: C version uses jd->workbuf (from pool)");
    
    println!("\n=== C version comparison ===");
    println!("C JDEC struct: ~120 bytes (only pointers)");
    println!("C workspace (JD_FASTDECODE=2): 9644 bytes (from pool/heap)");
    
    println!("\n=== ESP32 Stack Summary ===");
    println!("ESP32 default task stack: 8192 bytes");
    let decoder_stack = size_of::<JpegDecoder>();
    let decode_mcu_stack = 256 + 64; // tmp + other locals
    let total_estimated = decoder_stack + decode_mcu_stack + 200; // + callback overhead
    println!("Estimated peak stack usage:");
    println!("  JpegDecoder: {} bytes", decoder_stack);
    println!("  decode_mcu locals: ~{} bytes", decode_mcu_stack);
    println!("  Callback overhead: ~200 bytes");
    println!("  -------------------------");
    println!("  Total: ~{} bytes", total_estimated);
    
    if total_estimated < 2048 {
        println!("\n[OK] Stack usage is safe for ESP32 (< 2KB)");
    } else if total_estimated < 4096 {
        println!("\n[OK] Stack usage is acceptable for ESP32 (< 4KB)");
    } else {
        println!("\n[WARNING] Stack usage may be too high for some ESP32 configurations");
    }
}
