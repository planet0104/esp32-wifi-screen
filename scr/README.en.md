# ESP32 WiFi Screen

An **ESP32-S2 / ESP32-S3** small-screen project. It supports high-speed image transfer and rendering to a TFT screen via **WiFi** (HTTP/WebSocket/MQTT) or **USB serial**.

[中文](README.md)

## Features

- **Multiple display support**: ST7735, ST7789, ST7796 series TFT displays
- **Multiple transport options**: HTTP API, WebSocket, MQTT, USB serial (ESP32-S2 USB CDC / ESP32-S3 USB Serial JTAG)
- **Web configuration UI**: Configure WiFi and display parameters in a browser
- **Image transfer**: Supports JPEG, RGB565, and LZ4 compressed formats
- **Canvas drawing**: Draw text, shapes, images, and other elements
- **Color tuning**: Adjust RGB channels in real time to correct color cast issues

## Performance Tests

### WiFi throughput (ESP32-S2 @ 240MHz + 2MB PSRAM)

| Protocol | Min | Max | Typical | Notes |
|------|----------|----------|----------|------|
| HTTP Echo | 120 KB/s | 694 KB/s | ~280 KB/s | Single request/response |
| WebSocket Echo | 128 KB/s | 751 KB/s | ~350 KB/s | Persistent connection, more stable |

> **Test conditions**: 100KB round-trip Echo test, 2.4GHz WiFi. Actual throughput depends on signal strength and environmental interference.

### USB serial throughput (ESP32-S2/S3)

| Item | Host-side throughput | Device-side receive | Notes |
|------|------------|------------|------|
| USB serial one-way downlink (SpeedTest) | ~462.6 KB/s | ~491.0 KB/s | chunk=4096, device prints `SPEEDRESULT;5050368;10045` |

> **Note**: The USB serial speed test scripts are in `examples/tools/`. The device outputs `SPEEDRESULT;{bytes};{ms}` as the summary.

### Notes on performance

- **WiFi peak throughput**: about 750 KB/s (6 Mbps), close to the ESP32-S2 WiFi theoretical limit
- **WiFi variability**: 2.4GHz interference may cause fluctuations
- **WebSocket advantage**: Persistent connections avoid HTTP connection overhead, giving higher average throughput
- **USB serial typical throughput**: about 450–500 KB/s (depends on host driver, serial stack buffers, chunk size, etc.)

## Hardware Requirements

- ESP32-S2 or ESP32-S3 development board (with PSRAM, recommended: 4MB Flash + 2MB PSRAM)
- Supported TFT display (see wiring below)
- USB data cable (for flashing/logging/USB serial image transfer)

## Wiring

### Common pin mapping

| Display pin | ESP32 pin | Notes |
|-----------|--------------|------|
| GND | GND | Ground |
| VCC | 3V3 | 3.3V power |
| SCL/CLK | GPIO6 | SPI clock |
| SDA/MOSI | GPIO7 | SPI data |
| RST/RES | GPIO8 | Reset |
| DC/AO | GPIO5 | Data/command select |
| CS | GPIO4 | Chip select (some displays require it) |
| BL/BLK | Floating or VBUS | Backlight (can be connected to VBUS 5V) |

### Wiring references by display

#### ST7735S 80x160 (with CS)

![ST7735S 80x160](images/ST7735S_80x160_CS.png)

```
GND -> GND
VCC -> 3V3
SCL -> GPIO6
SDA -> GPIO7
RST -> GPIO8
DC  -> GPIO5
CS  -> GPIO4
BLK -> (Floating or VBUS)
```

#### ST7735S 128x160 (with CS)

![ST7735S 128x160](images/ST7735S_128x160_CS.png)

```
GND -> GND
VCC -> 3V3
SCL -> GPIO6
SDA -> GPIO7
RST -> GPIO8
DC  -> GPIO5
CS  -> GPIO4
BL  -> (Floating or VBUS)
```

#### ST7789 240x240 (no CS)

![ST7789 240x240](images/ST7789_240x240.png)

```
GND -> GND
VCC -> 3V3
SCL -> GPIO6
SDA -> GPIO7
RES -> GPIO8
DC  -> GPIO5
BLK -> (Floating or VBUS)
```

#### ST7789 240x320 (with CS)

![ST7789 240x320](images/ST7789_240x320_CS.png)

```
GND -> GND
VCC -> 3V3
SCL -> GPIO6
SDA -> GPIO7
RST -> GPIO8
AO  -> GPIO5
CS  -> GPIO4
BL  -> VBUS
```

#### ST7789V 135x240 (with CS)

![ST7789V 135x240](images/ST7789V_135x240_CS.png)

