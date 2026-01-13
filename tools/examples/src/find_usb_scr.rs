use anyhow::Result;
use std::time::{Instant, Duration};
use image::imageops::FilterType;
use image::RgbImage;

// Small helper to provide a program-relative timestamp for logs.
fn ts() -> String {
    use std::sync::Once;
    static START_ONCE: Once = Once::new();
    static mut START: Option<Instant> = None;
    START_ONCE.call_once(|| unsafe { START = Some(Instant::now()); });
    let start = unsafe { START.unwrap() };
    let d = start.elapsed();
    let s = d.as_secs();
    let mins = s / 60;
    let secs = s % 60;
    let ms = d.subsec_millis();
    format!("[{:02}:{:02}.{:03}]", mins, secs, ms)
}

macro_rules! ts_println {
    ($($arg:tt)*) => { info!("{} {}", ts(), format!($($arg)*)); }
}
macro_rules! ts_eprintln {
    ($($arg:tt)*) => { error!("{} {}", ts(), format!($($arg)*)); }
}

// Constants copied from the esp32 reader implementation so host and device agree
const IMAGE_AA: u64 = 7596835243154170209u64;
const IMAGE_BB: u64 = 7596835243154170466u64;
// binary-speedtest markers (must match device)
const SPEED_AA_BYTES: [u8;8] = *b"SPDTEST1";
const SPEED_BB_BYTES: [u8;8] = *b"SPDEND!!";

// Minimal stubs for helpers that live in the device repo; these are host-side helpers
fn find_candidate_ports() -> Vec<serialport::SerialPortInfo> {
    match serialport::available_ports() {
        Ok(p) => p,
        Err(_) => Vec::new(),
    }
}

fn probe_port_for_line(_port: &str, _magic: u64, _timeout_ms: u64) -> anyhow::Result<Option<String>> {
    use std::io::{Read, Write};
    // Try to open the named port at a reasonable baud and send the magic bytes,
    // then wait up to timeout for a newline-terminated response. If no reply,
    // try sending an ASCII `ReadInfo\n` probe which some firmware respond to.
    let timeout = Duration::from_millis(_timeout_ms);
    match serialport::new(_port, 115200).timeout(Duration::from_millis(200)).open() {
        Ok(mut port) => {
            // drain any pending data
            let _ = port.read(&mut [0u8; 1024]);

            fn try_read(port: &mut dyn serialport::SerialPort, total_wait: Duration) -> Option<String> {
                let start = Instant::now();
                let mut buf: Vec<u8> = Vec::new();
                let mut tmp = [0u8; 256];
                while start.elapsed() < total_wait {
                    match port.read(&mut tmp) {
                        Ok(n) if n > 0 => {
                            buf.extend_from_slice(&tmp[..n]);
                            if let Some(pos) = buf.iter().position(|&b| b == b'\n') {
                                let line = String::from_utf8_lossy(&buf[..pos]).to_string();
                                return Some(line);
                            }
                        }
                        Ok(_) => {}
                        Err(ref e) if e.kind() == std::io::ErrorKind::TimedOut => {}
                        Err(_) => break,
                    }
                }
                None
            }

            // First try: send magic bytes and wait a short while
            let _ = port.write_all(&_magic.to_be_bytes());
            let _ = port.flush();
            if let Some(line) = try_read(&mut *port, Duration::from_millis(200)) {
                return Ok(Some(line));
            }

            // Second try: send ASCII ReadInfo probe (some firmwares echo this)
            let _ = port.write_all(b"ReadInfo\n");
            let _ = port.flush();
            if let Some(line) = try_read(&mut *port, timeout) {
                return Ok(Some(line));
            }

            Ok(None)
        }
        Err(_) => Ok(None),
    }
}

// READ_INF_MAGIC placeholder
const READ_INF_MAGIC: u64 = 0x52656164496e666f;

#[inline]
fn rgb_to_rgb565(r: u8, g: u8, b: u8) -> u16 {
    ((r as u16 & 0b11111000) << 8) | ((g as u16 & 0b11111100) << 3) | (b as u16 >> 3)
}

fn rgb888_to_rgb565_be(img: &RgbImage) -> Vec<u8> {
    let width = img.width() as usize;
    let height = img.height() as usize;
    let raw = img.as_raw();
    let mut rgb565 = Vec::with_capacity(width * height * 2);
    for p in raw.chunks(3) {
        let pixel = rgb_to_rgb565(p[0], p[1], p[2]);
        rgb565.extend_from_slice(&pixel.to_be_bytes());
    }
    rgb565
}

