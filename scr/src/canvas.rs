use ab_glyph::{point, Font, FontRef, GlyphId, OutlinedGlyph, PxScale, ScaleFont};
use anyhow::{anyhow, Result};
use csscolorparser::Color;
use embedded_graphics::geometry::AngleUnit;
use embedded_graphics::prelude::{Point, Primitive, RgbColor, Size};
use embedded_graphics::primitives::{PrimitiveStyle, PrimitiveStyleBuilder};
use embedded_graphics::pixelcolor::Rgb888;
use image::imageops::overlay;
use image::{Pixel, Rgb, RgbImage, Rgba, RgbaImage};
use serde::{Deserialize, Deserializer};
use std::collections::HashMap;
use crate::utils::decode_base64;
use crate::{
    display::{draw_rgb_image_fast, rgb565_to_rgb888, DisplayManager},
    imageproc::{drawing::text_size, pixelops::weighted_sum},
    tjpgd, with_context, Context,
};

use crate::{ImageCache, WIFI_AP_SSID};

#[derive(Clone)]
pub struct CSSColor(csscolorparser::Color);

impl CSSColor {
    pub fn rgba(&self) -> [u8; 4] {
        self.0.to_rgba8()
    }
}

impl<'de> Deserialize<'de> for CSSColor {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s: String = Deserialize::deserialize(deserializer)?;
        csscolorparser::parse(&s)
            .map(|c| CSSColor(c))
            .map_err(|err| serde::de::Error::custom(format_args!("invalid css color: {err:?}")))
    }
}

#[derive(Clone, Deserialize)]
pub enum Element {
    Text(Text),
    // #[serde(skip)]
    // TextWithFont((Text, FontRef<'static>)),
    Image(Image),
    #[serde(skip)]
    #[allow(dead_code)]
    RawImage((i32, i32, Box<RgbaImage>)),
    #[serde(skip)]
    RawRgbImage((i32, i32, Box<RgbImage>)),
    Line(Line),
    Circle(Circle),
    Ellipse(Ellipse),
    Arc(Arc),
    Sector(Sector),
    Rectangle(Rectangle),
    RoundedRectangle(RoundedRectangle),
    Polyline(Polyline),
    Triangle(Triangle),
}

#[derive(Clone, Deserialize)]
pub struct Text {
    x: i32,
    y: i32,
    text: String,
    size: f32,
    color: CSSColor,
}

#[derive(Clone, Deserialize)]
pub struct Line {
    start: (i32, i32),
    end: (i32, i32),
    stroke_width: u32,
    color: CSSColor,
}

#[derive(Clone, Deserialize)]
pub struct Rectangle {
    left: i32,
    top: i32,
    width: u32,
    height: u32,
    stroke_width: u32,
    fill_color: Option<CSSColor>,
    stroke_color: Option<CSSColor>,
}

#[derive(Clone, Deserialize)]
pub struct RoundedRectangle {
    left: i32,
    top: i32,
    width: u32,
    height: u32,
    stroke_width: u32,
    fill_color: Option<CSSColor>,
    stroke_color: Option<CSSColor>,
    top_left_corner: (u32, u32),
    top_right_corner: (u32, u32),
    bottom_right_corner: (u32, u32),
    bottom_left_corner: (u32, u32),
}

#[derive(Clone, Deserialize)]
pub struct Circle {
    top_left: (i32, i32),
    diameter: u32,
    stroke_width: u32,
    fill_color: Option<CSSColor>,
    stroke_color: Option<CSSColor>,
}

#[derive(Clone, Deserialize)]
pub struct Arc {
    top_left: (i32, i32),
    diameter: u32,
    stroke_width: u32,
    angle_start: f32,
    angle_sweep: f32,
    color: CSSColor,
}

#[derive(Clone, Deserialize)]
pub struct Sector {
    top_left: (i32, i32),
    diameter: u32,
    stroke_width: u32,
    angle_start: f32,
    angle_sweep: f32,
    fill_color: Option<CSSColor>,
    stroke_color: Option<CSSColor>,
}

