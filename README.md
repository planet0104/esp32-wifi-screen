# ESP32 WiFi Screen

基于 **ESP32-S2 / ESP32-S3** 的小屏幕项目，支持通过 **WiFi**（HTTP/WebSocket/MQTT）或 **USB 串口** 高速传输图像并显示到 TFT 屏幕。

## 效果展示

![screen0](images/screen0.jpg)

![screen1](images/screen1.jpg)

![screen2](images/screen2.jpg)

![screen3](images/screen3.jpg)

## 目录结构

本仓库已将 ESP32 固件项目移动到仓库根目录（不再有 `scr/` 子项目），常用目录如下：

| 目录/文件 | 说明 |
|---|---|
| `src/` | 固件核心代码（HTTP/WebSocket/MQTT、USB 串口、显示驱动封装、画布/绘图等） |
| `html/` | Web 配置界面静态页面（设备端提供访问） |
| `images/` | 接线图与使用截图（README 引用的图片都在这里） |
| `tools/examples/` | 上位机示例（Nodejs、Python、Rust；以及 USB 串口示例） |
| `tools/speedtest/` | 串口测速与发送图片脚本（Nodejs） |
| `tools/wifi-screen-client/` | 屏幕镜像客户端（截屏推流到 WiFi 屏幕） |
| `mipidsi/` | 显示屏驱动子 crate（上游/定制代码） |
| `build_esp32s2.ps1`/`flash_esp32s2.ps1` | ESP32-S2 构建/烧录脚本 |
| `build_esp32s3.ps1`/`flash_esp32s3.ps1` | ESP32-S3 构建/烧录脚本 |
| `esp32-wifi-screen-esp32s2-merged.bin`/`esp32-wifi-screen-esp32s3-merged.bin` | 预编译完整镜像（merged binary，可直接从 0x0 烧录） |

## 功能特性

- **多种显示屏支持**：ST7735S、ST7789/ST7789V、ST7796 系列 TFT 显示屏
- **多种通信方式**：HTTP API、WebSocket、MQTT、USB 串口（ESP32-S2 USB CDC / ESP32-S3 USB Serial JTAG）
- **Web 配置界面**：通过浏览器配置 WiFi、显示屏参数
- **图像传输**：支持 JPEG、RGB565、LZ4 压缩格式
- **画布绘制**：支持文字、图形、图像等元素绘制
- **色调调整**：实时调整屏幕 RGB 通道，修正色偏问题

## 硬件要求

- ESP32-S2 或 ESP32-S3 开发板（带 PSRAM，建议 4MB Flash + 2MB PSRAM）
- 支持的 TFT 显示屏（见下方接线说明）
- USB 数据线（用于烧写/日志/USB 串口传图）

## 屏幕接线说明

### 通用引脚定义

| 显示屏引脚 | ESP32 引脚 | 说明 |
|-----------|--------------|------|
| GND | GND | 接地 |
| VCC | 3V3 | 电源 3.3V |
| SCL/CLK | GPIO6 | SPI 时钟 |
| SDA/MOSI | GPIO7 | SPI 数据 |
| RST/RES | GPIO8 | 复位 |
| DC/AO | GPIO5 | 数据/命令选择 |
| CS | GPIO4 | 片选（部分屏幕需要） |
| BL/BLK | 悬空或 VBUS | 背光（可接 VBUS 5V） |

### 各屏幕接线参考

#### ST7735S 80x160（带 CS）

![ST7735S 80x160](images/ST7735S_80x160_CS.png)

```
GND -> GND
VCC -> 3V3
SCL -> GPIO6
SDA -> GPIO7
RST -> GPIO8
DC  -> GPIO5
CS  -> GPIO4
BLK -> (悬空或 VBUS)
```

#### ST7735S 128x160（带 CS）

![ST7735S 128x160](images/ST7735S_128x160_CS.png)

```
GND -> GND
VCC -> 3V3
SCL -> GPIO6
SDA -> GPIO7
RST -> GPIO8
DC  -> GPIO5
CS  -> GPIO4
BL  -> (悬空或 VBUS)
```

#### ST7789 240x240（无 CS）

![ST7789 240x240](images/ST7789_240x240.png)

```
GND -> GND
VCC -> 3V3
SCL -> GPIO6
SDA -> GPIO7
RES -> GPIO8
DC  -> GPIO5
BLK -> (悬空或 VBUS)
```

#### ST7789 240x320（带 CS）

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

#### ST7789V 135x240（带 CS）

![ST7789V 135x240](images/ST7789V_135x240_CS.png)

```
GND -> GND
VCC -> 3V3
SCL -> GPIO6
SDA -> GPIO7
RES -> GPIO8
DC  -> GPIO5
CS  -> GPIO4
BLK -> (悬空或 VBUS)
```

#### ST7796 320x480（带 CS）

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

## 烧录固件

### 方式一：使用仓库内置 merged bin + esptool（最省事）

1. 让开发板进入烧录模式：按住 Boot 并上电/复位进入下载模式，系统会出现串口设备

