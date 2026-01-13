#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::{net::Ipv4Addr, str::FromStr, sync::mpsc::{channel, Receiver}, time::Duration};

use anyhow::{anyhow, Result};
use async_std::{fs::File, io::{ReadExt, WriteExt}, task::spawn_blocking};
use image::{codecs::jpeg::JpegEncoder, imageops::resize};
use ini::Ini;
use recorder::{start_with_config_alert, ImageFormat, RecorderConfig};
use rfd::{AsyncMessageDialog, MessageDialog};
use serde::{Deserialize, Serialize};
use slint::{spawn_local, SharedString, VecModel};
use xcap::Monitor;

pub const CONFIG_FILE_NAME:&str = "wifi-screen-client.ini";
pub const APP_NAME:&str = "ESP32-WIFI-SCREEN";

#[allow(dead_code)]
mod rgb565;
mod recorder;
mod usb_serial;

use tao::{
    event::Event,
    event_loop::{ControlFlow, EventLoopBuilder},
};
use tray_icon::{
    menu::{AboutMetadata, Menu, MenuEvent, MenuItem, PredefinedMenuItem},
    TrayIconBuilder, TrayIconEvent,
};

enum UserEvent {
    TrayIconEvent(tray_icon::TrayIconEvent),
    MenuEvent(tray_icon::menu::MenuEvent),
    UpdateConfig(RecorderConfig),
    UpdateStatus((String, String))
}

slint::slint!{
    import { Button, VerticalBox, ComboBox, HorizontalBox, LineEdit } from "std-widgets.slint";

    export component App inherits Window {
        title: "ESP32-WIFI-SCREEN";
        min-width: 400px;
        min-height: 300px;
        icon: @image-url("icon.png");

        callback confirm(string, string, string, string, string, string);
        callback test-screen(string, string, string, string);

        in-out property <bool> is_testing: false;
        in-out property <[string]> screens;
        in-out property <string> current-screen;
        in-out property <[string]> targets : [
            "WiFi (IP)",
            "USB 串口"
        ];
        in-out property <string> current-target: "WiFi (IP)";
        in-out property <string> screen-ip;
        in-out property <[string]> usb-devices;
        in-out property <string> current-usb-device;
        in-out property <[string]> formats : [
            "RGB565",
            "JPG 100%",
            "JPG 90%",
            "JPG 80%",
            "JPG 70%",
            "JPG 60%",
            "JPG 50%",
            "JPG 40%",
            "JPG 30%",
            "JPG 20%",
            "JPG 10%",
            "JPG 5%",
            "GIF"
        ];
        in-out property <string> current-format: "JPG 30%";
        in-out property <string> delay-ms: "200";

        VerticalBox{
            HorizontalBox {
                Text {
                    vertical-alignment: center;
                    text: "连接方式:";
                    min-width: 70px;
                }
                ComboBox {
                    model: targets;
                    current-value <=> current-target;
                }
            }
            HorizontalBox {
                Text {
                    vertical-alignment: center;
                    text: "选择显示器:";
                    min-width: 70px;
                }
                ComboBox {
                    model: screens;
                    current-value <=> current-screen;
                }
            }
            if current-target == "WiFi (IP)" : HorizontalBox {
                Text {
                    vertical-alignment: center;
                    text: "WiFi屏幕IP:";
                    min-width: 70px;
                }
                LineEdit {
                    text <=> screen-ip;
                    placeholder-text: "输入IP";
                }
            }
            if current-target == "USB 串口" : HorizontalBox {
                Text {
                    vertical-alignment: center;
                    text: "USB设备:";
                    min-width: 70px;
                }
                ComboBox {
                    model: usb-devices;
                    current-value <=> current-usb-device;
                }
            }
            HorizontalBox {
                Text {
                    vertical-alignment: center;
                    text: "传输格式:";
                    min-width: 70px;
                }
                ComboBox {
                    model: formats;
                    current-value <=> current-format;
                }
            }
            HorizontalBox {
                Text {
                    vertical-alignment: center;
                    text: "截屏延迟:";
                    min-width: 70px;
                }
                LineEdit {
                    text <=> delay-ms;
                    placeholder-text: "毫秒";
                }
            }
            HorizontalBox {
                Button{
                    enabled: !is_testing;
                    text: "启动";
                    clicked => {
                        confirm(current-screen, current-target, screen-ip, current-usb-device, current-format, delay-ms)
                    }
                }
                Button{
                    enabled: !is_testing;
                    text: "测试";
                    clicked => {
                        test-screen(current-screen, current-target, screen-ip, current-usb-device)
                    }
                }
            }
        }
    }
}

