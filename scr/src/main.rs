use core::convert::TryInto;
use std::{collections::HashMap, net::Ipv4Addr, num::NonZero, sync::Mutex, time::{Duration, Instant}};

use anyhow::{anyhow, Result};
use canvas::{
    draw_splash_with_error, draw_splash_with_error1,
};
use config::Config;
use display::{DisplayManager, DisplayPins};
use embedded_svc::wifi::{AccessPointConfiguration, AuthMethod, Configuration};

use esp_idf_hal::{io::EspIOError, sys::{esp_restart, esp_wifi_set_ps, wifi_ps_type_t_WIFI_PS_NONE, wifi_ps_type_t_WIFI_PS_MIN_MODEM, ESP_FAIL}};
use esp_idf_svc::{ipv4::{Mask, Subnet}, wifi::{BlockingWifi, ClientConfiguration, EspWifi, WifiDriver}};
use esp_idf_svc::netif::{EspNetif, NetifConfiguration, NetifStack};
use esp_idf_svc::{eventloop::EspSystemEventLoop, nvs::EspDefaultNvsPartition};
use esp_idf_svc::{
    hal::prelude::Peripherals,
    ipv4::{self, RouterConfiguration},
    nvs::{EspNvs, NvsDefault},
    sys::EspError,
};

use http_server::print_memory;
use image::{RgbImage, RgbaImage};
use log::*;
use once_cell::sync::Lazy;
use serde::Serialize;
mod utils;
mod canvas;
mod config;
mod display;
#[allow(unused)]
mod imageproc;
mod mqtt_client;
mod http_server;

// Need lots of stack to parse JSON
// With CONFIG_SPIRAM_ALLOW_STACK_EXTERNAL_MEMORY, stacks can use PSRAM
const STACK_SIZE: usize = 1024 * 10;

pub const WIFI_AP_SSID: &str = "ESP32-WiFiScreen";

const MAX_HTTP_PAYLOAD_LEN: usize = 1024 * 512;

pub enum ImageCache {
    RgbImage(Box<RgbImage>),
    RgbaImage(Box<RgbaImage>),
}

#[derive(Serialize)]
pub struct Context {
    #[serde(skip)]
    display_pins: DisplayPins,
    #[serde(skip)]
    config_nvs: EspNvs<NvsDefault>,
    config: Config,
    free_heap: u32,
    free_internal_heap: u32,
    #[serde(skip)]
    wifi: BlockingWifi<EspWifi<'static>>,
    #[serde(skip)]
    display: Option<DisplayManager<'static>>,
    //存放上传的图片
    #[serde(skip)]
    image_cache: HashMap<String, ImageCache>,
    //记录最后一次访问配置页面的时间，用于防止配置期间自动重启
    //如果超过10分钟没有访问配置，则认为用户已离开，允许自动重启
    #[serde(skip)]
    last_config_time: Option<Instant>
}

static CONTEXT: Lazy<Mutex<Option<Box<Context>>>> = Lazy::new(|| Mutex::new(None));

pub fn with_context<F, T>(f: F) -> Result<T>
where
    F: FnOnce(&mut Context) -> Result<T>,
{
    let mut ctx = CONTEXT.lock().map_err(|err| anyhow!("{err:?}"))?;
    match ctx.as_mut() {
        Some(ctx) => f(ctx),
        None => Err(anyhow!("context init fail!")),
    }
}

pub fn with_context1<F, T>(f: F) -> Result<T, EspIOError>
where
    F: FnOnce(&mut Context) -> Result<T, EspIOError>,
{
    let mut ctx = CONTEXT.lock().map_err(|_err| 
        EspIOError(EspError::from_non_zero(NonZero::new(ESP_FAIL).unwrap())))?;
    match ctx.as_mut() {
        Some(ctx) => f(ctx),
        None => Err(EspIOError(EspError::from_non_zero(NonZero::new(ESP_FAIL).unwrap()))),
    }
}

