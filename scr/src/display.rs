use crate::canvas::draw_splash_with_error;
use crate::config::DisplayConfig;
use crate::with_context;
use ab_glyph::FontRef;
use anyhow::{anyhow, Result};
use esp_idf_hal::gpio::Output;
use esp_idf_hal::peripheral::Peripheral;
use esp_idf_hal::spi::SpiDriver;
use esp_idf_hal::{
    delay::Ets,
    gpio::{Gpio4, Gpio5, Gpio6, Gpio7, Gpio8, PinDriver},
    spi::{
        config::{self, MODE_0, MODE_1, MODE_2, MODE_3},
        SpiDeviceDriver, SpiDriverConfig, SPI2,
    },
    units::FromValueType,
};
use image::RgbImage;
use log::info;
use std::time::Duration;
use mipidsi::interface::SpiInterface;
use mipidsi::models::{Model, ST7796};
use mipidsi::options::{ColorInversion, Orientation};
use mipidsi::Builder;
use mipidsi::{
    models::{ST7735s, ST7789},
    Display,
};
use serde::{Deserialize, Serialize};
use static_cell::StaticCell;

#[derive(Serialize, Deserialize, PartialEq, Clone, Debug)]
pub enum DisplayType {
    ST7735s,
    ST7789,
    ST7796,
}

pub struct DisplayManager<'a> {
    pub display: DisplayInterface,
    pub display_config: DisplayConfig,
    pub font: FontRef<'a>,
}

impl <'a> DisplayManager<'a>{
    /// 屏幕旋转之后，宽高要对调，这样绘制的时候才不会出错
    pub fn get_screen_size(&self) -> (u16, u16){
        match self.display_config.rotation{
            crate::config::DisplayRotation::Deg0 => {
                (self.display_config.width.get(),
                    self.display_config.height.get())
            }
            crate::config::DisplayRotation::Deg90 => {
                (self.display_config.height.get(),
                    self.display_config.width.get())
            }
            crate::config::DisplayRotation::Deg180 => {
                (self.display_config.width.get(),
                    self.display_config.height.get())
            }
            crate::config::DisplayRotation::Deg270 => {
                (self.display_config.height.get(),
                    self.display_config.width.get())
            }
        }
    }

    pub fn get_screen_width(&self) -> u16{
        self.get_screen_size().0
    }

    pub fn get_screen_height(&self) -> u16{
        self.get_screen_size().1
    }
}

pub enum DisplayInterface {
    ST7735s(
        Display<SpiInterface<'static, SpiDeviceDriver<'static, SpiDriver<'static>>, PinDriver<'static, Gpio5, Output>>, ST7735s, PinDriver<'static, Gpio8, Output>>,
    ),
    ST7789(
        Display<SpiInterface<'static, SpiDeviceDriver<'static, SpiDriver<'static>>, PinDriver<'static, Gpio5, Output>>, ST7789, PinDriver<'static, Gpio8, Output>>,
    ),
    ST7796(
        Display<SpiInterface<'static, SpiDeviceDriver<'static, SpiDriver<'static>>, PinDriver<'static, Gpio5, Output>>, ST7796, PinDriver<'static, Gpio8, Output>>,
    ),
}

pub struct DisplayPins {
    pub spi2: SPI2,
    pub cs: Gpio4,
    pub dc: Gpio5,
    pub sclk: Gpio6,
    pub miso_mosi: Gpio7,
    pub rst: Gpio8,
}

pub fn check_screen_size(config: &DisplayConfig) -> Result<()>{
    let to_u32 = |(a, b)| (u32::from(a), u32::from(b));
    let (width, height) = (config.width.get() as u32, config.height.get() as u32);
    let (offset_x, offset_y) = to_u32((config.x_offset, config.y_offset));

    let (max_width, max_height) = match config.display_type {
        DisplayType::ST7735s => {
            to_u32(ST7735s::FRAMEBUFFER_SIZE)
        }
        DisplayType::ST7796 => {
            to_u32(ST7796::FRAMEBUFFER_SIZE)
        }
        DisplayType::ST7789 => {
            to_u32(ST7789::FRAMEBUFFER_SIZE)
        }
    };

    if !(width as u32 + offset_x <= max_width){
        return Err(anyhow!("width+offset_x <= {max_width}"));
    }
    if !(height + offset_y <= max_height){
        return Err(anyhow!("height+offset_y <= {max_height}"));
    }
    Ok(())
}

