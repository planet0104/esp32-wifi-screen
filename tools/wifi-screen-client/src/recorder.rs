//启动录屏

//结束录屏

use std::{io::{Cursor, Write}, net::TcpStream, sync::{Arc, Mutex}, time::{Duration, Instant}};
use anyhow::{anyhow, Result};
use fast_image_resize::{images::Image, Resizer};
use image::{buffer::ConvertBuffer, codecs::jpeg::JpegEncoder, RgbImage, RgbaImage};
use once_cell::sync::Lazy;
use serialport::SerialPort;
use tungstenite::{stream::MaybeTlsStream, WebSocket};
use xcap::Monitor;

use crate::{rgb565::rgb888_to_rgb565_be, show_alert_async, usb_serial, DisplayConfig};

// WiFi帧差分协议 Magic Numbers (8字节)
// 格式: MAGIC(8) + WIDTH(2) + HEIGHT(2) + LZ4_COMPRESSED_DATA
const WIFI_KEY_MAGIC: &[u8; 8] = b"wflz4ke_"; // lz4压缩的关键帧(完整RGB565)
const WIFI_DLT_MAGIC: &[u8; 8] = b"wflz4dl_"; // lz4压缩的差分帧(XOR差分数据)
const WIFI_NOP_MAGIC: &[u8; 8] = b"wflz4no_"; // 无变化帧(屏幕静止，跳过绘制)

// 无变化帧阈值：压缩后小于此大小认为画面没变化
const NO_CHANGE_THRESHOLD: usize = 200;

// WiFi帧差分编码器
// 用于在上位机端对RGB565数据进行帧差分+lz4压缩
pub struct DeltaEncoder {
    prev_frame: Vec<u8>,       // 上一帧RGB565数据
    frame_count: u32,          // 帧计数
    key_frame_interval: u32,   // 关键帧间隔(默认60帧)
}

impl DeltaEncoder {
    pub fn new(key_frame_interval: u32) -> Self {
        Self {
            prev_frame: Vec::new(),
            frame_count: 0,
            key_frame_interval,
        }
    }

    // 编码一帧RGB565数据
    // 返回: (编码后的数据, 是否为关键帧)
    pub fn encode(&mut self, rgb565_data: &[u8], width: u16, height: u16) -> (Vec<u8>, bool) {
        let need_key_frame = self.prev_frame.len() != rgb565_data.len()
            || self.frame_count == 0
            || self.frame_count % self.key_frame_interval == 0;

        if need_key_frame {
            // 关键帧: 直接压缩完整数据
            let compressed = self.lz4_compress(rgb565_data);
            
            // 构建帧数据: MAGIC + WIDTH + HEIGHT + COMPRESSED_DATA
            let mut frame = Vec::with_capacity(12 + compressed.len());
            frame.extend_from_slice(WIFI_KEY_MAGIC);
            frame.extend_from_slice(&width.to_be_bytes());
            frame.extend_from_slice(&height.to_be_bytes());
            frame.extend_from_slice(&compressed);
            
            // 保存当前帧作为参考帧
            self.prev_frame = rgb565_data.to_vec();
            self.frame_count = self.frame_count.wrapping_add(1);
            
            (frame, true)
        } else {
            // 差分帧: 计算XOR差分并压缩
            let delta: Vec<u8> = rgb565_data.iter()
                .zip(self.prev_frame.iter())
                .map(|(curr, prev)| curr ^ prev)
                .collect();
            
            let compressed_delta = self.lz4_compress(&delta);
            
            // 如果压缩后数据很小，说明画面几乎没变化，发送无变化帧
            if compressed_delta.len() < NO_CHANGE_THRESHOLD {
                // 发送无变化帧，ESP32收到后直接返回ACK，不做解码和绘制
                let mut frame = Vec::with_capacity(12);
                frame.extend_from_slice(WIFI_NOP_MAGIC);
                frame.extend_from_slice(&width.to_be_bytes());
                frame.extend_from_slice(&height.to_be_bytes());
                
                // 不更新参考帧和计数（画面没变）
                self.frame_count = self.frame_count.wrapping_add(1);
                
                (frame, false)
            } else {
                let compressed_key = self.lz4_compress(rgb565_data);
                
                // 如果差分帧比关键帧还大，使用关键帧
                if compressed_delta.len() >= compressed_key.len() {
                    let mut frame = Vec::with_capacity(12 + compressed_key.len());
                    frame.extend_from_slice(WIFI_KEY_MAGIC);
                    frame.extend_from_slice(&width.to_be_bytes());
                    frame.extend_from_slice(&height.to_be_bytes());
                    frame.extend_from_slice(&compressed_key);
                    
                    self.prev_frame = rgb565_data.to_vec();
                    self.frame_count = self.frame_count.wrapping_add(1);
                    
                    (frame, true)
                } else {
                    // 使用差分帧
                    let mut frame = Vec::with_capacity(12 + compressed_delta.len());
                    frame.extend_from_slice(WIFI_DLT_MAGIC);
                    frame.extend_from_slice(&width.to_be_bytes());
                    frame.extend_from_slice(&height.to_be_bytes());
                    frame.extend_from_slice(&compressed_delta);
                    
                    // 更新参考帧
                    self.prev_frame = rgb565_data.to_vec();
                    self.frame_count = self.frame_count.wrapping_add(1);
                    
                    (frame, false)
                }
            }
        }
    }

