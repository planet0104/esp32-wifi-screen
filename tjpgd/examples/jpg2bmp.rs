//! JPEG to BMP Converter using tjpgd
//! 
//! Usage: cargo run --example jpg2bmp <input.jpg> [output.bmp]

use std::env;
use std::fs::File;
use std::io::{Read, Write};
use std::path::Path;

use tjpgd::{JpegDecoder, Rectangle};

/// BMP文件头 (14 bytes)
#[repr(C, packed)]
struct BitmapFileHeader {
    bf_type: u16,        // 文件类型，必须是 'BM' (0x4D42)
    bf_size: u32,        // 文件大小
    bf_reserved1: u16,   // 保留，必须为0
    bf_reserved2: u16,   // 保留，必须为0
    bf_off_bits: u32,    // 位图数据偏移量
}

/// BMP信息头 (40 bytes)
#[repr(C, packed)]
struct BitmapInfoHeader {
    bi_size: u32,            // 此头的大小
    bi_width: i32,           // 图像宽度
    bi_height: i32,          // 图像高度
    bi_planes: u16,          // 颜色平面数
    bi_bit_count: u16,       // 每像素位数
    bi_compression: u32,     // 压缩类型
    bi_size_image: u32,      // 图像大小
    bi_x_pels_per_meter: i32, // 水平分辨率
    bi_y_pels_per_meter: i32, // 垂直分辨率
    bi_clr_used: u32,        // 使用的颜色数
    bi_clr_important: u32,   // 重要颜色数
}

/// 将RGB888 framebuffer保存为BMP文件
fn save_bmp(filename: &str, framebuffer: &[u8], width: u32, height: u32) -> std::io::Result<()> {
    let mut file = File::create(filename)?;
    
    let row_size = (width * 3) as usize;
    let padding = (4 - (row_size % 4)) % 4;
    let padded_row_size = row_size + padding;
    
    // 文件头
    let file_header = BitmapFileHeader {
        bf_type: 0x4D42, // "BM"
        bf_size: (14 + 40 + padded_row_size * height as usize) as u32,
        bf_reserved1: 0,
        bf_reserved2: 0,
        bf_off_bits: 14 + 40,
    };
    
    // 信息头
    let info_header = BitmapInfoHeader {
        bi_size: 40,
        bi_width: width as i32,
        bi_height: height as i32,  // 正数表示自底向上
        bi_planes: 1,
        bi_bit_count: 24,
        bi_compression: 0,
        bi_size_image: (padded_row_size * height as usize) as u32,
        bi_x_pels_per_meter: 2835,
        bi_y_pels_per_meter: 2835,
        bi_clr_used: 0,
        bi_clr_important: 0,
    };
    
    // 写入文件头
    file.write_all(unsafe {
        std::slice::from_raw_parts(
            &file_header as *const _ as *const u8,
            std::mem::size_of::<BitmapFileHeader>()
        )
    })?;
    
    // 写入信息头
    file.write_all(unsafe {
        std::slice::from_raw_parts(
            &info_header as *const _ as *const u8,
            std::mem::size_of::<BitmapInfoHeader>()
        )
    })?;
    
    // 写入像素数据 (BMP是自底向上存储，且是BGR顺序)
    let pad_bytes = [0u8; 3];
    let mut row_buffer = vec![0u8; row_size];
    
    for y in (0..height as usize).rev() {
        let src_row = &framebuffer[y * row_size..(y + 1) * row_size];
        
        // RGB -> BGR 转换
        for x in 0..width as usize {
            row_buffer[x * 3 + 0] = src_row[x * 3 + 2]; // B
            row_buffer[x * 3 + 1] = src_row[x * 3 + 1]; // G
            row_buffer[x * 3 + 2] = src_row[x * 3 + 0]; // R
        }
        
        file.write_all(&row_buffer)?;
        if padding > 0 {
            file.write_all(&pad_bytes[..padding])?;
        }
    }
    
    println!("Output saved to {}", filename);
    Ok(())
}