pub fn init() -> Result<()> {
    with_context(|ctx| {
        let display_config = match ctx.config.display_config.as_ref() {
            Some(c) => c,
            None => {
                return Err(anyhow!("display config is none!"));
            }
        };

        check_screen_size(display_config)?;

        // info!("init display:{display_config:?}");
        let pins = &mut ctx.display_pins;
        // let cs = PinDriver::output(unsafe { pins.cs.clone_unchecked() })?;
        let dc = PinDriver::output(unsafe { pins.dc.clone_unchecked() })?;
        let rst = PinDriver::output(unsafe { pins.rst.clone_unchecked() })?;

        let mut delay = Ets;

        // configuring the spi interface, note that in order for the ST7789 to work, the data_mode needs to be set to MODE_3
        let config = config::Config::new().baudrate(60.MHz().into()).data_mode(
            match display_config.spi_mode {
                1 => MODE_1,
                2 => MODE_2,
                3 => MODE_3,
                _ => MODE_0,
            },
        );

        let sdi_none: Option<Gpio7> = None;
        let cs_none: Option<Gpio4> = None;

        static STATIC_BUFFER: StaticCell<Box<[u8]>> = StaticCell::new();
        let spi_buffer = STATIC_BUFFER.init(Box::new([0u8; 1024]));

        let create_di = move |has_cs| -> Result<SpiInterface<'_, SpiDeviceDriver<'_, SpiDriver<'_>>, PinDriver<'_, Gpio5, Output>>>{
            let spi_device = SpiDeviceDriver::new_single(
                unsafe { pins.spi2.clone_unchecked() },
                unsafe { pins.sclk.clone_unchecked() },
                unsafe { pins.miso_mosi.clone_unchecked() },
                sdi_none,
                if has_cs{ Some(unsafe { pins.cs.clone_unchecked() })  }else{ cs_none },
                &SpiDriverConfig {
                    dma: esp_idf_hal::spi::Dma::Auto(4096),
                    ..Default::default()
                },
                &config,
            )?;
            
            // Define the display interface with no chip select
            let di = SpiInterface::new(spi_device, dc, spi_buffer.as_mut());
            Ok(di)
        };

        // fn handle_window_offset(_: &ModelOptions) -> (u16, u16) {
        //     (unsafe { X_OFFSET }, unsafe { Y_OFFSET })
        // }

        let color_inversion = if display_config.color_inversion {
            ColorInversion::Inverted
        } else {
            ColorInversion::Normal
        };

        info!("init display: width:{}", display_config.width);
        info!("init display: height:{}", display_config.height);
        info!("init display: x_offset:{}", display_config.x_offset);
        info!("init display: y_offset:{}", display_config.y_offset);
        info!("init display: color_inversion:{}", display_config.color_inversion);
        info!("init display: with_cs:{}", display_config.with_cs);
        info!("init display>02: Creating SPI interface...");

        let color_order = match display_config.color_order{
            crate::config::DisplayColorOrder::Rgb => mipidsi::options::ColorOrder::Rgb,
            crate::config::DisplayColorOrder::Bgr => mipidsi::options::ColorOrder::Bgr,
        };

        let mut orientation = Orientation::new();
        orientation.mirrored = display_config.mirrored;
        orientation.rotation = match display_config.rotation{
            crate::config::DisplayRotation::Deg0 => mipidsi::options::Rotation::Deg0,
            crate::config::DisplayRotation::Deg90 => mipidsi::options::Rotation::Deg90,
            crate::config::DisplayRotation::Deg180 => mipidsi::options::Rotation::Deg180,
            crate::config::DisplayRotation::Deg270 => mipidsi::options::Rotation::Deg270,
        };

        info!("init display>03: Creating display interface...");
        let display_interface = match display_config.display_type {
            DisplayType::ST7735s => {
                info!("init display>04: Creating ST7735s display...");
                let di = create_di(true)?;
                // st7735s 驱动
                let display = Builder::new(ST7735s, di)
                    .color_order(color_order)
                    .orientation(orientation)
                    .reset_pin(rst)
                    .display_size(display_config.width.get(), display_config.height.get())
                    .display_offset(display_config.x_offset, display_config.y_offset)
                    .invert_colors(color_inversion)
                    .init(&mut delay)
                    .map_err(|err| anyhow!("{err:?}"))?;
                info!("init display>05: ST7735s display created successfully");
                DisplayInterface::ST7735s(display)
            }
            DisplayType::ST7796 => {
                info!("init display>04: Creating ST7796 display...");
                let di = create_di(true)?;
                // st7796 驱动
                let display = Builder::new(ST7796, di)
                    .color_order(color_order)
                    .orientation(orientation)
                    .reset_pin(rst)
                    .display_size(display_config.width.get(), display_config.height.get())
                    .display_offset(display_config.x_offset, display_config.y_offset)
                    .invert_colors(color_inversion)
                    .init(&mut delay)
                    .map_err(|err| anyhow!("{err:?}"))?;
                info!("init display>05: ST7796 display created successfully");
                DisplayInterface::ST7796(display)
            }
            DisplayType::ST7789 => {
                info!("init display>04: Creating ST7789 display...");
                let di = create_di(display_config.with_cs)?;
                let display = Builder::new(ST7789, di)
                .color_order(color_order)
                .orientation(orientation)
                .reset_pin(rst)
                .display_size(display_config.width.get(), display_config.height.get())
                .display_offset(display_config.x_offset, display_config.y_offset)
                .invert_colors(color_inversion)
                .init(&mut delay)
                .map_err(|err| anyhow!("{err:?}"))?;
                info!("init display>05: ST7789 display created successfully");
                DisplayInterface::ST7789(display)
            }
        };

        info!("init display>06: Loading font...");
        let font = FontRef::try_from_slice(include_bytes!("../VonwaonBitmap-12pxLite.otf"))
            .map_err(|err| anyhow!("{err:?}"))?;
        info!("init display>07: Font loaded successfully");

        info!("init display>08: Creating DisplayManager...");
        let display_manager = DisplayManager {
            display_config: display_config.clone(),
            display: display_interface,
            font,
        };

        ctx.display.replace(display_manager);
        info!("init display>09: DisplayManager created, drawing splash screen...");

        match draw_splash_with_error(ctx, Some("正在初始化"), Some("...")) {
            Ok(_) => {},
            Err(e) => {
                std::thread::sleep(Duration::from_secs(2));
                return Err(e);
            }
        }

        Ok(())
    })
}