#[derive(Clone, Deserialize)]
pub struct Polyline {
    points: Vec<(i32, i32)>,
    stroke_width: u32,
    color: CSSColor,
}

#[derive(Clone, Debug, Deserialize)]
pub struct Image {
    x: i32,
    y: i32,
    key: Option<String>,
    base64: Option<Box<String>>,
}

#[derive(Clone, Deserialize)]
pub struct Ellipse {
    top_left: (i32, i32),
    size: (u32, u32),
    stroke_width: u32,
    fill_color: Option<CSSColor>,
    stroke_color: Option<CSSColor>,
}

#[derive(Clone, Deserialize)]
pub struct Triangle {
    vertex1: (i32, i32),
    vertex2: (i32, i32),
    vertex3: (i32, i32),
    stroke_width: u32,
    fill_color: Option<CSSColor>,
    stroke_color: Option<CSSColor>,
}

pub fn draw_elements(
    display_manager: &mut DisplayManager,
    image_cache: &HashMap<String, ImageCache>,
    elements: &[Element],
) -> Result<()> {
    let (width, height) = display_manager.get_screen_size();
    let (width, height) = (width as u32, height as u32);

    let mut canvas =  Box::new(RgbImage::new(width, height));

    for element in elements {
        match element {
            Element::Text(text) => {
                draw_text(
                    &mut canvas,
                    text.x,
                    text.y,
                    &display_manager.font,
                    text.size,
                    &text.text,
                    Rgba(text.color.rgba()),
                )?;
            }
            // Element::TextWithFont((text, font)) => {
            //     draw_text_mut(canvas.as_mut(), Rgba(text.color.to_rgba8()), text.x, text.y, text.size, font, &text.text);
            // }
            Element::RawRgbImage((x, y, image)) => {
                draw_rgb_image(&mut canvas, image, *x as i64, *y as i64)?;
            }
            Element::RawImage((x, y, image)) => {
                draw_image(&mut canvas, image, *x as i64, *y as i64)?;
            }
            Element::Image(image) => {
                if let Some(key) = &image.key {
                    match image_cache.get(key) {
                        Some(img) => {
                            match img {
                                ImageCache::RgbImage(img) => {
                                    draw_rgb_image(
                                        &mut canvas,
                                        img,
                                        image.x as i64,
                                        image.y as i64,
                                    )?;
                                }
                                ImageCache::RgbaImage(img) => {
                                    draw_image(&mut canvas, img, image.x as i64, image.y as i64)?;
                                }
                            }
                            continue;
                        }
                        None => {
                            return Err(anyhow!("image key not exist:{key}"));
                        }
                    }
                }

                if let Some(b64) = &image.base64 {
                    let image_data = decode_base64(b64.as_str())?;
                    let mime = mimetype::detect(&image_data);
                    if mime.extension.ends_with("jpg") || mime.extension.ends_with("jpeg") {
                        let img = decode_jpg_to_rgb(image_data)
                            .map_err(|err| anyhow!("decode jpg:{err:?}"))?;
                        draw_rgb_image(&mut canvas, &img, image.x as i64, image.y as i64)?;
                    } else {
                        let img = image::load_from_memory(&image_data)?.to_rgba8();
                        draw_image(&mut canvas, &img, image.x as i64, image.y as i64)?;
                    }
                    continue;
                }
                return Err(anyhow!("请填写图像的\"key\"或者\"base64\"字符串"));
            }
            Element::Line(line) => {
                let color = line.color.rgba();
                let pixels = embedded_graphics::primitives::Line::new(
                    Point::new(line.start.0, line.start.1),
                    Point::new(line.end.0, line.end.1),
                )
                .into_styled(PrimitiveStyle::with_stroke(
                    Rgb888::new(color[0], color[1], color[2]),
                    line.stroke_width,
                )).pixels();
                for p in pixels{
                    let pt = p.0;
                    if (0..canvas.width() as i32).contains(&pt.x) && (0..canvas.height() as i32).contains(&pt.y){
                        let c = p.1;
                        *canvas.get_pixel_mut(pt.x as u32, pt.y as u32) = Rgb([c.r(), c.g(), c.b()]);
                    }
                }
            }
            Element::Triangle(triangle) => {
                let mut builder = PrimitiveStyleBuilder::new();
                builder = builder.stroke_width(triangle.stroke_width);
                if let Some(stroke_color) = triangle.stroke_color.as_ref() {
                    let stroke_color = stroke_color.rgba();
                    builder = builder.stroke_color(Rgb888::new(
                        stroke_color[0],
                        stroke_color[1],
                        stroke_color[2],
                    ));
                }
                if let Some(fill_color) = triangle.fill_color.as_ref() {
                    let fill_color = fill_color.rgba();
                    builder = builder.fill_color(Rgb888::new(
                        fill_color[0],
                        fill_color[1],
                        fill_color[2],
                    ));
                }

                let pixels = embedded_graphics::primitives::Triangle::new(
                    Point::new(triangle.vertex1.0, triangle.vertex1.1),
                    Point::new(triangle.vertex2.0, triangle.vertex2.1),
                    Point::new(triangle.vertex3.0, triangle.vertex3.1),
                )
                .into_styled(builder.build())
                .pixels();
                for p in pixels{
                    let pt = p.0;
                    if (0..canvas.width() as i32).contains(&pt.x) && (0..canvas.height() as i32).contains(&pt.y){
                        let c = p.1;
                        *canvas.get_pixel_mut(pt.x as u32, pt.y as u32) = Rgb([c.r(), c.g(), c.b()]);
                    }
                }
            }
            Element::Circle(circle) => {
                let mut builder = PrimitiveStyleBuilder::new();
                builder = builder.stroke_width(circle.stroke_width);
                if let Some(stroke_color) = circle.stroke_color.as_ref() {
                    let stroke_color = stroke_color.rgba();
                    builder = builder.stroke_color(Rgb888::new(
                        stroke_color[0],
                        stroke_color[1],
                        stroke_color[2],
                    ));
                }
                if let Some(fill_color) = circle.fill_color.as_ref() {
                    let fill_color = fill_color.rgba();
                    builder = builder.fill_color(Rgb888::new(
                        fill_color[0],
                        fill_color[1],
                        fill_color[2],
                    ));
                }

                let pixels = embedded_graphics::primitives::Circle::new(
                    Point::new(circle.top_left.0, circle.top_left.1),
                    circle.diameter,
                )
                .into_styled(builder.build())
                .pixels();
                for p in pixels{
                    let pt = p.0;
                    if (0..canvas.width() as i32).contains(&pt.x) && (0..canvas.height() as i32).contains(&pt.y){
                        let c = p.1;
                        *canvas.get_pixel_mut(pt.x as u32, pt.y as u32) = Rgb([c.r(), c.g(), c.b()]);
                    }
                }
            }
            Element::Ellipse(ellipse) => {
                let mut builder = PrimitiveStyleBuilder::new();
                builder = builder.stroke_width(ellipse.stroke_width);
                if let Some(stroke_color) = ellipse.stroke_color.as_ref() {
                    let stroke_color = stroke_color.rgba();
                    builder = builder.stroke_color(Rgb888::new(
                        stroke_color[0],
                        stroke_color[1],
                        stroke_color[2],
                    ));
                }
                if let Some(fill_color) = ellipse.fill_color.as_ref() {
                    let fill_color = fill_color.rgba();
                    builder = builder.fill_color(Rgb888::new(
                        fill_color[0],
                        fill_color[1],
                        fill_color[2],
                    ));
                }

                let pixels = embedded_graphics::primitives::Ellipse::new(
                    Point::new(ellipse.top_left.0, ellipse.top_left.1),
                    Size::new(ellipse.size.0, ellipse.size.1),
                )
                .into_styled(builder.build())
                .pixels();
                for p in pixels{
                    let pt = p.0;
                    if (0..canvas.width() as i32).contains(&pt.x) && (0..canvas.height() as i32).contains(&pt.y){
                        let c = p.1;
                        *canvas.get_pixel_mut(pt.x as u32, pt.y as u32) = Rgb([c.r(), c.g(), c.b()]);
                    }
                }
            }
            Element::RoundedRectangle(rect) => {
                let mut builder = PrimitiveStyleBuilder::new();
                builder = builder.stroke_width(rect.stroke_width);
                if let Some(stroke_color) = rect.stroke_color.as_ref() {
                    let stroke_color = stroke_color.rgba();
                    builder = builder.stroke_color(Rgb888::new(
                        stroke_color[0],
                        stroke_color[1],
                        stroke_color[2],
                    ));
                }
                if let Some(fill_color) = rect.fill_color.as_ref() {
                    let fill_color = fill_color.rgba();
                    builder = builder.fill_color(Rgb888::new(
                        fill_color[0],
                        fill_color[1],
                        fill_color[2],
                    ));
                }
                let corner = embedded_graphics::primitives::CornerRadii {
                    top_left: Size::new(rect.top_left_corner.0, rect.top_left_corner.1),
                    top_right: Size::new(rect.top_right_corner.0, rect.top_right_corner.1),
                    bottom_right: Size::new(rect.bottom_right_corner.0, rect.bottom_right_corner.1),
                    bottom_left: Size::new(rect.bottom_left_corner.0, rect.bottom_left_corner.1),
                };
                let pixels = embedded_graphics::primitives::RoundedRectangle::new(
                    embedded_graphics::primitives::Rectangle::new(
                        Point::new(rect.left, rect.top),
                        Size::new(rect.width, rect.height),
                    ),
                    corner,
                )
                .into_styled(builder.build())
                .pixels();
                for p in pixels{
                    let pt = p.0;
                    if (0..canvas.width() as i32).contains(&pt.x) && (0..canvas.height() as i32).contains(&pt.y){
                        let c = p.1;
                        *canvas.get_pixel_mut(pt.x as u32, pt.y as u32) = Rgb([c.r(), c.g(), c.b()]);
                    }
                }
            }
            Element::Rectangle(rect) => {
                let mut builder = PrimitiveStyleBuilder::new();
                builder = builder.stroke_width(rect.stroke_width);
                if let Some(stroke_color) = rect.stroke_color.as_ref() {
                    let stroke_color = stroke_color.rgba();
                    builder = builder.stroke_color(Rgb888::new(
                        stroke_color[0],
                        stroke_color[1],
                        stroke_color[2],
                    ));
                }
                if let Some(fill_color) = rect.fill_color.as_ref() {
                    let fill_color = fill_color.rgba();
                    builder = builder.fill_color(Rgb888::new(
                        fill_color[0],
                        fill_color[1],
                        fill_color[2],
                    ));
                }

                let pixels = embedded_graphics::primitives::Rectangle::new(
                    Point::new(rect.left, rect.top),
                    Size::new(rect.width, rect.height),
                )
                .into_styled(builder.build())
                .pixels();
                for p in pixels{
                    let pt = p.0;
                    if (0..canvas.width() as i32).contains(&pt.x) && (0..canvas.height() as i32).contains(&pt.y){
                        let c = p.1;
                        *canvas.get_pixel_mut(pt.x as u32, pt.y as u32) = Rgb([c.r(), c.g(), c.b()]);
                    }
                }
            }
            Element::Arc(arc) => {
                let stroke_color = arc.color.rgba();
                let pixels = embedded_graphics::primitives::Arc::new(
                    Point::new(arc.top_left.0, arc.top_left.1),
                    arc.diameter,
                    arc.angle_start.deg(),
                    arc.angle_sweep.deg(),
                )
                .into_styled(PrimitiveStyle::with_stroke(
                    Rgb888::new(stroke_color[0], stroke_color[1], stroke_color[2]),
                    arc.stroke_width,
                ))
                .pixels();
                for p in pixels{
                    let pt = p.0;
                    if (0..canvas.width() as i32).contains(&pt.x) && (0..canvas.height() as i32).contains(&pt.y){
                        let c = p.1;
                        *canvas.get_pixel_mut(pt.x as u32, pt.y as u32) = Rgb([c.r(), c.g(), c.b()]);
                    }
                }
            }
            Element::Sector(sector) => {
                let mut builder = PrimitiveStyleBuilder::new();
                builder = builder.stroke_width(sector.stroke_width);
                if let Some(stroke_color) = sector.stroke_color.as_ref() {
                    let stroke_color = stroke_color.rgba();
                    builder = builder.stroke_color(Rgb888::new(
                        stroke_color[0],
                        stroke_color[1],
                        stroke_color[2],
                    ));
                }
                if let Some(fill_color) = sector.fill_color.as_ref() {
                    let fill_color = fill_color.rgba();
                    builder = builder.fill_color(Rgb888::new(
                        fill_color[0],
                        fill_color[1],
                        fill_color[2],
                    ));
                }
                let pixels = embedded_graphics::primitives::Sector::new(
                    Point::new(sector.top_left.0, sector.top_left.1),
                    sector.diameter,
                    sector.angle_start.deg(),
                    sector.angle_sweep.deg(),
                )
                .into_styled(builder.build())
                .pixels();
                for p in pixels{
                    let pt = p.0;
                    if (0..canvas.width() as i32).contains(&pt.x) && (0..canvas.height() as i32).contains(&pt.y){
                        let c = p.1;
                        *canvas.get_pixel_mut(pt.x as u32, pt.y as u32) = Rgb([c.r(), c.g(), c.b()]);
                    }
                }
            }
            Element::Polyline(polyline) => {
                let mut points = vec![];
                let stroke_color = polyline.color.rgba();
                for (x, y) in &polyline.points {
                    points.push(Point::new(*x, *y));
                }
                let pixels = embedded_graphics::primitives::Polyline::new(&points)
                    .into_styled(PrimitiveStyle::with_stroke(
                        Rgb888::new(stroke_color[0], stroke_color[1], stroke_color[2]),
                        polyline.stroke_width,
                    ))
                    .pixels();
                for p in pixels{
                    let pt = p.0;
                    if (0..canvas.width() as i32).contains(&pt.x) && (0..canvas.height() as i32).contains(&pt.y){
                        let c = p.1;
                        *canvas.get_pixel_mut(pt.x as u32, pt.y as u32) = Rgb([c.r(), c.g(), c.b()]);
                    }
                }
            }
        }
    }
    draw_rgb_image_fast(display_manager, 0, 0, &canvas)?;
    Ok(())
}

