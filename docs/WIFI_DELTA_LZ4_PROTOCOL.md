# WiFi 帧差分传输协议 (LZ4压缩 + ACK确认)

本文档描述ESP32 WiFi屏幕项目中使用的帧差分传输协议，用于优化WebSocket传输RGB565图像数据。

## 概述

该协议通过以下技术减少传输数据量和提高传输效率：

1. **帧差分编码** - 只传输与上一帧的差异（XOR）
2. **LZ4压缩** - 对数据进行高速压缩
3. **ACK确认机制** - 确保帧同步，避免帧堆积
4. **智能帧类型选择** - 自动选择最优的帧类型

## 帧类型

### 1. 关键帧 (KEY Frame)

完整的RGB565图像数据，LZ4压缩后发送。

**触发条件：**
- 首帧
- 每隔60帧强制发送（可配置）
- 图像尺寸变化
- 差分帧比关键帧更大时
- 收到NACK后

### 2. 差分帧 (DLT Frame)

当前帧与上一帧的XOR差分数据，LZ4压缩后发送。

**特点：**
- 静态画面时数据量极小（大量连续0值压缩率极高）
- 动态画面时数据量取决于变化区域大小

### 3. 无变化帧 (NOP Frame)

当画面几乎没有变化时发送的最小帧。

**触发条件：**
- 差分数据LZ4压缩后小于200字节

**优势：**
- ESP32收到后直接返回ACK，跳过解码和绘制
- 静态画面时帧率可达100+ FPS

## 协议格式

### 帧头结构 (12字节)

```
+----------+----------+----------+----------+
|  MAGIC   |  MAGIC   |  MAGIC   |  MAGIC   |
| (8bytes) | (8bytes) | (8bytes) | (8bytes) |
+----------+----------+----------+----------+
|  WIDTH   |  HEIGHT  |
| (2bytes) | (2bytes) |
+----------+----------+
|     LZ4 COMPRESSED DATA (变长)           |
+------------------------------------------+
```

### Magic Numbers (8字节)

| Magic | 值 | 说明 |
|-------|-----|------|
| WIFI_KEY_MAGIC | `wflz4ke_` | LZ4压缩的关键帧 |
| WIFI_DLT_MAGIC | `wflz4dl_` | LZ4压缩的差分帧 |
| WIFI_NOP_MAGIC | `wflz4no_` | 无变化帧 |

### 字段说明

| 字段 | 大小 | 字节序 | 说明 |
|------|------|--------|------|
| MAGIC | 8字节 | - | 帧类型标识 |
| WIDTH | 2字节 | Big-Endian | 图像宽度(像素) |
| HEIGHT | 2字节 | Big-Endian | 图像高度(像素) |
| DATA | 变长 | - | LZ4压缩数据(NOP帧无此字段) |

## ACK/NACK 机制

### 通信流程

```
上位机                          ESP32
   |                              |
   |-------- 发送帧 ------------->|
   |                              | 解码
   |                              | 绘制
   |<-------- ACK/NACK -----------|
   |                              |
   |-------- 下一帧 ------------->|
   |         ...                  |
```

### ACK响应

ESP32在以下情况发送 `ACK`：
- 成功解码并绘制完成
- 收到NOP帧（无需处理）

### NACK响应

ESP32在以下情况发送 `NACK`：
- 收到差分帧但没有参考帧
- 解码失败
- 数据校验错误

上位机收到 `NACK` 后：
- 重置差分编码器
- 下一帧发送关键帧

### 超时处理

- 上位机等待ACK超时时间：3秒
- 超时后重置编码器，发送关键帧

## 编码器实现 (上位机)

```rust
struct DeltaEncoder {
    prev_frame: Vec<u8>,       // 上一帧RGB565数据
    frame_count: u32,          // 帧计数
    key_frame_interval: u32,   // 关键帧间隔(默认60)
}

impl DeltaEncoder {
    fn encode(&mut self, rgb565_data: &[u8], width: u16, height: u16) 
        -> (Vec<u8>, &'static str);
    fn reset(&mut self);
}
```

### 编码流程