pub fn draw_rgb_image_fast(
    display_manager: &mut DisplayManager,
    x: u16,
    y: u16,
    image: &RgbImage,
) -> Result<()> {
    let mut pixels = Box::new(Vec::with_capacity(
        image.width() as usize * image.height() as usize,
    ));
    for pixel in image.pixels() {
        pixels.push(rgb888_to_rgb565(
            pixel[0], pixel[1], pixel[2],
        ).to_be());
    }
    let (width, height) = (image.width() as u16, image.height() as u16);

    let (end_x, end_y) = if display_manager.display_config.inclusive_end_coords{
        (x + width - 1, y + height - 1)
    }else{
        (x + width, y + height)
    };

    match &mut display_manager.display {
        DisplayInterface::ST7735s(display) => {
            display.set_pixels_buffer_u16(x, y, end_x, end_y, pixels.as_mut())
        }
        DisplayInterface::ST7789(display) => {
            display.set_pixels_buffer_u16(x, y, end_x, end_y, pixels.as_mut())
        }
        DisplayInterface::ST7796(display) => {
            display.set_pixels_buffer_u16(x, y, end_x, end_y, pixels.as_mut())
        }
    }
    .map_err(|err| anyhow!("draw error:{err:?}"))?;
    Ok(())
}

pub fn draw_rgb565_fast(
    display_manager: &mut DisplayManager,
    x: u16,
    y: u16,
    width: u16,
    height: u16,
    pixels: &[u16],
) -> Result<()> {
    let (end_x, end_y) = if display_manager.display_config.inclusive_end_coords{
        (x + width - 1, y + height - 1)
    }else{
        (x + width, y + height)
    };
    match &mut display_manager.display {
        DisplayInterface::ST7735s(display) => {
            display.set_pixels_buffer_u16(x, y, end_x, end_y, pixels.as_ref())
        }
        DisplayInterface::ST7789(display) => {
            display.set_pixels_buffer_u16(x, y, end_x, end_y, pixels.as_ref())
        }
        DisplayInterface::ST7796(display) => {
            display.set_pixels_buffer_u16(x, y, end_x, end_y, pixels.as_ref())
        }
    }
    .map_err(|err| anyhow!("draw error:{err:?}"))?;
    Ok(())
}

