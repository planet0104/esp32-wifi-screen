use futures_lite::future::block_on;
use image::{Rgb, RgbImage};
use nusb::Interface;
use anyhow::Result;
use serialport::{SerialPort, SerialPortInfo, SerialPortType};

use crate::rgb565::rgb888_to_rgb565_be;

use std::time::{Instant, Duration};
use std::io::Write;

// READ_INF_MAGIC 占位常量（与 find_usb_scr.rs 中一致）
const READ_INF_MAGIC: u64 = 0x52656164496e666f;

// 尝试向指定串口发送探测信号并读取一行响应。
// 逻辑：先以 115200 波特打开端口并清空旧数据；先发送 magic（二进制），短时间等待是否有换行结尾的响应；
// 如果没有，再发送 ASCII 文本 "ReadInfo\n" 并在更长的 timeout_ms 内等待响应。
// 成功时返回第一行（不包括换行符），否则返回 None。
fn probe_port_for_line(port_name: &str, magic: u64, timeout_ms: u64) -> anyhow::Result<Option<String>> {
    use std::io::{Read, Write};
    let timeout = Duration::from_millis(timeout_ms);
    match serialport::new(port_name, 115200).timeout(Duration::from_millis(200)).open() {
        Ok(mut port) => {
            // 尝试清空任何挂起的数据
            let _ = port.read(&mut [0u8; 1024]);

            // 在 total_wait 时长内读取并返回首个换行分隔的行
            fn try_read(port: &mut dyn serialport::SerialPort, total_wait: Duration) -> Option<String> {
                let start = Instant::now();
                let mut buf: Vec<u8> = Vec::new();
                let mut tmp = [0u8; 256];
                while start.elapsed() < total_wait {
                    match port.read(&mut tmp) {
                        Ok(n) if n > 0 => {
                            buf.extend_from_slice(&tmp[..n]);
                            while let Some(pos) = buf.iter().position(|&b| b == b'\n') {
                                let line = String::from_utf8_lossy(&buf[..pos]).to_string();
                                return Some(line);
                            }
                        }
                        Ok(_) => {}
                        Err(ref e) if e.kind() == std::io::ErrorKind::TimedOut => {}
                        Err(_) => break,
                    }
                }
                None
            }

            // 先发送二进制 magic 并短时等待读取响应
            let _ = port.write_all(&magic.to_be_bytes());
            let _ = port.flush();
            if let Some(line) = try_read(&mut *port, Duration::from_millis(200)) {
                return Ok(Some(line));
            }

            // 再发送 ASCII 探针 ReadInfo\n，一些固件会对该探针做出响应
            let _ = port.write_all(b"ReadInfo\n");
            let _ = port.flush();
            if let Some(line) = try_read(&mut *port, timeout) {
                return Ok(Some(line));
            }

            Ok(None)
        }
        Err(_) => Ok(None),
    }
}

pub const BULK_OUT_EP: u8 = 0x01;
pub const BULK_IN_EP: u8 = 0x81;

pub fn open_usb_screen() -> Result<Option<Interface>>{
    let mut di = nusb::list_devices()?;
    for d in di{
        if d.serial_number().unwrap_or("").starts_with("USBSCR"){
            let device = d.open()?;
            let interface = device.claim_interface(0)?;
            return Ok(Some(interface));
        }
    }
    Ok(None)
}

// 兼容说明：
// - 新设备（推荐）：如果 USB 设备在底层暴露 serial_number 并以 "USBSCR" 开头，优先使用此信息识别设备，
//   然后通过 nusb 打开并 claim interface（直接走 USB raw 路径）。这种方式快速且不需要串口协议解析。
// - 旧设备或未暴露序列号的设备：回退使用串口探测逻辑，遍历所有可用串口，向串口发送 magic 二进制或 ASCII 探针 "ReadInfo\n",
//   并解析返回的文本行中是否包含 "ESP32-WIFI-SCREEN" 且包含 "PROTO:USB-SCREEN"。匹配则认为是 usb-screen 设备。
// - 可选扩展：probe 返回的行通常为格式 "ESP32-WIFI-SCREEN;{width};{height};PROTO:USB-SCREEN"，可解析出分辨率并用于后续显示操作。
// - 注意事项：probe 会短暂打开串口（115200）进行探测；若端口被占用或不响应，将被跳过，因此 probe 不是破坏性操作但可能被其他程序阻塞。

