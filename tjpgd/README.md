# TJpgDec-rs - 微型 JPEG 解码器

ChaN 的 TJpgDec 库的 Rust 实现 - 专为嵌入式系统设计的轻量级 JPEG 解码器。

[English Version](README.en.md) | [中文文档](README.md)

## 特性

- **轻量级**：针对内存受限的嵌入式系统优化
- **高性能**：三种优化级别可选，与 C 版本完全一致
- **灵活性**：支持多种输出格式（RGB888、RGB565、灰度）
- **no_std 兼容**：可在无标准库环境下运行
- **与 C 版本完全一致**：内存管理方式与原版 C 代码完全对应

## 支持的功能

- 基线 JPEG（SOF0）
- 灰度和 YCbCr 色彩空间
- 采样因子：4:4:4、4:2:0、4:2:2
- 输出缩放（1/1、1/2、1/4、1/8）
- RGB888 输出格式

## JD_FASTDECODE 优化级别

与 C 版本完全一致的三种优化级别：

| Level | Feature | 描述 | 工作区大小 | 适用平台 |
|-------|---------|------|-----------|---------|
| 0 | `fast-decode-0` | 基础优化 | 3100 bytes | 8/16 位 MCU |
| 1 | `fast-decode-1` | + 32 位桶移位器 | 3500 bytes | 32 位 MCU（推荐 ESP32） |
| 2 | `fast-decode-2` | + Huffman 快速查找表 | 9644 bytes | 最快，需要更多内存 |

## 使用方法

### 基本用法

```rust
use tjpgd::{JpegDecoder, MemoryPool, RECOMMENDED_POOL_SIZE, Result};

fn decode_jpeg(jpeg_data: &[u8]) -> Result<()> {
    // 分配内存池（与 C 版本完全一致）
    let mut pool_buffer = vec![0u8; RECOMMENDED_POOL_SIZE];
    let mut pool = MemoryPool::new(&mut pool_buffer);
    
    // 创建解码器
    let mut decoder = JpegDecoder::new();
    
    // 准备解码（对应 C 版本的 jd_prepare）
    decoder.prepare(jpeg_data, &mut pool)?;
    
    // 获取图像信息
    let width = decoder.width();
    let height = decoder.height();
    println!("图像尺寸: {}x{}", width, height);
    
    // 计算所需缓冲区大小
    let mcu_size = decoder.mcu_buffer_size();
    let work_size = decoder.work_buffer_size();
    
    // 分配工作缓冲区
    let mut mcu_buffer = vec![0i16; mcu_size];
    let mut work_buffer = vec![0u8; work_size];
    
    // 分配输出帧缓冲区
    let mut framebuffer = vec![0u8; (width as usize * height as usize * 3)];
    let fb_width = width as usize;
    
    // 解压缩（对应 C 版本的 jd_decomp）
    decoder.decompress(
        jpeg_data,
        0,  // scale: 0=1/1, 1=1/2, 2=1/4, 3=1/8
        &mut mcu_buffer,
        &mut work_buffer,
        &mut |_decoder, bitmap, rect| {
            // bitmap 是 RGB888 格式，每像素 3 字节
            let rect_width = (rect.right - rect.left + 1) as usize;
            let bytes_per_row = rect_width * 3;
            
            for y in rect.top..=rect.bottom {
                let src_offset = ((y - rect.top) as usize) * bytes_per_row;
                let dst_offset = (y as usize) * fb_width * 3 + (rect.left as usize) * 3;
                
                if src_offset + bytes_per_row <= bitmap.len()
                   && dst_offset + bytes_per_row <= framebuffer.len() {
                    framebuffer[dst_offset..dst_offset + bytes_per_row]
                        .copy_from_slice(&bitmap[src_offset..src_offset + bytes_per_row]);
                }
            }
            Ok(true)  // 继续解码
        }
    )?;
    
    Ok(())
}
```

### ESP32 示例