fn run_setting_window(receiver: Receiver<String>, proxy: tao::event_loop::EventLoopProxy<UserEvent>) -> Result<()>{
    println!("设置窗口线程启动，等待打开命令...");
    loop {
        // 等待托盘“设置”点击信号
        let _ = receiver.recv().map_err(|err| anyhow!("{err}"))?;

        // 每次打开都重新扫描显示器和 USB 设备
        println!("开始查找显示器...");
        let mut monitor_size: Option<(u32, u32)> = None;
        let mut monitor_sizes: Vec<(u32, u32)> = vec![];
        let monitors: Vec<SharedString> = Monitor::all().unwrap_or_default()
            .iter()
            .map(|m| {
                let width = m.width().unwrap_or(0);
                let height = m.height().unwrap_or(0);
                if width == 0 || height == 0 {
                    return "无效显示器".into();
                }
                monitor_sizes.push((width, height));
                if monitor_size.is_none() {
                    monitor_size = Some((width, height));
                } else {
                    let (w, h) = monitor_size.unwrap();
                    if width * height < w * h {
                        monitor_size = Some((width, height));
                    }
                }
                format!("显示器{}x{}", width, height).into()
            })
            .collect();

        let usb_list = crate::usb_serial::find_usb_screen_serial_devices().unwrap_or_default();
        let usb_items: Vec<SharedString> = usb_list.iter().map(|s| s.clone().into()).collect();

        // 读取配置用于回填
        let cfg = async_std::task::block_on(load_config()).ok();

        // 每次打开都新建一个窗口，关闭后返回这里等待下一次打开
        let app = App::new()?;
        let app_weak = app.as_weak();

        // 初始化下拉数据
        app.set_screens(VecModel::from_slice(&monitors));
        if let Some((w, h)) = monitor_size {
            app.set_current_screen(format!("显示器{}x{}", w, h).into());
        }
        if !usb_items.is_empty() {
            app.set_usb_devices(VecModel::from_slice(&usb_items));
            app.set_current_usb_device(usb_items[0].clone());
        } else {
            let empty: [SharedString; 0] = [];
            app.set_usb_devices(VecModel::from_slice(&empty));
            app.set_current_usb_device("".into());
        }

        // 回填配置
        if let Some((width, height, target, ip, usb_device, format, delay_ms)) = cfg {
            app.set_screen_ip(ip.into());
            app.set_current_target(target.into());
            app.set_current_usb_device(usb_device.into());
            app.set_current_format(format.into());
            app.set_delay_ms(format!("{delay_ms}").into());

            let mut found = false;
            for (w, h) in monitor_sizes.iter() {
                if *w == width && *h == height {
                    found = true;
                    break;
                }
            }
            if found {
                app.set_current_screen(format!("显示器{width}x{height}").into());
            }
        }

        // 绑定按钮逻辑
        let proxy_confirm = proxy.clone();
        let app_weak_confirm = app_weak.clone();
        app.on_confirm(move |screen, target, ip, usb_device, format, delay_ms|{
            let target = target.to_string();
            let ip = ip.to_string();
            let usb_device = usb_device.to_string();
            let delay_ms = delay_ms.to_string();

            let is_usb = target == "USB 串口";
            if !is_usb {
                if let Err(_err) = Ipv4Addr::from_str(&ip){
                    show_alert("请输入正确的IP地址");
                    return;
                }
            } else {
                if usb_device.trim().is_empty() {
                    show_alert("请选择 USB 串口设备");
                    return;
                }
                // USB 仅支持 RGB565+LZ4
                if !format.to_string().starts_with("RGB565") {
                    show_alert("USB 串口仅支持 RGB565(LZ4) 传输格式");
                    return;
                }
            }
            
            let delay_ms = match delay_ms.parse::<u64>(){
                Err(_err) => {
                    show_alert("请输入正确的延迟毫秒");
                    return;
                }
                Ok(v) => v
            };

            let proxy_clone = proxy_confirm.clone();
            let format_name = format.to_string();
            let app_clone = app_weak_confirm.clone();
            let _ = spawn_local(async move {
                let app = match app_clone.upgrade(){
                    Some(ap) => ap,
                    None => return
                };
                let ret = save_config(screen.to_string(), target.clone(), ip.clone(), usb_device.clone(), format_name.clone(), delay_ms).await;
                if ret.is_err(){
                    show_alert("配置文件保存失败");
                    return;
                }
                let (screen_width, screen_height) = get_screen_size(&screen.to_string()).unwrap();
                let format = if format_name == "GIF"{
                    ImageFormat::GIF
                }else if format_name.starts_with("JPG"){
                    let quality = match format_name
                    .replace("JPG ", "")
                    .replace("%", "").parse::<u8>(){
                        Err(_) => 30,
                        Ok(q) => q
                    };
                    ImageFormat::JPG(quality)
                }else{
                    ImageFormat::Rgb565Lz4Compressed
                };
                println!("点击确认按钮，测试连接...");
                app.set_is_testing(true);
                match test_screen(screen.to_string(), target.clone(), ip.clone(), usb_device.clone()).await {
                    Ok(display_config) => {
                        app.set_is_testing(false);
                        let target_cfg = if target == "USB 串口" {
                            let port = crate::usb_serial::extract_port_name(&usb_device);
                            recorder::OutputTarget::UsbSerial { port_name: port }
                        } else {
                            recorder::OutputTarget::Wifi { ip: ip.clone() }
                        };
                        let _ = proxy_clone.send_event(UserEvent::UpdateConfig(RecorderConfig {
                            target: target_cfg,
                            format,
                            display_config,
                            monitor_width: screen_width,
                            monitor_height: screen_height,
                            delay_ms
                       }));
                       // 直接退出本次 slint 事件循环，窗口会被关闭，线程回到 recv 等待下一次打开
                       slint::quit_event_loop();
                    }
                    Err(err) => {
                        app.set_is_testing(false);
                        println!("测试失败:{}", err.root_cause());
                        let err = &format!("{}", err.root_cause());
                        show_alert(err);
                    }
                };
            });
        });

        let app_clone = app.as_weak();
        app.on_test_screen(move |screen, target, ip, usb_device|{
            let screen = screen.to_string();
            let target = target.to_string();
            let ip = ip.to_string();
            let usb_device = usb_device.to_string();
            let app_clone1 = app_clone.clone();
            let _ = spawn_local(async move {
                let _ = app_clone1.upgrade_in_event_loop(move |app|
                {
                    app.set_is_testing(true);
                });
                let msg = match test_screen(screen, target, ip, usb_device).await {
                    Ok(_) => "测试成功!",
                    Err(err) => {
                        eprintln!("测试失败:{}", err.root_cause());
                        &format!("{}", err.root_cause())
                    }
                };
                let _ = app_clone1.upgrade_in_event_loop(move |app|
                {
                    app.set_is_testing(false);
                });
                show_alert(msg);
            });
        });

        println!("显示设置窗口...");
        app.run()?;
        println!("设置窗口已关闭，等待下一次打开...");
    }
}