fn main() -> anyhow::Result<()> {
    esp_idf_svc::sys::link_patches();

    esp_idf_svc::log::EspLogger::initialize_default();

    // 启动后等待5秒，确保串口能够连接
    info!("=== ESP32 WiFi Screen Starting ===");

    print_memory("start>01");

    let peripherals = Peripherals::take()?;
    let sys_loop = EspSystemEventLoop::take()?;
    let nvs_default_partition = EspDefaultNvsPartition::take()?;

    print_memory("start>02");

    let mut config_nvs =
        match esp_idf_svc::nvs::EspNvs::new(nvs_default_partition.clone(), "config_ns", true) {
            Ok(nvs) => {
                info!("Got namespace from default partition");
                nvs
            }
            Err(e) => panic!("Could't get namespace {:?}", e),
        };

    let mut sta_ip_config = ipv4::ClientConfiguration::default();

    //读取配置参数
    info!("Reading configuration from NVS...");
    let config = match config::read_config(&mut config_nvs) {
        Err(err) => {
            error!("config read fail:{err:?}");
            info!("Using default configuration");
            Config::default()
        }
        Ok(c) => {
            info!("Configuration loaded successfully");
            if let Some(wifi_c) = c.wifi_config.as_ref() {
                info!("WiFi config found: SSID={}", wifi_c.ssid);
                // if let (Some(ip), Some(gw)) = (wifi_c.device_ip.clone(), wifi_c.gateway_ip.clone())
                if let Some(ip) = wifi_c.device_ip.clone()
                {
                    info!("Static IP configured: {}", ip);
                    sta_ip_config = ipv4::ClientConfiguration::Fixed(ipv4::ClientSettings {
                        ip,
                        ..Default::default()
                    });
                }
            }
            if let Some(display_c) = c.display_config.as_ref() {
                info!("Display config found: type={:?}, size={}x{}", 
                    display_c.display_type, display_c.width, display_c.height);
            } else {
                info!("No display configuration found");
            }
            c
        }
    };
    print_memory("config loaded");
    
    info!("Creating WiFi driver...");
    let wifi = EspWifi::wrap_all(
        WifiDriver::new(
            peripherals.modem,
            sys_loop.clone(),
            Some(nvs_default_partition),
        )?,
        EspNetif::new_with_conf(&NetifConfiguration {
            ip_configuration: Some(ipv4::Configuration::Client(sta_ip_config)),
            ..NetifConfiguration::wifi_default_client()
        })?,
        EspNetif::new_with_conf(&NetifConfiguration {
            key: "WIFI_AP_DEF".try_into().unwrap(),
            description: "ap".try_into().unwrap(),
            route_priority: 10,
            ip_configuration: Some(ipv4::Configuration::Router(RouterConfiguration {
                subnet: Subnet {
                    gateway: Ipv4Addr::new(192, 168, 72, 1),
                    mask: Mask(24),
                },
                ..Default::default()
            })),
            stack: NetifStack::Ap,
            custom_mac: None,
            got_ip_event_id: None,
            lost_ip_event_id: None,
            flags: 0,
        })?,
        // EspNetif::new(NetifStack::Ap)?
    )?;

    info!("WiFi driver created successfully");
    print_memory("wifi driver created");
    std::thread::sleep(Duration::from_millis(500));

    let wifi = BlockingWifi::wrap(wifi, sys_loop)?;
    info!("WiFi wrapper created");
    print_memory("wifi wrapper created");

    {
        info!("Initializing context...");
        let display_pins = DisplayPins {
            spi2: peripherals.spi2,
            cs: peripherals.pins.gpio4,
            dc: peripherals.pins.gpio5,
            sclk: peripherals.pins.gpio6,
            miso_mosi: peripherals.pins.gpio7,
            rst: peripherals.pins.gpio8,
        };
        let mut ctx = CONTEXT.lock().map_err(|err| anyhow!("{err:?}"))?;
        ctx.replace(Box::new(Context {
            display: None,
            config_nvs,
            display_pins,
            config,
            free_heap: 0,
            free_internal_heap: 0,
            wifi,
            image_cache: HashMap::new(),
            last_config_time: None,
        }));
        info!("Context initialized successfully");
    }
    print_memory("context initialized");
    std::thread::sleep(Duration::from_millis(500));

    //尝试初始化屏幕
    info!("========================================");
    info!("Starting display initialization...");
    print_memory("init display>01");
    std::thread::sleep(Duration::from_secs(2));
    
    match display::init() {
        Ok(_) => {
            info!("Display initialized successfully!");
            print_memory("display init success");
        }
        Err(err) => {
            error!("Display initialization failed: {err:?}");
            print_memory(&format!("display init error: {err:?}"));
            std::thread::sleep(Duration::from_secs(3)); // 延迟3秒确保串口接收到错误信息
        }
    }
    print_memory("init display>02");
    std::thread::sleep(Duration::from_secs(2));
    info!("Display initialization completed");
    info!("========================================");

    //启动wifi热点
    info!("Starting WiFi...");
    if let Err(err) = start_wifi() {
        error!("WiFi start failed: {err:?}");
        let _ = draw_splash_with_error1(Some("WiFi连接失败!"), Some(&format!("{err:?}")));
        std::thread::sleep(Duration::from_secs(2));
    } else {
        info!("WiFi started successfully");
    }
    print_memory("init start wifi");
    std::thread::sleep(Duration::from_secs(1));
    
    //启动http服务器
    info!("Starting HTTP server...");
    http_server::start_http_server()?;
    info!("HTTP server started successfully");
    print_memory("http server started");

    //启动mqtt客户端
    info!("Starting MQTT client...");
    if let Err(err) = mqtt_client::listen_config(){
        error!("MQTT listen config failed (attempt 1): {err:?}");
        std::thread::sleep(Duration::from_secs(3));
        if let Err(err) = mqtt_client::listen_config(){
            error!("MQTT listen config failed (attempt 2): {err:?}");
        } else {
            info!("MQTT client started successfully (attempt 2)");
        }
    } else {
        info!("MQTT client started successfully");
    }
    info!("=== Initialization Complete ===");
    Ok(())
}



