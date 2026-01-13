use std::{net::Ipv4Addr, num::NonZero};

use anyhow::{anyhow, Result};
use esp_idf_svc::{mqtt::client::QoS, nvs::{EspNvs, NvsDefault}};
use log::info;
use non_empty_string::NonEmptyString;
use serde::{Deserialize, Serialize};

use crate::display::DisplayType;

#[derive(Serialize, Deserialize, PartialEq, Clone, Debug)]
pub struct RemoteServerConfig {
    pub mqtt_url: Option<NonEmptyString>,
    pub mqtt_client_id: Option<NonEmptyString>,
    pub mqtt_topic: Option<NonEmptyString>,
    pub mqtt_qos: QoS,
    pub mqtt_username: Option<NonEmptyString>,
    pub mqtt_password: Option<NonEmptyString>,
}

#[derive(Serialize, Deserialize, PartialEq, Clone, Debug)]
pub enum DisplayColorOrder{
    /// RGB subpixel order.
    Rgb,
    /// BGR subpixel order.
    Bgr,   
}

#[derive(Serialize, Deserialize, PartialEq, Clone, Debug)]
pub enum DisplayRotation {
    /// No rotation.
    Deg0,
    /// 90° clockwise rotation.
    Deg90,
    /// 180° clockwise rotation.
    Deg180,
    /// 270° clockwise rotation.
    Deg270,
}

#[derive(Serialize, Deserialize, PartialEq, Clone, Debug)]
pub struct DisplayConfig {
    pub display_type: DisplayType,
    pub with_cs: bool,
    pub width: NonZero<u16>,
    pub height: NonZero<u16>,
    pub color_inversion: bool,
    pub color_order: DisplayColorOrder,
    /// Rotation.
    pub rotation: DisplayRotation,
    /// Mirrored.
    pub mirrored: bool,
    pub x_offset: u16,
    pub y_offset: u16,
    pub spi_mode: u8,
    /// 绘制时是否包含结束坐标
    pub inclusive_end_coords: bool,
    pub rotated_width: Option<NonZero<u16>>,
    pub rotated_height: Option<NonZero<u16>>,
    /// 色调调整：红色通道偏移 (-100 到 +100)
    #[serde(default)]
    pub color_adjust_r: i8,
    /// 色调调整：绿色通道偏移 (-100 到 +100)
    #[serde(default)]
    pub color_adjust_g: i8,
    /// 色调调整：蓝色通道偏移 (-100 到 +100)
    #[serde(default)]
    pub color_adjust_b: i8,
    /// 屏幕亮度 (0-100)，默认100
    #[serde(default = "default_brightness")]
    pub brightness: u8,
}

impl DisplayConfig{
    pub fn get_screen_size(&self) -> (u16, u16){
        match self.rotation{
            crate::config::DisplayRotation::Deg0 => {
                (self.width.get(),
                    self.height.get())
            }
            crate::config::DisplayRotation::Deg90 => {
                (self.height.get(),
                    self.width.get())
            }
            crate::config::DisplayRotation::Deg180 => {
                (self.width.get(),
                    self.height.get())
            }
            crate::config::DisplayRotation::Deg270 => {
                (self.height.get(),
                    self.width.get())
            }
        }
    }
}

fn default_brightness() -> u8 { 100 }

#[derive(Serialize, Deserialize, PartialEq, Clone, Debug)]
pub struct WifiConfig {
    pub ssid: String,
    pub password: String,
    pub device_ip: Option<Ipv4Addr>,
    // pub gateway_ip: Option<Ipv4Addr>,
}

#[derive(Serialize, Deserialize, PartialEq, Clone, Debug)]
pub struct Config {
    pub wifi_config: Option<WifiConfig>,
    pub display_config: Option<DisplayConfig>,
    pub remote_server_config: Option<RemoteServerConfig>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            wifi_config: Default::default(),
            display_config: Default::default(),
            remote_server_config: Default::default(),
        }
    }
}

// pub fn parse_config(data: Vec<u8>) -> Result<Config> {
//     let data_str = String::from_utf8(data)?;
//     info!("Receive Data:{data_str}");
//     let config = serde_json::from_str::<Config>(&data_str)?;
//     Ok(config)
// }

pub fn parse_display_config(data: Vec<u8>) -> Result<DisplayConfig> {
    let data_str = String::from_utf8(data)?;
    info!("Receive Data:{data_str}");
    let config = serde_json::from_str::<DisplayConfig>(&data_str)?;
    Ok(config)
}

pub fn parse_wifi_config(data: Vec<u8>) -> Result<WifiConfig> {
    let data_str = String::from_utf8(data)?;
    info!("Receive Data:{data_str}");
    let config = serde_json::from_str::<WifiConfig>(&data_str)?;
    Ok(config)
}

pub fn parse_remote_server_config(data: Vec<u8>) -> Result<RemoteServerConfig> {
    let data_str = String::from_utf8(data)?;
    info!("Receive Data:{data_str}");
    let config = serde_json::from_str::<RemoteServerConfig>(&data_str)?;
    Ok(config)
}

pub fn save_config(nvs: &mut EspNvs<NvsDefault>, cfg: &Config) -> Result<()> {
    let cfg_str = serde_json::to_string(cfg)?;
    nvs.set_str("cfg.json", &cfg_str)?;
    Ok(())
}

pub fn delete_config(nvs: &mut EspNvs<NvsDefault>) -> Result<()> {
    nvs.remove("cfg.json")?;
    Ok(())
}

pub fn read_config(nvs: &mut EspNvs<NvsDefault>) -> Result<Config> {
    let buf = &mut [0u8; 2048];
    match nvs.get_str("cfg.json", buf)? {
        Some(data) => serde_json::from_str::<Config>(data).map_err(|err| anyhow!("{err:?}")),
        None => Err(anyhow!("no config!")),
    }
}