fn main() -> Result<()> {

    // 使用 arduino ide 测试http、websocket传输速度！！！！！
    
    let (sender, receiver) = channel();
    
    let event_loop = EventLoopBuilder::<UserEvent>::with_user_event().build();

    let proxy = event_loop.create_proxy();
    std::thread::spawn(move ||{
        run_setting_window(receiver, proxy).unwrap();
    });

    // 取消无配置自动弹出设置窗口，仅在托盘点击“设置”时打开

    // set a tray event handler that forwards the event and wakes up the event loop
    let proxy = event_loop.create_proxy();
    TrayIconEvent::set_event_handler(Some(move |event| {
        let _ = proxy.send_event(UserEvent::TrayIconEvent(event));
    }));

    // set a menu event handler that forwards the event and wakes up the event loop
    let proxy = event_loop.create_proxy();
    MenuEvent::set_event_handler(Some(move |event| {
        let _ = proxy.send_event(UserEvent::MenuEvent(event));
    }));

    let tray_menu = Menu::new();

    let quit_i = MenuItem::new("退出", true, None);
    let setting_i = MenuItem::new("设置", true, None);
    let screen_status_i = MenuItem::new("录屏状态: 未知", true, None);
    let uploader_status_i = MenuItem::new("屏幕状态: 未知", true, None);
    tray_menu.append_items(&[
        &PredefinedMenuItem::about(
            None,
            Some(AboutMetadata {
                name: Some(APP_NAME.to_string()),
                copyright: Some("Copyright https://github.com/planet0104".to_string()),
                ..Default::default()
            }),
        ),
        &PredefinedMenuItem::separator(),
        &screen_status_i,
        &uploader_status_i,
        &PredefinedMenuItem::separator(),
        &setting_i,
        &quit_i,
    ])?;

    let mut tray_icon = None;

    // let menu_channel = MenuEvent::receiver();
    // let tray_channel = TrayIconEvent::receiver();

    //每隔两秒刷新状态
    let proxy = event_loop.create_proxy();
    std::thread::spawn(move ||{
        loop{
            let (mut recorder_status, mut uploader_status) = ("录屏状态: 未知".to_string(), "屏幕状态: 未知".to_string());
            if let Ok((monitor_status, socket_status)) = recorder::get_status_sync(){
                recorder_status = match monitor_status{
                        recorder::Status::Connected => format!("录屏状态: 已启动"),
                        recorder::Status::ConnectFail => "录屏状态: 启动失败".to_string(),
                        recorder::Status::Disconnected => "录屏状态: 未启动".to_string(),
                        recorder::Status::Connecting => "录屏状态: 启动中".to_string(),
                };
                uploader_status = match socket_status{
                        recorder::Status::Connected => format!("屏幕状态: 已连接"),
                        recorder::Status::ConnectFail => "屏幕状态: 启动失败".to_string(),
                        recorder::Status::Disconnected => "屏幕状态: 未启动".to_string(),
                        recorder::Status::Connecting => "屏幕状态: 连接中".to_string(),
                };
            }
            let _ = proxy.send_event(UserEvent::UpdateStatus((recorder_status, uploader_status)));
            std::thread::sleep(Duration::from_secs(1));
        }
    });

    event_loop.run(move |event, _, control_flow| {
        *control_flow = ControlFlow::Wait;

        match event {
            Event::NewEvents(tao::event::StartCause::Init) => {
                let icon = load_icon().expect("fail to load icon.png");

                // We create the icon once the event loop is actually running
                // to prevent issues like https://github.com/tauri-apps/tray-icon/issues/90
                tray_icon = Some(
                    TrayIconBuilder::new()
                        .with_menu(Box::new(tray_menu.clone()))
                        .with_tooltip(APP_NAME)
                        .with_icon(icon)
                        .build()
                        .unwrap(),
                );

                // We have to request a redraw here to have the icon actually show up.
                // Tao only exposes a redraw method on the Window so we use core-foundation directly.
                #[cfg(target_os = "macos")]
                unsafe {
                    use core_foundation::runloop::{CFRunLoopGetMain, CFRunLoopWakeUp};

                    let rl = CFRunLoopGetMain();
                    CFRunLoopWakeUp(rl);
                }
            }

            Event::UserEvent(UserEvent::TrayIconEvent(_event)) => {
                // println!("TrayIconEvent {event:?}");
            }
            Event::UserEvent(UserEvent::UpdateConfig(config)) => {
                println!("update config");
                start_with_config_alert(config);
            }
            Event::UserEvent(UserEvent::UpdateStatus((recorder_status, uploader_status))) => {
                // println!("接收recorder_status:{recorder_status}");
                // println!("接收uploader_status:{uploader_status}");
                screen_status_i.set_text(recorder_status);
                uploader_status_i.set_text(uploader_status);
            }

            Event::UserEvent(UserEvent::MenuEvent(event)) => {
                println!("MenuEvent {event:?}");

                if event.id == quit_i.id() {
                    tray_icon.take();
                    *control_flow = ControlFlow::Exit;
                }else if event.id == setting_i.id() {
                    let _ = sender.send("open".to_string());
                }
            }

            _ => {}
        }
    });
}

