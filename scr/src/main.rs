use core::convert::TryInto;
use std::{collections::HashMap, net::Ipv4Addr, num::NonZero, sync::Mutex, time::Duration};

use anyhow::{anyhow, Result};
use canvas::{
    draw_splash_with_error, draw_splash_with_error1,
};
use config::Config;
use display::{DisplayManager, DisplayPins};
use embedded_svc::wifi::{AccessPointConfiguration, AuthMethod, Configuration};

use esp_idf_hal::{io::EspIOError, sys::{esp_restart, esp_wifi_set_ps, wifi_ps_type_t_WIFI_PS_NONE, ESP_FAIL}};
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
mod tjpgd;
// mod tjpgd_rgb565;
mod mqtt_client;
mod http_server;

// Need lots of stack to parse JSON
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
    //是否进入了设置界面，进入设置洁面后，即使wifi断开也不重启
    #[serde(skip)]
    enter_config: bool
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

    let peripherals = Peripherals::take()?;
    let sys_loop = EspSystemEventLoop::take()?;
    let nvs_default_partition = EspDefaultNvsPartition::take()?;

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
    let config = match config::read_config(&mut config_nvs) {
        Err(err) => {
            error!("config read fail:{err:?}");
            Config::default()
        }
        Ok(c) => {
            if let Some(wifi_c) = c.wifi_config.as_ref() {
                // if let (Some(ip), Some(gw)) = (wifi_c.device_ip.clone(), wifi_c.gateway_ip.clone())
                if let Some(ip) = wifi_c.device_ip.clone()
                {
                    sta_ip_config = ipv4::ClientConfiguration::Fixed(ipv4::ClientSettings {
                        ip,
                        ..Default::default()
                    });
                }
            }
            c
        }
    };
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

    let wifi = BlockingWifi::wrap(wifi, sys_loop)?;

    {
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
            enter_config: false,
        }));
    }

    //尝试初始化屏幕
    print_memory("init display>01");
    std::thread::sleep(Duration::from_secs(1));
    if let Err(err) = display::init() {
        error!("display init error:{err:?}");
    }
    print_memory("init display>02");
    std::thread::sleep(Duration::from_secs(1));

    //启动wifi热点
    if let Err(err) = start_wifi() {
        let _ = draw_splash_with_error1(Some("WiFi连接失败!"), Some(&format!("{err:?}")));
    }
    print_memory("init start wifi");
    std::thread::sleep(Duration::from_secs(1));
    //启动http服务器
    http_server::start_http_server()?;

    //启动mqtt客户端
    if let Err(err) = mqtt_client::listen_config(){
        error!("listen config:{err:?}");
        std::thread::sleep(Duration::from_secs(3));
        if let Err(err) = mqtt_client::listen_config(){
            error!("listen config:{err:?}");
        }
    }
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

        unsafe{
            
            esp_wifi_set_ps(wifi_ps_type_t_WIFI_PS_NONE);
            // esp_wifi_config_80211_tx_rate(wifi_interface_t_WIFI_IF_STA, wifi_phy_rate_t_WIFI_PHY_RATE_11M_S);
            // let mut getprotocol = 0;
            // let err = esp_wifi_get_protocol(wifi_interface_t_WIFI_IF_STA, &mut getprotocol);
            // info!("getprotocol -> err = {err} getprotocol = {getprotocol}");
            // if getprotocol as u32 & WIFI_PROTOCOL_11N > 0 {
            //     info!("getprotocol -> WiFi_Protocol_11n");
            // }
            // if getprotocol as u32 & esp_idf_svc::sys::WIFI_PROTOCOL_11G > 0 {
            //     info!("getprotocol -> WiFi_Protocol_11g");
            // }
            // if getprotocol as u32 & WIFI_PROTOCOL_11B > 0 {
            //     info!("getprotocol -> WiFi_Protocol_11b");
            // }
            // if getprotocol as u32 & esp_idf_svc::sys::WIFI_PROTOCOL_11AX > 0 {
            //     info!("getprotocol -> WIFI_PROTOCOL_11AX");
            // }
        }

        if let Err(err) = ctx.wifi.start(){
            error!("wifi start: {err:?}");
            let _ = draw_splash_with_error(ctx, Some("热点启动失败"), None);
            return Ok(());
        }

        let mut err2 = match ctx.wifi.connect(){
            Ok(_) => None,
            Err(err) => {
                error!("wifi connect: {err:?}");
                Some("Wifi连接失败".to_string())
            }
        };

        if let Err(err) = ctx.wifi.wait_netif_up(){
            error!("wait_netif_up: {err:?}");
        }else{
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
        std::thread::spawn(move ||{
            loop{
                std::thread::sleep(Duration::from_secs(60));
                let _ = with_context(|ctx| {
                    if ctx.config.wifi_config.is_some(){
                        let connected = ctx.wifi.is_connected().unwrap_or(false);
                        print_memory(&format!("idle connected={connected}"));
                        if !connected{
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