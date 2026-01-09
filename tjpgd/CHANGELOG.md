# Changelog

All notable changes to this project will be documented in this file.

## [0.4.0] - 2026-01-09

### Added
- **三种 JD_FASTDECODE 优化级别**：与 C 版本完全一致
  - `fast-decode-0`: 基础优化，适合 8/16 位 MCU (3100 bytes)
  - `fast-decode-1`: + 32 位桶移位器，推荐 ESP32 (3500 bytes)
  - `fast-decode-2`: + Huffman 快速查找表，最快 (9644 bytes)
- **`fastdecode_level()` 函数**：运行时查询当前优化级别
- **多模式测试脚本**：`compare_outputs.ps1` 支持测试所有模式

### Changed
- **统一 API**：移除了 `JpegDecoderPool`，只保留 `JpegDecoder`（内存池版本）
- **Feature 重构**：
  - `fast-decode` 现在是 `fast-decode-2` 的别名（向后兼容）
  - 默认使用 `fast-decode-1`（适合 32 位 MCU）
- **项目结构简化**：删除了 `decoder_pool.rs` 和 `huffman_pool.rs`

### Fixed
- **LUT 快速解码 bug**：修复了 `decode_fast` 在 LUT 未命中时的逻辑错误
- **Huffman 解码**：确保与 C 版本 `huffext()` 函数完全一致

### Performance
- Level 1/2 测试：13/13 测试图片全部通过
- 与 C 版本一致性：舍入误差 ≤3（每像素）

## [0.3.1] - 2026-01-08

### Added
- **内存节约型 API**：新增 `decompress_with_buffers()` 方法，接受外部缓冲区
- **缓冲区大小计算**：新增 `mcu_buffer_size()` 和 `work_buffer_size()` 辅助方法
- **Feature 控制**：新增 `alloc-buffers` feature，控制自动缓冲区分配的 `decompress()` 方法（默认关闭）
- **完整测试套件**：11 个集成测试验证所有功能
- **C/Rust 对比测试**：PowerShell 脚本验证输出一致性

### Changed
- **API 变更**：`decompress()` 方法改为可选 feature（`alloc-buffers`），默认不可用
- **推荐 API**：现在推荐使用 `decompress_with_buffers()` 以获得更好的内存控制
- **文档更新**：所有示例代码更新为使用内存节约型 API

### Fixed
- **ESP32 栈溢出**：解决 ESP32 上 `decompress()` 方法栈溢出问题
- **内存使用**：大幅减少栈内存使用，适合嵌入式系统（MCU 缓冲区：384-768 字节，工作缓冲区：200-6000 字节）

### Performance
- 与 C 版本输出一致性：8 个测试图片全部通过，误差 <2% （IDCT 舍入误差）
- 内存占用：典型配置下总计约 1-7KB（取决于采样格式和 feature）

## [0.3.0] - 2026-01-06

### Added
- Initial Rust implementation of TJpgDec R0.03
- Support for baseline JPEG (SOF0)
- Huffman decoding with optional fast lookup tables
- Inverse DCT using Arai algorithm
- YCbCr to RGB color space conversion
- Support for RGB888, RGB565, and Grayscale output formats
- Support for 4:4:4, 4:2:2, and 4:2:0 sampling
- Output scaling (1/1, 1/2, 1/4, 1/8)
- `no_std` compatible implementation
- Comprehensive error handling

### Features
- `std`: Enable standard library support (default)
- `fast-decode`: Enable fast Huffman decoding with lookup tables
- `table-clip`: Use lookup table for value clipping
- `use-scale`: Enable output scaling support

## Original C Version

Based on TJpgDec R0.03 by ChaN (2021)
- Oct 04, 2011 R0.01  First release
- Feb 19, 2012 R0.01a Fixed decompression fails when scan starts with an escape seq
- Sep 03, 2012 R0.01b Added JD_TBLCLIP option
- Mar 16, 2019 R0.01c Supported stdint.h
- Jul 01, 2020 R0.01d Fixed wrong integer type usage
- May 08, 2021 R0.02  Supported grayscale image. Separated configuration options
- Jun 11, 2021 R0.02a Some performance improvement
- Jul 01, 2021 R0.03  Added JD_FASTDECODE option. Some performance improvement