fn load_icon() -> Result<tray_icon::Icon> {
    let (icon_rgba, icon_width, icon_height) = {
        let image = image::load_from_memory_with_format(include_bytes!("../icon.png"), image::ImageFormat::Png)?
            .into_rgba8();
        let (width, height) = image.dimensions();
        let rgba = image.into_raw();
        (rgba, width, height)
    };
    let icon = tray_icon::Icon::from_rgba(icon_rgba, icon_width, icon_height)?;
    Ok(icon)
}

fn get_screen_size(screen_config:&str) -> Result<(i32, i32)>{
    let screen = screen_config.replace("显示器", "");
    let screen_size:Vec<&str> = screen.split("x").collect();
    if screen_size.len() != 2{
        return Err(anyhow!("下拉框屏幕参数错误"));
    }
    let screen_width:i32 = screen_size[0].parse()?;
    let screen_height:i32 = screen_size[1].parse()?;
    Ok((screen_width, screen_height))
}

async fn save_config(screen_config: String, target: String, ip: String, usb_device: String, format:String, delay_ms: u64) -> Result<()>{
    let (screen_width, screen_height) = get_screen_size(&screen_config)?;
    let mut conf = Ini::new();
    conf.with_section(None::<String>).set("screen_width", format!("{screen_width}"));
    conf.with_section(None::<String>).set("screen_height", format!("{screen_height}"));
    conf.with_section(None::<String>).set("target", target);
    conf.with_section(None::<String>).set("ip", format!("{ip}"));
    conf.with_section(None::<String>).set("usb_device", usb_device);
    conf.with_section(None::<String>).set("format", format!("{format}"));
    conf.with_section(None::<String>).set("delay_ms", format!("{delay_ms}"));
    let mut file_content = vec![];
    conf.write_to(&mut file_content)?;
    let mut f = File::create(CONFIG_FILE_NAME).await?;
    f.write_all(&file_content).await?;
    Ok(())
}

