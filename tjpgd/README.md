# TJpgDec-rs - 微型 JPEG 解码器

ChaN 的 TJpgDec 库的现代 Rust 实现 - 专为嵌入式系统设计的轻量级 JPEG 解码器。

[English Version](README.en.md) | [中文文档](README.md)

## 特性

- **轻量级**：针对内存受限的嵌入式系统优化
- **高性能**：多种优化级别可选
- **灵活性**：支持多种输出格式（RGB888、RGB565、灰度）
- **no_std 兼容**：可在无标准库环境下运行
- **安全的 Rust**：现代 Rust 实现，具有安全性保证

## 支持的功能

- 基线 JPEG（SOF0）
- 灰度和 YCbCr 色彩空间
- 采样因子：4:4:4、4:2:0、4:2:2
- 输出缩放（1/1、1/2、1/4、1/8）
- RGB888、RGB565 和灰度输出格式

## 使用方法

### 基本用法（推荐用于嵌入式系统）

```rust
use tjpg_decoder::{JpegDecoder, Result};

fn decode_jpeg(jpeg_data: &[u8]) -> Result<()> {
    let mut decoder = JpegDecoder::new();
    
    // 准备解码器
    decoder.prepare(jpeg_data)?;
    
    // 计算所需缓冲区大小
    let mcu_size = decoder.mcu_buffer_size();
    let work_size = decoder.work_buffer_size();
    
    // 分配缓冲区（可以是栈上或静态内存）
    let mut mcu_buffer = vec![0i16; mcu_size];
    let mut work_buffer = vec![0u8; work_size];
    
    // 使用外部缓冲区解压缩（内存节约型）
    decoder.decompress_with_buffers(
        jpeg_data, 0, 
        &mut mcu_buffer, 
        &mut work_buffer,
        &mut |_decoder, bitmap, rect| {
            // 处理解码后的矩形区域
            println!("解码块: {}x{} at ({}, {})", 
                     rect.width(), rect.height(), rect.left, rect.top);
            Ok(true)
        }
    )?;
    
    Ok(())
}
```

### 自动分配缓冲区（需要启用 `alloc-buffers` feature）

```rust
// 启用 alloc-buffers feature 后可用
use tjpg_decoder::{JpegDecoder, Result};

fn decode_jpeg_auto(jpeg_data: &[u8]) -> Result<()> {
    let mut decoder = JpegDecoder::new();
    decoder.prepare(jpeg_data)?;
    
    // 自动分配内部缓冲区（需要更多栈空间）
    decoder.decompress(jpeg_data, 0, &mut |_decoder, bitmap, rect| {
        println!("解码块: {}x{} at ({}, {})", 
                 rect.width(), rect.height(), rect.left, rect.top);
        Ok(true)
    })?;
    
    Ok(())
}
```

## 安装

在你的 `Cargo.toml` 中添加：

```toml
[dependencies]
tjpg-decoder = { path = "path/to/tjpg-decoder", features = ["fast-decode"] }
```

### 特性标志

- `std`（默认）- 启用标准库支持
- `fast-decode` - 启用快速 Huffman 解码（需要额外 ~6KB RAM）
- `table-clip` - 使用查找表进行值剪裁（增加 ~1KB 代码）
- `use-scale` - 启用输出缩放支持
- `alloc-buffers` - 启用自动缓冲区分配的 `decompress()` 方法（默认关闭，需要更多栈空间）

### 针对不同平台的配置

**8/16 位 MCU（最小内存）：**
```toml
[dependencies.tjpg-decoder]
path = "path/to/tjpg-decoder"
default-features = false
```

**32 位 MCU（如 ESP32）：**
```toml
[dependencies.tjpg-decoder]
path = "path/to/tjpg-decoder"
features = ["fast-decode", "table-clip"]
```

## 内存需求

### 使用 `decompress_with_buffers()`（推荐）
- MCU 缓冲区：192-384 个 i16 元素（384-768 字节）
  - 4:4:4 采样：192 个元素
  - 4:2:0/4:2:2 采样：384 个元素
- 工作缓冲区：
  - 基本模式：192-768 字节
  - 快速解码模式（`fast-decode`）：约 6KB
- **总计**：约 1-7KB（取决于配置和采样格式）

### 使用 `decompress()`（需要 `alloc-buffers` feature）
- 在栈上自动分配所有缓冲区
- ESP32 默认栈（3-4KB）可能不足，需要增加栈大小
- **不推荐用于嵌入式系统**

## API 文档