fn send_gif(path: &str, port: &mut dyn serialport::SerialPort, width: u16, height: u16, _delay_ms: u64) -> Result<()> {
    // For now, fall back to sending the GIF as a single image (first frame).
    // This keeps behavior simple and avoids relying on external GIF frame iteration code.
    send_image_file(path, port, width, height)
}

fn send_speed_tests(port: &mut dyn serialport::SerialPort, runs: usize, size: usize) -> Result<()> {
    use rand::RngCore;
    let mut rng = rand::thread_rng();
    let mut _rates: Vec<f64> = Vec::new();

    for run in 0..runs {
            ts_println!("Speed test {}/{}: sending {} bytes...", run+1, runs, size);
        // drain any residual data before starting
        let mut _drain = [0u8; 1024];
        while let Ok(n) = port.read(&mut _drain) { if n==0 { break } }

        // prepare payload
        let mut bytes_sent = 0usize;
        let mut full_payload = vec![0u8; size];
        rng.fill_bytes(&mut full_payload);

        // build frame: SPEED_AA + payload + SPEED_BB
        let mut frame = Vec::with_capacity(SPEED_AA_BYTES.len() + size + SPEED_BB_BYTES.len());
        frame.extend_from_slice(&SPEED_AA_BYTES);
        frame.extend_from_slice(&full_payload);
        frame.extend_from_slice(&SPEED_BB_BYTES);

        // record local start time and send frame (single write_all preferred)
        let local_start = Instant::now();
        let _ = port.set_timeout(Duration::from_secs(10));
        // try single write and measure how long the write call takes
        let write_start = Instant::now();
        let write_res = port.write_all(&frame);
        let write_dur = write_start.elapsed();
        match write_res {
            Ok(()) => {
                // don't force a blocking flush here; allow OS to buffer
                bytes_sent = size;
                ts_println!("Single write completed in {} ms", write_dur.as_millis());
            }
            Err(e) => {
                ts_println!("Single write failed: {:?}; falling back to chunked send", e);
                // chunked fallback: send AA, then chunks, then BB. Measure total chunked write time.
                let _ = port.write_all(&SPEED_AA_BYTES);
                let chunk_size = 8 * 1024;
                let mut remaining = size;
                let mut offset = 0usize;
                let chunk_write_start = Instant::now();
                while remaining > 0 {
                    let send_now = std::cmp::min(chunk_size, remaining);
                    let end = offset + send_now;
                    let slice = &full_payload[offset..end];
                    if let Err(e) = port.write_all(slice) {
                        ts_println!("Write error during chunked send: {:?}", e);
                        let _ = port.write_all(b"SPEEDCANCEL\n");
                        break;
                    }
                    // avoid forcing flush or sleeping per-chunk which adds latency
                    remaining -= send_now;
                    offset = end;
                    bytes_sent += send_now;
                }
                let chunk_write_dur = chunk_write_start.elapsed();
                // send trailer
                let _ = port.write_all(&SPEED_BB_BYTES);
                ts_println!("Chunked write completed in {} ms", chunk_write_dur.as_millis());
            }
        }
        let _ = port.set_timeout(Duration::from_secs(2));

        // wait for device SPEEDRESULT
        let mut resp_buf = Vec::new();
        let mut read_buf = [0u8; 256];
        let mut got_result = false;
        let wait_start = Instant::now();
        while wait_start.elapsed() < Duration::from_secs(30) {
            match port.read(&mut read_buf) {
                Ok(n) if n > 0 => {
                    resp_buf.extend_from_slice(&read_buf[..n]);
                    while let Some(pos) = resp_buf.iter().position(|&b| b == b'\n') {
                        let line = String::from_utf8_lossy(&resp_buf[..pos]).to_string();
                        resp_buf.drain(..=pos);
                        let ltrim = line.trim();
                        if let Some(idx) = ltrim.find("SPEEDRESULT;") {
                            let payload = &ltrim[idx..];
                            let parts: Vec<&str> = payload.splitn(3, ';').collect();
                            if parts.len() >= 3 {
                                if let (Ok(bytes_rx), Ok(ms)) = (parts[1].parse::<usize>(), parts[2].parse::<u128>()) {
                                    let local_secs = local_start.elapsed().as_secs_f64();
                                    let kb = (bytes_sent as f64) / 1024.0;
                                    let kb_s_local = if local_secs > 0.0 { kb / local_secs } else { 0.0 };
                                    ts_println!("Run {} result (device): {} bytes in {} ms", run+1, bytes_rx, ms);
                                    ts_println!("Run {} local measured: sent {} bytes in {:.3} s -> {:.2} KB/s", run+1, bytes_sent, local_secs, kb_s_local);
                                    got_result = true;
                                    break;
                                }
                            }
                        } else {
                            ts_eprintln!("Ignored device log during result wait: {}", ltrim);
                        }
                    }
                }
                Ok(_) => {}
                Err(ref e) if e.kind() == std::io::ErrorKind::TimedOut => {}
                Err(e) => { println!("Read error waiting for speed result: {:?}", e); break; }
            }
            if got_result { break; }
        }
        if !got_result {
            let local_secs = local_start.elapsed().as_secs_f64();
            let kb = (bytes_sent as f64) / 1024.0;
            let kb_s_local = if local_secs > 0.0 { kb / local_secs } else { 0.0 };
            ts_println!("Speed test {}/{}: no SPEEDRESULT within timeout", run+1, runs);
            ts_println!("Local measured: sent {} bytes in {:.3} s -> {:.2} KB/s", bytes_sent, local_secs, kb_s_local);
        }
        // small delay between runs
        std::thread::sleep(std::time::Duration::from_millis(200));
    }

    // For single-run mode we already printed local or device-reported speed per run.
    Ok(())
}