async fn test_screen(screen_config: String, target: String, ip: String, usb_device: String) -> Result<DisplayConfig>{
    let screen = screen_config.replace("显示器", "");
    let screen_size:Vec<&str> = screen.split("x").collect();
    if screen_size.len() != 2{
        return Err(anyhow!("屏幕参数错误"));
    }
    let screen_width:u32 = screen_size[0].parse()?;
    let screen_height:u32 = screen_size[1].parse()?;

    let resp = if target == "USB 串口" {
        let port = crate::usb_serial::extract_port_name(&usb_device);
        if port.is_empty() {
            return Err(anyhow!("请选择 USB 串口设备"));
        }
        let (w, h) = crate::usb_serial::query_screen_size(&port)?;
        DisplayConfig {
            display_type: Some("USB-SCREEN".to_string()),
            rotated_width: w as u32,
            rotated_height: h as u32,
        }
    } else {
        let _ = Ipv4Addr::from_str(&ip).map_err(|_err| anyhow!("错误的IP地址!"))?;
        let ip_clone = ip.clone();
        let resp = spawn_blocking(move ||{
            reqwest::blocking::Client::builder()
            .timeout(Duration::from_secs(5))
            .no_proxy()
            .build()?
            .get(&format!("http://{ip_clone}/display_config"))
            .timeout(Duration::from_secs(5))
            .send()?
            .json::<DisplayConfig>()
        }).await?;
        // 绘制一次（WiFi 模式原有逻辑）
        let _ = spawn_blocking(move || {
            reqwest::blocking::Client::builder()
            .timeout(Duration::from_secs(5))
            .no_proxy()
            .build()?
            .get(&format!("http://{ip}/draw_image"))
            .timeout(Duration::from_secs(5))
            .send()?
            .text()
        }).await?;
        resp
    };
    println!("屏幕大小:{}x{}", resp.rotated_width, resp.rotated_height);

    //屏幕截图
    let monitors = Monitor::all()?;
    let mut find_monitor = None;
    for m in monitors{
        let width = m.width().unwrap_or(0);
        let height = m.height().unwrap_or(0);
        if width == screen_width && height == screen_height{
            find_monitor = Some(m);
            break;
        }
    }
    let m = match find_monitor{
        None => return Err(anyhow!("未找到{screen_width}x{screen_height}分辨率的显示器")),
        Some(m) => m
    };
    let img = m.capture_image()?;
    let img = resize(&img, resp.rotated_width, resp.rotated_height, image::imageops::FilterType::Nearest);
    let mut out = vec![];
    let mut jpg = JpegEncoder::new_with_quality(&mut out, 50);
    jpg.encode_image(&img)?;
    Ok(resp)
}

