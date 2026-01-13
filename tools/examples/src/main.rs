use std::{thread::sleep, time::{Duration, Instant}};
use anyhow::Result;
use image::open;
use rgb565::rgb888_to_rgb565_le;
use usb_screen::find_usb_serial_device;
use log::{info, error};
mod rgb565;
mod rgb2yuv;
mod usb_screen;
mod draw_bitmap;
mod clock;
mod draw_gif;
mod reboot;
mod draw_png;
#[cfg(feature = "usb-serial")]
fn main() -> Result<()>{
    //配置envlogger打印nfo级别的日志
    env_logger::builder().filter_level(log::LevelFilter::Info).init();
    // test_serial()?;

    // use reboot::reboot_serial;
    // reboot_serial()?;
    
    info!("查找 usb screen...");
    let usb_screens = find_usb_serial_device()?;
    info!("找到 usb screen 数量: {}", usb_screens.len());

    if usb_screens.len() == 0{
        info!("没有找到 usb screen 设备");
        return Ok(());
    }
    info!("使用第一个设备进行绘制...");

    // 选择第一个找到的设备，若 probe 返回了分辨率则使用之，否则使用默认值
    let (port_info, maybe_wh) = &usb_screens[0];
    // Use high baud for bulk transfers where supported to avoid long waits over 115200
    let baud_rate = 2_000_000;
    info!("opening serial port {} at {} baud...", port_info.port_name, baud_rate);
    // 使用短超时（100ms）以便快速轮询读取回复，避免阻塞
    let mut screen = serialport::new(&port_info.port_name, baud_rate)
        .timeout(Duration::from_millis(100))
        .open()?;

    let (width, height) = match maybe_wh {
        Some((w,h)) => (*w, *h),
        None => (160u16, 128u16),
    };
    // let width = 320;
    // let height = 240;

    info!("使用设备: {} (分辨率 {}x{})", port_info.port_name, width, height);
    info!("开始绘制...");
    // draw_png::draw(screen.as_mut(), width, height)?;
    draw_gif::draw(screen.as_mut(), width, height)?;
    info!("绘制完成");

    // sleep(Duration::from_secs(2));

    // // clock::draw(screen.as_mut(), width, height)?;

    // draw_gif::draw(screen.as_mut(), width, height)?;

    Ok(())
}

#[cfg(feature = "usb-raw")]
fn main() -> Result<()>{
    // use reboot::reboot_usb_raw;
    // reboot_usb_raw()?;

    info!("open usb usb screen...");
    let interface = usb_screen::open_usb_screen()?.unwrap();
    info!("open usb usb OK number:{}", interface.interface_number());

    let width = 160;
    let height = 128;

    // draw_bitmap::draw(&interface, width, height)?;

    // sleep(Duration::from_millis(2));

    // clock::draw(screen.as_mut(), width, height)?;

    draw_gif::draw(&interface, width, height)?;

    Ok(())
}

fn lz4test() -> Result<()> {
    use lz4_flex::compress_prepend_size;
    let img = open("./assets/rgb24.bmp")?.to_rgb8();
    println!("图像大小:{}x{}", img.width(), img.height());
    let rgb565 = rgb888_to_rgb565_le(&img, img.width() as usize, img.height() as usize);
    println!("rgb565:{}字节", rgb565.len());
    let result = compress_prepend_size(&rgb565);
    
    println!("压缩后:{}字节", result.len());

    std::fs::write("assets/127x64_le.lz4", &result)?;

    Ok(())
}

fn test_serial() -> Result<()>{
    let usb_screens = find_usb_serial_device()?;

    if usb_screens.len() == 0{
        return Ok(());
    }
    
    let (port_info, _maybe_wh) = &usb_screens[0];
    let mut screen = serialport::new(&port_info.port_name, 115_200).open()?;

    let img = open("./assets/320x240.png")?.to_rgb8();
    let t = Instant::now();

    for _ in 0..13{
        usb_screen::draw_rgb_image_serial(0, 0, &img, screen.as_mut())?;
    }

    info!("{}ms", t.elapsed().as_millis());
    Ok(())
}

fn test_usb() -> Result<()> {
    let interface = usb_screen::open_usb_screen()?.unwrap();

    let img = open("./assets/160x128.png")?.to_rgb8();
    let t = Instant::now();

    for _ in 0..40{
        usb_screen::draw_rgb_image(0, 0, &img, &interface)?;
    }
    info!("{}ms", t.elapsed().as_millis());

    Ok(())
}