![boot](images/esp32s2boot.jpg)

2. 使用 `esptool.exe` 直接从 `0x0` 写入完整镜像：

#### ESP32-S2

```powershell
.\esptool.exe -p COM6 --before default_reset --after hard_reset --chip esp32s2 write_flash 0x0 esp32-wifi-screen-esp32s2-merged.bin
```

#### ESP32-S3

```powershell
.\esptool.exe -p COM6 --before default_reset --after hard_reset --chip esp32s3 write_flash 0x0 esp32-wifi-screen-esp32s3-merged.bin
```

> 说明：merged binary 已包含 bootloader、partition table 和 app，无需分段烧录。

### 方式二：从源码构建并烧录（适合二次开发）

- 构建脚本：`build_esp32s2.ps1` / `build_esp32s3.ps1`
- 烧录脚本：`flash_esp32s2.ps1` / `flash_esp32s3.ps1`
- 注意：ESP-IDF 构建路径长度有限制，建议在 `.cargo/config.toml` 配置 `target-dir` 为短路径（例如 `C:/esp/target`）

## 配置 WiFi 与屏幕参数（Web 配置界面）

固件烧录后设备会开启 AP 热点，SSID 通常类似 `ESP32-WiFiScreen` 或 `ESP32-Screen-XXXXXX`，以设备实际广播为准。

1. 连接设备热点

![setup0](images/setup0.jpg)

2. 浏览器访问 `http://192.168.72.1` 打开配置界面

![setup1](images/setup1.jpg)

3. 设置屏幕参数：点击左上角“预设”选择屏幕型号，或手动修改

![setup2](images/setup2.jpg)

4. 保存屏幕设置后设备会重启，重新连接热点再继续配置

![setup3](images/setup3.jpg)

### 屏幕色调调整

可在配置界面中实时调整 RGB 通道，修正偏蓝/偏黄等色偏问题：

![color_adjust](images/color_adjust.jpg)

### WiFi 扫描与连接路由器

- 支持扫描附近 WiFi 并自动填充 SSID
- 保存网络设置后设备重启，之后可在配置界面查看设备的局域网 IP，再用局域网 IP 访问配置界面

![setup4](images/setup4.jpg)
![setup5](images/setup5.jpg)
![setup6](images/setup6.jpg)

### MQTT（可选）

配置 MQTT 后设备启动会自动连接；也支持实时重连与清除配置：

![setup7](images/setup7.jpg)
![setup8](images/setup8.jpg)

### 屏幕测试与速度测试

- 屏幕测试：配置页可进入测试页面，选择示例后点击“发送”
- 速度测试：可测试 HTTP 与 WebSocket 的吞吐

![setup9](images/setup9.jpg)
![setup10](images/setup10.jpg)
![setup11](images/setup11.jpg)

#### 传输速度测试步骤

1. 选择测试数据大小（10KB 到 1MB）
2. 点击“测试HTTP”按钮测试 HTTP 传输速度
3. 点击“测试WebSocket”按钮测试 WebSocket 传输速度
4. 测试结果会显示传输速率（KB/s 或 MB/s）和耗时
5. 测试日志会显示详细的测试过程信息

一般情况下，WebSocket 传输速度会比 HTTP 更快，更适合实时屏幕更新。

## USB 串口传图

### 适用芯片

- ESP32-S2：TinyUSB CDC（固件启动后会输出 `READY:USB-CDC`）
- ESP32-S3：USB Serial JTAG（固件启动后会输出 `READY:HIGH_SPEED_RX`）

### 上位机示例

- 示例工程：`tools/examples/`
- Rust 示例入口：`tools/examples/src/main.rs`（会自动查找 usb-screen 设备并发送图像）

### 串口通信协议（固件侧：`src/usb_reader.rs`）

- 设备信息查询（ReadInfo）
  - 主机发送：`ReadInfo`（8 字节二进制）或 ASCII `ReadInfo\n`
  - 设备回复：`ESP32-WIFI-SCREEN;{width};{height};PROTO:USB-SCREEN`
- 图像帧传输（LZ4 + RGB565）
  - 帧头：`image_aa`（8 字节） + `width,height,x,y`（4 个 `u16`，Big-Endian，共 8 字节）
  - 负载：`lz4_flex::compress_prepend_size(rgb565_bytes)` 输出
  - 帧尾：`image_bb`（8 字节）
- 测速（SpeedTest）
  - 主机发送：`SPDTEST1`（8 字节） + 任意数据 + `SPDEND!!`（8 字节）
  - 设备回复：`SPEEDRESULT;{bytes};{ms}`（为提高可靠性会重复发送）

## 性能测试

### WiFi 传输速度（ESP32-S2 @ 240MHz + 2MB PSRAM）

| 协议 | 最低速度 | 最高速度 | 典型速度 | 备注 |
|------|----------|----------|----------|------|
| HTTP Echo | 120 KB/s | 694 KB/s | ~280 KB/s | 单次请求/响应 |
| WebSocket Echo | 128 KB/s | 751 KB/s | ~350 KB/s | 长连接，性能更稳定 |

