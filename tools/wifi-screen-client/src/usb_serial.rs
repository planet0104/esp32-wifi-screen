use std::io::{Read, Write};
use std::time::{Duration, Instant};

use anyhow::{anyhow, Result};
use serialport::{SerialPort, SerialPortInfo, SerialPortType};

// Must match device (`scr/src/usb_reader.rs`)
const READ_INFO_MAGIC: u64 = 0x52656164496e666f; // "ReadInfo"
const IMAGE_AA: u64 = 0x696d6167655f6161; // "image_aa"
const IMAGE_BB: u64 = 0x696d6167655f6262; // "image_bb"

pub const DEFAULT_BAUD: u32 = 2_000_000;

fn try_read_line(port: &mut dyn SerialPort, total_wait: Duration) -> Option<String> {
    let start = Instant::now();
    let mut buf: Vec<u8> = Vec::new();
    let mut tmp = [0u8; 256];
    while start.elapsed() < total_wait {
        match port.read(&mut tmp) {
            Ok(n) if n > 0 => {
                buf.extend_from_slice(&tmp[..n]);
                if let Some(pos) = buf.iter().position(|&b| b == b'\n') {
                    return Some(String::from_utf8_lossy(&buf[..pos]).to_string());
                }
            }
            Ok(_) => {}
            Err(ref e) if e.kind() == std::io::ErrorKind::TimedOut => {}
            Err(_) => break,
        }
    }
    None
}

fn probe_port_for_readinfo_line(port_name: &str, timeout_ms: u64) -> Result<Option<String>> {
    let timeout = Duration::from_millis(timeout_ms);
    match serialport::new(port_name, 115_200)
        .timeout(Duration::from_millis(200))
        .open()
    {
        Ok(mut port) => {
            // drain any pending data
            let _ = port.read(&mut [0u8; 1024]);

            // 1) binary magic
            let _ = port.write_all(&READ_INFO_MAGIC.to_be_bytes());
            let _ = port.flush();
            if let Some(line) = try_read_line(&mut *port, Duration::from_millis(200)) {
                return Ok(Some(line));
            }

            // 2) ascii probe
            let _ = port.write_all(b"ReadInfo\n");
            let _ = port.flush();
            if let Some(line) = try_read_line(&mut *port, timeout) {
                return Ok(Some(line));
            }

            Ok(None)
        }
        Err(_) => Ok(None),
    }
}

/// Return list of candidate usb-screen serial ports as display strings.
/// Each entry is the actual port name (e.g. "COM6"), optionally decorated with parsed screen size.
pub fn find_usb_screen_serial_devices() -> Result<Vec<String>> {
    let ports: Vec<SerialPortInfo> = serialport::available_ports().unwrap_or_default();
    let mut out: Vec<String> = Vec::new();

    // Prefer devices that expose USB serial number starts with "USBSCR"
    for p in &ports {
        if let SerialPortType::UsbPort(usb) = &p.port_type {
            if usb.serial_number.clone().unwrap_or_default().starts_with("USBSCR") {
                out.push(p.port_name.clone());
            }
        }
    }
    if !out.is_empty() {
        out.sort();
        out.dedup();
        return Ok(out);
    }

    // Fallback: probe all USB serial ports by sending ReadInfo and parsing response
    for p in &ports {
        if !matches!(p.port_type, SerialPortType::UsbPort(_)) {
            continue;
        }
        if let Ok(Some(line)) = probe_port_for_readinfo_line(&p.port_name, 800) {
            // Some firmware prefixes logs; locate substring case-insensitively
            let up = line.to_uppercase();
            if let Some(pos) = up.find("ESP32-WIFI-SCREEN") {
                let payload = &line[pos..];
                if payload.contains("PROTO:USB-SCREEN") {
                    // try parse width/height; decorate but keep port name first for UI clarity
                    let parts: Vec<&str> = payload.split(';').collect();
                    if parts.len() >= 3 {
                        if let (Ok(w), Ok(h)) = (parts[1].parse::<u16>(), parts[2].parse::<u16>()) {
                            if w > 0 && h > 0 {
                                out.push(format!("{} ({}x{})", p.port_name, w, h));
                                continue;
                            }
                        }
                    }
                    out.push(p.port_name.clone());
                }
            }
        }
    }

    out.sort();
    out.dedup();
    Ok(out)
}

/// Parse the chosen UI string (e.g. "COM6 (240x240)") into "COM6".
pub fn extract_port_name(ui_value: &str) -> String {
    ui_value.split_whitespace().next().unwrap_or("").to_string()
}

/// Query screen size by sending ReadInfo probe over serial (115200).
pub fn query_screen_size(port_name: &str) -> Result<(u16, u16)> {
    let line = probe_port_for_readinfo_line(port_name, 800)?
        .ok_or_else(|| anyhow!("设备无 ReadInfo 响应（{}）", port_name))?;

    let up = line.to_uppercase();
    let pos = up
        .find("ESP32-WIFI-SCREEN")
        .ok_or_else(|| anyhow!("ReadInfo 返回格式异常：{}", line))?;
    let payload = &line[pos..];
    let parts: Vec<&str> = payload.split(';').collect();
    if parts.len() < 3 {
        return Err(anyhow!("ReadInfo 返回格式异常：{}", payload));
    }
    let w = parts[1].parse::<u16>().map_err(|_| anyhow!("解析宽度失败：{}", payload))?;
    let h = parts[2].parse::<u16>().map_err(|_| anyhow!("解析高度失败：{}", payload))?;
    if w == 0 || h == 0 {
        return Err(anyhow!("设备返回的分辨率无效：{}x{}", w, h));
    }
    Ok((w, h))
}

pub fn open_screen_serial(port_name: &str) -> Result<Box<dyn SerialPort>> {
    let mut port = serialport::new(port_name, DEFAULT_BAUD)
        .timeout(Duration::from_millis(100))
        .open()?;
    // best-effort drain
    let _ = port.read(&mut [0u8; 1024]);
    Ok(port)
}

/// Send one LZ4-prepended RGB565 frame to device over serial.
/// `lz4_payload` must be the output of `lz4_flex::compress_prepend_size(rgb565_bytes)`.
pub fn send_lz4_rgb565_frame(
    port: &mut dyn SerialPort,
    x: u16,
    y: u16,
    width: u16,
    height: u16,
    lz4_payload: &[u8],
) -> Result<()> {
    let mut header = [0u8; 16];
    header[0..8].copy_from_slice(&IMAGE_AA.to_be_bytes());
    header[8..10].copy_from_slice(&width.to_be_bytes());
    header[10..12].copy_from_slice(&height.to_be_bytes());
    header[12..14].copy_from_slice(&x.to_be_bytes());
    header[14..16].copy_from_slice(&y.to_be_bytes());

    port.write_all(&header)?;
    port.write_all(lz4_payload)?;
    port.write_all(&IMAGE_BB.to_be_bytes())?;
    port.flush()?;
    Ok(())
}