pub fn generate_wifi_name_text(
    display_manager: &mut DisplayManager,
    wifi_ssid: &str,
    ip: &str,
) -> Vec<Element> {
    let mut elements = vec![];
    let font_size = 20.;
    let text_color = Color::new(0., 1., 0., 1.);
    let wifi_name = format!("已连接:{wifi_ssid}");
    //绘制wifi名字
    let (text_width, _) = text_size(font_size, &display_manager.font, &wifi_name);
    let text_x = display_manager.get_screen_width() as i32 / 2 - text_width as i32 / 2;
    elements.push(Element::Text(Text {
        x: text_x,
        y: 100,
        text: wifi_name,
        size: font_size,
        color: CSSColor(text_color.clone()),
    }));
    //绘制ip地址
    let (text_width, _) = text_size(font_size, &display_manager.font, &ip);
    let text_x = display_manager.get_screen_width() as i32 / 2 - text_width as i32 / 2;
    elements.push(Element::Text(Text {
        x: text_x,
        y: 123,
        text: ip.to_string(),
        size: font_size,
        color: CSSColor(text_color.clone()),
    }));
    //绘制横线
    elements.push(Element::Line(Line {
        start: (text_x, 123 + 21),
        end: (text_x + text_width as i32, 123 + 21),
        stroke_width: 1,
        color: CSSColor(text_color.clone()),
    }));
    elements
}