```
GND -> GND
VCC -> 3V3
SCL -> GPIO6
SDA -> GPIO7
RES -> GPIO8
DC  -> GPIO5
CS  -> GPIO4
BLK -> (Floating or VBUS)
```

#### ST7796 320x480 (with CS)

![ST7796 320x480](images/ST7796.jpg)

```
GND -> GND
VCC -> 3V3
SCL -> GPIO6
SDA -> GPIO7
RST -> GPIO8
DC  -> GPIO5
CS  -> GPIO4
BL  -> VBUS
```

> **Note**: Wiring images are stored in the `images/` folder.

## Installation

### Option 1: Install with espup (recommended)

1. **Install espup**

```powershell
cargo +stable install espup
```

2. **Install the ldproxy linker**

```powershell
# Temporarily remove the project's toolchain override
rustup override unset

# Install ldproxy
cargo install ldproxy

# Restore the project's esp toolchain
rustup override set esp
```

3. **Install the ESP Rust toolchain**

```powershell
espup install
```

> Note: If the install appears stuck for more than 5 minutes, press Ctrl+C to interrupt. The toolchain may already have been installed successfully.

4. **Configure the project**

a) **Set an ultra-short build output path** (**required**, to avoid ESP-IDF path-too-long errors)

Edit `scr/.cargo/config.toml` (in the `scr` sub-project directory it is `.cargo/config.toml`):

```toml
[build]
# ESP32-S2: xtensa-esp32s2-espidf
# ESP32-S3: xtensa-esp32s3-espidf
target = "xtensa-esp32s2-espidf"
target-dir = "C:/esp/target"  # Use a very short path. ESP-IDF requires total path length under a certain limit.
```

Create the directory:

```powershell
New-Item -ItemType Directory -Path "C:/esp/target" -Force
```

b) **Chip selection and environment variables (script recommended)**

- **Recommended**: Use the build scripts provided by this repository. They automatically set environment variables like `MCU` / `ESP_IDF_VERSION` / `ESP_IDF_SDKCONFIG_DEFAULTS`, and select the correct target/features:
  - ESP32-S2: `build_esp32s2.ps1` / `flash_esp32s2.ps1`
  - ESP32-S3: `build_esp32s3.ps1` / `flash_esp32s3.ps1`
- **Note**: This project pins ESP-IDF to `v5.3.4`. `rustflags` are already configured in `scr/.cargo/config.toml`, so manual changes are usually unnecessary.

5. **Install espflash (for flashing firmware)**

```powershell
cargo install espflash
```

6. **Set environment variables before each build**

```powershell
. .\\espup_env.ps1
```

7. **Build and flash**

```powershell
# Option 1 (recommended): one-click build/flash scripts per chip
.\build_esp32s2.ps1
.\flash_esp32s2.ps1

# ESP32-S3:
# .\build_esp32s3.ps1
# .\flash_esp32s3.ps1

# Option 2: run commands step by step
# Build
cargo build --release

# Generate firmware image
espflash save-image --chip esp32s2 --partition-table partitions.csv target/xtensa-esp32s2-espidf/release/esp32-wifi-screen esp32-wifi-screen.bin

# Flash firmware to the device
espflash flash --monitor target/xtensa-esp32s2-espidf/release/esp32-wifi-screen

# ESP32-S3: switch --chip / target path to esp32s3 / xtensa-esp32s3-espidf, or use scripts (recommended)
```

### Option 2: Use the official ESP-IDF installer

1. **Download and install ESP-IDF 5.3.4**
   - Official site: `https://dl.espressif.com.cn/dl/esp-idf/index.html`
   - You can choose to install Rust-related components

2. **Use the ESP-IDF console**
   - Open the ESP-IDF 5.3.4 CMD console
   - Enter the project directory and build

## Common Commands

```powershell
# List available serial ports
espflash board-info

# Flash and monitor serial output
espflash flash --monitor target/xtensa-esp32s2-espidf/release/esp32-wifi-screen

# Flash a specific COM port
espflash flash --port COM3 target/xtensa-esp32s2-espidf/release/esp32-wifi-screen

# Erase flash
espflash erase-flash
```

## Build Script Notes

All build scripts support reading `target-dir` from `.cargo/config.toml` automatically.

| Script | Purpose |
|------|------|
| `build_esp32s2.ps1` | Build ESP32-S2 firmware and generate a merged image (`*-merged.bin`) |
| `flash_esp32s2.ps1` | Flash the ESP32-S2 merged image (optional serial monitor) |
| `build_esp32s3.ps1` | Build ESP32-S3 firmware and generate a merged image (`*-merged.bin`) |
| `flash_esp32s3.ps1` | Flash the ESP32-S3 merged image (optional serial monitor) |
| `monitor.ps1` | Monitor serial output (usage: `.\\monitor.ps1 -p COM3`) |

