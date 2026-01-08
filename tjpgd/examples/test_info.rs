use tjpgd::{JpegDecoder, Result};

fn main() -> Result<()> {
    let jpeg_data: &[u8] = include_bytes!("../test_images/test1.jpg");
    let mut decoder = JpegDecoder::new();
    decoder.prepare(jpeg_data)?;
    
    println!("Width: {}", decoder.width());
    println!("Height: {}", decoder.height());
    println!("Components: {}", decoder.components());
    
    Ok(())
}