    // 重置编码器状态
    pub fn reset(&mut self) {
        self.prev_frame.clear();
        self.frame_count = 0;
    }

    // lz4压缩 (比zstd解压快5-10倍)
    fn lz4_compress(&self, data: &[u8]) -> Vec<u8> {
        lz4_flex::compress_prepend_size(data)
    }
}

#[allow(unused)]
#[derive(Debug, Clone)]
pub enum ImageFormat{
    Rgb565Lz4Compressed,
    RGB565,
    JPG(u8),
    PNG,
    GIF
}

impl Default for ImageFormat{
    fn default() -> Self {
        ImageFormat::RGB565
    }
}

#[derive(Debug, Clone)]
pub enum Status{
    Connected,
    ConnectFail,
    Disconnected,
    Connecting,
}

#[derive(Clone, Debug)]
pub struct RecorderConfig{
    pub target: OutputTarget,
    pub format: ImageFormat,
    pub display_config: DisplayConfig,
    pub monitor_width: i32,
    pub monitor_height: i32,
    pub delay_ms: u64,
}

#[derive(Clone, Debug)]
pub enum OutputTarget {
    Wifi { ip: String },
    UsbSerial { port_name: String },
}

pub struct Recorder{
    pub config: Option<RecorderConfig>,
    pub monitor_status: Status,
    pub websocket_status: Status,
    pub pointer_image: RgbaImage,
}

static RECORDER: Lazy<Arc<Mutex<Recorder>>> = Lazy::new(|| {
    let cfg = Arc::new(Mutex::new(Recorder{
        config:None,
        monitor_status: Status::Disconnected,
        websocket_status: Status::Disconnected,
        pointer_image: image::load_from_memory(include_bytes!("../mouse_pointer.png")).unwrap().to_rgba8(),
    }));
    let cfg_clone = cfg.clone();
    std::thread::spawn(move ||{
        run_recorder(cfg_clone)
    });
    cfg
});

pub fn start_with_config_alert(config: RecorderConfig){
    println!("启动录屏:{:?}...", config);
    std::thread::spawn(move ||{
        if let Err(err) = set_config_sync(Some(config)){
            show_alert_async(&format!("启动失败:{}", err.root_cause()));
        }else{
            println!("启动录屏成功..");
        }
    });
}

pub fn set_config_sync(config: Option<RecorderConfig>) -> Result<()>{
    println!("set_config_sync 001.");
    let mut recorder = RECORDER.lock().map_err(|err| anyhow!("{err:?}"))?;
    recorder.config = config;
    println!("set_config_sync 002.");
    Ok(())
}

pub fn get_status_sync() -> Result<(Status, Status)>{
    let recorder = RECORDER.try_lock().map_err(|err| anyhow!("{err:?}"))?;
    Ok((recorder.monitor_status.clone(), recorder.websocket_status.clone()))
}

