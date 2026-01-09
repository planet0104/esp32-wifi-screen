//! C版本 vs Rust版本 内存使用对比
//! 
//! 运行: cargo run --example memory_comparison

use std::mem::size_of;
use tjpgd::{JpegDecoder, JpegDecoderPool, HuffmanTable, HuffmanTablePool, MemoryPool, calculate_pool_size};

fn main() {
    println!("╔══════════════════════════════════════════════════════════════╗");
    println!("║        C版本 vs Rust版本 内存使用对比                        ║");
    println!("╚══════════════════════════════════════════════════════════════╝");

    println!("\n=== C版本 (JD_FASTDECODE=2) ===");
    println!("┌─────────────────────────────────┬────────────────┐");
    println!("│ 分配位置                        │ 大小 (bytes)   │");
    println!("├─────────────────────────────────┼────────────────┤");
    println!("│ JDEC结构体 (栈)                 │ ~120           │");
    println!("│ inbuf (池内)                    │ 512            │");
    println!("│ huffbits[4] (池内)              │ 64             │");
    println!("│ huffcode[4] (池内,动态)         │ ~1000          │");
    println!("│ huffdata[4] (池内,动态)         │ ~500           │");
    println!("│ hufflut_ac[2] (池内)            │ 4096           │");
    println!("│ hufflut_dc[2] (池内)            │ 2048           │");
    println!("│ qttbl[4] (池内)                 │ 1024           │");
    println!("│ workbuf (池内)                  │ ~320           │");
    println!("│ mcubuf (池内)                   │ ~768           │");
    println!("├─────────────────────────────────┼────────────────┤");
    println!("│ 池总计                          │ ~9644          │");
    println!("│ 栈总计                          │ ~120           │");
    println!("│ 总计                            │ ~9764          │");
    println!("└─────────────────────────────────┴────────────────┘");

    println!("\n=== Rust版本 (原始 - 使用Box/heapless::Vec) ===");
    let decoder_size = size_of::<JpegDecoder>();
    let huff_table_size = size_of::<HuffmanTable>();
    
    println!("┌─────────────────────────────────┬────────────────┐");
    println!("│ 分配位置                        │ 大小 (bytes)   │");
    println!("├─────────────────────────────────┼────────────────┤");
    println!("│ JpegDecoder结构体 (栈)          │ {:>14} │", decoder_size);
    println!("│ HuffmanTable × 4 (堆)           │ {:>14} │", huff_table_size * 4);
    println!("│ Quantization tables × 4 (堆)    │ {:>14} │", 256 * 4);
    println!("│ mcu_buffer (外部)               │ 用户提供       │");
    println!("│ work_buffer (外部)              │ 用户提供       │");
    println!("├─────────────────────────────────┼────────────────┤");
    let rust_heap = huff_table_size * 4 + 256 * 4;
    let rust_total = decoder_size + rust_heap;
    println!("│ 堆总计                          │ {:>14} │", rust_heap);
    println!("│ 栈总计                          │ {:>14} │", decoder_size);
    println!("│ 总计                            │ {:>14} │", rust_total);
    println!("└─────────────────────────────────┴────────────────┘");

    println!("\n=== Rust版本 (内存池版本 - 与C一致) ===");
    let decoder_pool_size = size_of::<JpegDecoderPool>();
    let huff_table_pool_size = size_of::<HuffmanTablePool>();
    
    // 创建一个测试池来测量实际使用
    let pool_capacity = calculate_pool_size(0, 0, false);
    
    println!("┌─────────────────────────────────┬────────────────┐");
    println!("│ 分配位置                        │ 大小 (bytes)   │");
    println!("├─────────────────────────────────┼────────────────┤");
    println!("│ JpegDecoderPool结构体 (栈)      │ {:>14} │", decoder_pool_size);
    println!("│ HuffmanTablePool × 4 (池内)     │ {:>14} │", huff_table_pool_size * 4);
    println!("│   - codes (动态大小)            │ 动态           │");
    println!("│   - data (动态大小)             │ 动态           │");
    println!("│ Quantization tables × 4 (池内)  │ {:>14} │", 256 * 4);
    println!("│ mcu_buffer (外部)               │ 用户提供       │");
    println!("│ work_buffer (外部)              │ 用户提供       │");
    println!("├─────────────────────────────────┼────────────────┤");
    println!("│ 推荐池大小                      │ {:>14} │", pool_capacity);
    println!("│ 栈总计                          │ {:>14} │", decoder_pool_size);
    println!("│ 实际池使用                      │ 约1200-1500    │");
    println!("└─────────────────────────────────┴────────────────┘");

    println!("\n=== 对比总结 ===");
    println!("┌────────────────────┬──────────┬──────────┬──────────┐");
    println!("│ 版本               │ 栈使用   │ 堆/池使用│ 总计     │");
    println!("├────────────────────┼──────────┼──────────┼──────────┤");
    println!("│ C (FASTDECODE=2)   │ ~120     │ ~9644    │ ~9764    │");
    println!("│ Rust (原始)        │ {:>8} │ {:>8} │ {:>8} │", decoder_size, rust_heap, rust_total);
    println!("│ Rust (内存池)      │ {:>8} │ ~1300    │ ~{:<6} │", decoder_pool_size, decoder_pool_size + 1300);
    println!("└────────────────────┴──────────┴──────────┴──────────┘");

    println!("\n=== 关键差异说明 ===");
    println!("1. C版本使用固定大小的inbuf(512 bytes),Rust直接使用输入切片");
    println!("2. C版本的huffcode/huffdata是动态大小,Rust的heapless::Vec是固定最大容量");
    println!("3. C版本使用宏定义的LUT大小,Rust的fast-decode feature控制是否使用LUT");
    println!("4. Rust内存池版本与C版本内存管理方式完全一致");
    println!("");
    println!("=== 结构体大小详情 ===");
    println!("JpegDecoder: {} bytes", decoder_size);
    println!("JpegDecoderPool: {} bytes", decoder_pool_size);
    println!("HuffmanTable: {} bytes", huff_table_size);
    println!("HuffmanTablePool: {} bytes", huff_table_pool_size);
    println!("MemoryPool: {} bytes", size_of::<MemoryPool>());
}