pub fn generate_no_wifi_name_text(display_manager: &mut DisplayManager) -> Vec<Element> {
    let mut elements = vec![];
    let font_size = 20.;
    let text_color = Color::new(1., 0.647, 0., 1.);
    let wifi_name = format!("WiFi未连接");
    //绘制wifi名字
    let (text_width, _) = text_size(font_size, &display_manager.font, &wifi_name);
    let text_x = display_manager.get_screen_width() as i32 / 2 - text_width as i32 / 2;
    elements.push(Element::Text(Text {
        x: text_x,
        y: 100,
        text: wifi_name,
        size: font_size,
        color: CSSColor(text_color.clone()),
    }));
    elements
}

// 绘制闪屏，日志信息
pub fn draw_splash(ctx: &mut Context, add_elements: &[Element]) -> Result<()> {
    let display_manager = match ctx.display.as_mut() {
        Some(v) => v,
        None => return Ok(()),
    };
    let mut elements = Box::new(vec![]);
    let screen_width = display_manager.get_screen_width() as u32;
    let screen_height = display_manager.get_screen_height() as u32;

    //绘制底色
    elements.push(Element::Rectangle(Rectangle {
        left: 0,
        top: 0,
        width: screen_width,
        height: screen_height,
        stroke_width: 0,
        fill_color: Some(CSSColor(Color::new(0.0666, 0.0666, 0.0666, 1.))),
        stroke_color: None,
    }));

    //绘制logo
    let logo = Box::new(Vec::from(include_bytes!("../monitor.jpg")));
    let logo = decode_jpg_to_rgb(logo)?;

    elements.push(Element::RawRgbImage((
        screen_width as i32 / 2 - logo.width() as i32 / 2,
        15,
        logo,
    )));

    //热点名字
    let font_size = 20.;
    let wifi_label = "WiFi热点:";
    let (text_width, _) = text_size(font_size, &display_manager.font, &wifi_label);
    let text_x = display_manager.get_screen_width() as i32 / 2 - text_width as i32 / 2;
    elements.push(Element::Text(Text {
        x: text_x,
        y: 52,
        text: wifi_label.to_string(),
        size: font_size,
        color: CSSColor(Color::new(1., 1., 1., 1.)),
    }));

    //绘制wifi名字
    let (text_width, _) = text_size(font_size, &display_manager.font, WIFI_AP_SSID);
    let text_x = display_manager.get_screen_width() as i32 / 2 - text_width as i32 / 2;
    elements.push(Element::Text(Text {
        x: text_x,
        y: 77,
        text: WIFI_AP_SSID.to_string(),
        size: font_size,
        color: CSSColor(Color::new(1., 1., 1., 1.)),
    }));

    elements.extend_from_slice(add_elements);

    let mut wifi_connected = false;
    if let Some(cfg) = ctx.config.wifi_config.as_mut() {
        if let (Ok(ip_info), Ok(true)) = (
            ctx.wifi.wifi().sta_netif().get_ip_info(),
            ctx.wifi.is_connected(),
        ) {
            cfg.device_ip = Some(ip_info.ip.clone());
            // cfg.gateway_ip = Some(ip_info.subnet.gateway.clone());
            let el =
                generate_wifi_name_text(display_manager, &cfg.ssid, &format!("{}", ip_info.ip));
            elements.extend_from_slice(&el);
            wifi_connected = true;
        } else {
            cfg.device_ip = None;
            // cfg.gateway_ip = None;
        }
    }

    if !wifi_connected {
        let el = generate_no_wifi_name_text(display_manager);
        elements.extend_from_slice(&el);
    }

    draw_elements(display_manager, &HashMap::new(), &elements)?;
    Ok(())
}

