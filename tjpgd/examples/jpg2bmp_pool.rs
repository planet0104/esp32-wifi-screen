//! 使用内存池版本的JPEG解码示例
//! 
//! 与C版本完全一致的内存管理方式

use std::env;
use std::fs::File;
use std::io::{Read, Write};
use tjpgd::{JpegDecoderPool, MemoryPool, Rectangle, Result, calculate_pool_size};

fn main() -> Result<()> {
    let args: Vec<String> = env::args().collect();
    
    if args.len() < 3 {
        eprintln!("Usage: {} <input.jpg> <output.bmp>", args[0]);
        std::process::exit(1);
    }

    let input_path = &args[1];
    let output_path = &args[2];

    // 读取JPEG文件
    let mut file = File::open(input_path).expect("Failed to open input file");
    let mut jpeg_data = Vec::new();
    file.read_to_end(&mut jpeg_data).expect("Failed to read input file");

    println!("=== Memory Pool Version ===");
    println!("JPEG file size: {} bytes", jpeg_data.len());

    // 计算所需的工作内存池大小
    let pool_size = calculate_pool_size(0, 0, cfg!(feature = "fast-decode"));
    println!("Calculated pool size: {} bytes", pool_size);

    // 分配工作内存池（与C版本完全一致的API）
    let mut workspace = vec![0u8; pool_size];
    let mut pool = MemoryPool::new(&mut workspace);
    
    println!("Pool capacity: {} bytes", pool.capacity());

    // 创建解码器
    let mut decoder = JpegDecoderPool::new();
    
    // 准备解码（解析JPEG头部，从池中分配内部数据）
    decoder.prepare(&jpeg_data, &mut pool)?;
    
    println!("Pool used after prepare: {} bytes", pool.used());
    println!("Pool remaining: {} bytes", pool.remaining());
    
    println!("\nImage dimensions: {}x{}", decoder.width(), decoder.height());
    println!("Components: {}", decoder.components());

    // 分配MCU和工作缓冲区（这些在C版本中也是从池外分配的）
    let mcu_size = decoder.mcu_buffer_size();
    let work_size = decoder.work_buffer_size();
    
    println!("\nMCU buffer size: {} bytes", mcu_size * 2);  // i16 = 2 bytes
    println!("Work buffer size: {} bytes", work_size);

    let mut mcu_buffer = vec![0i16; mcu_size];
    let mut work_buffer = vec![0u8; work_size];

    // 准备输出BMP数据
    let width = decoder.width() as usize;
    let height = decoder.height() as usize;
    let row_stride = (width * 3 + 3) & !3;  // 4字节对齐
    let mut bmp_data = vec![0u8; row_stride * height];

    // 解码
    decoder.decompress(
        &jpeg_data,
        0,  // scale = 0 (原始大小)
        &mut mcu_buffer,
        &mut work_buffer,
        &mut |_decoder, rgb_data, rect: &Rectangle| {
            // 将RGB数据复制到BMP缓冲区（BGR顺序，倒置行）
            let rect_width = (rect.right - rect.left + 1) as usize;
            let rect_height = (rect.bottom - rect.top + 1) as usize;
            
            for y in 0..rect_height {
                let src_row = y * rect_width * 3;
                let dst_y = height - 1 - (rect.top as usize + y);
                let dst_row = dst_y * row_stride + rect.left as usize * 3;
                
                for x in 0..rect_width {
                    let src_idx = src_row + x * 3;
                    let dst_idx = dst_row + x * 3;
                    
                    if dst_idx + 2 < bmp_data.len() && src_idx + 2 < rgb_data.len() {
                        // RGB -> BGR
                        bmp_data[dst_idx] = rgb_data[src_idx + 2];     // B
                        bmp_data[dst_idx + 1] = rgb_data[src_idx + 1]; // G
                        bmp_data[dst_idx + 2] = rgb_data[src_idx];     // R
                    }
                }
            }
            
            Ok(true)
        },
    )?;

    println!("\nDecoding complete!");

    // 写BMP文件
    let mut bmp_file = File::create(output_path).expect("Failed to create output file");
    
    // BMP头
    let file_size = 54 + bmp_data.len();
    let bmp_header: [u8; 54] = create_bmp_header(width as u32, height as u32, file_size as u32);
    
    bmp_file.write_all(&bmp_header).expect("Failed to write BMP header");
    bmp_file.write_all(&bmp_data).expect("Failed to write BMP data");

    println!("Output saved to: {}", output_path);

    // 内存使用摘要
    println!("\n=== Memory Usage Summary ===");
    println!("Pool (internal structures): {} bytes", pool.used());
    println!("MCU buffer: {} bytes", mcu_size * 2);
    println!("Work buffer: {} bytes", work_size);
    println!("Total: {} bytes", pool.used() + mcu_size * 2 + work_size);
    println!("\nC version comparison:");
    println!("  Pool size (JD_FASTDECODE=2): ~9644 bytes");

    Ok(())
}

fn create_bmp_header(width: u32, height: u32, file_size: u32) -> [u8; 54] {
    let row_stride = (width * 3 + 3) & !3;
    let image_size = row_stride * height;
    
    let mut header = [0u8; 54];
    
    // BMP signature
    header[0] = b'B';
    header[1] = b'M';
    
    // File size
    header[2..6].copy_from_slice(&file_size.to_le_bytes());
    
    // Reserved
    header[6..10].fill(0);
    
    // Data offset
    header[10..14].copy_from_slice(&54u32.to_le_bytes());
    
    // DIB header size
    header[14..18].copy_from_slice(&40u32.to_le_bytes());
    
    // Width
    header[18..22].copy_from_slice(&width.to_le_bytes());
    
    // Height
    header[22..26].copy_from_slice(&height.to_le_bytes());
    
    // Planes
    header[26..28].copy_from_slice(&1u16.to_le_bytes());
    
    // Bits per pixel
    header[28..30].copy_from_slice(&24u16.to_le_bytes());
    
    // Compression
    header[30..34].fill(0);
    
    // Image size
    header[34..38].copy_from_slice(&image_size.to_le_bytes());
    
    // Resolution (72 DPI)
    header[38..42].copy_from_slice(&2835u32.to_le_bytes());
    header[42..46].copy_from_slice(&2835u32.to_le_bytes());
    
    // Colors
    header[46..54].fill(0);
    
    header
}