fn run_recorder(recorder: Arc<Mutex<Recorder>>) -> !{
    let mut monitor = None;
    let mut socket: Option<WebSocket<MaybeTlsStream<TcpStream>>> = None;
    let mut server_ip = String::new();
    let mut serial_port: Option<Box<dyn SerialPort>> = None;
    let mut serial_port_name = String::new();
    let mut monitor_width = 0;
    let mut monitor_height = 0;

    let mut sleep_duration = Duration::from_millis(3000);
    
    // WiFi帧差分编码器 (关键帧间隔60帧)
    let mut delta_encoder = DeltaEncoder::new(60);
    
    loop{
        //尝试锁定，锁定失败延迟
        // println!("recorder loop...");
        std::thread::sleep(sleep_duration);

        {
            if let Ok(mut recorder) = recorder.lock() {
                //更新状态
                let config = match recorder.config.clone(){
                    None => {
                        println!("没有配置...");
                        //配置删除，结束录制
                        recorder.monitor_status = Status::Disconnected;
                        recorder.websocket_status = Status::Disconnected;
                        let _ = monitor.take();
                        let _ = socket.take();
                        sleep_duration = Duration::from_millis(3000);
                        continue;
                    }
                    Some(c) => c
                };
    
                // Ensure output connection is ready for current target
                match &config.target {
                    OutputTarget::Wifi { ip } => {
                        // Switching target -> drop serial
                        if serial_port.is_some() {
                            let _ = serial_port.take();
                            serial_port_name.clear();
                        }
                        // ip地址变更，重新连接socket
                        if (server_ip.len() > 0 && server_ip != *ip) || server_ip.is_empty() {
                            recorder.websocket_status = Status::Disconnected;
                            let _ = socket.take();
                            server_ip = ip.clone();
                            // 重置帧差分编码器，确保重新连接后发送关键帧
                            delta_encoder.reset();
                            println!("更新了IP:{server_ip}...");
                            sleep_duration = Duration::from_millis(3000);
                            continue;
                        }

                        if socket.is_none(){
                            //连接socket
                            recorder.websocket_status = Status::Connecting;
                            let url = format!("ws://{server_ip}/ws");
                            println!("开始连接:{url}");
                            if let Ok((s, _resp)) = tungstenite::connect(url){
                                socket = Some(s);
                                recorder.websocket_status = Status::Connected;
                                println!("连接成功{server_ip}..");
                            }else{
                                recorder.websocket_status = Status::ConnectFail;
                                println!("连接失败{server_ip}..");
                                let _ = socket.take();
                                sleep_duration = Duration::from_millis(3000);
                                continue;
                            }
                        }
                    }
                    OutputTarget::UsbSerial { port_name } => {
                        // Switching target -> drop websocket
                        if socket.is_some() {
                            recorder.websocket_status = Status::Disconnected;
                            let _ = socket.take();
                            server_ip.clear();
                        }
                        if serial_port_name != *port_name || serial_port.is_none() {
                            recorder.websocket_status = Status::Connecting;
                            serial_port_name = port_name.clone();
                            match usb_serial::open_screen_serial(&serial_port_name) {
                                Ok(p) => {
                                    serial_port = Some(p);
                                    recorder.websocket_status = Status::Connected;
                                    println!("串口已连接: {serial_port_name}");
                                }
                                Err(err) => {
                                    recorder.websocket_status = Status::ConnectFail;
                                    eprintln!("串口连接失败({serial_port_name}): {}", err.root_cause());
                                    let _ = serial_port.take();
                                    sleep_duration = Duration::from_millis(3000);
                                    continue;
                                }
                            }
                        }
                    }
                }
    
                // 显示变更，重新连接显示器
                let m = match 
                if monitor_width != config.monitor_width || monitor_height != config.monitor_height{
                    monitor_width = config.monitor_width;
                    monitor_height = config.monitor_height;
                    monitor = find_monitor(monitor_width, monitor_height);
                    monitor.as_ref()
                }else{
                    monitor.as_ref()
                }{
                    None => {
                        println!("monitor未找到...");
                        recorder.monitor_status = Status::Disconnected;
                        sleep_duration = Duration::from_millis(3000);
                        continue;
                    }
                    Some(m) => {
                        recorder.monitor_status = Status::Connected;
                        m
                    }
                };
    
                //尝试截屏，截屏失败后重新连接显示器
                let mut image = match m.capture_image(){
                    Ok(img) => img,
                    Err(_err) => {
                        println!("monitor截图失败...");
                        recorder.monitor_status = Status::Disconnected;
                        let _ = monitor.take();
                        sleep_duration = Duration::from_millis(3000);
                        continue;
                    }
                };
    
                //压缩
                let monitor_left = m.x().unwrap_or(0) as i32;
                let monitor_top = m.y().unwrap_or(0) as i32;
                let monitor_right = monitor_left + m.width().unwrap_or(0) as i32;
                let monitor_bottom = monitor_top + m.height().unwrap_or(0) as i32;
    
                let position = mouse_position::mouse_position::Mouse::get_mouse_position();
                let (mouse_x, mouse_y) = match position {
                    mouse_position::mouse_position::Mouse::Position { x, y } => {
                        if x >= monitor_left && x<monitor_right
                        && y >= monitor_top && y<monitor_bottom{
                            ( x - monitor_left, y - monitor_top )
                        }else{
                            (-1, -1)
                        }
                    },
                    mouse_position::mouse_position::Mouse::Error => {
                        (-1, -1)
                    }
                };
                
                if mouse_x > 0 && mouse_y > 0{
                    image::imageops::overlay(&mut image, &recorder.pointer_image, mouse_x as i64, mouse_y as i64);
                }
    
                let t1 = Instant::now();
    
                let (dst_width, dst_height) = (config.display_config.rotated_width, config.display_config.rotated_height);
                
                let img = match fast_resize(&mut image, dst_width, dst_height){
                    Ok(v) => v,
                    Err(err) => {
                        eprintln!("图片压缩失败:{}", err.root_cause());
                        continue;
                    }
                };
    
                // Encode payload based on target
                let (out, is_key_frame) = match &config.target {
                    OutputTarget::UsbSerial { .. } => {
                        // USB serial path only supports RGB565 + LZ4 (device protocol)
                        // USB传输不使用帧差分，保持原有协议不变
                        let rgb565 = rgb888_to_rgb565_be(&img, img.width() as usize, img.height() as usize);
                        (lz4_flex::compress_prepend_size(&rgb565), true)
                    }
                    OutputTarget::Wifi { .. } => {
                        match &config.format{
                            ImageFormat::Rgb565Lz4Compressed | ImageFormat::RGB565 => {
                                // 使用帧差分+lz4压缩 (WiFi传输优化)
                                let rgb565 = rgb888_to_rgb565_be(&img, img.width() as usize, img.height() as usize);
                                delta_encoder.encode(&rgb565, dst_width as u16, dst_height as u16)
                            }
                            ImageFormat::JPG(quality) => {
                                // JPG格式不使用帧差分
                                let mut out = vec![];
                                let mut encoder = JpegEncoder::new_with_quality(&mut out, *quality);
                                if let Err(err) = encoder.encode_image(&img){
                                    println!("jpg 编码失败:{err:?}");
                                }
                                (out, true)
                            }
                            ImageFormat::GIF | ImageFormat::PNG => {
                                // GIF/PNG格式不使用帧差分
                                let mut bytes: Vec<u8> = Vec::new();
                                if let Err(err) = img.write_to(&mut Cursor::new(&mut bytes), image::ImageFormat::Gif){
                                    println!("gif 编码失败:{err:?}");
                                }
                                (bytes, true)
                            }
                        }
                    }
                };
    
                let frame_type = if is_key_frame { "KEY" } else { "DLT" };
                let encode_ms = t1.elapsed().as_millis();
                let out_bytes = out.len(); // 记录发送字节数
                
                // 记录发送开始时间
                let send_start = Instant::now();
    
                //发送
                let send_ok = match &config.target {
                    OutputTarget::Wifi { .. } => {
                        let soc = socket.as_mut().unwrap();
                        
                        // 设置读取超时3秒 (用于等待ACK)
                        if let tungstenite::stream::MaybeTlsStream::Plain(stream) = soc.get_mut() {
                            let _ = stream.set_read_timeout(Some(Duration::from_secs(3)));
                        }
                        
                        // 发送帧
                        let ret1 = soc.write(tungstenite::Message::Binary(out.into()));
                        let ret2 = soc.flush();
                        if ret1.is_err() || ret2.is_err() {
                            false
                        } else {
                            // 等待ACK确认 (只对帧差分协议，即RGB565格式)
                            let mut ack_ok = true;
                            if matches!(config.format, ImageFormat::Rgb565Lz4Compressed | ImageFormat::RGB565) {
                                // 等待ESP32的ACK/NACK响应 (3秒超时)
                                match soc.read() {
                                    Ok(msg) => {
                                        match msg {
                                            tungstenite::Message::Text(text) => {
                                                if text == "NACK" {
                                                    // 收到NACK，重置编码器，下一帧发送关键帧
                                                    println!("收到NACK，重置编码器");
                                                    delta_encoder.reset();
                                                }
                                                // ACK或其他消息都继续
                                            }
                                            tungstenite::Message::Close(_) => {
                                                ack_ok = false;
                                            }
                                            _ => {
                                                // 忽略其他消息类型(Ping/Pong等)
                                            }
                                        }
                                    }
                                    Err(e) => {
                                        // 超时或连接错误，重置编码器发送关键帧
                                        eprintln!("等待ACK超时/失败: {}，重置编码器", e);
                                        delta_encoder.reset();
                                        // 超时不算连接断开，继续尝试
                                    }
                                }
                            }
                            ack_ok
                        }
                    }
                    OutputTarget::UsbSerial { .. } => {
                        match serial_port.as_mut() {
                            Some(p) => usb_serial::send_lz4_rgb565_frame(
                                p.as_mut(),
                                0,
                                0,
                                config.display_config.rotated_width as u16,
                                config.display_config.rotated_height as u16,
                                &out,
                            )
                            .is_ok(),
                            None => false,
                        }
                    }
                };
                
                let send_ms = send_start.elapsed().as_millis();
                let total_ms = t1.elapsed().as_millis();
                println!("[FRAME] type={} {}x{} bytes={} encode={}ms send+ack={}ms total={}ms", 
                    frame_type, img.width(), img.height(), out_bytes, encode_ms, send_ms, total_ms);
                
                if !send_ok {
                    recorder.websocket_status = Status::Disconnected;
                    let _ = socket.take();
                    let _ = serial_port.take();
                    // 发送失败，重置帧差分编码器
                    delta_encoder.reset();
                    sleep_duration = Duration::from_millis(3000);
                    continue;
                }
                // ACK确认后无需额外延迟，立即截取下一帧
                sleep_duration = Duration::from_millis(1);
            }
        }
    }
}