pub fn decode_jpg_to_rgb(jpg_data: Box<Vec<u8>>) -> Result<Box<RgbImage>> {
    let (_, w, h, pixels) = tjpgd::decode_jpg(jpg_data)?;
    let mut rgb = Vec::with_capacity(w as usize * h as usize * 3);
    for pixel in pixels.iter() {
        let (r, g, b) = rgb565_to_rgb888(pixel.to_be());
        rgb.extend_from_slice(&[r, g, b]);
    }
    Ok(Box::new(RgbImage::from_raw(w as u32, h as u32, rgb).unwrap()))
}

pub fn draw_splash_with_error1(err1: Option<&str>, err2: Option<&str>) -> Result<()> {
    with_context(move |ctx| draw_splash_with_error(ctx, err1, err2))
}

// 绘制闪屏，日志信息
pub fn draw_splash_with_error(
    ctx: &mut Context,
    err1: Option<&str>,
    err2: Option<&str>,
) -> Result<()> {
    let display_manager = match ctx.display.as_mut() {
        Some(v) => v,
        None => return Ok(()),
    };
    let mut elements = Box::new(vec![]);
    let font_size = 20.;
    let text_color = Color::new(1., 0., 0., 1.);
    if let Some(err1) = err1 {
        let (text_width, _) = text_size(font_size, &display_manager.font, err1);
        let text_x = display_manager.get_screen_width() as i32 / 2 - text_width as i32 / 2;
        elements.push(Element::Text(Text {
            x: text_x,
            y: 160,
            text: err1.to_string(),
            size: font_size,
            color: CSSColor(text_color.clone()),
        }));
    }
    if let Some(err2) = err2 {
        let (text_width, _) = text_size(font_size, &display_manager.font, err2);
        let text_x = display_manager.get_screen_width() as i32 / 2 - text_width as i32 / 2;
        elements.push(Element::Text(Text {
            x: text_x,
            y: 185,
            text: err2.to_string(),
            size: font_size,
            color: CSSColor(text_color.clone()),
        }));
    }
    draw_splash(ctx, &elements)?;
    Ok(())
}

