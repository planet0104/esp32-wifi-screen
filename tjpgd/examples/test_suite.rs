//! Comprehensive test suite for tjpgd decoder
//! Demonstrates various usage scenarios and error handling

use tjpgd::{JpegDecoder, Error};

fn main() {
    println!("=== TJpgDec Rust Library Test Suite ===\n");
    
    let mut passed = 0;
    let mut failed = 0;
    
    // Test 1: Create decoder
    print!("Test 1: Create decoder... ");
    if test_create_decoder() {
        println!("✓ PASSED");
        passed += 1;
    } else {
        println!("✗ FAILED");
        failed += 1;
    }
    
    // Test 2: Invalid JPEG data
    print!("Test 2: Invalid JPEG data handling... ");
    if test_invalid_jpeg_data() {
        println!("✓ PASSED");
        passed += 1;
    } else {
        println!("✗ FAILED");
        failed += 1;
    }
    
    // Test 3: Empty data
    print!("Test 3: Empty data handling... ");
    if test_empty_data() {
        println!("✓ PASSED");
        passed += 1;
    } else {
        println!("✗ FAILED");
        failed += 1;
    }
    
    // Test 4: Buffer size calculation
    print!("Test 4: Buffer size calculation... ");
    if test_buffer_size_calculation() {
        println!("✓ PASSED");
        passed += 1;
    } else {
        println!("✗ FAILED");
        failed += 1;
    }
    
    // Test 5: Decompress with buffers
    print!("Test 5: Decompress with external buffers... ");
    if test_decompress() {
        println!("✓ PASSED");
        passed += 1;
    } else {
        println!("✗ FAILED");
        failed += 1;
    }
    
    // Test 6: Insufficient buffer
    print!("Test 6: Insufficient buffer handling... ");
    if test_insufficient_buffer() {
        println!("✓ PASSED");
        passed += 1;
    } else {
        println!("✗ FAILED");
        failed += 1;
    }
    
    // Test 7: Different scale factors
    print!("Test 7: Different scale factors... ");
    if test_different_scale_factors() {
        println!("✓ PASSED");
        passed += 1;
    } else {
        println!("✗ FAILED");
        failed += 1;
    }
    
    // Summary
    println!("\n=== Test Summary ===");
    println!("Passed: {}", passed);
    println!("Failed: {}", failed);
    println!("Total:  {}", passed + failed);
    
    if failed == 0 {
        println!("\n✓ All tests passed!");
    } else {
        println!("\n✗ Some tests failed!");
        std::process::exit(1);
    }
}

fn test_create_decoder() -> bool {
    let decoder = JpegDecoder::new();
    decoder.width() == 0 && decoder.height() == 0 && decoder.components() == 0
}

fn test_invalid_jpeg_data() -> bool {
    let invalid_data = vec![0xFF, 0x00, 0x01, 0x02]; // Invalid JPEG
    let mut decoder = JpegDecoder::new();
    
    let result = decoder.prepare(&invalid_data);
    result.is_err()
}

fn test_empty_data() -> bool {
    let empty_data: Vec<u8> = vec![];
    let mut decoder = JpegDecoder::new();
    
    let result = decoder.prepare(&empty_data);
    result.is_err()
}

fn test_buffer_size_calculation() -> bool {
    let jpeg_data = include_bytes!("../test_images/test1.jpg");
    let mut decoder = JpegDecoder::new();
    
    if decoder.prepare(jpeg_data).is_err() {
        return false;
    }
    
    let mcu_size = decoder.mcu_buffer_size();
    let work_size = decoder.work_buffer_size();
    
    // Validate sizes are reasonable
    mcu_size > 0 && work_size > 0 && mcu_size <= 10000 && work_size <= 100000
}

fn test_decompress() -> bool {
    let jpeg_data = include_bytes!("../test_images/test1.jpg");
    let mut decoder = JpegDecoder::new();
    
    if decoder.prepare(jpeg_data).is_err() {
        return false;
    }
    
    // Get required buffer sizes
    let mcu_size = decoder.mcu_buffer_size();
    let work_size = decoder.work_buffer_size();
    
    // Allocate buffers
    let mut mcu_buf = vec![0i16; mcu_size];
    let mut work_buf = vec![0u8; work_size];
    
    let mut callback_count = 0;
    let result = decoder.decompress(
        jpeg_data,
        0,
        &mut mcu_buf,
        &mut work_buf,
        &mut |_decoder, _bitmap, _rect| {
            callback_count += 1;
            Ok(true)
        },
    );
    
    result.is_ok() && callback_count > 0
}

fn test_insufficient_buffer() -> bool {
    let jpeg_data = include_bytes!("../test_images/test1.jpg");
    let mut decoder = JpegDecoder::new();
    
    if decoder.prepare(jpeg_data).is_err() {
        return false;
    }
    
    // Allocate buffers that are too small
    let mut mcu_buf = vec![0i16; 10];
    let mut work_buf = vec![0u8; 10];
    
    let result = decoder.decompress(
        jpeg_data,
        0,
        &mut mcu_buf,
        &mut work_buf,
        &mut |_decoder, _bitmap, _rect| Ok(true),
    );
    
    result.is_err() && result.unwrap_err() == Error::InsufficientMemory
}

fn test_different_scale_factors() -> bool {
    let jpeg_data = include_bytes!("../test_images/test1.jpg");
    
    for scale in 0..=3 {
        let mut decoder = JpegDecoder::new();
        if decoder.prepare(jpeg_data).is_err() {
            return false;
        }
        
        let orig_width = decoder.width();
        let orig_height = decoder.height();
        
        let mcu_size = decoder.mcu_buffer_size();
        let work_size = decoder.work_buffer_size();
        
        let mut mcu_buf = vec![0i16; mcu_size];
        let mut work_buf = vec![0u8; work_size];
        
        let mut dimensions_ok = true;
        let result = decoder.decompress(
            jpeg_data,
            scale,
            &mut mcu_buf,
            &mut work_buf,
            &mut |decoder, _bitmap, _rect| {
                // Verify scaled dimensions
                if decoder.width() != (orig_width >> scale) || 
                   decoder.height() != (orig_height >> scale) {
                    dimensions_ok = false;
                }
                Ok(true)
            },
        );
        
        if result.is_err() || !dimensions_ok {
            return false;
        }
    }
    
    true
}
