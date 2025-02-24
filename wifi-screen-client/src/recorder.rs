use std::{net::Ipv4Addr, str::FromStr, sync::RwLock, time::Duration};

use anyhow::{anyhow, Result};
use ini::Ini;
use once_cell::sync::Lazy;
use uuid::Uuid;
use xcap::Monitor;

use crate::{show_alert_async, uploader::{self, ImageFormat, SendImage}, CONFIG_FILE_NAME};

#[derive(Default)]
pub struct Config{
    pub monitor: Option<Monitor>,
    // pub recorder: Option<Arc<VideoRecorder>>,
    pub recorder: Option<String>,
    pub format: ImageFormat,
}

unsafe impl Sync for Config{}
unsafe impl Send for Config {}

pub static CONFIG: Lazy<RwLock<Config>> = Lazy::new(|| {
    RwLock::new(Config::default())
});

pub fn start_with_config_alert(format: ImageFormat){
    println!("启动录屏:{:?}...",format);
    std::thread::spawn(move ||{
        if let Err(err) = start_with_config_sync(format.clone()){
            show_alert_async(&format!("启动失败:{}", err.root_cause()));
        }else{
            println!("启动录屏成功:{:?}",format);
        }
    });
}

/// 读取配置文件，并开始录制屏幕
pub fn start_with_config_sync(format: ImageFormat) -> Result<()>{
    let conf = Ini::load_from_file(CONFIG_FILE_NAME)?;
    let screen_width = match conf.get_from(None::<String>, "screen_width"){
        None => {
            return Err(anyhow!("配置文件缺少screen_width"));
        }
        Some(v) => v
    };
    let screen_height = match conf.get_from(None::<String>, "screen_height"){
        None => {
            return Err(anyhow!("配置文件缺少screen_height"));
        }
        Some(v) => v
    };
    let ip = match conf.get_from(None::<String>, "ip"){
        None => {
            return Err(anyhow!("配置文件缺少ip"));
        }
        Some(v) => v
    };
    let delay_ms = match conf.get_from(None::<String>, "delay_ms"){
        None => {
            150
        }
        Some(v) => {
            match v.parse::<u64>(){
                Ok(v) => v,
                Err(_) => 150,
            }
        }
    };
    let _ = Ipv4Addr::from_str(&ip)?;
    let width: u32 = screen_width.parse()?;
    let height: u32 = screen_height.parse()?;
    uploader::send_message(uploader::Message::SetIp(ip.to_string()))?;
    uploader::set_delay_ms(delay_ms)?;
    //找到显示器
    let monitors = Monitor::all()?;
    let mut find_monitor = None;
    for m in monitors{
        if m.width() == width && m.height() == height{
            find_monitor = Some(m);
            break;
        }
    }
    let m = match find_monitor{
        None => return Err(anyhow!("未找到{width}x{height}分辨率的显示器")),
        Some(m) => m
    };
    println!("启动录屏...");
    start_record(Some(m), format)?;
    println!("录屏启动成功.");
    Ok(())
}

pub fn start_record(monitor: Option<Monitor>, format: ImageFormat) -> Result<()>{
    //先结束原有录制
    let _ = stop_record();
    let uuid = Uuid::new_v4().to_string();
    {
        CONFIG.write().map_err(|err| anyhow!("{err:?}"))?.recorder = Some(uuid.clone());
    }

    let mut config = CONFIG.write().map_err(|err| anyhow!("{err:?}"))?;
    config.format = format;
    if let Some(monitor) = monitor{
        open_recorder(&monitor, uuid, config.format.clone())?;
        config.monitor = Some(monitor.clone());
    }else{
        //启动原有的monitor
        if let Some(monitor) = config.monitor.as_ref(){
            open_recorder(monitor, uuid, config.format.clone())?;
        }else{
            return Err(anyhow!("未设置显示器!"));
        }
    }
    Ok(())
}

pub fn stop_record() -> Result<()>{
    println!("锁定CONFIG...");
    let mut config = CONFIG.write().map_err(|err| anyhow!("{err:?}"))?;
    if let Some(_) = config.recorder.take(){
        // r.stop()?;
    }
    println!("结束录制 OK.");
    Ok(())
}

fn open_recorder(monitor: &Monitor, uuid: String, format: ImageFormat) -> Result<()>{
    println!("显示器大小:{}x{} {}x{}", monitor.x(), monitor.y(), monitor.width(), monitor.height());
    // let ip = "192.168.121.226";
    // let url = format!("ws://{ip}/ws");
    // println!("开始连接:{url}");
    // let mut socket = if let Ok((s, _resp)) = connect(url){
    //     // let _ = set_status(None, Status::Connected);
    //     println!("连接成功{ip}..");
    //     s
    // }else{
    //     println!("连接失败{ip}..");
    //     // let _ = set_status(None, Status::ConnectFail);
    //     return "".to_string();
    // };

    // let video_recorder = Arc::new(monitor.video_recorder()?);
    // let video_recorder_clone = video_recorder.clone();
    let monitor_left = monitor.x();
    let monitor_top = monitor.y();
    let monitor_right = monitor_left + monitor.width() as i32;
    let monitor_bottom = monitor_top + monitor.height() as i32;
    
    std::thread::spawn(move ||{
        println!("启动录屏线程...");
        loop{
            if let Ok(cfg) = CONFIG.try_read(){
                match cfg.recorder.as_ref(){
                    Some(id)=>{
                        if id != &uuid{
                            eprintln!("uuid不符, 线程结束!");
                            break;
                        }
                    }
                    None => {
                        std::thread::sleep(Duration::from_millis(100));
                        continue;
                    }
                }
                if let Some(monitor) = cfg.monitor.as_ref(){
                    if let Ok(image) = monitor.capture_image(){
                        let position = mouse_position::mouse_position::Mouse::get_mouse_position();
                        let (x, y) = match position {
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
                        let _ = uploader::send_message(uploader::Message::Image(SendImage{
                            image, mouse_x: x, mouse_y: y, format: format.clone()
                        }));
                        continue;
                    }
                }
            }
            std::thread::sleep(Duration::from_millis(5));
        }

        /*
        let _ = video_recorder_clone.on_frame(move |f|{
            
            let stride = f.raw.len() / f.height as usize;
            let mut buffer = Vec::with_capacity(f.width as usize*f.height as usize * 4);
            for row in f.raw.chunks(stride){
                let row_buf_len = f.width as usize * 4;
                if row.len() >= row_buf_len{
                    buffer.extend_from_slice(&row[0..row_buf_len]);
                }
            }
            let img = match RgbaImage::from_raw(f.width, f.height, buffer){
                None => {
                    return Ok(());
                },
                Some(f) => f
            };
            // println!("录屏数据:{}x{}", img.width(), img.height());
            let position = mouse_position::mouse_position::Mouse::get_mouse_position();
            match position {
                mouse_position::mouse_position::Mouse::Position { x, y } => {
                    if x >= monitor_left && x<monitor_right
                    && y >= monitor_top && y<monitor_bottom{
                        let (x, y) = ( x - monitor_left, y - monitor_top );
                        println!("鼠标在显示器区域!! {x}x{y}");
                    }
                },
                mouse_position::mouse_position::Mouse::Error => println!("Error getting mouse position"),
            }
            let _ = uploader::try_send_message(uploader::Message::Image(img));
            Ok(())
        });
         */
        println!("录屏线程结束...");
    });
    // video_recorder.start()?;
    Ok(())
}