fn layout_glyphs(
    scale: impl Into<PxScale> + Copy,
    font: &impl Font,
    text: &str,
    mut f: impl FnMut(OutlinedGlyph, ab_glyph::Rect),
) -> (u32, u32) {
    if text.is_empty() {
        return (0, 0);
    }
    let font = font.as_scaled(scale);

    let mut w = 0.0;
    let mut prev: Option<GlyphId> = None;

    for c in text.chars() {
        let glyph_id = font.glyph_id(c);
        let glyph = glyph_id.with_scale_and_position(scale, point(w, font.ascent()));
        w += font.h_advance(glyph_id);
        if let Some(g) = font.outline_glyph(glyph) {
            if let Some(prev) = prev {
                w += font.kern(glyph_id, prev);
            }
            prev = Some(glyph_id);
            let bb = g.px_bounds();
            f(g, bb);
        }
    }

    let w = w.ceil();
    let h = font.height().ceil();
    assert!(w >= 0.0);
    assert!(h >= 0.0);
    (1 + w as u32, h as u32)
}

fn draw_text<'a>(
    target: &mut RgbImage,
    x: i32,
    y: i32,
    font: &FontRef<'a>,
    font_size: f32,
    text: &str,
    color: Rgba<u8>,
) -> Result<()> {
    let image_width = target.width() as i32;
    let image_height = target.height() as i32;

    layout_glyphs(font_size, font, text, |g, bb| {
        let x_shift = x + bb.min.x.round() as i32;
        let y_shift = y + bb.min.y.round() as i32;
        g.draw(|gx, gy, gv| {
            let image_x = gx as i32 + x_shift;
            let image_y = gy as i32 + y_shift;

            if (0..image_width).contains(&image_x) && (0..image_height).contains(&image_y) {
                let src_pixel = target.get_pixel_mut_checked(image_x as u32, image_y as u32).unwrap();
                let pixel = src_pixel.to_rgba();
                let gv = gv.clamp(0.0, 1.0);
                let weighted_color = weighted_sum(pixel, color, 1.0 - gv, gv);
                *src_pixel = weighted_color.to_rgb();
            }
        })
    });
    Ok(())
}