```
输入: RGB565数据 (width * height * 2 字节)
      |
      v
[是否需要关键帧?] --是--> 压缩完整数据 --> 输出KEY帧
      |
      否
      |
      v
计算XOR差分: current ^ previous
      |
      v
LZ4压缩差分数据
      |
      v
[压缩后 < 200字节?] --是--> 输出NOP帧
      |
      否
      |
      v
[差分帧 >= 关键帧?] --是--> 压缩完整数据 --> 输出KEY帧
      |
      否
      |
      v
输出DLT帧
```

## 解码器实现 (ESP32)

```rust
struct DeltaDecoder {
    prev_frame: Vec<u8>,    // 参考帧
    error_count: u32,       // 错误计数
}

impl DeltaDecoder {
    fn decode_key_frame(&mut self, lz4_data: &[u8]) -> Result<&[u8], &'static str>;
    fn decode_delta_frame(&mut self, lz4_data: &[u8]) -> Result<&[u8], &'static str>;
    fn reset(&mut self);
}
```

### 解码流程

```
接收WebSocket二进制数据
      |
      v
[检查Magic] --> NOP帧 --> 发送ACK --> 结束
      |
      v
      +--> KEY帧 --> LZ4解压 --> 保存为参考帧 --> 绘制 --> 发送ACK
      |
      +--> DLT帧 --> [有参考帧?] --否--> 发送NACK
                          |
                          是
                          |
                          v
                     LZ4解压差分数据
                          |
                          v
                     XOR还原: prev_frame ^= delta
                          |
                          v
                     绘制 --> 发送ACK
```

## 性能数据 (ESP32-S3, 480x320分辨率)

### 各阶段耗时

| 阶段 | 耗时 | 说明 |
|------|------|------|
| LZ4解压 | ~85ms | 从1-2KB解压到307200字节 |
| XOR还原 | ~60ms | 处理307200字节 |
| SPI绘制 | ~145ms | 传输307200字节到屏幕 |
| **总计** | **~290ms** | **约3.4 FPS** |

### 不同场景对比

| 场景 | 帧类型 | 压缩后大小 | 处理时间 |
|------|--------|-----------|----------|
| 静态画面 | NOP | 12字节 | <5ms |
| 轻微变化(鼠标) | DLT | 1-5KB | ~290ms |
| 较大变化 | DLT | 10-50KB | ~290ms |
| 全屏变化 | KEY | 80-150KB | ~270ms |

### 压缩效果

原始数据: 307,200字节 (480x320x2)

| 帧类型 | 压缩后 | 压缩比 |
|--------|--------|--------|
| KEY帧 | ~80-150KB | 2-4x |
| DLT帧(静态) | ~1-2KB | 150-300x |
| DLT帧(动态) | ~5-50KB | 6-60x |
| NOP帧 | 12字节 | 25600x |

## WebSocket连接管理

### 连接建立

1. 上位机通过HTTP获取 `/display_config` 获取屏幕分辨率
2. 连接 `ws://{ip}/ws` WebSocket端点
3. 重置差分编码器
4. 开始发送帧

### 断开重连

1. 检测到连接断开或超时
2. 重置差分编码器
3. 3秒后尝试重新连接
4. 重新获取屏幕配置
5. 发送关键帧作为首帧

## 相关文件

### ESP32端
- `src/http_server.rs` - WebSocket处理和帧解码
- `src/display.rs` - 屏幕绘制

### 上位机端
- `tools/wifi-screen-client/src/recorder.rs` - 帧差分编码
- `USB-Screen/src/wifi_screen.rs` - WiFi屏幕客户端

## 与USB协议对比

| 特性 | WiFi协议 | USB协议 |
|------|----------|---------|
| 压缩算法 | LZ4 | ZSTD (可选LZ4) |
| 通信方式 | WebSocket | USB Serial |
| 确认机制 | ACK/NACK | 无 |
| 最大帧率 | ~3.4 FPS | ~10-15 FPS |
| 延迟 | 较高(网络延迟) | 低 |

## 版本历史

- v1.0 - 初始实现，使用ZSTD压缩
- v1.1 - 改用LZ4压缩，解码速度提升2-3倍
- v1.2 - 添加ACK/NACK机制，避免帧堆积
- v1.3 - 添加NOP帧，静态画面优化