**Recommended workflow**:

```powershell
# ESP32-S2: build and generate image
.\build_esp32s2.ps1

# ESP32-S2: flash (one step)
.\flash_esp32s2.ps1
```

## Notes

- **Pure Rust implementation**: This project already uses the pure-Rust `tjpgd` crate, so no C library linking is required
- **Path length limitation**: ESP-IDF requires a very short build output path. You **must** use something like `target-dir = "C:/esp/target"` (you can make it even shorter, e.g. `C:/t`)
- **Network proxy**: The first build requires a global proxy to access GitHub
- **First build time**: The first build may take 10–30 minutes

## Usage

### WiFi mode (original)

1. After flashing, the device creates a WiFi AP named `ESP32-Screen-XXXXXX`
2. Connect to the AP and open `http://192.168.72.1` to access the configuration page
3. Configure WiFi and display parameters and save
4. After reboot, the device connects to the configured WiFi and becomes accessible on the LAN

### USB serial image transfer (new)

**Supported chips**:
- **ESP32-S2**: TinyUSB CDC (prints `READY:USB-CDC` after boot)
- **ESP32-S3**: USB Serial JTAG (prints `READY:HIGH_SPEED_RX` after boot)

**Host examples**:
- Example projects are in `examples/` (default feature is `usb-serial`)
- `examples/src/main.rs` automatically finds the usb-screen device (`find_usb_serial_device()`), then sends images to the screen via serial

**Serial protocol (firmware side: `scr/src/usb_reader.rs`)**:

- **Device info query (ReadInfo)**:
  - Host sends: `ReadInfo` (8-byte binary) or ASCII `ReadInfo\n`
  - Device replies: `ESP32-WIFI-SCREEN;{width};{height};PROTO:USB-SCREEN`
- **Image frame transfer (LZ4 + RGB565)**:
  - Header: `image_aa` (8 bytes) + `width,height,x,y` (four `u16`, Big-Endian, total 8 bytes)
  - Payload: output of `lz4_flex::compress_prepend_size(rgb565_bytes)`
  - Tail: `image_bb` (8 bytes)
  - Notes: Firmware LZ4-decompresses and draws in `RGB565`. For higher throughput, the firmware disables debug ACK echoes related to drawing by default.
- **Speed test (SpeedTest)**:
  - Host sends: `SPDTEST1` (8 bytes) + arbitrary data + `SPDEND!!` (8 bytes)
  - Device replies: `SPEEDRESULT;{bytes};{ms}` (repeated for reliability)
- **Boot handshake (optional)**:
  - The host can wait for `READY:*` output before sending data

### Web configuration UI features

**Display settings**:
- Display model selection (ST7735s, ST7789, ST7796)
- Resolution config (width, height, X/Y offsets)
- Display options (CS pin, color inversion, mirroring, coordinate mode)
- Rotation (0°, 90°, 180°, 270°), supports real-time switching
- Color order (RGB/BGR) and SPI mode
- Presets: quick selection for common display models

**Network settings**:
- WiFi scan: scan nearby networks and show signal strength
- WiFi config: SSID, password, static IP
- Live reconnect: test new WiFi config without reboot

**MQTT remote server**:
- MQTT server URL, client ID, username/password
- Subscribe topic and QoS settings
- Live reconnect and config deletion

**Transfer speed test**:
- HTTP and WebSocket speed tests
- Optional data sizes (10KB - 1MB)
- Real-time throughput and round-trip time

**Screen test**:
- Sample page to test display output

### Color tuning

Some TFT displays may have color cast issues (too blue, too yellow, etc.). You can correct this in real time via the "Color Tuning" section in the web configuration UI:

![Color tuning UI](../images/color_adjust.jpg)

**Highlights**:
- **Real-time effect**: Changes apply immediately when dragging sliders, no reboot required
- **Independent RGB adjustment**: Adjust R/G/B channels separately, range: -100 to +100
- **Quick presets**: 4 preset schemes to fix common color casts
  - **Fix too blue**: reduce blue channel (-30)
  - **Fix too yellow**: reduce red and green channels (R:-15, G:-15)
  - **Warm tone**: increase red, reduce blue (R:+20, G:+10, B:-15)
  - **Cool tone**: reduce red, increase blue (R:-15, B:+20)
- **Config persistence**: Values are automatically saved on the device and retained after reboot

**How to use**:
1. Find the "Color Tuning" section on the configuration page
2. Drag the R/G/B sliders or click a preset button
3. Observe the live effect on the screen (300ms debounce delay)
4. Once satisfied, the values will be saved automatically

## License

MIT License