### JpegDecoder

主解码器结构体。

```rust
// 创建新的解码器
let mut decoder = JpegDecoder::new();

// 设置字节交换（用于某些显示器）
decoder.set_swap_bytes(true);

// 准备解码（解析 JPEG 头）
decoder.prepare(jpeg_data)?;

// 获取图像信息
let width = decoder.width();
let height = decoder.height();
let components = decoder.components();

// 计算所需缓冲区大小
let mcu_size = decoder.mcu_buffer_size();
let work_size = decoder.work_buffer_size();

// 分配缓冲区
let mut mcu_buffer = vec![0i16; mcu_size];
let mut work_buffer = vec![0u8; work_size];

// 解压缩图像（使用外部缓冲区）
decoder.decompress_with_buffers(
    jpeg_data,
    scale,  // 0=1/1, 1=1/2, 2=1/4, 3=1/8
    &mut mcu_buffer,
    &mut work_buffer,
    &mut |decoder, bitmap, rect| {
        // 处理位图数据
        Ok(true)  // 返回 true 继续，false 中断
    }
)?;
```

### 错误处理

所有操作都返回 `Result<T, Error>`：

- `Error::Ok` - 成功
- `Error::Interrupted` - 被输出函数中断
- `Error::Input` - 输入流错误
- `Error::InsufficientMemory` - 内存不足
- `Error::InsufficientBuffer` - 缓冲区不足
- `Error::Parameter` - 参数错误
- `Error::FormatError` - 格式错误
- `Error::UnsupportedFormat` - 不支持的格式
- `Error::UnsupportedStandard` - 不支持的 JPEG 标准

## 示例

### 示例 1：解码到内存缓冲区

```rust
use tjpg_decoder::{JpegDecoder, Result};

fn decode_to_buffer(jpeg_data: &[u8]) -> Result<Vec<u8>> {
    let mut decoder = JpegDecoder::new();
    decoder.prepare(jpeg_data)?;
    
    let width = decoder.width() as usize;
    let height = decoder.height() as usize;
    let mut output = vec![0u8; width * height * 3]; // RGB888
    
    // 分配解码缓冲区
    let mcu_size = decoder.mcu_buffer_size();
    let work_size = decoder.work_buffer_size();
    let mut mcu_buffer = vec![0i16; mcu_size];
    let mut work_buffer = vec![0u8; work_size];
    
    decoder.decompress_with_buffers(
        jpeg_data, 0, 
        &mut mcu_buffer, 
        &mut work_buffer,
        &mut |_, bitmap, rect| {
            let x = rect.left as usize;
            let y = rect.top as usize;
            let w = rect.width() as usize;
            
            // 复制到输出缓冲区
            for dy in 0..rect.height() as usize {
                let src_offset = dy * w * 3;
                let dst_offset = ((y + dy) * width + x) * 3;
                output[dst_offset..dst_offset + w * 3]
                    .copy_from_slice(&bitmap[src_offset..src_offset + w * 3]);
            }
            
            Ok(true)
        }
    )?;
    
    Ok(output)
}
```

### 示例 2：在 ESP32 上显示（内存优化）

```rust
use tjpg_decoder::{JpegDecoder, Result};

fn display_jpeg(jpeg_data: &[u8], display: &mut Display) -> Result<()> {
    let mut decoder = JpegDecoder::new();
    decoder.set_swap_bytes(true); // 如果显示器需要
    
    decoder.prepare(jpeg_data)?;
    
    // 分配解码缓冲区（可在静态内存中分配以节省栈空间）
    let mcu_size = decoder.mcu_buffer_size();
    let work_size = decoder.work_buffer_size();
    let mut mcu_buffer = vec![0i16; mcu_size];  // ESP32: 384-768 字节
    let mut work_buffer = vec![0u8; work_size]; // ESP32: 约 200-6000 字节
    
    decoder.decompress_with_buffers(
        jpeg_data, 0,
        &mut mcu_buffer,
        &mut work_buffer,
        &mut |_, bitmap, rect| {
            // 将 RGB 数据写入显示器
            display.draw_image(
                rect.left,
                rect.top,
                rect.width(),
                rect.height(),
                bitmap,
            )?;
            
            Ok(true)
        }
    )?;
    
    Ok(())
}
```

## 性能对比

| 平台 | 优化级别 | 解码时间 | 内存使用 |
|------|---------|---------|---------|
| ESP32 | fast-decode | ~快 40% | 9.6KB |
| ESP32 | 基本模式 | 基准 | 3.5KB |
| STM32F4 | fast-decode | ~快 35% | 9.6KB |