/// 生成输出文件名
fn generate_output_filename(input_file: &str) -> String {
    let path = Path::new(input_file);
    if let Some(stem) = path.file_stem() {
        // 输出到 test_output 文件夹
        return format!("test_output/{}.bmp", stem.to_string_lossy());
    }
    format!("test_output/{}.bmp", input_file)
}

fn main() {
    println!("JPEG to BMP Converter using tjpgd (Rust)");
    println!("=========================================\n");
    
    let args: Vec<String> = env::args().collect();
    
    if args.len() < 2 {
        println!("Usage: {} <input.jpg> [output.bmp]", args[0]);
        println!("  input.jpg  - Input JPEG file");
        println!("  output.bmp - Output BMP file (optional, default: test_output/<name>.bmp)");
        println!("\nExamples:");
        println!("  {} monitor.jpg                    -> test_output/monitor.bmp", args[0]);
        println!("  {} photo.jpg test_output/out.bmp  -> test_output/out.bmp", args[0]);
        std::process::exit(1);
    }
    
    let input_file = &args[1];
    let output_file = if args.len() >= 3 {
        args[2].clone()
    } else {
        generate_output_filename(input_file)
    };
    
    // 确保 test_output 目录存在
    if let Err(e) = std::fs::create_dir_all("test_output") {
        println!("Warning: Cannot create test_output directory: {}", e);
    }
    
    // 读取JPEG文件
    let mut file = match File::open(input_file) {
        Ok(f) => f,
        Err(e) => {
            println!("Error: Cannot open input file {}: {}", input_file, e);
            std::process::exit(1);
        }
    };
    
    let mut jpeg_data = Vec::new();
    if let Err(e) = file.read_to_end(&mut jpeg_data) {
        println!("Error: Cannot read input file: {}", e);
        std::process::exit(1);
    }
    
    println!("Input file: {}", input_file);
    println!("Output file: {}", output_file);
    println!("File size: {} bytes", jpeg_data.len());
    
    // 创建解码器并准备
    let mut decoder = JpegDecoder::new();
    
    if let Err(e) = decoder.prepare(&jpeg_data) {
        println!("Error: prepare() failed: {:?}", e);
        std::process::exit(1);
    }
    
    let width = decoder.width() as u32;
    let height = decoder.height() as u32;
    let components = decoder.components();
    
    println!("Image size: {} x {}", width, height);
    println!("Components: {}", components);
    
    // 获取所需缓冲区大小
    let mcu_size = decoder.mcu_buffer_size();
    let work_size = decoder.work_buffer_size();
    
    println!("MCU buffer size: {} (i16 elements)", mcu_size);
    println!("Work buffer size: {} bytes", work_size);
    
    // 分配缓冲区（使用内存节约版本）
    let mut mcu_buffer = vec![0i16; mcu_size];
    let mut work_buffer = vec![0u8; work_size];
    let mut framebuffer = vec![0u8; (width * height * 3) as usize];
    let fb_width = width as usize;
    
    // 解码回调函数 - 将MCU数据复制到framebuffer
    let mut callback = |_decoder: &JpegDecoder, bitmap: &[u8], rect: &Rectangle| -> Result<bool, tjpgd::Error> {
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
        
        Ok(true) // 继续解码
    };
    
    println!("Decompressing with external buffers (memory-efficient mode)...");
    
    if let Err(e) = decoder.decompress_with_buffers(&jpeg_data, 0, &mut mcu_buffer, &mut work_buffer, &mut callback) {
        println!("Error: decompress_with_buffers() failed: {:?}", e);
        std::process::exit(1);
    }
    
    println!("Decompression completed successfully!");
    
    // 保存为BMP
    if let Err(e) = save_bmp(&output_file, &framebuffer, width, height) {
        println!("Error: Cannot save BMP file: {}", e);
        std::process::exit(1);
    }
    
    println!("\nDone!");
}
