use std::io::Cursor;
use std::thread::sleep;
use std::time::{Duration, Instant};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use anyhow::Result;
use image::{buffer::ConvertBuffer, imageops::resize, RgbImage, RgbaImage};

// ============ 配置参数 ============
// 帧间延迟（毫秒），防止 USB 缓冲区溢出
// ESP32 处理 240x240 图像约需 30-50ms，设置为 35ms 较为安全
const FRAME_DELAY_MS: u64 = 35;

pub fn draw(
    #[cfg(feature = "usb-serial")]
    port: &mut dyn serialport::SerialPort,
    #[cfg(feature = "usb-raw")]
    interface:&nusb::Interface,
    screen_width: u16,
    screen_height: u16,
) -> Result<()>{
    // 设置 Ctrl+C 信号处理，优雅退出
    let running = Arc::new(AtomicBool::new(true));
    let r = running.clone();
    ctrlc::set_handler(move || {
        println!("\nReceived Ctrl+C, stopping gracefully...");
        r.store(false, Ordering::SeqCst);
    }).expect("Error setting Ctrl-C handler");
    
    let file = Cursor::new(include_bytes!("../assets/tothesky.gif"));

    let mut gif_opts = gif::DecodeOptions::new();
    // Important:
    gif_opts.set_color_output(gif::ColorOutput::Indexed);
    
    let mut decoder = gif_opts.read_info(file)?;
    let mut screen = gif_dispose::Screen::new_decoder(&decoder);

    let mut frames = vec![];
    while let Some(frame) = decoder.read_next_frame()? {
        screen.blit_frame(&frame)?;
        let pixels = screen.pixels_rgba();
        let mut data = vec![];
        for pix in pixels{
            data.extend_from_slice(&[pix.r, pix.g, pix.b, pix.a]);
        }
        let img = RgbaImage::from_raw(screen.width() as u32, screen.height() as u32, data.to_vec()).unwrap();
        let rgb:RgbImage = img.convert();
        let rgb = resize(&rgb, screen_width as u32, screen_height as u32, image::imageops::FilterType::Triangle);
        frames.push(rgb);
    }
    
    println!("Loaded {} frames, starting playback (Ctrl+C to stop)...", frames.len());

    let mut counter: usize = 0;
    let start_time = Instant::now();
    
    while running.load(Ordering::SeqCst) {
        for frame in frames.iter(){
            // 检查是否需要停止
            if !running.load(Ordering::SeqCst) {
                break;
            }
            
            #[cfg(feature = "usb-serial")]
            crate::usb_screen::draw_rgb_image_serial(0, 0, frame, port)?;
            #[cfg(feature = "usb-raw")]
            crate::usb_screen::draw_rgb_image(0, 0, frame, interface)?;

            counter += 1;
            if counter % 30 == 0 {
                let elapsed = start_time.elapsed().as_secs_f32();
                let fps = counter as f32 / elapsed;
                println!("sent {} frames, {:.1} fps", counter, fps);
            }
            
            // 帧间延迟，防止 USB 缓冲区溢出
            sleep(Duration::from_millis(FRAME_DELAY_MS));
        }
    }
    
    println!("Stopped after {} frames", counter);
    Ok(())
}


#[test]
pub fn resize_gif() -> anyhow::Result<()>{
    use gif::{Encoder, Frame, Repeat};
    use image::{imageops::resize, RgbaImage};
    use image::GenericImage;
    let file = std::fs::File::open("assets/image.gif")?;

    let mut gif_opts = gif::DecodeOptions::new();
    // Important:
    gif_opts.set_color_output(gif::ColorOutput::Indexed);
    
    let mut decoder = gif_opts.read_info(file)?;
    let mut screen = gif_dispose::Screen::new_decoder(&decoder);

    let mut image = std::fs::File::create("assets/image1.gif")?;
    let mut encoder = Encoder::new(&mut image, 160, 128, &[])?;
    encoder.set_repeat(Repeat::Infinite)?;

    let mut i = 0;
    while let Some(frame) = decoder.read_next_frame()? {
        screen.blit_frame(&frame)?;
        let pixels = screen.pixels_rgba();
        let mut data = vec![];
        for pix in pixels{
            data.extend_from_slice(&[pix.r, pix.g, pix.b, pix.a]);
        }
        let img = RgbaImage::from_raw(screen.width() as u32, screen.height() as u32, data.to_vec()).unwrap();
        let mut img = resize(&img, 227, 128, image::imageops::FilterType::Lanczos3);
        let mut img = img.sub_image(33, 0, 160, 128).to_image();
        let mut frame = Frame::from_rgba(img.width() as u16, img.height() as u16, img.as_mut());
        frame.delay = 6;
        if i % 2 == 0{
            encoder.write_frame(&frame)?;
        }
        i+= 1;
    }
    Ok(())
}