## 与 C 版本的对应关系

| C 函数/类型 | Rust 对应 |
|------------|----------|
| `jd_prepare()` | `decoder.prepare()` |
| `jd_decomp()` | `decoder.decompress_with_buffers()` |
| `jd_decomp()`（自动分配） | `decoder.decompress()`（需要 `alloc-buffers` feature） |
| `JDEC` | `JpegDecoder` |
| `JRESULT` | `Result<T>` |
| `JRECT` | `Rectangle` |

**注意**：C 版本使用外部提供的工作缓冲区，对应 Rust 的 `decompress_with_buffers()` 方法。

## 开发和测试

```bash
# 检查编译
cargo check

# 运行测试
cargo test

# 运行测试（启用 alloc-buffers）
cargo test --features alloc-buffers

# 构建 release 版本
cargo build --release

# 使用所有特性构建
cargo build --all-features

# no_std 模式构建
cargo build --no-default-features

# 运行示例
cargo run --example basic
cargo run --example jpg2bmp
cargo run --example test_suite
```

## 快速开始指南

### 项目结构

```
tjpgd/
├── Cargo.toml          # 项目配置
├── README.md           # 项目说明
├── LICENSE             # 许可证
├── CHANGELOG.md        # 变更日志
├── DEVELOPMENT.md      # 开发指南
├── src/
│   ├── lib.rs         # 库入口
│   ├── types.rs       # 类型定义
│   ├── tables.rs      # 常量表
│   ├── huffman.rs     # Huffman 解码
│   ├── idct.rs        # IDCT 和颜色转换
│   └── decoder.rs     # 主解码器
└── examples/
    ├── basic.rs       # 基本使用示例
    ├── jpg2bmp.rs     # JPEG 转 BMP 工具
    ├── test_info.rs   # 显示图片信息
    └── test_suite.rs  # 综合测试套件
```

### 不同平台的性能优化

**8/16 位 MCU（最小内存）：**
```toml
[dependencies.tjpgd]
path = "tjpgd"
default-features = false
```

**32 位 MCU（如 ESP32）：**
```toml
[dependencies.tjpgd]
path = "tjpgd"
features = ["fast-decode", "table-clip"]
```

**桌面/服务器（性能优先）：**
```toml
[dependencies.tjpgd]
path = "tjpgd"
features = ["std", "fast-decode", "table-clip", "use-scale", "alloc-buffers"]
```

### 与原项目集成

在你的主项目中，可以这样使用：

```rust
// 在 src/main.rs 中
mod tjpgd_wrapper {
    use tjpgd::{JpegDecoder, Rectangle, Result};
    
    pub fn decode_jpeg_to_rgb565(
        jpeg_data: &[u8],
        output: &mut [u16],
    ) -> Result<(u16, u16)> {
        let mut decoder = JpegDecoder::new();
        decoder.set_swap_bytes(true);
        
        decoder.prepare(jpeg_data)?;
        let (width, height) = (decoder.width(), decoder.height());
        
        // 分配解码缓冲区
        let mcu_size = decoder.mcu_buffer_size();
        let work_size = decoder.work_buffer_size();
        let mut mcu_buffer = vec![0i16; mcu_size];
        let mut work_buffer = vec![0u8; work_size];
        
        decoder.decompress_with_buffers(
            jpeg_data, 0,
            &mut mcu_buffer,
            &mut work_buffer,
            &mut |_, bitmap, rect| {
                // 转换并写入输出缓冲区
                // ...
                Ok(true)
            }
        )?;
        
        Ok((width, height))
    }
}
```

## 常见问题

### Q: ESP32 上出现栈溢出怎么办？
A: 使用 `decompress_with_buffers()` 方法，它可以在静态内存或堆上分配缓冲区，而不是在栈上。

### Q: 如何减少内存使用？
A: 
1. 不要启用 `fast-decode` feature（节省 ~6KB）
2. 使用 `decompress_with_buffers()` 并在静态内存中分配缓冲区
3. 考虑使用较小的缩放因子

### Q: 为什么输出与 C 版本略有不同？
A: IDCT 计算中的舍入误差，通常差异 <2%，不影响视觉效果。

## API 更新说明（v0.3.1）

### 新增 API

1. **`mcu_buffer_size()`** - 计算所需 MCU 缓冲区大小
2. **`work_buffer_size()`** - 计算所需工作缓冲区大小
3. **`decompress_with_buffers()`** - 使用外部缓冲区解压缩（推荐）