```rust
use tjpgd::{JpegDecoder, MemoryPool, RECOMMENDED_POOL_SIZE};

pub fn decode_jpeg_to_rgb565(jpeg_data: &[u8]) -> Result<(u16, u16, Vec<u16>)> {
    // 分配内存池
    let mut pool_buffer = vec![0u8; RECOMMENDED_POOL_SIZE];
    let mut pool = MemoryPool::new(&mut pool_buffer);
    
    let mut decoder = JpegDecoder::new();
    decoder.prepare(jpeg_data, &mut pool)?;
    
    let width = decoder.width();
    let height = decoder.height();
    
    let mut mcu_buffer = vec![0i16; decoder.mcu_buffer_size()];
    let mut work_buffer = vec![0u8; decoder.work_buffer_size()];
    let mut output = vec![0u16; width as usize * height as usize];
    let fb_width = width as usize;
    
    decoder.decompress(
        jpeg_data, 0,
        &mut mcu_buffer, &mut work_buffer,
        &mut |_decoder, bitmap, rect| {
            let rect_width = (rect.right - rect.left + 1) as usize;
            let bytes_per_row = rect_width * 3;
            
            for y in rect.top..=rect.bottom {
                let y_offset = (y - rect.top) as usize;
                let src_offset = y_offset * bytes_per_row;
                let dst_row = y as usize * fb_width + rect.left as usize;
                
                for x in 0..rect_width {
                    let byte_idx = src_offset + x * 3;
                    if byte_idx + 2 < bitmap.len() {
                        let r = bitmap[byte_idx];
                        let g = bitmap[byte_idx + 1];
                        let b = bitmap[byte_idx + 2];
                        // RGB888 转 RGB565
                        let pixel = ((r as u16 & 0xF8) << 8)
                                  | ((g as u16 & 0xFC) << 3)
                                  | ((b as u16) >> 3);
                        output[dst_row + x] = pixel.swap_bytes();
                    }
                }
            }
            Ok(true)
        }
    )?;
    
    Ok((width, height, output))
}
```

## 安装

在你的 `Cargo.toml` 中添加：

```toml
[dependencies]
tjpgd = { path = "path/to/tjpgd", features = ["fast-decode-2"] }
```

### 特性标志

| Feature | 描述 |
|---------|------|
| `std`（默认） | 启用标准库支持 |
| `fast-decode-0` | JD_FASTDECODE=0：基础优化，适合 8/16 位 MCU |
| `fast-decode-1` | JD_FASTDECODE=1：32 位桶移位器（推荐 ESP32） |
| `fast-decode-2` | JD_FASTDECODE=2：+ Huffman 快速查找表（最快） |
| `fast-decode` | `fast-decode-2` 的别名 |
| `table-clip` | 使用查找表进行值剪裁（增加 ~1KB 代码） |
| `use-scale` | 启用输出缩放支持 |
| `debug-huffman` | 启用 Huffman 解码调试输出 |

### 针对不同平台的配置

**ESP32（推荐配置）：**
```toml
[dependencies.tjpgd]
path = "tjpgd"
default-features = false
features = ["fast-decode-2"]
```

**内存受限的 32 位 MCU：**
```toml
[dependencies.tjpgd]
path = "tjpgd"
default-features = false
features = ["fast-decode-1"]
```

**8/16 位 MCU（实验性）：**
```toml
[dependencies.tjpgd]
path = "tjpgd"
default-features = false
features = ["fast-decode-0"]
```

## 内存需求

| 优化级别 | 解码器结构 | 工作区 | 说明 |
|---------|-----------|--------|------|
| Level 0 | ~120 bytes | 3100 bytes | 基础模式 |
| Level 1 | ~120 bytes | 3500 bytes | + 32 位寄存器 |
| Level 2 | ~120 bytes | 9644 bytes | + Huffman LUT |

### 缓冲区需求
- MCU 缓冲区：192-384 个 i16 元素（384-768 字节）
- 工作缓冲区：192-768 字节

## API 文档

### JpegDecoder