// 返回: Vec<(SerialPortInfo, Option<(width, height)>)>
pub fn find_usb_serial_device() -> Result<Vec<(SerialPortInfo, Option<(u16, u16)>)>>{
    // 第一步：优先查找那些在 USB 层暴露 serial_number 并以 "USBSCR" 开头的常见 USB-串口设备
    let ports: Vec<SerialPortInfo> = serialport::available_ports().unwrap_or(vec![]);
    let mut usb_screen: Vec<(SerialPortInfo, Option<(u16, u16)>)> = vec![];
    for p in &ports {
        match p.port_type.clone(){
            SerialPortType::UsbPort(port) => {
                if port.serial_number.unwrap_or("".to_string()).starts_with("USBSCR"){
                    println!("找到 usb-screen（通过 USB 序列号）: {} {:?}", p.port_name, p.port_type);
                    // 通过 USB 序列号发现时未获得分辨率信息，使用 None
                    usb_screen.push((p.clone(), None));
                }
            }
            _ => ()
        }
    }
    // 如果通过 USB 序列号找到了设备，则直接返回这些结果
    if !usb_screen.is_empty() {
        println!("find_usb_serial_device: returning {} devices (by serial)", usb_screen.len());
        // try to flush stdout for immediate visibility
        let _ = std::io::stdout().flush();
        return Ok(usb_screen);
    }

    // 回退方案：遍历所有串口，通过发送 magic 或 ASCII 探针并解析返回行来识别设备
    for p in &ports {
        // Only probe USB ports to avoid blocking on non-USB ports (Bluetooth, etc.)
        match &p.port_type {
            SerialPortType::UsbPort(_) => {
                println!("probing USB port {}...", p.port_name);
            }
            _ => {
                println!("skipping non-USB port {} ({:?})", p.port_name, p.port_type);
                let _ = std::io::stdout().flush();
                continue;
            }
        }
        let _ = std::io::stdout().flush();
        let _ = std::io::stdout().flush();
        if let Ok(Some(line)) = probe_port_for_line(&p.port_name, READ_INF_MAGIC, 800) {
            // 打印 probe 返回的行以便调试
            println!("探测到 {} -> {}", p.port_name, line);
            // 在返回行中查找不区分大小写的 "ESP32-WIFI-SCREEN" 子串并确认包含 "PROTO:USB-SCREEN"
            if let Some(pos) = line.to_uppercase().find("ESP32-WIFI-SCREEN") {
                let payload = &line[pos..];
                if payload.contains("PROTO:USB-SCREEN") {
                    // 尝试解析格式 ESP32-WIFI-SCREEN;{width};{height};PROTO:USB-SCREEN
                    let mut parsed_wh: Option<(u16, u16)> = None;
                    let parts: Vec<&str> = payload.split(';').collect();
                    if parts.len() >= 3 {
                        if let (Ok(w), Ok(h)) = (parts[1].parse::<u16>(), parts[2].parse::<u16>()) {
                            if w > 0 && h > 0 {
                                parsed_wh = Some((w, h));
                            }
                        }
                    }
                    if let Some((w,h)) = parsed_wh {
                        println!("找到 usb-screen（通过串口探测）: {} -> {} (解析到分辨率 {}x{})", p.port_name, payload, w, h);
                    } else {
                        println!("找到 usb-screen（通过串口探测）: {} -> {} (未解析到分辨率)", p.port_name, payload);
                    }
                    usb_screen.push((p.clone(), parsed_wh));
                }
            }
        } else {
            println!("no probe response from {}", p.port_name);
            let _ = std::io::stdout().flush();
        }
    }

    // 返回所有找到的串口设备（可能为空），每个条目包含可选的 (width, height)
    println!("find_usb_serial_device: returning {} devices (after probe)", usb_screen.len());
    let _ = std::io::stdout().flush();
    Ok(usb_screen)
}

pub fn clear_screen(color: Rgb<u8>, interface:&Interface, width: u16, height: u16) -> anyhow::Result<()>{
    let mut img = RgbImage::new(width as u32, height as u32);
    for p in img.pixels_mut(){
        *p = color;
    }
    draw_rgb_image(0, 0, &img, interface)
}

pub fn clear_screen_serial(color: Rgb<u8>, port:&mut dyn SerialPort, width: u16, height: u16) -> anyhow::Result<()>{
    let mut img = RgbImage::new(width as u32, height as u32);
    for p in img.pixels_mut(){
        *p = color;
    }
    draw_rgb_image_serial(0, 0, &img, port)
}

pub fn draw_rgb_image(x: u16, y: u16, img:&RgbImage, interface:&Interface) -> anyhow::Result<()>{
    //ST7789驱动使用的是Big-Endian
    let rgb565 = rgb888_to_rgb565_be(&img, img.width() as usize, img.height() as usize);
    draw_rgb565(&rgb565, x, y, img.width() as u16, img.height() as u16, interface)
}

pub fn draw_rgb565(rgb565:&[u8], x: u16, y: u16, width: u16, height: u16, interface:&Interface) -> anyhow::Result<()>{
    let rgb565_u8_slice = lz4_flex::compress_prepend_size(rgb565);

    const IMAGE_AA:u64 = 7596835243154170209;
    const BOOT_USB:u64 = 7093010483740242786;
    const IMAGE_BB:u64 = 7596835243154170466;

    let img_begin = &mut [0u8; 16];
    img_begin[0..8].copy_from_slice(&IMAGE_AA.to_be_bytes());
    img_begin[8..10].copy_from_slice(&width.to_be_bytes());
    img_begin[10..12].copy_from_slice(&height.to_be_bytes());
    img_begin[12..14].copy_from_slice(&x.to_be_bytes());
    img_begin[14..16].copy_from_slice(&y.to_be_bytes());
    // println!("draw:{x}x{y} {width}x{height}");

    block_on(interface.bulk_out(BULK_OUT_EP, img_begin.into())).status?;
    //读取
    // let result = block_on(interface.bulk_in(BULK_IN_EP, RequestBuffer::new(64))).data;
    // let msg = String::from_utf8(result)?;
    // println!("{msg}ms");

    block_on(interface.bulk_out(BULK_OUT_EP, rgb565_u8_slice.into())).status?;
    block_on(interface.bulk_out(BULK_OUT_EP, IMAGE_BB.to_be_bytes().into())).status?;
    Ok(())
}

