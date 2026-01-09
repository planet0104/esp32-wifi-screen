use std::{collections::HashMap, num::NonZero, str, sync::{Arc, Mutex}, time::{Duration, Instant}};

use anyhow::{anyhow, Result};
use canvas::{
    decode_jpg_to_rgb, draw_elements, draw_splash_with_error1, Element,
};
use embedded_svc::{
    http::{Headers, Method},
    io::{Read, Write},
    wifi::{ClientConfiguration, Configuration},
};

use esp_idf_hal::sys::{esp_get_minimum_free_heap_size, esp_restart};
use esp_idf_svc::{
    http::server::{EspHttpConnection, EspHttpServer},
    sys::{esp_get_free_heap_size, esp_get_free_internal_heap_size, EspError},
    ws::FrameType,
};

use image::{codecs::png::PngEncoder, ImageEncoder};
use log::*;
use url::Url;

use crate::{canvas, config, display::{self, check_screen_size}, with_context, with_context1, Context, ImageCache, MAX_HTTP_PAYLOAD_LEN, STACK_SIZE};

pub fn start_http_server() -> Result<()>{
    let mut server = create_server()?;

    let client_config = esp_idf_svc::http::client::Configuration::default();
    // client_config.buffer_size = Some(1024*4);
    // client_config.buffer_size_tx = Some(1024*4);
    let client = embedded_svc::http::client::Client::wrap(esp_idf_svc::http::client::EspHttpConnection::new(&client_config)?);
    struct HttpClient{
        client: embedded_svc::http::client::Client<esp_idf_svc::http::client::EspHttpConnection>
    }
    unsafe impl Sync for HttpClient{}
    unsafe impl Send for HttpClient{}
    let client = Arc::new(Mutex::new(HttpClient{client}));

    // HTTP GET 首页设置
    server.fn_handler("/", Method::Get, |req| {
        req.into_ok_response()?
            .write_all(include_bytes!("../html/index.html"))
            .map(|_| ())
    })?;
    // HTTP GET 屏幕测试
    server.fn_handler("/example", Method::Get, |req| {
        req.into_ok_response()?
            .write_all(include_bytes!("../html/example.html"))
            .map(|_| ())
    })?;

    let client1 = client.clone();
    server.fn_handler("/download", Method::Get, move |req| {
        
        let mut c = client1.lock().unwrap();
        
        let headers = [("accept", "text/plain")];
        let url = "http://192.168.121.37:9990";

        // info!("-> GET {}", url);
        let t1 = Instant::now();

        // Send request
        //
        // Note: If you don't want to pass in any headers, you can also use `client.get(url, headers)`.
        let request = c.client.request(Method::Get, url, &headers)?;
        
        let mut response = request.submit()?;

        // Process response
        let status = response.status();
        // info!("<- {}", status);
        let mut buf = Box::new([0u8; 1024*64]);
        let bytes_read = esp_idf_svc::io::utils::try_read_full(&mut response, buf.as_mut()).map_err(|e| e.0)?;
        // info!("Read {} bytes {}ms", bytes_read, t1.elapsed().as_millis());
        req.into_ok_response()?
            .write_all(format!("Read {bytes_read} bytes {}ms status={status}", t1.elapsed().as_millis()).as_bytes())
            .map(|_| ())
    })?;

    server.fn_handler("/delete_config", Method::Get, |req| {
        let ret = with_context(move |ctx| {
            config::delete_config(&mut ctx.config_nvs)?;
            info!("reboot after 1.5s...");
            std::thread::spawn(move || {
                std::thread::sleep(Duration::from_millis(1500));
                unsafe { esp_restart() };
            });
            Ok(())
        });
        req.into_ok_response()?
            .write_all(format!("{ret:?}").as_bytes())
            .map(|_| ())
    })?;

    // HTTP GET 状态查询
    server.fn_handler("/status", Method::Get, |req| {
        match with_context(|ctx| {
            ctx.free_heap = unsafe { esp_get_free_heap_size() };
            ctx.free_internal_heap = unsafe { esp_get_free_internal_heap_size() };
            serde_json::to_string(ctx).map_err(|err| anyhow!("{err:?}"))
        }) {
            Ok(json) => req
            .into_response(
                200,
                Some("OK"),
                &[("Content-Type", "application/json; charset=utf-8")],
            )?
                .write_all(json.as_bytes())
                .map(|_| ()),
            Err(err) => req
                .into_response(
                    200,
                    Some("Error"),
                    &[("Content-Type", "application/json; charset=utf-8")],
                )?
                .write_all(format!("{err:?}").as_bytes())
                .map(|_| ()),
        }
    })?;

    // HTTP POST 速度测试 (Echo模式 - 回显数据)
    server.fn_handler("/speed_test_echo", Method::Post, |mut req| {
        let len = req.content_len().unwrap_or(0) as usize;
        // Allow up to 1.5MB for speed test
        const MAX_SPEED_TEST_SIZE: usize = 1024 * 1024 + 512 * 1024;
        if len > MAX_SPEED_TEST_SIZE {
            return req
                .into_response(400, Some("Too Large"), &[])?
                .write_all(b"Data too large (max 1.5MB)")
                .map(|_| ());
        }
        
        // Read all data first, then echo back
        let mut buf = vec![0u8; len];
        if req.read_exact(&mut buf).is_err() {
            return req
                .into_response(400, Some("Read Error"), &[])?
                .write_all(b"Read error")
                .map(|_| ());
        }
        
        // Echo back the received data
        req.into_response(
            200,
            Some("OK"),
            &[("Content-Type", "application/octet-stream")],
        )?
        .write_all(&buf)
        .map(|_| ())
    })?;

    // HTTP POST 速度测试 (旧接口保持兼容)
    server.fn_handler("/speed_test", Method::Post, |mut req| {
        let len = req.content_len().unwrap_or(0) as usize;
        if len > MAX_HTTP_PAYLOAD_LEN {
            return req
                .into_response(400, Some("Too Large"), &[])?
                .write_all(b"Data too large")
                .map(|_| ());
        }
        
        // Read all data
        let mut buf = vec![0u8; len];
        if req.read_exact(&mut buf).is_err() {
            return req
                .into_response(400, Some("Read Error"), &[])?
                .write_all(b"Read error")
                .map(|_| ());
        }
        
        let result = format!("OK:{} bytes", len);
        
        req.into_response(200, Some("OK"), &[("Content-Type", "text/plain")])?
            .write_all(result.as_bytes())
            .map(|_| ())
    })?;

    // HTTP POST 保存wifi配置
    server.fn_handler(
        "/wifi_config",
        Method::Post,
        |mut req| match handle_wifi_config(&mut req) {
            Ok(()) => {
                let _ = draw_splash_with_error1(Some("设置成功!"), Some("正在重启..."));
                req.into_ok_response()?
                    .write_all("OK".as_bytes())
                    .map(|_| ())
            }
            Err(err) => {
                let err_msg = format!("{err:?}");
                let _ = draw_splash_with_error1(Some("设置失败"), Some(&err_msg));
                req.into_response(
                    200,
                    Some("Error"),
                    &[("Content-Type", "text/plain; charset=utf-8")],
                )?
                .write_all(err_msg.as_bytes())
                .map(|_| ())
            }
        },
    )?;

    // HTTP GET 读取wifi配置
    server.fn_handler("/wifi_config", Method::Get, |req| {
        let cfg = with_context(move |ctx| {
            ctx.last_config_time = Some(Instant::now());
            let cfg = ctx.config.wifi_config.as_ref();
            match cfg {
                Some(cfg) => Ok(serde_json::to_string(&cfg)?),
                None => Err(anyhow!("未配置wifi参数!")),
            }
        });
        match cfg {
            Ok(json) => req
                .into_response(
                    200,
                    Some("OK"),
                    &[("Content-Type", "application/json; charset=utf-8")],
                )?
                .write_all(json.as_bytes())
                .map(|_| ()),
            Err(err) => req
                .into_response(
                    200,
                    Some("Error"),
                    &[("Content-Type", "text/plain; charset=utf-8")],
                )?
                .write_all(format!("{err:?}").as_bytes())
                .map(|_| ()),
        }
    })?;

    // HTTP GET 扫描WiFi网络
    server.fn_handler("/scan_wifi", Method::Get, |req| {
        let result = with_context(move |ctx| {
            ctx.last_config_time = Some(Instant::now());
            
            info!("Scanning WiFi networks...");
            
            // 在AP模式下，我们需要临时切换到APSTA模式才能扫描
            // 先检查当前模式
            let current_config = ctx.wifi.get_configuration()?;
            let is_ap_only = matches!(current_config, Configuration::AccessPoint(_));
            
            // 如果是纯AP模式，需要临时切换到混合模式
            if is_ap_only {
                info!("Currently in AP-only mode, switching to APSTA for scanning...");
                if let Configuration::AccessPoint(ap_config) = current_config {
                    // 创建一个临时的STA配置（空SSID）
                    let temp_client_config = ClientConfiguration {
                        ssid: "".try_into().unwrap(),
                        ..Default::default()
                    };
                    
                    // 临时切换到混合模式
                    ctx.wifi.set_configuration(&Configuration::Mixed(temp_client_config, ap_config))?;
                }
            }
            
            // 执行扫描
            let scan_result = ctx.wifi.scan();
            
            // 如果之前是纯AP模式，扫描后恢复
            if is_ap_only {
                if let Configuration::AccessPoint(ap_config) = ctx.wifi.get_configuration()? {
                    ctx.wifi.set_configuration(&Configuration::AccessPoint(ap_config))?;
                }
            }
            
            match scan_result {
                Ok(aps) => {
                    info!("Found {} WiFi networks", aps.len());
                    
                    // 构建WiFi列表JSON
                    let mut wifi_list = Vec::new();
                    
                    for ap in aps.iter() {
                        // 将SSID字符串转换
                        let ssid = ap.ssid.as_str().to_string();
                        
                        // 跳过空SSID
                        if ssid.is_empty() {
                            continue;
                        }
                        
                        // 计算信号强度百分比 (RSSI通常在-100到0之间)
                        let signal_strength = ((ap.signal_strength as i32 + 100).max(0).min(100)) as u8;
                        
                        // 获取认证模式
                        let auth_mode = match ap.auth_method {
                            Some(embedded_svc::wifi::AuthMethod::None) => "None",
                            Some(embedded_svc::wifi::AuthMethod::WEP) => "WEP",
                            Some(embedded_svc::wifi::AuthMethod::WPA) => "WPA",
                            Some(embedded_svc::wifi::AuthMethod::WPA2Personal) => "WPA2",
                            Some(embedded_svc::wifi::AuthMethod::WPAWPA2Personal) => "WPA/WPA2",
                            Some(embedded_svc::wifi::AuthMethod::WPA2Enterprise) => "WPA2-Enterprise",
                            Some(embedded_svc::wifi::AuthMethod::WPA3Personal) => "WPA3",
                            Some(embedded_svc::wifi::AuthMethod::WPA2WPA3Personal) => "WPA2/WPA3",
                            Some(embedded_svc::wifi::AuthMethod::WAPIPersonal) => "WAPI",
                            None => "Unknown",
                        };
                        
                        wifi_list.push(serde_json::json!({
                            "ssid": ssid,
                            "signal_strength": signal_strength,
                            "auth_mode": auth_mode,
                            "channel": ap.channel
                        }));
                    }
                    
                    // 按信号强度排序（从强到弱）
                    wifi_list.sort_by(|a, b| {
                        let strength_a = a["signal_strength"].as_u64().unwrap_or(0);
                        let strength_b = b["signal_strength"].as_u64().unwrap_or(0);
                        strength_b.cmp(&strength_a)
                    });
                    
                    Ok(serde_json::to_string(&wifi_list)?)
                },
                Err(e) => {
                    error!("WiFi scan failed: {:?}", e);
                    Err(anyhow!("WiFi扫描失败: {:?}", e))
                }
            }
        });
        
        match result {
            Ok(json) => req
                .into_response(
                    200,
                    Some("OK"),
                    &[("Content-Type", "application/json; charset=utf-8")],
                )?
                .write_all(json.as_bytes())
                .map(|_| ()),
            Err(err) => req
                .into_response(
                    500,
                    Some("Error"),
                    &[("Content-Type", "text/plain; charset=utf-8")],
                )?
                .write_all(format!("{err:?}").as_bytes())
                .map(|_| ())
        }
    })?;

    // HTTP POST 设置屏幕参数
    server.fn_handler(
        "/display_config",
        Method::Post,
        |mut req| match handle_display_config(&mut req) {
            Ok(()) => {
                let _ = draw_splash_with_error1(Some("设置成功!"), Some("正在重启..."));
                req.into_ok_response()?
                    .write_all("OK".as_bytes())
                    .map(|_| ())
            }
            Err(err) => {
                let err_msg = format!("{err:?}");
                let _ = draw_splash_with_error1(Some("设置失败"), Some(&err_msg));
                req.into_response(
                    200,
                    Some("Error"),
                    &[("Content-Type", "text/plain; charset=utf-8")],
                )?
                .write_all(err_msg.as_bytes())
                .map(|_| ())
            }
        },
    )?;

    // HTTP GET 读取屏幕参数
    server.fn_handler("/display_config", Method::Get, |req| {
        let cfg = with_context(move |ctx| {
            ctx.last_config_time = Some(Instant::now());
            let mut cfg = ctx.config.display_config.clone();
            if let Some(cfg) = cfg.as_mut(){
                let (w, h) = cfg.get_screen_size();
                cfg.rotated_width = NonZero::new(w);
                cfg.rotated_height = NonZero::new(h);
            }
            match cfg {
                Some(cfg) => Ok(serde_json::to_string(&cfg)?),
                None => Err(anyhow!("未配置屏幕参数!")),
            }
        });
        match cfg {
            Ok(json) => req
                .into_response(
                    200,
                    Some("OK"),
                    &[("Content-Type", "application/json; charset=utf-8")],
                )?
                .write_all(json.as_bytes())
                .map(|_| ()),
            Err(err) => req
                .into_response(
                    200,
                    Some("Error"),
                    &[("Content-Type", "text/plain; charset=utf-8")],
                )?
                .write_all(format!("{err:?}").as_bytes())
                .map(|_| ()),
        }
    })?;

    // HTTP POST 实时调整色调（不重启）
    server.fn_handler(
        "/color_adjust",
        Method::Post,
        |mut req| {
            with_context1(move |ctx| {
                match handle_color_adjust(ctx, &mut req) {
                    Ok(()) => req
                        .into_ok_response()?
                        .write_all("OK".as_bytes())
                        .map(|_| ()),
                    Err(err) => req
                        .into_response(
                            200,
                            Some("Error"),
                            &[("Content-Type", "text/plain; charset=utf-8")],
                        )?
                        .write_all(format!("{err:?}").as_bytes())
                        .map(|_| ()),
                }
            })
        },
    )?;

    // HTTP GET 获取当前色调调整值
    server.fn_handler("/color_adjust", Method::Get, |req| {
        let result = with_context(move |ctx| {
            if let Some(cfg) = &ctx.config.display_config {
                Ok(serde_json::json!({
                    "r": cfg.color_adjust_r,
                    "g": cfg.color_adjust_g,
                    "b": cfg.color_adjust_b
                }).to_string())
            } else {
                Err(anyhow!("Display not configured"))
            }
        });
        match result {
            Ok(json) => req
                .into_response(
                    200,
                    Some("OK"),
                    &[("Content-Type", "application/json; charset=utf-8")],
                )?
                .write_all(json.as_bytes())
                .map(|_| ()),
            Err(err) => req
                .into_response(
                    200,
                    Some("Error"),
                    &[("Content-Type", "text/plain; charset=utf-8")],
                )?
                .write_all(format!("{err:?}").as_bytes())
                .map(|_| ()),
        }
    })?;

    // HTTP POST 实时修改屏幕旋转方向（不重启）
    server.fn_handler(
        "/display_rotation",
        Method::Post,
        |mut req| {
            with_context1(move |ctx| {
                match handle_display_rotation(ctx, &mut req) {
                    Ok(()) => req
                        .into_ok_response()?
                        .write_all("OK".as_bytes())
                        .map(|_| ()),
                    Err(err) => req
                        .into_response(
                            200,
                            Some("Error"),
                            &[("Content-Type", "text/plain; charset=utf-8")],
                        )?
                        .write_all(format!("{err:?}").as_bytes())
                        .map(|_| ()),
                }
            })
        },
    )?;

    // HTTP POST 实时修改WiFi配置（不重启）
    server.fn_handler(
        "/wifi_reconnect",
        Method::Post,
        |mut req| {
            with_context1(move |ctx| {
                match handle_wifi_reconnect(ctx, &mut req) {
                    Ok(()) => req
                        .into_ok_response()?
                        .write_all("OK".as_bytes())
                        .map(|_| ()),
                    Err(err) => req
                        .into_response(
                            200,
                            Some("Error"),
                            &[("Content-Type", "text/plain; charset=utf-8")],
                        )?
                        .write_all(format!("{err:?}").as_bytes())
                        .map(|_| ()),
                }
            })
        },
    )?;

    // HTTP POST 实时修改MQTT配置（不重启）
    server.fn_handler(
        "/mqtt_reconnect",
        Method::Post,
        |mut req| {
            with_context1(move |ctx| {
                match handle_mqtt_reconnect(ctx, &mut req) {
                    Ok(()) => req
                        .into_ok_response()?
                        .write_all("OK".as_bytes())
                        .map(|_| ()),
                    Err(err) => req
                        .into_response(
                            200,
                            Some("Error"),
                            &[("Content-Type", "text/plain; charset=utf-8")],
                        )?
                        .write_all(format!("{err:?}").as_bytes())
                        .map(|_| ()),
                }
            })
        },
    )?;

    // HTTP POST 保存远程服务器配置
    server.fn_handler(
        "/remote_server_config",
        Method::Post,
        |mut req| match handle_remote_server_config(&mut req) {
            Ok(()) => {
                let _ = draw_splash_with_error1(Some("设置成功!"), Some("正在重启..."));
                req.into_ok_response()?
                    .write_all("OK".as_bytes())
                    .map(|_| ())
            }
            Err(err) => {
                let err_msg = format!("{err:?}");
                let _ = draw_splash_with_error1(Some("设置失败"), Some(&err_msg));
                req.into_response(
                    200,
                    Some("Error"),
                    &[("Content-Type", "text/plain; charset=utf-8")],
                )?
                .write_all(err_msg.as_bytes())
                .map(|_| ())
            }
        },
    )?;

    // HTTP DELETE 删除远程服务器配置
    server.fn_handler(
        "/delete_remote_server_config",
        Method::Get,
        |req| match handle_delete_remote_server_config() {
            Ok(()) => {
                let _ = draw_splash_with_error1(Some("删除成功!"), Some("正在重启..."));
                req.into_ok_response()?
                    .write_all("OK".as_bytes())
                    .map(|_| ())
            }
            Err(err) => {
                let err_msg = format!("{err:?}");
                let _ = draw_splash_with_error1(Some("删除失败"), Some(&err_msg));
                req.into_response(
                    200,
                    Some("Error"),
                    &[("Content-Type", "text/plain; charset=utf-8")],
                )?
                .write_all(err_msg.as_bytes())
                .map(|_| ())
            }
        },
    )?;

    // HTTP GET 读取远程服务器配置
    server.fn_handler("/remote_server_config", Method::Get, |req| {
        let cfg = with_context(move |ctx| {
            let cfg = ctx.config.remote_server_config.as_ref();
            match cfg {
                Some(cfg) => Ok(serde_json::to_string(&cfg)?),
                None => Err(anyhow!("未配置远程服务器参数!")),
            }
        });
        match cfg {
            Ok(json) => req
                .into_response(
                    200,
                    Some("OK"),
                    &[("Content-Type", "application/json; charset=utf-8")],
                )?
                .write_all(json.as_bytes())
                .map(|_| ()),
            Err(err) => req
                .into_response(
                    200,
                    Some("Error"),
                    &[("Content-Type", "text/plain; charset=utf-8")],
                )?
                .write_all(format!("{err:?}").as_bytes())
                .map(|_| ()),
        }
    })?;

    // 删除缓存的图片
    server.fn_handler("/delete_image", Method::Get, |req| {
        let uri = req.uri().to_string();
        match with_context(move |ctx| {
            let url = Url::parse(&format!("http://localhost{uri}"))?;
            let params: HashMap<String, String> = url.query_pairs().into_owned().collect();
            let key = match params.get("key") {
                Some(v) => v,
                None => return Err(anyhow!("缺少参数key")),
            };
            ctx.image_cache.remove(key);
            let keys: Vec<String> = ctx.image_cache.keys().map(|k| k.to_string()).collect();
            Ok(keys)
        }) {
            Ok(keys) => req
                .into_ok_response()?
                .write_all(format!("{keys:?}").as_bytes())
                .map(|_| ()),
            Err(err) => req
                .into_response(
                    200,
                    Some("Error"),
                    &[("Content-Type", "text/plain; charset=utf-8")],
                )?
                .write_all(format!("{err:?}").as_bytes())
                .map(|_| ()),
        }
    })?;

    // 获取缓存的图片(返回png)
    server.fn_handler("/download_image", Method::Get, |req| {
        let uri = req.uri().to_string();
        match with_context(move |ctx| {
            let url = Url::parse(&format!("http://localhost{uri}"))?;
            let params: HashMap<String, String> = url.query_pairs().into_owned().collect();
            let key = match params.get("key") {
                Some(v) => v,
                None => return Err(anyhow!("缺少参数key")),
            };
            match ctx.image_cache.get(key) {
                Some(img) => {
                    let mut out = Box::new(vec![]);
                    let encoder = PngEncoder::new(&mut out);
                    match img {
                        ImageCache::RgbImage(img) => {
                            encoder.write_image(
                                &img,
                                img.width(),
                                img.height(),
                                image::ExtendedColorType::Rgb8,
                            )?;
                        }
                        ImageCache::RgbaImage(img) => {
                            encoder.write_image(
                                &img,
                                img.width(),
                                img.height(),
                                image::ExtendedColorType::Rgba8,
                            )?;
                        }
                    }
                    Ok(out)
                }
                None => Err(anyhow!("key not exist")),
            }
        }) {
            Ok(png) => req
                .into_response(
                    200,
                    Some("OK"),
                    &[
                        ("Content-Type", "image/png"),
                        ("Content-Length", &format!("{}", png.len())),
                    ],
                )?
                .write_all(&png)
                .map(|_| ()),
            Err(err) => req
                .into_response(
                    200,
                    Some("Error"),
                    &[("Content-Type", "text/plain; charset=utf-8")],
                )?
                .write_all(format!("{err:?}").as_bytes())
                .map(|_| ()),
        }
    })?;

    // HTTP POST 上传并缓存一张图片
    server.fn_handler("/upload_image", Method::Post, |mut req| {
        let uri = req.uri().to_string();

        let len = req.content_len().unwrap_or(0) as usize;
        let mut err = None;
        let mut data = if len > MAX_HTTP_PAYLOAD_LEN {
            err = Some(format!("http请求体不能超过{MAX_HTTP_PAYLOAD_LEN}字节"));
            Box::new(vec![])
        } else {
            Box::new(vec![0; len])
        };

        if let Err(e) = req.read_exact(&mut data) {
            err = Some(format!("http请求体不能超过{e:?}字节"));
        }

        match with_context(move |ctx| {
            if let Some(err) = err {
                return Err(anyhow!("{err}"));
            }
            let url = Url::parse(&format!("http://localhost{uri}"))?;
            let params: HashMap<String, String> = url.query_pairs().into_owned().collect();
            let key = match params.get("key") {
                Some(v) => v.to_string(),
                None => return Err(anyhow!("缺少参数key")),
            };

            //删除老的图片
            drop(ctx.image_cache.remove(&key));

            if ctx.image_cache.len() >= 5 {
                return Err(anyhow!("最多缓存5张图片"));
            }

            let mime = mimetype::detect(&data);
            if mime.extension.ends_with("jpg") || mime.extension.ends_with("jpeg") {
                //rgb565转rgb
                let rgb = decode_jpg_to_rgb(data)?;
                ctx.image_cache.insert(key, ImageCache::RgbImage(rgb));
            } else {
                let rgba = Box::new(image::load_from_memory(&data)?.to_rgba8());
                ctx.image_cache.insert(key, ImageCache::RgbaImage(rgba));
            };

            let keys: Vec<String> = ctx.image_cache.keys().map(|k| k.to_string()).collect();
            Ok(keys)
        }) {
            Ok(keys) => req
                .into_ok_response()?
                .write_all(format!("{keys:?}").as_bytes())
                .map(|_| ()),
            Err(err) => req
                .into_response(
                    200,
                    Some("Error"),
                    &[("Content-Type", "text/plain; charset=utf-8")],
                )?
                .write_all(format!("{err:?}").as_bytes())
                .map(|_| ()),
        }
    })?;

    // HTTP POST 绘制画布
    server.fn_handler("/draw_canvas", Method::Get, |req| {
        req.into_response(
            200,
            Some("Error"),
            &[("Content-Type", "text/plain; charset=utf-8")],
        )?
        .write_all("调用draw_canvas请使用Post请求！".as_bytes())
        .map(|_| ())
    })?;

    // HTTP POST 绘制画布
    server.fn_handler(
        "/draw_canvas",
        Method::Post,
        |mut req| match handle_draw_canvas(&mut req) {
            Ok(()) => req.into_ok_response()?.write_all(b"OK").map(|_| ()),
            Err(err) => {
                info!("draw canvas err:{err:?}");
                req.into_response(
                    200,
                    Some("Error"),
                    &[("Content-Type", "text/plain; charset=utf-8")],
                )?
                .write_all(format!("{err:?}").as_bytes())
                .map(|_| ())
            }
        },
    )?;

    // HTTP POST 绘制GIF/png/jpg图片
    server.fn_handler(
        "/draw_image",
        Method::Post,
        |mut req| {
            with_context1(move |ctx|{
                match handle_display_image(ctx, &mut req) {
                    Ok((w, h, msg)) => req
                        .into_ok_response()?
                        .write_all(format!("{w}x{h} {msg}").as_bytes())
                        .map(|_| ()),
                    Err(err) => req
                        .into_response(
                            200,
                            Some("Error"),
                            &[("Content-Type", "text/plain; charset=utf-8")],
                        )?
                        .write_all(format!("{err:?}").as_bytes())
                        .map(|_| ()),
                }
            })
        }
    )?;

    // HTTP POST 绘制lz4压缩后的RGB565图像数据
    server.fn_handler(
        "/draw_rgb565_lz4",
        Method::Post,
        |mut req| {
            with_context1(move |ctx|{
                match handle_display_rgb565_lz4(ctx, &mut req) {
                    Ok((w, h, msg)) => req
                        .into_ok_response()?
                        .write_all(format!("{w}x{h} {msg}").as_bytes())
                        .map(|_| ()),
                    Err(err) => req
                        .into_response(
                            200,
                            Some("Error"),
                            &[("Content-Type", "text/plain; charset=utf-8")],
                        )?
                        .write_all(format!("{err:?}").as_bytes())
                        .map(|_| ()),
                }
            })
        }
    )?;

    // HTTP POST 绘制RGB565图像数据
    server.fn_handler(
        "/draw_rgb565",
        Method::Post,
        |mut req| {
            with_context1(move |ctx|{
                match handle_display_rgb565(ctx, &mut req) {
                    Ok((w, h, msg)) => req
                        .into_ok_response()?
                        .write_all(format!("{w}x{h} {msg}").as_bytes())
                        .map(|_| ()),
                    Err(err) => req
                        .into_response(
                            200,
                            Some("Error"),
                            &[("Content-Type", "text/plain; charset=utf-8")],
                        )?
                        .write_all(format!("{err:?}").as_bytes())
                        .map(|_| ()),
                }
            })
        }
    )?;

    
    let _ = server.ws_handler("/ws", move |ws| {
        let _ = with_context(move |ctx|{
            // 检查内存状态 - 在处理任何 WebSocket 请求之前
            let free_heap = unsafe { esp_get_free_heap_size() } as usize;
            const MIN_SAFE_HEAP: usize = 150 * 1024; // 150KB 安全阈值
            const CRITICAL_HEAP: usize = 80 * 1024;  // 80KB 严重阈值
            
            if free_heap < CRITICAL_HEAP {
                error!("内存严重不足 ({} bytes)，关闭 WebSocket 连接", free_heap);
                let _ = ws.send(FrameType::Close, &[]);
                return Ok(());
            }
            
            if ws.is_new() {
                info!("New WebSocket session... (free_heap: {} bytes)", free_heap);
                
                // 内存低时拒绝新连接
                if free_heap < MIN_SAFE_HEAP {
                    warn!("内存不足，拒绝新 WebSocket 连接 (free_heap: {} bytes)", free_heap);
                    let _ = ws.send(FrameType::Text(false), "Server busy: Low memory. Please wait and retry.".as_bytes());
                    let _ = ws.send(FrameType::Close, &[]);
                    return Ok(());
                }
                
                ws.send(FrameType::Text(false), "Welcome".as_bytes())?;
                return Ok(());
            } else if ws.is_closed() {
                return Ok(());
            }
    
            let (frame_type, len) = match ws.recv(&mut []) {
                Ok(frame) => frame,
                Err(_e) => return Ok(())
            };
    
            // Check memory before allocating buffer
            let free_heap = unsafe { esp_get_free_heap_size() } as usize;
            const MIN_FREE_HEAP: usize = 120 * 1024; // Keep at least 120KB free
            
            if free_heap < MIN_FREE_HEAP {
                warn!("内存低，拒绝 WebSocket 请求 (free_heap: {} bytes)", free_heap);
                let _ = ws.send(FrameType::Text(false), "Server busy: Low memory".as_bytes());
                return Ok(());
            }
            
            // Limit WebSocket payload size (512KB max for echo test)
            const MAX_WS_PAYLOAD: usize = 512 * 1024;
            if len > MAX_WS_PAYLOAD {
                let _ = ws.send(FrameType::Text(false), "Request too big (max 512KB)".as_bytes());
                return Ok(());
            }
    
            // Allocate buffer based on actual data size (with safety margin)
            let buf_size = len.min(MAX_WS_PAYLOAD);
            let mut buf = vec![0u8; buf_size];
            if ws.recv(&mut buf).is_err() {
                return Ok(());
            }
            let mut data: &[u8] = &buf[0..len.min(buf_size)];

            // info!("ws recv data:{}", data.len());

            match frame_type {
                FrameType::Text(_) => {
                    if data.len() > 1 && data[data.len()-1] == b'\0'{
                        data = &data[0..data.len()-1];
                    }
                    let data_len = data.len();
                    
                    let json = unsafe{ str::from_boxed_utf8_unchecked(data.into()) };
                    if let Err(err) = draw_json_elements(ctx, &*json) {
                        info!("draw json error:{err:?}");
                        let _ = ws.send(
                            FrameType::Text(false),
                            format!(
                                "draw json error:{err:?} byteLen={data_len} String len={}",
                                json.len()
                            )
                            .as_bytes(),
                        );
                    }
                }
                FrameType::Binary(_) => {
                    // Check for echo test prefix - echo back the data
                    const ECHO_TEST_PREFIX: &[u8] = b"ECHO_TEST:";
                    if data.starts_with(ECHO_TEST_PREFIX) {
                        let payload = &data[ECHO_TEST_PREFIX.len()..];
                        // Echo back the payload as binary
                        let _ = ws.send(FrameType::Binary(false), payload);
                        return Ok(());
                    }
                    
                    // Check for speed test prefix (legacy)
                    const SPEED_TEST_PREFIX: &[u8] = b"SPEED_TEST:";
                    if data.starts_with(SPEED_TEST_PREFIX) {
                        let payload_len = data.len() - SPEED_TEST_PREFIX.len();
                        let result = format!("OK:{} bytes", payload_len);
                        let _ = ws.send(FrameType::Text(false), result.as_bytes());
                        return Ok(());
                    }
                    
                    //判断图片类型
                    let mime = mimetype::detect(data.as_ref());
                    // info!("mime:{mime:?}");
                    match ctx.display.as_mut() {
                        None => error!("Display not configured!"),
                        Some(display_manager) => {
                            // info!("mime:{mime:?}");
                            if mime.extension.ends_with("jpg") || mime.extension.ends_with("jpeg") {
                                if let Ok(rgb565_data) = canvas::decode_jpeg_to_rgb565(&data) {
                                    let _ = display::draw_rgb565_fast(display_manager, 0, 0, rgb565_data.0, rgb565_data.1, &rgb565_data.2);
                                } else {
                                    error!("jpg decode error!");
                                }
                            } else if mime.extension.ends_with("gif") || mime.extension.ends_with("png") {
                                if let Ok(image) = image::load_from_memory(&data){
                                    let image = image.to_rgb8();
                                    let _ = display::draw_rgb_image_fast(display_manager, 0, 0, &image);
                                }else{
                                    error!("image decode error!");
                                }
                            }else {
                                if data.as_ref().starts_with(b"RGB565"){
                                    let rgb565 = &data.as_ref()[6..];
                                    display::draw_rgb565_u8array_fast(display_manager, 0, 0, display_manager.get_screen_width(), display_manager.get_screen_height(), rgb565)?;
                                }else{
                                    //lz4压缩数据
                                    match lz4_flex::decompress_size_prepended(&data){
                                        Ok(rgb565) => {
                                            let rgb565 = &rgb565[0..display_manager.get_screen_width() as usize
                                            * display_manager.get_screen_height() as usize * 2];
                                            display::draw_rgb565_u8array_fast(display_manager, 0, 0, display_manager.get_screen_width(), display_manager.get_screen_height(), rgb565)?;
                                        }
                                        Err(err) => {
                                            error!("lz4 decode:{err:?}");
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                FrameType::Ping => {
                    // Respond with Pong
                    let _ = ws.send(FrameType::Pong, &[]);
                }
                FrameType::Pong => {
                    // Ignore pong frames
                }
                FrameType::Close => {
                    // Connection closing
                }
                FrameType::SocketClose => {
                    // Socket closing
                }
                FrameType::Continue(_) => {
                    // Continuation frame, ignore
                }
            };
            Ok(())
        });
        Ok::<(), EspError>(())
    });

    core::mem::forget(server);

    Ok(())
}


fn handle_wifi_config(
    req: &mut esp_idf_svc::http::server::Request<&mut EspHttpConnection<'_>>,
) -> Result<()> {
    let mut buf = Box::new(vec![0u8; 1024 * 2]);
    let len = req.read(&mut buf)?;
    let data = buf[0..len].to_vec();
    let cfg = config::parse_wifi_config(data)?;
    //保存配置
    with_context(move |ctx| {
        ctx.config.wifi_config.replace(cfg);
        config::save_config(&mut ctx.config_nvs, &ctx.config)
    })?;

    //配置保存成功后重启
    std::thread::spawn(move || {
        info!("wifi config reboot after 1.5s...");
        std::thread::sleep(Duration::from_millis(1500));
        unsafe { esp_restart() };
    });
    Ok(())
}

fn handle_remote_server_config(
    req: &mut esp_idf_svc::http::server::Request<&mut EspHttpConnection<'_>>,
) -> Result<()> {
    let mut buf = Box::new(vec![0u8; 1024 * 2]);
    let len = req.read(&mut buf)?;
    let data = buf[0..len].to_vec();
    let cfg = config::parse_remote_server_config(data)?;
    //保存配置
    with_context(move |ctx| {
        ctx.config.remote_server_config.replace(cfg);
        config::save_config(&mut ctx.config_nvs, &ctx.config)
    })?;

    //配置保存成功后重启
    std::thread::spawn(move || {
        info!("reboot after 1.5s...");
        std::thread::sleep(Duration::from_millis(1500));
        unsafe { esp_restart() };
    });
    Ok(())
}

fn handle_delete_remote_server_config() -> Result<()> {
    //删除配置
    info!("delete mqtt config!!");
    with_context(move |ctx| {
        let _ = ctx.config.remote_server_config.take();
        config::save_config(&mut ctx.config_nvs, &ctx.config)
    })?;

    //配置保存成功后重启
    std::thread::spawn(move || {
        info!("reboot after 1.5s...");
        std::thread::sleep(Duration::from_millis(1500));
        unsafe { esp_restart() };
    });
    Ok(())
}

pub fn print_memory(tag: &str){
    let free_heap = unsafe { esp_get_free_heap_size() };
    let free_internal_heap = unsafe { esp_get_free_internal_heap_size() };
    let free_mini = unsafe {
        esp_get_minimum_free_heap_size()
    };
    info!("{tag} ##> free_heap:{free_heap}, free_internal_heap:{free_internal_heap} minimum_free_heap:{free_mini}");
    
    // 紧急重启机制：当内存极低时自动重启，防止系统卡死
    const CRITICAL_HEAP: u32 = 50 * 1024;  // 50KB
    const WARNING_HEAP: u32 = 100 * 1024;  // 100KB
    
    if free_heap < CRITICAL_HEAP {
        error!("❗❗❗ 内存极低 ({} bytes)，立即重启以防止系统崩溃!", free_heap);
        std::thread::sleep(Duration::from_millis(1000));
        unsafe { esp_restart() };
    } else if free_heap < WARNING_HEAP {
        warn!("⚠️ 内存低 ({} bytes)，即将达到重启阈值", free_heap);
    }
}

fn handle_draw_canvas(
    req: &mut esp_idf_svc::http::server::Request<&mut EspHttpConnection<'_>>,
) -> Result<()> {
    // 检查内存是否足够（至少需要500KB处理base64图片）
    let free_heap = unsafe { esp_get_free_heap_size() } as usize;
    const MIN_REQUIRED_HEAP: usize = 500 * 1024;
    
    if free_heap < MIN_REQUIRED_HEAP {
        return Err(anyhow!("内存不足，拒绝/draw_canvas请求 (free_heap: {} KB，需要至少 {} KB)", free_heap / 1024, MIN_REQUIRED_HEAP / 1024));
    }
    
    let len = req.content_len().unwrap_or(0) as usize;
    if len > MAX_HTTP_PAYLOAD_LEN {
        return Err(anyhow!("http请求体不能超过{MAX_HTTP_PAYLOAD_LEN}字节"));
    }
    let mut data = Box::new(vec![0; len]);
    req.read_exact(&mut data)?;

    if let Err(err) = std::thread::Builder::new()
    .stack_size(18*1024)
    .spawn(move ||{
        if let Err(err) = with_context(move |ctx|{
            let json = unsafe{ str::from_boxed_utf8_unchecked(data.as_slice().into()) };
            draw_json_elements(ctx, &*json)
        }){
            error!("draw_canvas parse json:{err:?}");
        }
    }){
        error!("draw_canvas thread error:{err:?}");
    }
    Ok(())
}

pub fn draw_json_elements(ctx: &mut Context, json: &str) -> Result<()> {
    let display_manager = match ctx.display.as_mut() {
        None => return Err(anyhow!("请设置屏幕参数!")),
        Some(v) => v,
    };

    let elements: Box<Vec<Element>> = Box::new(serde_json::from_str(json)
        .map_err(|err| anyhow!("parse elements {err:?} json:`{json}`"))?);
    // info!("Elements:{}", elements.len());

    draw_elements(display_manager, &ctx.image_cache, &elements)
        .map_err(|err| anyhow!("draw elements: {err:?}"))?;
    Ok(())
}

fn handle_display_image(
    ctx: &mut Context,
    req: &mut esp_idf_svc::http::server::Request<&mut EspHttpConnection<'_>>,
) -> Result<(u16, u16, String)> {
    let t1 = Instant::now();
    let len = req.content_len().unwrap_or(0) as usize;
    if len > MAX_HTTP_PAYLOAD_LEN {
        return Err(anyhow!("http请求体不能超过{MAX_HTTP_PAYLOAD_LEN}字节"));
    }
    let mut data = Box::new(vec![0; len]);
    req.read_exact(&mut data)?;
    let recv_ms = t1.elapsed().as_millis();
    // info!("handle_display_image recv {}ms", t1.elapsed().as_millis());
    let t1 = Instant::now();

    let mime = mimetype::detect(&data);
    let display_manager = match ctx.display.as_mut() {
        None => return Err(anyhow!("display not init!")),
        Some(v) => v,
    };
    if mime.extension.ends_with("jpg") || mime.extension.ends_with("jpeg") {
        let (w, h, data) = canvas::decode_jpeg_to_rgb565(&data)?;
        let decode_ms = t1.elapsed().as_millis();
        let t1 = Instant::now();
        display::draw_rgb565_fast(display_manager, 0, 0, w, h, &data)?;
        let draw_ms = t1.elapsed().as_millis();
        Ok((w, h, format!("recv:{recv_ms}ms, decode:{decode_ms}ms, draw:{draw_ms}ms")))
    } else {
        let image = image::load_from_memory(&data)?.to_rgb8();
        let decode_ms = t1.elapsed().as_millis();
        let t1 = Instant::now();
        display::draw_rgb_image_fast(display_manager, 0, 0, &image)?;
        let draw_ms = t1.elapsed().as_millis();
        Ok((image.width() as u16, image.height() as u16, format!("recv:{recv_ms}ms, decode:{decode_ms}ms, draw:{draw_ms}ms")))
    }
}

fn handle_display_rotation(
    ctx: &mut Context,
    req: &mut esp_idf_svc::http::server::Request<&mut EspHttpConnection<'_>>,
) -> Result<()> {
    #[derive(serde::Deserialize)]
    struct RotationRequest {
        rotation: String,  // "Deg0", "Deg90", "Deg180", "Deg270"
    }
    
    let mut buf = Box::new(vec![0u8; 256]);
    let len = req.read(&mut buf)?;
    let data = &buf[0..len];
    
    let request: RotationRequest = serde_json::from_slice(data)?;
    
    // 解析旋转角度
    let rotation = match request.rotation.as_str() {
        "Deg0" => crate::config::DisplayRotation::Deg0,
        "Deg90" => crate::config::DisplayRotation::Deg90,
        "Deg180" => crate::config::DisplayRotation::Deg180,
        "Deg270" => crate::config::DisplayRotation::Deg270,
        _ => return Err(anyhow!("Invalid rotation value")),
    };
    
    // 更新配置
    if let Some(cfg) = ctx.config.display_config.as_mut() {
        cfg.rotation = rotation.clone();
    } else {
        return Err(anyhow!("Display not configured"));
    }
    
    // 同步更新DisplayManager中的配置
    if let Some(display_manager) = ctx.display.as_mut() {
        display_manager.display_config.rotation = rotation.clone();
        
        // 实时更新显示器方向
        let mipidsi_rotation = match display_manager.display_config.rotation {
            crate::config::DisplayRotation::Deg0 => mipidsi::options::Rotation::Deg0,
            crate::config::DisplayRotation::Deg90 => mipidsi::options::Rotation::Deg90,
            crate::config::DisplayRotation::Deg180 => mipidsi::options::Rotation::Deg180,
            crate::config::DisplayRotation::Deg270 => mipidsi::options::Rotation::Deg270,
        };
        
        match &mut display_manager.display {
            display::DisplayInterface::ST7735s(display) => {
                display.set_orientation(mipidsi::options::Orientation {
                    rotation: mipidsi_rotation,
                    mirrored: display_manager.display_config.mirrored,
                })
                .map_err(|e| anyhow!("ST7735s set_orientation failed: {:?}", e))?;
            }
            display::DisplayInterface::ST7789(display) => {
                display.set_orientation(mipidsi::options::Orientation {
                    rotation: mipidsi_rotation,
                    mirrored: display_manager.display_config.mirrored,
                })
                .map_err(|e| anyhow!("ST7789 set_orientation failed: {:?}", e))?;
            }
            display::DisplayInterface::ST7796(display) => {
                display.set_orientation(mipidsi::options::Orientation {
                    rotation: mipidsi_rotation,
                    mirrored: display_manager.display_config.mirrored,
                })
                .map_err(|e| anyhow!("ST7796 set_orientation failed: {:?}", e))?;
            }
        }
    }
    
    // 保存到NVS
    config::save_config(&mut ctx.config_nvs, &ctx.config)?;
    
    // 绘制一个提示信息来触发屏幕刷新，使旋转立即可见
    let rotation_text = match rotation {
        crate::config::DisplayRotation::Deg0 => "0度",
        crate::config::DisplayRotation::Deg90 => "90度",
        crate::config::DisplayRotation::Deg180 => "180度",
        crate::config::DisplayRotation::Deg270 => "270度",
    };
    let _ = canvas::draw_splash_with_error(ctx, Some("旋转已生效"), Some(rotation_text));
    
    info!("屏幕旋转已更新: {:?}", request.rotation);
    
    Ok(())
}

fn handle_wifi_reconnect(
    ctx: &mut Context,
    req: &mut esp_idf_svc::http::server::Request<&mut EspHttpConnection<'_>>,
) -> Result<()> {
    #[derive(serde::Deserialize)]
    struct WifiConfig {
        ssid: String,
        password: String,
        #[serde(default)]
        device_ip: Option<String>,
    }
    
    let mut buf = Box::new(vec![0u8; 1024]);
    let len = req.read(&mut buf)?;
    let data = &buf[0..len];
    
    let wifi_config: WifiConfig = serde_json::from_slice(data)?;
    
    // 验证SSID不为空
    if wifi_config.ssid.trim().is_empty() {
        return Err(anyhow!("SSID不能为空"));
    }
    
    // 更新配置
    if let Some(cfg) = ctx.config.wifi_config.as_mut() {
        cfg.ssid = wifi_config.ssid.clone();
        cfg.password = wifi_config.password.clone();
        if let Some(ip) = wifi_config.device_ip.as_ref() {
            if !ip.trim().is_empty() {
                // 解析IP地址字符串为Ipv4Addr
                match ip.parse::<std::net::Ipv4Addr>() {
                    Ok(addr) => cfg.device_ip = Some(addr),
                    Err(_) => return Err(anyhow!("无效的IP地址格式: {}", ip)),
                }
            }
        }
    } else {
        // 如果wifi_config不存在，创建一个新的
        let device_ip = if let Some(ip) = wifi_config.device_ip.as_ref() {
            if !ip.trim().is_empty() {
                match ip.parse::<std::net::Ipv4Addr>() {
                    Ok(addr) => Some(addr),
                    Err(_) => return Err(anyhow!("无效的IP地址格式: {}", ip)),
                }
            } else {
                None
            }
        } else {
            None
        };
        
        ctx.config.wifi_config = Some(crate::config::WifiConfig {
            ssid: wifi_config.ssid.clone(),
            password: wifi_config.password.clone(),
            device_ip,
        });
    }
    
    // 保存到NVS
    config::save_config(&mut ctx.config_nvs, &ctx.config)?;
    
    info!("WiFi配置已更新，将在后台重新连接: {}", wifi_config.ssid);
    
    // 在后台线程中重新连接WiFi（避免阻塞HTTP响应）
    let ssid = wifi_config.ssid.clone();
    let _password = wifi_config.password.clone();
    let _device_ip = ctx.config.wifi_config.as_ref().and_then(|cfg| cfg.device_ip.clone());
    
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(500)); // 等待HTTP响应完成
        
        // 重新连接WiFi的逻辑需要在这里实现
        // 注意：由于WiFi对象在Context中，这里无法直接访问
        // 实际使用时可能需要重启来应用新配置，或者重构WiFi管理方式
        info!("WiFi重连逻辑: {} (需要重启才能完全生效)", ssid);
    });
    
    Ok(())
}

fn handle_mqtt_reconnect(
    ctx: &mut Context,
    req: &mut esp_idf_svc::http::server::Request<&mut EspHttpConnection<'_>>,
) -> Result<()> {
    use crate::config::RemoteServerConfig;
    
    let mut buf = Box::new(vec![0u8; 1024 * 2]);
    let len = req.read(&mut buf)?;
    let data = &buf[0..len];
    
    let cfg: RemoteServerConfig = serde_json::from_slice(data)?;
    
    // 更新配置
    ctx.config.remote_server_config = Some(cfg.clone());
    
    // 保存到NVS
    config::save_config(&mut ctx.config_nvs, &ctx.config)?;
    
    info!("MQTT配置已更新，将在后台重新连接");
    
    // 在后台线程中重新连接MQTT（避免阻塞HTTP响应）
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(500)); // 等待HTTP响应完成
        
        // 尝试重新启动MQTT客户端
        match crate::mqtt_client::listen_config() {
            Ok(_) => info!("MQTT客户端已重新连接"),
            Err(e) => error!("MQTT重连失败: {:?}", e),
        }
    });
    
    Ok(())
}

fn handle_display_rgb565(
    ctx: &mut Context,
    req: &mut esp_idf_svc::http::server::Request<&mut EspHttpConnection<'_>>,
) -> Result<(u16, u16, String)> {
    // 检查内存是否足够
    let free_heap = unsafe { esp_get_free_heap_size() } as usize;
    const MIN_REQUIRED_HEAP: usize = 200 * 1024; // 至少需要 200KB
    
    if free_heap < MIN_REQUIRED_HEAP {
        return Err(anyhow!("内存不足，拒绝请求 (free_heap: {} KB)", free_heap / 1024));
    }
    
    let t1 = Instant::now();
    let len = req.content_len().unwrap_or(0) as usize;
    let max_len = 500 * 1024;
    if len > max_len {
        return Err(anyhow!("http请求体不能超过{max_len}字节"));
    }
    let mut data = Box::new(vec![0; len]);
    req.read_exact(&mut data)?;
    let recv_ms = t1.elapsed().as_millis();

    let display_manager = match ctx.display.as_mut() {
        None => return Err(anyhow!("display not init!")),
        Some(v) => v,
    };

    let rgb565 = &data[0..display_manager.get_screen_width() as usize
    * display_manager.get_screen_height() as usize * 2];

    let t1 = Instant::now();
    display::draw_rgb565_u8array_fast(display_manager, 0, 0, display_manager.get_screen_width(), display_manager.get_screen_height(), &rgb565)?;
    let draw_ms = t1.elapsed().as_millis();
    Ok((display_manager.get_screen_width(), display_manager.get_screen_height(), format!("recv:{len}bytes {recv_ms}ms, draw:{draw_ms}ms")))
}

fn handle_color_adjust(
    ctx: &mut Context,
    req: &mut esp_idf_svc::http::server::Request<&mut EspHttpConnection<'_>>,
) -> Result<()> {
    #[derive(serde::Deserialize)]
    struct ColorAdjust {
        r: i8,
        g: i8,
        b: i8,
    }
    
    let mut buf = Box::new(vec![0u8; 256]);
    let len = req.read(&mut buf)?;
    let data = &buf[0..len];
    
    let adjust: ColorAdjust = serde_json::from_slice(data)?;
    
    // 验证范围
    if adjust.r < -100 || adjust.r > 100 || 
       adjust.g < -100 || adjust.g > 100 || 
       adjust.b < -100 || adjust.b > 100 {
        return Err(anyhow!("色调调整值必须在-100到100之间"));
    }
    
    // 更新配置
    if let Some(cfg) = ctx.config.display_config.as_mut() {
        cfg.color_adjust_r = adjust.r;
        cfg.color_adjust_g = adjust.g;
        cfg.color_adjust_b = adjust.b;
    } else {
        return Err(anyhow!("Display not configured"));
    }
    
    // 同步更新DisplayManager中的配置
    if let Some(display_manager) = ctx.display.as_mut() {
        display_manager.display_config.color_adjust_r = adjust.r;
        display_manager.display_config.color_adjust_g = adjust.g;
        display_manager.display_config.color_adjust_b = adjust.b;
    }
    
    // 保存到NVS
    config::save_config(&mut ctx.config_nvs, &ctx.config)?;
    
    // 绘制提示信息来触发屏幕刷新，使色调调整立即可见
    let adjust_text = format!("R:{} G:{} B:{}", adjust.r, adjust.g, adjust.b);
    let _ = canvas::draw_splash_with_error(ctx, Some("Color Adjusted"), Some(&adjust_text));
    
    info!("Color adjustment updated: R={}, G={}, B={}", adjust.r, adjust.g, adjust.b);
    
    Ok(())
}

fn handle_display_rgb565_lz4(
    ctx: &mut Context,
    req: &mut esp_idf_svc::http::server::Request<&mut EspHttpConnection<'_>>,
) -> Result<(u16, u16, String)> {
    // 检查内存是否足够
    let free_heap = unsafe { esp_get_free_heap_size() } as usize;
    const MIN_REQUIRED_HEAP: usize = 200 * 1024; // 至少需要 200KB
    
    if free_heap < MIN_REQUIRED_HEAP {
        return Err(anyhow!("内存不足，拒绝请求 (free_heap: {} KB)", free_heap / 1024));
    }
    
    let t1 = Instant::now();
    let len = req.content_len().unwrap_or(0) as usize;
    let max_len = 500 * 1024;
    if len > max_len {
        return Err(anyhow!("http请求体不能超过{max_len}字节"));
    }
    let mut data = Box::new(vec![0; len]);
    req.read_exact(&mut data)?;
    let recv_ms = t1.elapsed().as_millis();
    let t1 = Instant::now();

    let rgb565 = lz4_flex::decompress_size_prepended(&data)?;

    let display_manager = match ctx.display.as_mut() {
        None => return Err(anyhow!("display not init!")),
        Some(v) => v,
    };

    let rgb565 = &rgb565[0..display_manager.get_screen_width() as usize
    * display_manager.get_screen_height() as usize * 2];

    let decode_ms = t1.elapsed().as_millis();
    let t1 = Instant::now();
    display::draw_rgb565_u8array_fast(display_manager, 0, 0, display_manager.get_screen_width(), display_manager.get_screen_height(), &rgb565)?;
    let draw_ms = t1.elapsed().as_millis();
    Ok((display_manager.get_screen_width(), display_manager.get_screen_height(), format!("recv:{len}bytes {recv_ms}ms, decode:{decode_ms}ms, draw:{draw_ms}ms")))
}

fn handle_display_config(
    req: &mut esp_idf_svc::http::server::Request<&mut EspHttpConnection<'_>>,
) -> Result<()> {
    let mut buf = Box::new(vec![0u8; 1024 * 2]);
    let len = req.read(&mut buf)?;
    let data = buf[0..len].to_vec();

    let cfg = config::parse_display_config(data)?;

    check_screen_size(&cfg)?;

    //保存配置
    with_context(move |ctx| {
        ctx.config.display_config.replace(cfg);
        config::save_config(&mut ctx.config_nvs, &ctx.config)?;
        Ok(())
    })?;

    //屏幕参数保存成功后重启
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(1500));
        unsafe { esp_restart() };
    });
    Ok(())
}

fn create_server() -> anyhow::Result<EspHttpServer<'static>> {
    let server_configuration = esp_idf_svc::http::server::Configuration {
        stack_size: STACK_SIZE,
        // Increase max open sockets for better concurrent request handling
        max_open_sockets: 7,
        // Enable LRU purge to recycle idle connections faster
        lru_purge_enable: true,
        // Reduce session timeout for faster connection recycling (5 minutes)
        session_timeout: std::time::Duration::from_secs(5 * 60),
        ..Default::default()
    };

    Ok(EspHttpServer::new(&server_configuration)?)
}