/// Calculate the region that can be copied from top to bottom.
///
/// Given image size of bottom and top image, and a point at which we want to place the top image
/// onto the bottom image, how large can we be? Have to wary of the following issues:
/// * Top might be larger than bottom
/// * Overflows in the computation
/// * Coordinates could be completely out of bounds
///
/// The returned value is of the form:
///
/// `(origin_bottom_x, origin_bottom_y, origin_top_x, origin_top_y, x_range, y_range)`
///
/// The main idea is to do computations on i64's and then clamp to image dimensions.
/// In particular, we want to ensure that all these coordinate accesses are safe:
/// 1. `bottom.get_pixel(origin_bottom_x + [0..x_range), origin_bottom_y + [0..y_range))`
/// 2. `top.get_pixel(origin_top_y + [0..x_range), origin_top_y + [0..y_range))`
///
fn overlay_bounds_ext(
    (bottom_width, bottom_height): (u32, u32),
    (top_width, top_height): (u32, u32),
    x: i64,
    y: i64,
) -> (u32, u32, u32, u32, u32, u32) {
    // Return a predictable value if the two images don't overlap at all.
    if x > i64::from(bottom_width)
        || y > i64::from(bottom_height)
        || x.saturating_add(i64::from(top_width)) <= 0
        || y.saturating_add(i64::from(top_height)) <= 0
    {
        return (0, 0, 0, 0, 0, 0);
    }

    // Find the maximum x and y coordinates in terms of the bottom image.
    let max_x = x.saturating_add(i64::from(top_width));
    let max_y = y.saturating_add(i64::from(top_height));

    // Clip the origin and maximum coordinates to the bounds of the bottom image.
    // Casting to a u32 is safe because both 0 and `bottom_{width,height}` fit
    // into 32-bits.
    let max_inbounds_x = max_x.clamp(0, i64::from(bottom_width)) as u32;
    let max_inbounds_y = max_y.clamp(0, i64::from(bottom_height)) as u32;
    let origin_bottom_x = x.clamp(0, i64::from(bottom_width)) as u32;
    let origin_bottom_y = y.clamp(0, i64::from(bottom_height)) as u32;

    // The range is the difference between the maximum inbounds coordinates and
    // the clipped origin. Unchecked subtraction is safe here because both are
    // always positive and `max_inbounds_{x,y}` >= `origin_{x,y}` due to
    // `top_{width,height}` being >= 0.
    let x_range = max_inbounds_x - origin_bottom_x;
    let y_range = max_inbounds_y - origin_bottom_y;

    // If x (or y) is negative, then the origin of the top image is shifted by -x (or -y).
    let origin_top_x = x.saturating_mul(-1).clamp(0, i64::from(top_width)) as u32;
    let origin_top_y = y.saturating_mul(-1).clamp(0, i64::from(top_height)) as u32;

    (
        origin_bottom_x,
        origin_bottom_y,
        origin_top_x,
        origin_top_y,
        x_range,
        y_range,
    )
}

