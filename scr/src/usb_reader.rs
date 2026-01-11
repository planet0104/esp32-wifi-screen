use std::io::{self, Read};
use std::sync::mpsc::Sender;
use std::thread;
use std::time::Duration;

use crate::with_context;
use crate::display;
// synchronous drawing: no global cache

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
    let spawn_res = thread::Builder::new()
        .name("usb_stdin_reader".to_string())
        .stack_size(64 * 1024)
        .spawn(move || {
            // use the cloned sender inside the thread
            let sender = sender_for_thread;
            let stdin = io::stdin();
            let mut handle = stdin.lock();
            let mut buf = Vec::<u8>::new();
            let mut read_buf = [0u8; 1024];

            // rp2040 protocol markers
            const IMAGE_AA: u64 = 7596835243154170209;
            const IMAGE_BB: u64 = 7596835243154170466;
            const BOOT_USB: u64 = 7093010483740242786;
            const READ_INF: u64 = 0x52656164496e666f; // "ReadInfo" big-endian
            // binary-speedtest markers: client will send SPEED_AA, raw payload, SPEED_BB
            const SPEED_AA_BYTES: [u8; 8] = *b"SPDTEST1";
            const SPEED_BB_BYTES: [u8; 8] = *b"SPDEND!!";

            let aa_bytes = IMAGE_AA.to_be_bytes();
            let bb_bytes = IMAGE_BB.to_be_bytes();
            let boot_bytes = BOOT_USB.to_be_bytes();
            let readinf_bytes = READ_INF.to_be_bytes();
            let readinf_ascii = b"ReadInfo";
            // convenience aliases for binary-speedtest markers
            let speed_aa = SPEED_AA_BYTES;
            let speed_bb = SPEED_BB_BYTES;

            let mut receiving = false;
            let mut image_buf: Vec<u8> = Vec::new();
            let mut last_reported_progress: usize = 0;
            // binary speedtest (SPEED_AA_BYTES ... data ... SPEED_BB_BYTES)
            let mut speedbin_active: bool = false;
            let mut speedbin_received: usize = 0;
            let mut speedbin_start: Option<std::time::Instant> = None;
            let mut image_width: u16 = 0;
            let mut image_height: u16 = 0;
            let mut image_x: u16 = 0;
            let mut image_y: u16 = 0;

            // helper to send info/error messages to main thread via channel
            let send_info = |sender: &Option<Sender<String>>, msg: String| {
                if let Some(s) = sender {
                    let _ = s.send(msg);
                }
            };
            let send_error = |sender: &Option<Sender<String>>, msg: String| {
                if let Some(s) = sender {
                    let _ = s.send(format!("ERROR:{}\n", msg));
                }
            };

            loop {
                match handle.read(&mut read_buf) {
                    Ok(0) => {
                        break;
                    }
                    Ok(n) => {
                        // received bytes
                        buf.extend_from_slice(&read_buf[..n]);

                        loop {
                            // If binary speedtest is active, consume raw bytes until trailer reached
                            if speedbin_active {
                                if buf.len() > 0 {
                                    if let Some(pos) = find_subslice(&buf, &speed_bb) {
                                        // found trailer: count bytes before trailer
                                        speedbin_received = speedbin_received.saturating_add(pos);
                                        // drain including trailer
                                        let _ = buf.drain(..pos + speed_bb.len());
                                        // compute elapsed and report
                                        if let Some(start) = speedbin_start.take() {
                                            let ms = start.elapsed().as_millis();
                                            let _ = send_info(&sender, format!("SPEEDRESULT;{};{}\n", speedbin_received, ms));
                                            // short re-send for reliability
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
                                        // no trailer yet: to avoid losing a trailer split across read
                                        // boundaries, keep the last (speed_bb.len() - 1) bytes in `buf`.
                                        let keep = if speed_bb.len() > 0 { speed_bb.len() - 1 } else { 0 };
                                        if buf.len() > keep {
                                            let take = buf.len() - keep;
                                            buf.drain(..take);
                                            speedbin_received = speedbin_received.saturating_add(take);
                                        }
                                    }
                                }
                                // if still active, continue reading more data from stdin
                                if speedbin_active {
                                    break;
                                }
                            }

                            /* ASCII SPEEDTEST support removed - use binary SPEED_AA...SPEED_BB flow */

                            if receiving {
                                // append new bytes to image buffer, then search for IMAGE_BB across boundaries
                                image_buf.extend_from_slice(&buf);
                                buf.clear();
                                if let Some(pos) = find_subslice(&image_buf, &bb_bytes) {
                                    // found trailer inside image_buf; split compressed payload and keep remainder
                                    let compressed_len = pos;
                                    let compressed_data = image_buf[..compressed_len].to_vec();
                                    // preserve any bytes after the trailer for next processing
                                    let remainder_start = pos + bb_bytes.len();
                                    let remainder = image_buf[remainder_start..].to_vec();
                                    image_buf.clear();
                                    // replace buf with remainder so outer loop can process following data
                                    buf.extend_from_slice(&remainder);
                                    let _ = send_info(&sender, format!("FRAME_END;compressed_len={}\n", compressed_len));
                                    // Attempt decompress
                                    match lz4_flex::decompress_size_prepended(&compressed_data) {
                                        Ok(decompressed) => {
                                            let _ = send_info(&sender, format!("DECOMPRESSED;len={}\n", decompressed.len()));
                                            // Validate size: expect width*height*2 bytes for RGB565
                                            let expected = image_width as usize * image_height as usize * 2;
                                            if decompressed.len() != expected {
                                                let _ = send_error(&sender, format!("decompressed size {} != expected {}", decompressed.len(), expected));
                                            } else {
                                                // Temporarily skip actual drawing; report parsed frame info back to main thread
                                                // so the host test client can observe parsing and decompression result.
                                                // Perform synchronous drawing and report result.
                                                let draw_result = std::panic::catch_unwind(|| {
                                                    let _ = send_info(&sender, "DRAWING_START\n".to_string());
                                                    with_context(|ctx| {
                                                        if let Some(display_manager) = ctx.display.as_mut() {
                                                            display::draw_rgb565_u8array_fast(
                                                                display_manager,
                                                                image_x,
                                                                image_y,
                                                                image_width,
                                                                image_height,
                                                                &decompressed,
                                                            )
                                                        } else {
                                                            Ok(())
                                                        }
                                                    })
                                                });

                                                match draw_result {
                                                    Ok(Ok(_)) => {
                                                        let _ = send_info(&sender, "DRAW_OK\n".to_string());
                                                    }
                                                    Ok(Err(e)) => {
                                                        let _ = send_error(&sender, format!("draw failed: {:?}", e));
                                                    }
                                                    Err(_panic_info) => {
                                                        let _ = send_error(&sender, "draw panicked".to_string());
                                                    }
                                                }
                                            }
                                        }
                                        Err(e) => { let _ = send_error(&sender, format!("LZ4 decompress failed: {:?}", e)); }
                                    }
                                    image_buf.clear();
                                    receiving = false;
                                    continue;
                                }
                                // no end marker yet, the new bytes were already appended to image_buf above.
                                // Previously we sent periodic RECV_PROGRESS to aid debugging; remove to reduce log noise.
                                buf.clear();
                                break;
                            } else {
                                // not receiving: first check for binary-speedtest start marker
                                if !speedbin_active {
                                    if let Some(pos) = find_subslice(&buf, &speed_aa) {
                                        // If we found the SPEED_AA marker, start binary speedtest mode
                                        // Remove marker from buffer and mark active
                                        buf.drain(..pos + speed_aa.len());
                                        speedbin_active = true;
                                        speedbin_received = 0;
                                        speedbin_start = Some(std::time::Instant::now());
                                        continue;
                                    }
                                }

                                // not receiving: look for IMAGE_AA
                                if let Some(pos) = find_subslice(&buf, &aa_bytes) {
                                    if pos > 0 {
                                        // ignore prefix
                                    }
                                    if buf.len() < pos + 16 {
                                        break;
                                    }
                                    let start = pos;
                                    image_width = u16::from_be_bytes([buf[start + 8], buf[start + 9]]);
                                    image_height = u16::from_be_bytes([buf[start + 10], buf[start + 11]]);
                                    image_x = u16::from_be_bytes([buf[start + 12], buf[start + 13]]);
                                    image_y = u16::from_be_bytes([buf[start + 14], buf[start + 15]]);
                                    buf.drain(..start + 16);
                                    let _ = send_info(&sender, format!("FRAME_START;{};{};{};{}\n", image_width, image_height, image_x, image_y));
                                    receiving = true;
                                    image_buf.clear();
                                    last_reported_progress = 0;
                                    continue;
                                }

                                // Before treating newline text, accept ReadInfo (binary/ASCII) and BOOT

                                let pos_bin = find_subslice(&buf, &readinf_bytes);
                                let pos_ascii = find_subslice(&buf, readinf_ascii);
                                if pos_bin.is_some() || pos_ascii.is_some() {
                                    let pos = match (pos_bin, pos_ascii) {
                                        (Some(p), Some(q)) => if p <= q { p } else { q },
                                        (Some(p), None) => p,
                                        (None, Some(q)) => q,
                                        _ => unreachable!(),
                                    };
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

                                // ignore plain newline-terminated text (speedtest now uses binary markers)
                                if let Some(nlpos) = buf.iter().position(|&b| b == b'\n') {
                                    buf.drain(..=nlpos);
                                    continue;
                                }

                                break;
                            }
                        }
                    }
                    Err(e) => {
                        if e.kind() == io::ErrorKind::WouldBlock {
                            thread::sleep(Duration::from_millis(20));
                            continue;
                        }
                        let _ = send_error(&sender, format!("Error reading stdin: {:?}", e));
                        break;
                    }
                }
            }

            let _ = send_info(&sender, "USB stdin reader thread exiting\n".to_string());
        });

    if let Err(e) = spawn_res {
        if let Some(s) = sender {
            let _ = s.send(format!("ERROR:Failed to spawn USB stdin reader thread: {:?}\n", e));
        }
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