> 测试条件：100KB 数据往返测试（Echo），WiFi 2.4GHz，实际速度受信号强度和环境干扰影响。

### USB 串口传输速度（ESP32-S2/S3）

| 项目 | 主机侧吞吐 | 设备侧接收 | 备注 |
|------|------------|------------|------|
| USB 串口单向下行（SpeedTest） | ~462.6 KB/s | ~491.0 KB/s | chunk=4096，设备输出 `SPEEDRESULT;5050368;10045` |

> 说明：Nodejs 脚本在 `tools/speedtest/`，包含测速与发送图片的示例（例如 `serial_speed_test.js`、`send_image.js`）。

### 性能说明

- WiFi 峰值速度：约 750 KB/s（6 Mbps），接近 ESP32-S2 WiFi 理论极限
- WiFi 速度波动：2.4GHz WiFi 环境干扰会导致速度波动
- WebSocket 优势：长连接避免了 HTTP 连接开销，平均速度更高
- USB 串口典型速度：约 450 到 500 KB/s（与主机驱动、串口栈缓冲、chunk 大小等有关）

## WiFi-Screen-Client（屏幕镜像客户端）

一个客户端，通过系统截屏方式将屏幕镜像输出到 WiFi 屏幕上。建议先安装 [Virtual Display Driver](https://github.com/VirtualDisplay/Virtual-Display-Driver)，并添加一个大小、比例合适的虚拟显示器。

- 客户端目录：`tools/wifi-screen-client/`

### 安装 Virtual Display Driver

1. 打开 Virtual Display Driver 的 GitHub 下载页：https://github.com/VirtualDisplay/Virtual-Display-Driver/releases/tag/24.12.24

2. 下载最新版本的安装包 [Virtual.Display.Driver-v24.12.24-setup-x64.exe](https://github.com/VirtualDisplay/Virtual-Display-Driver/releases/download/24.12.24/Virtual.Display.Driver-v24.12.24-setup-x64.exe)

![vdd0](images/vdd0.jpg)

3. 运行安装程序，按照默认步骤安装

![vdd1](images/vdd1.jpg)

![vdd2](images/vdd2.jpg)

![vdd3](images/vdd3.jpg)

![vdd4](images/vdd4.jpg)

![vdd5](images/vdd5.jpg)

4. 安装完成后，启动 Virtual Display Driver 托盘程序

![vdd6](images/vdd6.jpg)

5. 右键点击 Virtual Display Driver 托盘图标，点击 "Loaded from vdd_settings.xml" 菜单项

![vdd7](images/vdd7.jpg)

6. 在打开的浏览器界面中复制完整的 vdd_settings.xml，并在文本编辑器中打开它

![vdd8](images/vdd8.jpg)

7. 删掉多余的屏幕配置，只留下需要的。例如屏幕是 ST7789 240x240，可以将分辨率设置成相同比例的 480x480。分辨率太小的话，应用窗口可能无法移动过去或完全显示

![vdd9](images/vdd9.jpg)

8. 再次点击 Virtual Display Driver 托盘图标，点击 "Reload Settings" 菜单项刷新虚拟屏幕配置。如果无效可多点击几次，稍等片刻

![vdd10](images/vdd10.jpg)

9. 打开系统屏幕设置，可看到一个虚拟的小显示器。确认选择的分辨率是正确的。如果修改的分辨率配置没有生效，尝试点击托盘菜单的 "Reload Driver" 或 "Disable Driver"，重新启动虚拟显示器驱动。然后在系统屏幕设置中移动小屏位置并点击应用，屏幕就会刷新到修改后的分辨率

![vdd11](images/vdd11.jpg)

![vdd12](images/vdd12.jpg)

### 连接 WiFi 显示器

输入 WiFi 显示器的 IP 地址，测试通过后点击"启动"按钮即可。

屏幕分辨率越大，屏幕刷新速度越慢，要适当增加延迟时间(ms)。

![client0](images/client0.jpg)

![client](images/client.jpg)

连接成功后，虚拟显示器的屏幕内容就会显示到 WiFi 屏幕上。

### 参数建议

- **截屏延迟**：值越小刷新越快，但占用 CPU 和网络更高；屏幕分辨率越大建议延迟越大，避免卡顿
- **传输格式**：
  - `RGB565` 延迟低但带宽占用更高
  - `JPG xx%` 带宽更省但编码耗时更高，适合 WiFi 环境较差时

## USB Screen 客户端

[USB屏幕&编辑器](https://github.com/planet0104/USB-Screen) 也适配了 WiFi 屏幕，配置好 IP 地址即可连接。

![editor1](images/editor1.jpg)

## 其他语言示例

在 `tools/examples/` 中提供了 Nodejs、Python、Rust 示例代码，可用于通过 HTTP/WebSocket/MQTT/USB 串口控制屏幕。

## License

MIT License