/// Overlay an image at a given coordinate (x, y)
fn draw_image(bottom: &mut RgbImage, top: &RgbaImage, x: i64, y: i64) -> Result<()> {
    let bottom_dims = (bottom.width(), bottom.height());
    let top_dims = top.dimensions();

    // Crop our top image if we're going out of bounds
    let (origin_bottom_x, origin_bottom_y, origin_top_x, origin_top_y, range_width, range_height) =
        overlay_bounds_ext(bottom_dims, top_dims, x, y);

    for y in 0..range_height {
        for x in 0..range_width {
            let p = top.get_pixel(origin_top_x + x, origin_top_y + y);
            let (o_x, o_y) = (origin_bottom_x + x, origin_bottom_y + y);
            // let idx = o_y as usize * bottom_dims.0 as usize + o_x as usize;
            if (0..bottom.width()).contains(&o_x) && (0..bottom.height()).contains(&o_y) {
                let src_pixel = bottom.get_pixel_mut_checked(o_x, o_y).unwrap();
                let mut bottom_pixel = src_pixel.to_rgba();
                image::Pixel::blend(&mut bottom_pixel, &p);
                *src_pixel = bottom_pixel.to_rgb();
            }
        }
    }
    Ok(())
}

/// Overlay an image at a given coordinate (x, y)
fn draw_rgb_image(bottom: &mut RgbImage, top: &RgbImage, x: i64, y: i64) -> Result<()> {
    overlay(bottom, top, x, y);
    Ok(())
}