fn find_monitor(width: i32, height: i32) -> Option<Monitor>{
    //找到显示器
    let monitors = match Monitor::all(){
        Err(_err) => return None,
        Ok(list) => list
    };
    let mut find_monitor = None;
    for m in monitors{
        println!("检测显示器:{}x{}", m.width().unwrap_or(0), m.height().unwrap_or(0));
        if m.width().unwrap_or(0) as i32 == width && m.height().unwrap_or(0) as i32 == height{
            find_monitor = Some(m);
            break;
        }
    }
    find_monitor
}

fn fast_resize(src: &mut RgbaImage, dst_width: u32, dst_height: u32) -> Result<RgbImage>{
    let mut dst_image = Image::new(
        dst_width,
        dst_height,
        fast_image_resize::PixelType::U8x3,
    );
    let mut src:RgbImage = src.convert();
    if src.width() != dst_width || src.height() != dst_height{
        let v = Image::from_slice_u8(src.width(), src.height(), src.as_mut(), fast_image_resize::PixelType::U8x3)?;
        let mut resizer = Resizer::new();
        resizer.resize(&v, &mut dst_image, None)?;
        Ok(RgbImage::from_raw(dst_image.width(), dst_image.height(), dst_image.buffer().to_vec()).unwrap())
    }else{
        Ok(src.convert())
    }
}