pub fn draw_rgb_image_serial(x: u16, y: u16, img:&RgbImage, port:&mut dyn SerialPort) -> anyhow::Result<()>{
    //ST7789驱动使用的是Big-Endian
    let rgb565 = rgb888_to_rgb565_be(&img, img.width() as usize, img.height() as usize);
    draw_rgb565_serial(&rgb565, x, y, img.width() as u16, img.height() as u16, port)
}

pub fn draw_rgb565_serial(rgb565:&[u8], x: u16, y: u16, width: u16, height: u16, port:&mut dyn SerialPort) -> anyhow::Result<()>{
    let rgb565_u8_slice = lz4_flex::compress_prepend_size(rgb565);

    const IMAGE_AA:u64 = 7596835243154170209;
    const BOOT_USB:u64 = 7093010483740242786;
    const IMAGE_BB:u64 = 7596835243154170466;

    let img_begin = &mut [0u8; 16];
    img_begin[0..8].copy_from_slice(&IMAGE_AA.to_be_bytes());
    img_begin[8..10].copy_from_slice(&width.to_be_bytes());
    img_begin[10..12].copy_from_slice(&height.to_be_bytes());
    img_begin[12..14].copy_from_slice(&x.to_be_bytes());
    img_begin[14..16].copy_from_slice(&y.to_be_bytes());
    println!("[serial] header len=16, compressed payload len={} bytes", rgb565_u8_slice.len());

    port.write(img_begin)?;
    port.flush()?;
    println!("[serial] header sent");

    // write compressed payload in one go
    port.write(&rgb565_u8_slice)?;
    port.flush()?;
    println!("[serial] compressed payload sent");

    port.write(&IMAGE_BB.to_be_bytes())?;
    port.flush()?;
    println!("[serial] trailer sent");
    // After sending a frame, wait for device reply lines (DRAW_OK, FRAME_PARSED, ERROR:)
    let start = Instant::now();
    let mut resp_buf: Vec<u8> = Vec::new();
    let mut read_buf = [0u8; 256];
    while start.elapsed() < Duration::from_secs(8) {
        match port.read(&mut read_buf) {
            Ok(n) if n > 0 => {
                resp_buf.extend_from_slice(&read_buf[..n]);
                while let Some(pos) = resp_buf.iter().position(|&b| b == b'\n') {
                    let line = String::from_utf8_lossy(&resp_buf[..pos]).to_string();
                    resp_buf.drain(..=pos);
                    let ltrim = line.trim();
                    println!("device reply: {}", ltrim);
                    if ltrim.starts_with("DRAW_OK") || ltrim.starts_with("FRAME_PARSED") || ltrim.starts_with("ERROR:") {
                        return Ok(());
                    }
                }
            }
            Ok(_) => {}
            Err(ref e) if e.kind() == std::io::ErrorKind::TimedOut => {}
            Err(e) => { println!("Read error waiting for reply: {:?}", e); break; }
        }
    }
    println!("No DRAW_OK/FRAME_PARSED/ERROR reply within timeout");
    Ok(())
}

// 诊断用：发送一个 2x2 的确定性测试图案以便验证主机发送的 RGB565 字节
pub fn send_test_pattern_serial(port:&mut dyn SerialPort) -> anyhow::Result<()> {
    use image::Rgb;
    // 构造 2x2 测试图案：
    // (0,0) 红, (1,0) 绿
    // (0,1) 蓝, (1,1) 白
    let mut img = RgbImage::new(2, 2);
    img.put_pixel(0, 0, Rgb([255, 0, 0]));
    img.put_pixel(1, 0, Rgb([0, 255, 0]));
    img.put_pixel(0, 1, Rgb([0, 0, 255]));
    img.put_pixel(1, 1, Rgb([255, 255, 255]));

    // ST7789 使用 Big-Endian RGB565
    let rgb565 = crate::rgb565::rgb888_to_rgb565_be(&img, img.width() as usize, img.height() as usize);

    // 打印每个像素对应的两个字节，便于与设备端解压样本比对
    println!("[test] host raw rgb565 ({} bytes):", rgb565.len());
    for (i, chunk) in rgb565.chunks(2).enumerate() {
        if chunk.len() == 2 {
            println!("[test] pixel {}: {:02X}{:02X}", i, chunk[0], chunk[1]);
        }
    }

    // 发送并等待设备 ACK
    draw_rgb565_serial(&rgb565, 0u16, 0u16, 2u16, 2u16, port)?;
    Ok(())
}