pub async fn load_config() -> Result<(u32, u32, String, String, String, String, u64)>{
    let mut f = File::open(CONFIG_FILE_NAME).await?;
    let mut data = vec![];
    f.read_to_end(&mut data).await?;
    let cfg_str = String::from_utf8(data)?;
    let conf = Ini::load_from_str(&cfg_str)?;
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
    let ip = match conf.get_from(None::<String>, "ip"){
        None => {
            "192.168.72.1"
        }
        Some(v) => v
    };
    let target = conf.get_from(None::<String>, "target").unwrap_or("WiFi (IP)");
    let usb_device = conf.get_from(None::<String>, "usb_device").unwrap_or("").to_string();
    let format = match conf.get_from(None::<String>, "format"){
        None => {
            "JPG 30%"
        }
        Some(v) => v
    };
    if target == "WiFi (IP)" {
        let _ = Ipv4Addr::from_str(&ip)?;
    }
    let width: u32 = screen_width.parse()?;
    let height: u32 = screen_height.parse()?;

    Ok((width, height, target.to_string(), ip.to_string(), usb_device, format.to_string(), delay_ms))
}

fn show_alert(msg:&str){
    let msg = msg.to_string();
    let _ = spawn_local(async move {
        let _ = AsyncMessageDialog::new()
        .set_title(APP_NAME)
        .set_description(msg)
        .set_buttons(rfd::MessageButtons::Ok)
        .show().await;
    });
}

fn show_alert_async(msg:&str){
    let msg = msg.to_string();
    std::thread::spawn(move ||{
        let _ = MessageDialog::new()
        .set_title(APP_NAME)
        .set_description(msg)
        .set_buttons(rfd::MessageButtons::Ok)
        .show();
    });
}

#[derive(Serialize, Deserialize, Clone, Debug)]
struct DisplayConfig{
    display_type: Option<String>,
    rotated_width: u32,
    rotated_height: u32
}