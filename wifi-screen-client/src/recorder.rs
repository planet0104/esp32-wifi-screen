//启动录屏

//结束录屏

use std::{io::Cursor, net::TcpStream, sync::{Arc, Mutex}, time::{Duration, Instant}};
use anyhow::{anyhow, Result};
use fast_image_resize::{images::Image, Resizer};
use image::{buffer::ConvertBuffer, codecs::jpeg::JpegEncoder, RgbImage, RgbaImage};
use once_cell::sync::Lazy;
use tungstenite::{stream::MaybeTlsStream, WebSocket};
use xcap::Monitor;

use crate::{rgb565::rgb888_to_rgb565_be, show_alert_async, DisplayConfig};

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
        ImageFormat::JPG(30)
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
    pub ip: String,
    pub format: ImageFormat,
    pub display_config: DisplayConfig,
    pub monitor_width: i32,
    pub monitor_height: i32,
    pub delay_ms: u64,
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
    let mut monitor_width = 0;
    let mut monitor_height = 0;

    let mut sleep_duration = Duration::from_millis(3000);
    
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
    
                // ip地址变更，重新连接socket
                if (server_ip.len() > 0 && server_ip != config.ip) || server_ip.len() == 0{
                    recorder.websocket_status = Status::Disconnected;
                    let _ = socket.take();
                    server_ip = config.ip.clone();
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
    
                let soc = socket.as_mut().unwrap();
    
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
                let monitor_left = m.x();
                let monitor_top = m.y();
                let monitor_right = monitor_left + m.width() as i32;
                let monitor_bottom = monitor_top + m.height() as i32;
    
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
    
                let out = match &config.format{
                    ImageFormat::Rgb565Lz4Compressed | ImageFormat::RGB565 => {
                        let out = rgb888_to_rgb565_be(&img, img.width() as usize, img.height() as usize);
                        lz4_flex::compress_prepend_size(&out)
                    }
                    ImageFormat::JPG(quality) => {
                        let mut out = vec![];
                        let mut encoder = JpegEncoder::new_with_quality(&mut out, *quality);
                        if let Err(err) = encoder.encode_image(&img){
                            println!("jpg 编码失败:{err:?}");
                        }
                        out
                    }
                    ImageFormat::GIF | ImageFormat::PNG => {
                        let mut bytes: Vec<u8> = Vec::new();
                        if let Err(err) = img.write_to(&mut Cursor::new(&mut bytes), image::ImageFormat::Gif){
                            println!("gif 编码失败:{err:?}");
                        }
                        bytes
                    }
                };
    
                println!("类型{:?}:{}ms {}bytes {}x{}", config.format, t1.elapsed().as_millis(), out.len(), img.width(), img.height());
    
                //发送
                let ret1 = soc.write(tungstenite::Message::Binary(out.into()));
                let ret2 = soc.flush();
                if ret1.is_err() && ret2.is_err(){
                    recorder.websocket_status = Status::Disconnected;
                    let _ = socket.take();
                    sleep_duration = Duration::from_millis(3000);
                    continue;
                }
                sleep_duration = Duration::from_millis(config.delay_ms);
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
        if m.width() as i32 == width && m.height() as i32 == height{
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