fn start_wifi() -> anyhow::Result<()> {
    with_context(|ctx| {
        ctx.wifi.stop()?;

        let mut client_config = None;
        if let Some(cfg) = ctx.config.wifi_config.as_ref() {
            info!("wifi config:{cfg:?}");
            client_config = Some(ClientConfiguration {
                ssid: cfg.ssid.as_str().try_into().unwrap(),
                bssid: None,
                auth_method: AuthMethod::WPA2Personal,
                password: cfg.password.as_str().try_into().unwrap(),
                channel: None,
                ..Default::default()
            });
        }

        let ap_config = AccessPointConfiguration {
            ssid: WIFI_AP_SSID.try_into().unwrap(),
            ..Default::default()
        };

        if let Some(client_config) = client_config {
            let _ = draw_splash_with_error(ctx, Some("连接WiFi..."), None);
            ctx.wifi
                .set_configuration(&Configuration::Mixed(client_config, ap_config))?;
        } else {
            let _ = draw_splash_with_error(ctx, Some("启动热点..."), None);
            ctx.wifi
                .set_configuration(&Configuration::AccessPoint(ap_config))?;
        }

        // Set WiFi power save policy:
        // - On ESP32-S3 prefer MIN_MODEM to reduce power draw on constrained power supplies
        // - On other chips keep NONE for maximum throughput
        #[cfg(feature = "esp32s3")]
        unsafe { esp_wifi_set_ps(wifi_ps_type_t_WIFI_PS_MIN_MODEM) };

        #[cfg(not(feature = "esp32s3"))]
        unsafe { esp_wifi_set_ps(wifi_ps_type_t_WIFI_PS_NONE) };

        info!("About to start WiFi interface (ctx.wifi.start())...");
        match ctx.wifi.start() {
            Ok(_) => info!("ctx.wifi.start() returned Ok"),
            Err(err) => {
                error!("wifi start: {err:?}");
                let _ = draw_splash_with_error(ctx, Some("热点启动失败"), None);
                return Ok(());
            }
        }

        info!("Calling ctx.wifi.connect() to attach to STA network (if configured)...");
        let mut err2 = match ctx.wifi.connect(){
            Ok(_) => { info!("ctx.wifi.connect() returned Ok"); None },
            Err(err) => {
                error!("wifi connect: {err:?}");
                Some("Wifi连接失败".to_string())
            }
        };

        info!("Calling ctx.wifi.wait_netif_up() to wait for network interface up...");
        if let Err(err) = ctx.wifi.wait_netif_up() {
            error!("wait_netif_up: {err:?}");
        } else {
            //保存设备ip以及网关ip
            if let Some(cfg) = ctx.config.wifi_config.as_mut() {
                let mut need_reboot = false;
                if let Ok(ip_info) = ctx.wifi.wifi().sta_netif().get_ip_info() {
                    cfg.device_ip = Some(ip_info.ip.clone());
                    if let Some(ip) = cfg.device_ip.clone(){
                        err2 = Some(format!("局域网:{}", ip.to_string()));
                    }
                    let gateway = ip_info.subnet.gateway.clone();
                    info!("update device ip:{:?}", cfg.device_ip);
                    // info!("update gateway ip:{:?}", cfg.gateway_ip);
                    //如果设备ip和网关ip前缀不一致，删除设备以及网关ip，保存配置并重启!!
                    let d_ip = cfg.device_ip.clone().unwrap();
                    // let g_ip = cfg.gateway_ip.clone().unwrap();
                    let subnet_mask = Ipv4Addr::new(255, 255, 255, 0);
                    if !utils::is_same_subnet(d_ip, gateway, subnet_mask) {
                        error!("device IP and gateway Ip are not in the same subnet.");
                        need_reboot = true;
                        cfg.device_ip = None;
                        // cfg.gateway_ip = None;
                    }
                } else {
                    cfg.device_ip = None;
                    // cfg.gateway_ip = None;
                }
                config::save_config(&mut ctx.config_nvs, &ctx.config)?;
                if need_reboot{
                    std::thread::sleep(Duration::from_millis(1500));
                    unsafe { esp_restart() };
                }
            }
        }

        let _ = draw_splash_with_error(ctx, Some("IP:192.168.72.1"), err2.as_ref().map(|x| x.as_str()));

        //每隔60秒钟检查wifi是否连接，如果断开连接，自动重启
        //但如果用户正在配置（最后访问配置时间在10分钟内），则跳过重启
        std::thread::spawn(move ||{
            loop{
                std::thread::sleep(Duration::from_secs(60));
                let _ = with_context(|ctx| {
                    if ctx.config.wifi_config.is_some(){
                        let connected = ctx.wifi.is_connected().unwrap_or(false);
                        
                        // 检查用户是否在配置中（最后访问时间在10分钟内）
                        let in_config = if let Some(last_time) = ctx.last_config_time {
                            last_time.elapsed() < Duration::from_secs(10 * 60)  // 10分钟
                        } else {
                            false
                        };
                        
                        print_memory(&format!("idle connected={connected} in_config={in_config}"));
                        if !connected && !in_config {
                            info!("WiFi断开且用户未在配置中，准备重启...");
                            std::thread::sleep(Duration::from_millis(500));
                            unsafe { esp_restart() };
                        }
                    }
                    Ok(())
                });
            }
        });

        Ok(())
    })
}