pub fn draw_rgb565_u8array_fast(
    display_manager: &mut DisplayManager,
    x: u16,
    y: u16,
    width: u16,
    height: u16,
    pixels: &[u8],
) -> Result<()> {
    if pixels.len() != width as usize * height as usize * 2{
        return Err(anyhow!("error: pixels.len() != width*height*2"));
    }
    let (end_x, end_y) = if display_manager.display_config.inclusive_end_coords{
        (x + width - 1, y + height - 1)
    }else{
        (x + width, y + height)
    };
    match &mut display_manager.display {
        DisplayInterface::ST7735s(display) => {
            display.set_pixels_buffer(x, y, end_x, end_y, pixels)
        }
        DisplayInterface::ST7789(display) => {
            display.set_pixels_buffer(x, y, end_x, end_y, pixels)
        }
        DisplayInterface::ST7796(display) => {
            display.set_pixels_buffer(x, y, end_x, end_y, pixels)
        }
    }
    .map_err(|err| anyhow!("draw error:{err:?}"))?;
    Ok(())
}

// #[inline]
// fn rgb888_to_rgb565(r: u8, g: u8, b: u8) -> u16 {
//     // 缩放颜色分量到目标位数
//     let r5 = (r as u16 * 31 / 255) << 11; // 5 bits for red, shift left by 11 bits
//     let g6 = (g as u16 * 63 / 255) << 5;  // 6 bits for green, shift left by 5 bits
//     let b5 = b as u16 * 31 / 255;         // 5 bits for blue

//     // 组合成16位RGB565值
//     (r5 | g6 | b5) as u16
// }

macro_rules! generate_lut {
    ($name:ident, $factor:expr, $shift:expr) => {
        const $name: [u16; 256] = {
            let mut lut = [0u16; 256];
            let mut i = 0;
            while i < 256 {
                lut[i] = ((i as u16 * $factor) / 255) << $shift;
                i += 1;
            }
            lut
        };
    };
}

generate_lut!(RGB565_R_LUT, 31, 11); // 红色：5位，左移11位
generate_lut!(RGB565_G_LUT, 63, 5); // 绿色：6位，左移5位
generate_lut!(RGB565_B_LUT, 31, 0); // 蓝色：5位，不移位

#[inline(always)]
pub fn rgb888_to_rgb565(r: u8, g: u8, b: u8) -> u16 {
    // 使用查找表获取缩放后的颜色分量
    let r5 = RGB565_R_LUT[r as usize];
    let g6 = RGB565_G_LUT[g as usize];
    let b5 = RGB565_B_LUT[b as usize];

    // 组合成16位RGB565值
    r5 | g6 | b5
}

#[inline(always)]
pub fn rgb565_to_rgb888(pixel: u16) -> (u8, u8, u8) {
    // 分离颜色分量
    let r = ((pixel >> 11) & 0x1F) as u8; // 5 bits for red
    let g = ((pixel >> 5) & 0x3F) as u8; // 6 bits for green
    let b = (pixel & 0x1F) as u8; // 5 bits for blue

    // 扩展颜色分量到8位
    let r8 = (r as u16 * 255 / 31) as u8;
    let g8 = (g as u16 * 255 / 63) as u8;
    let b8 = (b as u16 * 255 / 31) as u8;

    (r8, g8, b8)
}
