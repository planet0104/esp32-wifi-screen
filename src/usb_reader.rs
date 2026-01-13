use std::sync::mpsc::Sender;
use std::thread;
use std::time::Duration;

use crate::with_context;
use crate::display;

// ============ 配置开关 ============
// 是否启用调试 ACK 回显（false 时不发送绘制相关的调试信息，提高传输速度）
// 测速相关的 SPEEDRESULT 不受此开关影响
const DEBUG_ACK_ENABLED: bool = false;

// ============ 安全限制 ============
// 图像接收缓冲区最大大小（防止内存溢出）
// 对于 320x240 RGB565 图像，压缩后约 50-150KB，设置为 512KB 足够
const MAX_IMAGE_BUF_SIZE: usize = 512 * 1024;
// 帧接收超时时间（毫秒），超时后重置接收状态
const FRAME_RECEIVE_TIMEOUT_MS: u128 = 3000;

// small helper: find the first occurrence of `needle` in `hay`
fn find_subslice(hay: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() {
        return Some(0);
    }
    hay.windows(needle.len()).position(|w| w == needle)
}

/// Start reader without a sender (keeps previous behaviour)
pub fn start() {
    start_with_sender(None);
}

/// Start reader; if `sender` is Some, responses will be sent to that channel
/// and the main thread can perform stdout writes and flushes.
pub fn start_with_sender(sender: Option<Sender<String>>) {
    // clone sender so we can use one copy inside the spawned thread
    let sender_for_thread = sender.clone();
    // If building for ESP32-S3, use USB Serial JTAG IDF API for high-speed reads
    #[cfg(feature = "esp32s3")]
    {
        use esp_idf_sys as sys;
        use core::ffi::c_void;
        let spawn_res = thread::Builder::new()
            .name("usb_s3_reader".to_string())
            .stack_size(64 * 1024)
            .spawn(move || {
                let sender = sender_for_thread;
                // buffer for reads
                let mut buf = Vec::<u8>::new();
                let mut read_buf = [0u8; 4096usize];

                // protocol markers (match example)
                const IMAGE_AA: u64 = 7596835243154170209;
                const IMAGE_BB: u64 = 7596835243154170466;
                const BOOT_USB: u64 = 7093010483740242786;
                const READ_INF: u64 = 0x52656164496e666f; // "ReadInfo"
                const SPEED_AA_BYTES: [u8; 8] = *b"SPDTEST1";
                const SPEED_BB_BYTES: [u8; 8] = *b"SPDEND!!";

                let aa_bytes = IMAGE_AA.to_be_bytes();
                let bb_bytes = IMAGE_BB.to_be_bytes();
                let boot_bytes = BOOT_USB.to_be_bytes();
                let readinf_bytes = READ_INF.to_be_bytes();
                let readinf_ascii = b"ReadInfo";
                let speed_aa = SPEED_AA_BYTES;
                let speed_bb = SPEED_BB_BYTES;

                let mut receiving = false;
                let mut image_buf: Vec<u8> = Vec::new();
                let mut speedbin_active: bool = false;
                let mut speedbin_received: usize = 0;
                let mut speedbin_start: Option<std::time::Instant> = None;
                let mut image_width: u16 = 0;
                let mut image_height: u16 = 0;
                let mut image_x: u16 = 0;
                let mut image_y: u16 = 0;
                // 帧接收开始时间（用于超时检测）
                let mut frame_start_time: Option<std::time::Instant> = None;
                // 空闲计数器（用于定期让出 CPU）
                let mut idle_count: u32 = 0;

                // 发送调试信息（受 DEBUG_ACK_ENABLED 控制）
                let send_debug = |sender: &Option<Sender<String>>, msg: String| {
                    if DEBUG_ACK_ENABLED {
                        if let Some(s) = sender { let _ = s.send(msg); }
                    }
                };
                // 发送重要信息（始终发送，如测速结果、设备信息）
                let send_info = |sender: &Option<Sender<String>>, msg: String| {
                    if let Some(s) = sender { let _ = s.send(msg); }
                };
                let send_error = |sender: &Option<Sender<String>>, msg: String| {
                    if let Some(s) = sender { let _ = s.send(format!("ERROR:{}\n", msg)); }
                };

                loop {
                    // Call IDF USB read (blocking with short timeout ticks)
                    let n = unsafe {
                        sys::usb_serial_jtag_read_bytes(
                            read_buf.as_mut_ptr() as *mut c_void,
                            read_buf.len() as u32,
                            10u32,
                        )
                    };
                    
                    if n <= 0 {
                        // 无数据时处理超时和空闲
                        idle_count += 1;
                        
                        // 每 100 次空闲循环让出一次 CPU，喂看门狗
                        if idle_count >= 100 {
                            idle_count = 0;
                            thread::sleep(Duration::from_millis(1));
                        }
                        
                        // 检查帧接收超时
                        if receiving {
                            if let Some(start) = frame_start_time {
                                if start.elapsed().as_millis() > FRAME_RECEIVE_TIMEOUT_MS {
                                    // 帧接收超时，重置状态
                                    log::warn!("[USB] Frame receive timeout, resetting state. buf_size={}", image_buf.len());
                                    receiving = false;
                                    image_buf.clear();
                                    buf.clear();
                                    frame_start_time = None;
                                }
                            }
                        }
                        continue;
                    }
                    
                    idle_count = 0;
                    let n_usize = n as usize;
                    buf.extend_from_slice(&read_buf[..n_usize]);

                    loop {
                        if speedbin_active {
                            if buf.len() > 0 {
                                if let Some(pos) = find_subslice(&buf, &speed_bb) {
                                    speedbin_received = speedbin_received.saturating_add(pos);
                                    let _ = buf.drain(..pos + speed_bb.len());
                                    if let Some(start) = speedbin_start.take() {
                                        let ms = start.elapsed().as_millis();
                                        let _ = send_info(&sender, format!("SPEEDRESULT;{};{}\n", speedbin_received, ms));
                                        thread::sleep(Duration::from_millis(10));
                                        let _ = send_info(&sender, format!("SPEEDRESULT;{};{}\n", speedbin_received, ms));
                                    } else {
                                        let _ = send_info(&sender, format!("SPEEDRESULT;{};0\n", speedbin_received));
                                        thread::sleep(Duration::from_millis(10));
                                        let _ = send_info(&sender, format!("SPEEDRESULT;{};0\n", speedbin_received));
                                    }
                                    speedbin_active = false;
                                    speedbin_received = 0;
                                    continue;
                                } else {
                                    let keep = if speed_bb.len() > 0 { speed_bb.len() - 1 } else { 0 };
                                    if buf.len() > keep {
                                        let take = buf.len() - keep;
                                        buf.drain(..take);
                                        speedbin_received = speedbin_received.saturating_add(take);
                                    }
                                }
                            }
                            if speedbin_active { break; }
                        }

                        if receiving {
                            image_buf.extend_from_slice(&buf);
                            buf.clear();
                            
                            // 检查缓冲区大小限制
                            if image_buf.len() > MAX_IMAGE_BUF_SIZE {
                                log::warn!("[USB] Image buffer overflow ({}), resetting", image_buf.len());
                                receiving = false;
                                image_buf.clear();
                                frame_start_time = None;
                                continue;
                            }
                            
                            if let Some(pos) = find_subslice(&image_buf, &bb_bytes) {
                                // 帧接收完成，清除超时计时器
                                frame_start_time = None;
                                
                                let compressed_len = pos;
                                let compressed_data = image_buf[..compressed_len].to_vec();
                                let remainder_start = pos + bb_bytes.len();
                                let remainder = image_buf[remainder_start..].to_vec();
                                image_buf.clear();
                                buf.extend_from_slice(&remainder);
                                // 计算压缩率（调试信息）
                                let compression_ratio = if compressed_len > 0 {
                                    (image_width as usize * image_height as usize * 2) as f32 / compressed_len as f32
                                } else { 0.0 };
                                send_debug(&sender, format!("FRAME_RECV;compressed={};ratio={:.1}\n", compressed_len, compression_ratio));
                                
                                match lz4_flex::decompress_size_prepended(&compressed_data) {
                                    Ok(decompressed) => {
                                        let expected = image_width as usize * image_height as usize * 2;
                                        send_debug(&sender, format!("LZ4_OK;decompressed={};expected={}\n", decompressed.len(), expected));
                                        if decompressed.len() != expected {
                                            let _ = send_error(&sender, format!("SIZE_MISMATCH;decompressed={};expected={}\n", decompressed.len(), expected));
                                        } else {
                                            // 记录绘制开始时间
                                            let draw_start = std::time::Instant::now();
                                            send_debug(&sender, format!("DRAW_START;x={};y={};w={};h={};bytes={}\n", 
                                                image_x, image_y, image_width, image_height, decompressed.len()));
                                            
                                            let draw_result = std::panic::catch_unwind(|| {
                                                with_context(|ctx| {
                                                    if let Some(display_manager) = ctx.display.as_mut() {
                                                        // 获取屏幕信息用于回复（调试信息）
                                                        let (screen_w, screen_h) = display_manager.get_screen_size();
                                                        send_debug(&sender, format!("SCREEN_SIZE;w={};h={}\n", screen_w, screen_h));
                                                        
                                                        display::draw_rgb565_u8array_fast(
                                                            display_manager,
                                                            image_x,
                                                            image_y,
                                                            image_width,
                                                            image_height,
                                                            &decompressed,
                                                        )
                                                    } else { 
                                                        let _ = send_error(&sender, "NO_DISPLAY\n".to_string());
                                                        Ok(()) 
                                                    }
                                                })
                                            });
                                            
                                            let draw_ms = draw_start.elapsed().as_millis();
                                            match draw_result {
                                                Ok(Ok(_)) => { 
                                                    // 绘制成功（调试信息）
                                                    send_debug(&sender, format!("DRAW_OK;x={};y={};w={};h={};ms={}\n", 
                                                        image_x, image_y, image_width, image_height, draw_ms)); 
                                                }
                                                Ok(Err(e)) => { 
                                                    let _ = send_error(&sender, format!("DRAW_FAIL;error={:?};ms={}\n", e, draw_ms)); 
                                                }
                                                Err(_) => { 
                                                    let _ = send_error(&sender, format!("DRAW_PANIC;ms={}\n", draw_ms)); 
                                                }
                                            }
                                        }
                                    }
                                    Err(e) => { let _ = send_error(&sender, format!("LZ4_FAIL;error={:?}\n", e)); }
                                }
                                image_buf.clear();
                                receiving = false;
                                continue;
                            }
                            buf.clear();
                            break;
                        } else {
                            if !speedbin_active {
                                if let Some(pos) = find_subslice(&buf, &speed_aa) {
                                    buf.drain(..pos + speed_aa.len());
                                    speedbin_active = true;
                                    speedbin_received = 0;
                                    speedbin_start = Some(std::time::Instant::now());
                                    continue;
                                }
                            }
                            if let Some(pos) = find_subslice(&buf, &aa_bytes) {
                                if buf.len() < pos + 16 { break; }
                                let start = pos;
                                image_width = u16::from_be_bytes([buf[start + 8], buf[start + 9]]);
                                image_height = u16::from_be_bytes([buf[start + 10], buf[start + 11]]);
                                image_x = u16::from_be_bytes([buf[start + 12], buf[start + 13]]);
                                image_y = u16::from_be_bytes([buf[start + 14], buf[start + 15]]);
                                buf.drain(..start + 16);
                                send_debug(&sender, format!("FRAME_START;{};{};{};{}\n", image_width, image_height, image_x, image_y));
                                receiving = true;
                                image_buf.clear();
                                // 记录帧接收开始时间
                                frame_start_time = Some(std::time::Instant::now());
                                continue;
                            }
                            let pos_bin = find_subslice(&buf, &readinf_bytes);
                            let pos_ascii = find_subslice(&buf, readinf_ascii);
                            if pos_bin.is_some() || pos_ascii.is_some() {
                                let pos = match (pos_bin, pos_ascii) { (Some(p), Some(q)) => if p <= q { p } else { q }, (Some(p), None) => p, (None, Some(q)) => q, _ => unreachable!(), };
                                let len = if pos + readinf_bytes.len() <= buf.len() && &buf[pos..pos + readinf_bytes.len()] == readinf_bytes { readinf_bytes.len() } else { readinf_ascii.len() };
                                buf.drain(..pos+len);
                                let resp = match query_screen_size() { Some((w,h)) => format!("ESP32-WIFI-SCREEN;{};{};PROTO:USB-SCREEN\n", w, h), None => "ESP32-WIFI-SCREEN;0;0;PROTO:USB-SCREEN\n".to_string() };
                                let _ = send_info(&sender, resp);
                                thread::sleep(Duration::from_millis(10));
                                continue;
                            }
                            if let Some(pos) = find_subslice(&buf, &boot_bytes) {
                                buf.drain(..pos + boot_bytes.len());
                                let resp = "BOOTED\n".to_string();
                                let _ = send_info(&sender, resp);
                                thread::sleep(Duration::from_millis(10));
                                continue;
                            }
                            if let Some(nlpos) = buf.iter().position(|&b| b == b'\n') {
                                buf.drain(..=nlpos);
                                continue;
                            }
                            break;
                        }
                    }
                }
            });
        if let Err(e) = spawn_res { if let Some(s) = sender { let _ = s.send(format!("ERROR:Failed to spawn USB s3 reader thread: {:?}\n", e)); } }
    }

    // ESP32-S2 path: use VFS read with timeout (similar to ESP32-S3)
    // Note: ESP32-S2 has less internal RAM than ESP32-S3, use smaller stack size (16KB)
    // The thread uses heap for buffers (Vec), so 16KB stack should be sufficient
    #[cfg(feature = "esp32s2")]
    {
        use esp_idf_sys as sys;
        use core::ffi::c_void;
        
        let spawn_res = thread::Builder::new()
            .name("usb_s2_reader".to_string())
            .stack_size(16 * 1024)
            .spawn(move || {
                let sender = sender_for_thread;
                let mut buf = Vec::<u8>::new();
                let mut read_buf = [0u8; 4096];
                
                // Get stdin file descriptor
                let stdin_fd: i32 = 0; // stdin is fd 0
                
                // Set stdin to non-blocking mode using fcntl
                unsafe {
                    let flags = sys::fcntl(stdin_fd, sys::F_GETFL as i32, 0);
                    if flags >= 0 {
                        sys::fcntl(stdin_fd, sys::F_SETFL as i32, flags | sys::O_NONBLOCK as i32);
                        log::info!("[USB-S2] Set stdin to non-blocking mode");
                    } else {
                        log::warn!("[USB-S2] Failed to get stdin flags, continuing anyway");
                    }
                }

                const IMAGE_AA: u64 = 7596835243154170209;
                const IMAGE_BB: u64 = 7596835243154170466;
                const BOOT_USB: u64 = 7093010483740242786;
                const READ_INF: u64 = 0x52656164496e666f;
                const SPEED_AA_BYTES: [u8; 8] = *b"SPDTEST1";
                const SPEED_BB_BYTES: [u8; 8] = *b"SPDEND!!";

                let aa_bytes = IMAGE_AA.to_be_bytes();
                let bb_bytes = IMAGE_BB.to_be_bytes();
                let boot_bytes = BOOT_USB.to_be_bytes();
                let readinf_bytes = READ_INF.to_be_bytes();
                let readinf_ascii = b"ReadInfo";
                let speed_aa = SPEED_AA_BYTES;
                let speed_bb = SPEED_BB_BYTES;

                let mut receiving = false;
                let mut image_buf: Vec<u8> = Vec::new();
                let mut speedbin_active: bool = false;
                let mut speedbin_received: usize = 0;
                let mut speedbin_start: Option<std::time::Instant> = None;
                let mut image_width: u16 = 0;
                let mut image_height: u16 = 0;
                let mut image_x: u16 = 0;
                let mut image_y: u16 = 0;
                let mut frame_start_time: Option<std::time::Instant> = None;
                let mut idle_count: u32 = 0;

                let send_debug = |sender: &Option<Sender<String>>, msg: String| {
                    if DEBUG_ACK_ENABLED {
                        if let Some(s) = sender { let _ = s.send(msg); }
                    }
                };
                let send_info = |sender: &Option<Sender<String>>, msg: String| {
                    if let Some(s) = sender { let _ = s.send(msg); }
                };
                let send_error = |sender: &Option<Sender<String>>, msg: String| {
                    if let Some(s) = sender { let _ = s.send(format!("ERROR:{}\n", msg)); }
                };

                log::info!("[USB-S2] Reader thread started");

                loop {
                    // Use libc read with short sleep for non-blocking behavior
                    // ESP32-S2 TinyUSB CDC doesn't have a timeout-based read API like ESP32-S3
                    let n = unsafe {
                        sys::read(stdin_fd, read_buf.as_mut_ptr() as *mut c_void, read_buf.len())
                    };
                    
                    if n <= 0 {
                        // No data or error - sleep briefly and retry
                        idle_count += 1;
                        if idle_count >= 100 {
                            idle_count = 0;
                            thread::sleep(Duration::from_millis(1));
                        } else {
                            // Short sleep to prevent busy loop but still be responsive
                            thread::sleep(Duration::from_micros(100));
                        }
                        
                        if receiving {
                            if let Some(start) = frame_start_time {
                                if start.elapsed().as_millis() > FRAME_RECEIVE_TIMEOUT_MS {
                                    log::warn!("[USB-S2] Frame timeout, resetting");
                                    receiving = false;
                                    image_buf.clear();
                                    buf.clear();
                                    frame_start_time = None;
                                }
                            }
                        }
                        continue;
                    }
                    
                    let n_usize = n as usize;
                    idle_count = 0;
                    buf.extend_from_slice(&read_buf[..n_usize]);

                    // Data processing loop (same logic as ESP32-S3)
                    loop {
                        if speedbin_active {
                            if !buf.is_empty() {
                                if let Some(pos) = find_subslice(&buf, &speed_bb) {
                                    speedbin_received = speedbin_received.saturating_add(pos);
                                    buf.drain(..pos + speed_bb.len());
                                    if let Some(start) = speedbin_start.take() {
                                        let ms = start.elapsed().as_millis();
                                        // Send result twice with delay (same as ESP32-S3)
                                        let _ = send_info(&sender, format!("SPEEDRESULT;{};{}\n", speedbin_received, ms));
                                        thread::sleep(Duration::from_millis(10));
                                        let _ = send_info(&sender, format!("SPEEDRESULT;{};{}\n", speedbin_received, ms));
                                    } else {
                                        let _ = send_info(&sender, format!("SPEEDRESULT;{};0\n", speedbin_received));
                                        thread::sleep(Duration::from_millis(10));
                                        let _ = send_info(&sender, format!("SPEEDRESULT;{};0\n", speedbin_received));
                                    }
                                    speedbin_active = false;
                                    speedbin_received = 0;
                                    // Log remaining buffer after speed test
                                    log::info!("[USB-S2] Speed test completed, buf remaining: {} bytes", buf.len());
                                    // Flush stdout to ensure speed result is sent
                                    let _ = std::io::Write::flush(&mut std::io::stdout());
                                    continue;
                                } else {
                                    let keep = speed_bb.len().saturating_sub(1);
                                    if buf.len() > keep {
                                        let take = buf.len() - keep;
                                        buf.drain(..take);
                                        speedbin_received = speedbin_received.saturating_add(take);
                                    }
                                }
                            }
                            if speedbin_active { break; }
                        }

                        if receiving {
                            image_buf.extend_from_slice(&buf);
                            buf.clear();
                            
                            if image_buf.len() > MAX_IMAGE_BUF_SIZE {
                                log::warn!("[USB-S2] Buffer overflow, resetting");
                                receiving = false;
                                image_buf.clear();
                                frame_start_time = None;
                                continue;
                            }
                            
                            if let Some(pos) = find_subslice(&image_buf, &bb_bytes) {
                                frame_start_time = None;
                                let compressed_data = image_buf[..pos].to_vec();
                                let remainder = image_buf[pos + bb_bytes.len()..].to_vec();
                                image_buf.clear();
                                buf.extend_from_slice(&remainder);
                                
                                send_debug(&sender, format!("FRAME_RECV;len={}\n", compressed_data.len()));
                                
                                match lz4_flex::decompress_size_prepended(&compressed_data) {
                                    Ok(decompressed) => {
                                        let expected = image_width as usize * image_height as usize * 2;
                                        if decompressed.len() != expected {
                                            let _ = send_error(&sender, format!("SIZE_MISMATCH\n"));
                                        } else {
                                            let draw_start = std::time::Instant::now();
                                            let draw_result = std::panic::catch_unwind(|| {
                                                with_context(|ctx| {
                                                    if let Some(dm) = ctx.display.as_mut() {
                                                        display::draw_rgb565_u8array_fast(dm, image_x, image_y, image_width, image_height, &decompressed)
                                                    } else { Ok(()) }
                                                })
                                            });
                                            let draw_ms = draw_start.elapsed().as_millis();
                                            match draw_result {
                                                Ok(Ok(_)) => { send_debug(&sender, format!("DRAW_OK;ms={}\n", draw_ms)); }
                                                Ok(Err(e)) => { let _ = send_error(&sender, format!("DRAW_FAIL;{:?}\n", e)); }
                                                Err(_) => { let _ = send_error(&sender, "DRAW_PANIC\n".to_string()); }
                                            }
                                        }
                                    }
                                    Err(e) => { let _ = send_error(&sender, format!("LZ4_FAIL;{:?}\n", e)); }
                                }
                                image_buf.clear();
                                receiving = false;
                                continue;
                            }
                            buf.clear();
                            break;
                        } else {
                            if !speedbin_active {
                                if let Some(pos) = find_subslice(&buf, &speed_aa) {
                                    buf.drain(..pos + speed_aa.len());
                                    speedbin_active = true;
                                    speedbin_received = 0;
                                    speedbin_start = Some(std::time::Instant::now());
                                    continue;
                                }
                            }
                            if let Some(pos) = find_subslice(&buf, &aa_bytes) {
                                if buf.len() < pos + 16 { break; }
                                image_width = u16::from_be_bytes([buf[pos + 8], buf[pos + 9]]);
                                image_height = u16::from_be_bytes([buf[pos + 10], buf[pos + 11]]);
                                image_x = u16::from_be_bytes([buf[pos + 12], buf[pos + 13]]);
                                image_y = u16::from_be_bytes([buf[pos + 14], buf[pos + 15]]);
                                buf.drain(..pos + 16);
                                send_debug(&sender, format!("FRAME_START;{}x{}\n", image_width, image_height));
                                receiving = true;
                                image_buf.clear();
                                frame_start_time = Some(std::time::Instant::now());
                                continue;
                            }
                            // Handle ReadInfo command (same logic as ESP32-S3)
                            let pos_bin = find_subslice(&buf, &readinf_bytes);
                            let pos_ascii = find_subslice(&buf, readinf_ascii);
                            // Debug: log buffer content when looking for ReadInfo
                            if buf.len() > 0 && buf.len() <= 32 {
                                log::debug!("[USB-S2] Checking buf ({} bytes): {:?}", buf.len(), 
                                    String::from_utf8_lossy(&buf[..std::cmp::min(buf.len(), 32)]));
                            }
                            if pos_bin.is_some() || pos_ascii.is_some() {
                                let pos = match (pos_bin, pos_ascii) { 
                                    (Some(p), Some(q)) => if p <= q { p } else { q }, 
                                    (Some(p), None) => p, 
                                    (None, Some(q)) => q, 
                                    _ => unreachable!() 
                                };
                                let len = if pos + readinf_bytes.len() <= buf.len() && 
                                             &buf[pos..pos + readinf_bytes.len()] == readinf_bytes { 
                                    readinf_bytes.len() 
                                } else { 
                                    readinf_ascii.len() 
                                };
                                buf.drain(..pos+len);
                                let resp = match query_screen_size() {
                                    Some((w,h)) => format!("ESP32-WIFI-SCREEN;{};{};PROTO:USB-SCREEN\n", w, h),
                                    None => "ESP32-WIFI-SCREEN;0;0;PROTO:USB-SCREEN\n".to_string()
                                };
                                let _ = send_info(&sender, resp.clone());
                                // Also write directly to stdout for immediate response
                                let _ = std::io::Write::write_all(&mut std::io::stdout(), resp.as_bytes());
                                let _ = std::io::Write::flush(&mut std::io::stdout());
                                thread::sleep(Duration::from_millis(10));
                                continue;
                            }
                            if let Some(pos) = find_subslice(&buf, &boot_bytes) {
                                buf.drain(..pos + boot_bytes.len());
                                let _ = send_info(&sender, "BOOTED\n".to_string());
                                continue;
                            }
                            if let Some(nlpos) = buf.iter().position(|&b| b == b'\n') {
                                buf.drain(..=nlpos);
                                continue;
                            }
                            break;
                        }
                    }
                }
                let _ = send_info(&sender, "USB-S2 reader exiting\n".to_string());
            });
        if let Err(e) = spawn_res { if let Some(s) = sender { let _ = s.send(format!("ERROR:USB-S2 spawn failed: {:?}\n", e)); } }
    }
}

fn query_screen_size() -> Option<(u16, u16)> {
    match with_context(|ctx| {
        if let Some(display_manager) = ctx.display.as_ref() {
            Ok((display_manager.get_screen_width(), display_manager.get_screen_height()))
        } else {
            Ok((0u16, 0u16))
        }
    }) {
        Ok((w, h)) if w > 0 && h > 0 => Some((w, h)),
        _ => None,
    }
}
