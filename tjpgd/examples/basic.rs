//! Example usage of tjpgd decoder (Memory-efficient version)

use tjpgd::{JpegDecoder, Result};

fn main() -> Result<()> {
    // Example JPEG data (use test1.jpg from test_images)
    let jpeg_data = include_bytes!("../test_images/test1.jpg");

    // Create decoder
    let mut decoder = JpegDecoder::new();

    // Prepare (parse headers)
    decoder.prepare(jpeg_data)?;

    println!("Image size: {}x{}", decoder.width(), decoder.height());
    println!("Components: {}", decoder.components());

    // Get required buffer sizes
    let mcu_size = decoder.mcu_buffer_size();
    let work_size = decoder.work_buffer_size();
    
    println!("MCU buffer size: {} (i16 elements)", mcu_size);
    println!("Work buffer size: {} bytes", work_size);

    // Allocate external buffers (memory-efficient approach)
    let mut mcu_buffer = vec![0i16; mcu_size];
    let mut work_buffer = vec![0u8; work_size];
    let mut output_buffer = Vec::new();

    // Decompress with external buffers
    decoder.decompress_with_buffers(
        jpeg_data, 
        0,  // scale = 0 (no scaling)
        &mut mcu_buffer,
        &mut work_buffer,
        &mut |_decoder, bitmap, rect| {
            println!(
                "Received block: ({}, {}) to ({}, {})",
                rect.left, rect.top, rect.right, rect.bottom
            );

            // In a real application, you would write bitmap data to display or file
            output_buffer.extend_from_slice(bitmap);

            Ok(true) // Continue processing
        }
    )?;

    println!("Decompression complete!");
    println!("Total output size: {} bytes", output_buffer.len());
    Ok(())
}