### API 变更

- `decompress()` 方法现在需要 `alloc-buffers` feature（默认关闭）
- 推荐使用 `decompress_with_buffers()` 以获得更好的内存控制

## 开发者指南

### 架构说明

库组织为多个模块：

#### 核心模块

1. **types.rs** - 类型定义和错误处理
2. **tables.rs** - 常量查找表和转换函数
3. **huffman.rs** - Huffman 解码实现
4. **idct.rs** - 逆离散余弦变换和颜色转换
5. **decoder.rs** - 主 JPEG 解码器实现

### 内存管理

库专为内存受限的嵌入式系统设计：

- 尽可能使用栈分配
- 在 `no_std` 环境中使用 `heapless` crate 处理固定大小集合
- 根据优化级别配置工作空间大小

### Feature 详细说明

#### `std`（默认）
启用标准库支持。在 `no_std` 环境中禁用。

#### `fast-decode`
使用查找表（LUT）启用快速 Huffman 解码：
- 增加约 6KB 内存使用
- 显著提高解码速度
- 推荐用于有足够 RAM 的 32 位 MCU

#### `table-clip`
使用 1KB 查找表进行值裁剪：
- 比条件裁剪更快
- 内存开销很小

#### `use-scale`
启用输出缩放支持（1/2、1/4、1/8）：
- 用于生成缩略图
- 代码大小增加很小

#### `alloc-buffers`
在 `decompress()` 方法中启用自动缓冲区分配：
- **默认禁用** - 可能在嵌入式系统上导致栈溢出
- 在栈上自动分配 MCU 和工作缓冲区
- 仅在栈空间充足（>4KB）的平台上使用
- 嵌入式系统请使用 `decompress_with_buffers()`

### 实现细节

#### 内存高效 API（v0.3.1+）

库现在提供两种解码 API：

1. **`decompress_with_buffers()`**（推荐用于嵌入式）
   - 接受外部 MCU 和工作缓冲区
   - 缓冲区可以在静态内存或堆中分配
   - 无栈溢出风险
   - 使用 `mcu_buffer_size()` 和 `work_buffer_size()` 计算大小

2. **`decompress()`**（需要 `alloc-buffers` feature）
   - 在栈上自动分配缓冲区
   - 方便但可能在 ESP32 上导致栈溢出
   - 仅适用于栈空间 >4KB 的系统

#### Huffman 解码

Huffman 解码器支持两种模式：
1. **增量搜索** - 较慢但使用更少内存
2. **快速 LUT** - 使用 10 位查找表处理常见代码

#### IDCT

使用 Arai、Agui 和 Nakajima 快速 DCT 算法：
- 使用定点运算提高效率
- 针对 8x8 块优化
- 尽可能进行就地计算

#### 颜色转换

使用定点运算进行 YCbCr 到 RGB 转换：
- 避免浮点运算
- 在仅支持整数的处理器上高效
- 处理色度子采样（4:2:0、4:2:2）

### 安全性

库优先考虑内存安全：
- 最小化 unsafe 代码（仅在示例的 BMP 头序列化中）
- 所有缓冲区访问都进行边界检查
- 正确的错误传播
- 发布版本中无 panic（返回 `Result`）
- 解压前进行缓冲区大小验证

### 贡献指南

贡献代码时，请确保：
1. 代码遵循 Rust 惯用法和最佳实践
2. 所有测试通过
3. 保持 `no_std` 兼容性
4. 对性能关键代码进行基准测试
5. 更新相关文档

## 贡献

欢迎贡献！请确保：
1. 代码遵循 Rust 规范
2. 所有测试通过
3. 保持 no_std 兼容性
4. 更新相关文档

## 许可证

本项目基于 [TJpg_Decoder](https://github.com/Bodmer/TJpg_Decoder)（原始作者：ChaN）  
Rust 实现：MIT License

原始 TJpgDec 许可：
```
TJpg_Decoder 基于 TJpgDec - Tiny JPEG Decompressor R0.03 (C)ChaN, 2021
```

原始 TJpgDec 许可：
```
TJpgDec 模块是免费软件，没有任何担保。
无使用限制。您可以使用、修改和重新分发它用于
个人、非营利或商业产品，风险自负。
源代码的重新分发必须保留上述版权声明。
```

## 相关链接

- [变更日志](CHANGELOG.md)
- [英文文档](README.en.md)

## 致谢

感谢 ChaN 创建了原始的 TJpgDec 库。