fn send_image_file(path: &str, port: &mut dyn serialport::SerialPort, width: u16, height: u16) -> Result<()> {
    // open and decode image file
    let img = image::open(path)?.to_rgb8();
    // resize if needed to target size
    let img = if img.width() as u16 != width || img.height() as u16 != height {
        image::imageops::resize(&img, width as u32, height as u32, FilterType::Nearest)
    } else {
        img
    };

    // convert to RgbImage for the encoder helper
    let rgb_img = RgbImage::from_raw(img.width(), img.height(), img.into_raw()).ok_or_else(|| anyhow::anyhow!("failed to create rgb image"))?;

    let rgb565 = rgb888_to_rgb565_be(&rgb_img);
    let compressed = lz4_flex::compress_prepend_size(&rgb565);

    // Build header
    let mut header = Vec::with_capacity(16);
    header.extend_from_slice(&IMAGE_AA.to_be_bytes());
    header.extend_from_slice(&width.to_be_bytes());
    header.extend_from_slice(&height.to_be_bytes());
    header.extend_from_slice(&0u16.to_be_bytes());
    header.extend_from_slice(&0u16.to_be_bytes());

    // send header + compressed + trailer in a single write to avoid fragmentation
    let mut frame = Vec::with_capacity(header.len() + compressed.len() + 8);
    frame.extend_from_slice(&header);
    frame.extend_from_slice(&compressed);
    frame.extend_from_slice(&IMAGE_BB.to_be_bytes());
    port.write_all(&frame)?;
    port.flush()?;
    // After sending, wait up to 8s for device response (DRAW_OK, FRAME_PARSED or ERROR:). Ignore unrelated log lines.
    let start = Instant::now();
    let mut resp_buf = Vec::new();
    let mut read_buf = [0u8; 256];
    while start.elapsed() < Duration::from_secs(8) {
        match port.read(&mut read_buf) {
            Ok(n) if n > 0 => {
                resp_buf.extend_from_slice(&read_buf[..n]);
                while let Some(pos) = resp_buf.iter().position(|&b| b == b'\n') {
                    let line = String::from_utf8_lossy(&resp_buf[..pos]).to_string();
                    // remove up to and including this newline
                    resp_buf.drain(..=pos);
                    let ltrim = line.trim();
                    if ltrim.starts_with("DRAW_OK") {
                        ts_println!("Device reply: {}", ltrim);
                        return Ok(());
                    } else if ltrim.starts_with("FRAME_PARSED") {
                        ts_println!("Device reply: {}", ltrim);
                        return Ok(());
                    } else if ltrim.starts_with("ERROR:") {
                        ts_println!("Device reply: {}", ltrim);
                        return Ok(());
                    } else {
                        // unrelated log line, print for debug and continue waiting
                        ts_eprintln!("Ignored device log: {}", ltrim);
                    }
                }
            }
            Ok(_) => {}
            Err(ref e) if e.kind() == std::io::ErrorKind::TimedOut => {}
            Err(e) => { println!("Read error waiting for reply: {:?}", e); break; }
        }
    }
    ts_println!("No DRAW_OK/FRAME_PARSED/ERROR reply within timeout");
    Ok(())
}