```rust
use tjpgd::{JpegDecoder, MemoryPool, RECOMMENDED_POOL_SIZE};

// 创建内存池
let mut pool_buffer = vec![0u8; RECOMMENDED_POOL_SIZE];
let mut pool = MemoryPool::new(&mut pool_buffer);

// 创建解码器
let mut decoder = JpegDecoder::new();

// 准备解码
decoder.prepare(jpeg_data, &mut pool)?;

// 获取图像信息
let width = decoder.width();      // 输出宽度（已应用缩放）
let height = decoder.height();    // 输出高度（已应用缩放）
let raw_width = decoder.raw_width();   // 原始宽度
let raw_height = decoder.raw_height(); // 原始高度
let components = decoder.components(); // 颜色分量数 (1=灰度, 3=彩色)

// 计算缓冲区大小
let mcu_size = decoder.mcu_buffer_size();
let work_size = decoder.work_buffer_size();

// 解压缩
decoder.decompress(jpeg_data, scale, &mut mcu_buf, &mut work_buf, callback)?;
```

### 查询优化级别

```rust
use tjpgd::fastdecode_level;

let level = fastdecode_level();
println!("当前 JD_FASTDECODE 级别: {}", level);
```

## 与 C 版本的对应关系

| C 函数/类型 | Rust 对应 |
|------------|----------|
| `jd_prepare()` | `decoder.prepare()` |
| `jd_decomp()` | `decoder.decompress()` |
| `JDEC` | `JpegDecoder` |
| `JRESULT` | `Result<T>` |
| `JRECT` | `Rectangle` |
| `alloc_pool()` | `MemoryPool` |
| `JD_FASTDECODE` | `fast-decode-0/1/2` features |

## 项目结构

```
tjpgd/
├── Cargo.toml
├── README.md / README.en.md
├── src/
│   ├── lib.rs           # 库入口
│   ├── types.rs         # 类型定义
│   ├── tables.rs        # 常量表
│   ├── huffman.rs       # Huffman 解码（支持三种优化级别）
│   ├── idct.rs          # IDCT 和颜色转换
│   ├── decoder.rs       # 主解码器
│   └── pool.rs          # 内存池实现
└── examples/
    ├── basic.rs         # 基本使用示例
    ├── jpg2bmp.rs       # JPEG 转 BMP 工具
    └── compare_outputs.ps1  # C/Rust 输出对比脚本
```

## 开发和测试

```bash
# 运行测试
cargo test

# 运行示例（使用 Level 2）
cargo run --example jpg2bmp -- input.jpg output.bmp

# 使用特定优化级别
cargo run --example jpg2bmp --no-default-features --features fast-decode-1 -- input.jpg

# 对比 C 和 Rust 输出（测试所有模式）
cd examples
powershell -ExecutionPolicy Bypass -File compare_outputs.ps1 -Mode all

# 只测试特定模式
powershell -ExecutionPolicy Bypass -File compare_outputs.ps1 -Mode 2
```

## 常见问题

### Q: ESP32 上出现栈溢出怎么办？
A: 确保使用 `MemoryPool` 分配内存。解码器本身只有 ~120 bytes，不会导致栈溢出。工作缓冲区应该在堆上分配（使用 `vec![]`）。

### Q: 如何选择优化级别？
A: 
- **ESP32**：推荐 `fast-decode-2`（最快）或 `fast-decode-1`（节省内存）
- **内存受限**：使用 `fast-decode-1`，工作区只需 3500 bytes
- **8/16 位 MCU**：使用 `fast-decode-0`（实验性）

### Q: 为什么输出与 C 版本略有不同？
A: IDCT 计算中的舍入误差，通常每个像素差异 ≤3，不影响视觉效果。

### Q: 如何减少内存使用？
A: 
1. 使用 `fast-decode-1` 替代 `fast-decode-2`（节省 ~6KB）
2. 使用较小的缩放因子（1/2、1/4、1/8）
3. 分块处理大图像

## 许可证

本项目基于 [TJpg_Decoder](https://github.com/Bodmer/TJpg_Decoder)（原始作者：ChaN）  
Rust 实现：MIT License

```
TJpgDec 模块是免费软件，没有任何担保。
无使用限制。您可以使用、修改和重新分发它用于
个人、非营利或商业产品，风险自负。
```

## 相关链接

- [变更日志](CHANGELOG.md)
- [英文文档](README.en.md)

## 致谢

感谢 ChaN 创建了原始的 TJpgDec 库。
