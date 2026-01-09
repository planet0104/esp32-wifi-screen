use esp_idf_svc::mqtt::client::{EspMqttClient, EspMqttEvent, EventPayload, MqttClientConfiguration};

use log::{error, info};
use serde::Deserialize;

use std::str;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use anyhow::{anyhow, Result};

use crate::canvas::{decode_jpg_to_rgb, draw_elements, Element};
use crate::utils::decode_base64;
use crate::{with_context, Context, ImageCache};

///接收到的mqtt消息
#[derive(Clone, Deserialize)]
pub enum TextMessage{
    //绘制消息
    Draw(Vec<Element>),
    //上传图片消息 (key, base64文件数据)
    Upload((String, String))
}

pub fn listen_config() -> Result<()> {
    let config = match with_context(move |ctx| {
        Ok(ctx.config.remote_server_config.clone())
    }){
        Ok(Some(c)) => c,
        _ => {
            return Err(anyhow!("no mqtt config!"));
        }
    };

    if config.mqtt_topic.is_none() || config.mqtt_url.is_none(){
        return Err(anyhow!("mqtt no topic!!"));
    }

    let mqtt_url = config.mqtt_url.unwrap();

    info!("create mqtt client...");

    let mut topic = String::new();
    if let Some(t) = config.mqtt_topic.as_ref(){
        topic = t.as_str().to_string();
    }

    let text_cache = Arc::new(Mutex::new(Box::new(String::new())));

    let mut client = match EspMqttClient::new_cb(
        mqtt_url.as_str(),
        &MqttClientConfiguration {
            client_id: config.mqtt_client_id.as_ref().map(|x| x.as_str()),
            password: config.mqtt_password.as_ref().map(|x| x.as_str()),
            username: config.mqtt_username.as_ref().map(|x| x.as_str()),
            ..Default::default()
        },
    move |event|{
        let text_cache_clone = text_cache.clone();
        if let Err(err) = parse_event(&event, text_cache_clone){
            error!("mqtt event parse error:{err:?}");
        }
    }){
        Ok(a) => a,
        Err(err) => {
            return Err(anyhow!("create mqtt client: {err:?}"));
        }
    };

    info!("mqtt client created...");

    std::thread::spawn(move || {
        let mut topic_subscribe_ok = false;

        loop {

            if !topic_subscribe_ok{

                if !topic_subscribe_ok{
                    if topic.len() > 0{
                        info!("mqtt subscribe text topic:{topic} qos:{:?}", config.mqtt_qos);
                        if let Err(err) = client.subscribe(topic.as_str(), config.mqtt_qos.clone()){
                            info!("mqtt subscribe fail:{err:?}");
                        }else{
                            topic_subscribe_ok = true;
                            info!("mqtt subscribe text Ok.");
                        }
                    }else{
                        topic_subscribe_ok = true;
                    }
                }

                // Re-try in 0.5s
                std::thread::sleep(Duration::from_millis(1000));

                continue;
            }

            info!("Subscribed complete...");

            // Just to give a chance of our connection to get even the first published message
            std::thread::sleep(Duration::from_millis(500));

            // let payload = "Hello from esp-mqtt-demo!";

            loop {
                // client.enqueue(topic, QoS::AtMostOnce, false, payload.as_bytes())?;

                // info!("Published \"{payload}\" to topic \"{topic}\"");

                let sleep_secs = 2;

                // info!("Now sleeping for {sleep_secs}s...");
                std::thread::sleep(Duration::from_secs(sleep_secs));
            }
        }
    });
    Ok(())
}

fn parse_event<'a>(event: &EspMqttEvent<'a>, text_cache:Arc<Mutex<Box<String>>>) -> Result<()>{
    match event.payload(){
        EventPayload::BeforeConnect => {
            info!("mqtt event BeforeConnect.");
        }
        EventPayload::Connected(_) => {
            info!("mqtt event Connected.");
        }
        EventPayload::Disconnected => {
            info!("mqtt event Disconnected.");
        }
        EventPayload::Subscribed(_) => {
            info!("mqtt event Subscribed.");
        }
        EventPayload::Unsubscribed(_) => {
            info!("mqtt event Unsubscribed.");
        }
        EventPayload::Published(_) => {
            info!("mqtt event Published.");
        }
        EventPayload::Received { id:_, topic, data, details: _ } => {
            info!("mqtt event Recv topic:{topic:?} len{}", data.len());
            let data = unsafe{ str::from_boxed_utf8_unchecked(data.into()) };
            //文本消息以r#"和 "#结尾
            let mut text_cache_lock = text_cache.lock().map_err(|err| anyhow!("{err:?}"))?;
            let mut text_end = false;
            if data.starts_with("r#\"") && data.ends_with("\"#"){
                //字符串开始了，清空缓存
                text_cache_lock.clear();
                text_cache_lock.push_str(&data[3..data.len()-2]);
                text_end = true;
            } else if data.starts_with("r#\""){
                //字符串开始了，清空缓存
                text_cache_lock.clear();
                text_cache_lock.push_str(&data[3..]);
            }else if data.ends_with("\"#"){
                //字符串结束了
                text_cache_lock.push_str(&data[..data.len()-2]);
                text_end = true;
            }else{
                //字符串中间部分
                text_cache_lock.push_str(&data);
            }
            if text_end{
                let json = Box::new(text_cache_lock.to_string());
                text_cache_lock.clear();
                info!("mqtt json len:{}", json.len());
                // With CONFIG_SPIRAM_ALLOW_STACK_EXTERNAL_MEMORY, stack uses PSRAM
                if let Err(err) = std::thread::Builder::new()
                .stack_size(12*1024)
                .spawn(move ||{
                    if let Err(err) = with_context(move |ctx|{
                        handle_mqtt_message(ctx, json)
                    }){
                        error!("mqtt parse json:{err:?}");
                    }
                }){
                    info!("mqtt thread error:{err:?}");
                }
            }
        }
        EventPayload::Deleted(_) => {
            info!("mqtt event Deleted.");
        }
        EventPayload::Error(err) => {
            info!("mqtt event err {err:?}");
        }
    }
    Ok(())
}

pub fn handle_mqtt_message(ctx: &mut Context, json: Box<String>) -> Result<()> {
    let display_manager = match ctx.display.as_mut() {
        None => return Err(anyhow!("请设置屏幕参数!")),
        Some(v) => v,
    };
    let msg: Box<TextMessage> = Box::new(serde_json::from_str(&json)
        .map_err(|err| anyhow!("parse message {err:?} json:`{json}`"))?);

    match msg.as_ref(){
        TextMessage::Draw(elements) => {
            draw_elements(display_manager, &ctx.image_cache, &elements)
                .map_err(|err| anyhow!("draw elements: {err:?}"))?;
        }
        TextMessage::Upload((key, base64)) => {
            //删除老的图片
            drop(ctx.image_cache.remove(key));
            if ctx.image_cache.len() >= 5 {
                return Err(anyhow!("最多缓存5张图片"));
            }
            let data = decode_base64(&base64)?;
            let mime = mimetype::detect(&data);
            if mime.extension.ends_with("jpg") || mime.extension.ends_with("jpeg") {
                //rgb565转rgb
                let rgb = decode_jpg_to_rgb(data)?;
                ctx.image_cache.insert(key.to_string(), ImageCache::RgbImage(rgb));
            } else {
                let rgba = Box::new(image::load_from_memory(&data)?.to_rgba8());
                ctx.image_cache.insert(key.to_string(), ImageCache::RgbaImage(rgba));
            };
        }
    }
    Ok(())
}