fn main() -> Result<()> {
    // init logger for debug output
    let _ = env_logger::builder().is_test(false).try_init();

    ts_println!("Scanning candidate COM ports and probing with short timeouts...");
    let ports = find_candidate_ports();
    if ports.is_empty() {
        ts_println!("No candidate COM ports found. Listing all ports:");
        for p in serialport::available_ports()? {
            ts_println!("- {} : {:?}", p.port_name, p.port_type);
        }
        anyhow::bail!("no ports found");
    }

    // Try to find a port and parse its reported screen size from ReadInfo response
    let mut found: Option<(String, u16, u16)> = None;
    for p in ports.iter() {
        ts_println!("Probing {} ({:?})...", p.port_name, p.port_type);
        if let Ok(Some(line)) = probe_port_for_line(&p.port_name, READ_INF_MAGIC, 800) {
            ts_println!("{} -> {}", p.port_name, line);
            // Some devices may prefix the ReadInfo line with logging tags, e.g.
            // "I (852279) esp32_wifi_screen: ESP32-WIFI-SCREEN;240;240;PROTO:USB-SCREEN"
            // Locate the substring "ESP32-WIFI-SCREEN" case-insensitively and parse from there.
            if let Some(pos) = line.to_uppercase().find("ESP32-WIFI-SCREEN") {
                let payload = &line[pos..];
                    if payload.contains("PROTO:USB-SCREEN") {
                    // Expected format: ESP32-WIFI-SCREEN;{width};{height};PROTO:USB-SCREEN
                    let parts: Vec<&str> = payload.split(';').collect();
                    if parts.len() >= 4 {
                        let w = parts.get(1).and_then(|s| s.parse::<u16>().ok());
                        let h = parts.get(2).and_then(|s| s.parse::<u16>().ok());
                            if let (Some(w), Some(h)) = (w, h) {
                                if w > 0 && h > 0 {
                                    found = Some((p.port_name.clone(), w, h));
                                    break;
                                } else {
                                        ts_println!("{} reported ESP32-WIFI-SCREEN but width/height are zero: {}. Falling back to default 240x240", p.port_name, payload);
                                    found = Some((p.port_name.clone(), 240u16, 240u16));
                                    break;
                                }
                            } else {
                                ts_println!("{} reported ESP32-WIFI-SCREEN but width/height parse failed: {}", p.port_name, payload);
                                found = Some((p.port_name.clone(), 240u16, 240u16));
                                break;
                            }
                    } else {
                        ts_println!("{} reported unexpected ReadInfo format: {}", p.port_name, payload);
                        found = Some((p.port_name.clone(), 240u16, 240u16));
                        break;
                    }
                }
            }
        }
    }

    let (port_name, width, height) = match found {
        Some((p, w, h)) => (p, w, h),
        None => {
            ts_println!("No ESP32 USB-screen device responded to probes. Available ports:");
            for p in serialport::available_ports()? {
                ts_println!("- {} : {:?}", p.port_name, p.port_type);
            }
            anyhow::bail!("no device found");
        }
    };

    ts_println!("Using port: {} ({}x{})", port_name, width, height);
    // Open the control port at a high baud to avoid throttling by some USB-serial drivers.
    // Probing earlier used 115200; here we open at 2_000_000 for bulk transfers where supported.
    let mut port = serialport::new(&port_name, 2_000_000)
        .timeout(Duration::from_secs(2))
        .open()?;

    // Display final info if available
    if let Ok(Some(line)) = probe_port_for_line(&port_name, READ_INF_MAGIC, 800) {
        ts_println!("ReadInfo => {}", line);
    }

    ts_println!("Running serial speed test (3 x 4KB)...");
    send_speed_tests(&mut *port, 3, 4 * 1024)?;

    ts_println!("Sending GIF frames from tothesky.gif ({}x{})...", width, height);
    // send frames from GIF in the current working directory
    send_gif("tothesky.gif", &mut *port, width, height, 40)?;
    ts_println!("Sent GIF frames");

    Ok(())
}
