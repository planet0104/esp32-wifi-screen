# ESP32 prepare() 卡死问题分析

## ⚠️ 重要更新

**在 PC 上测试表明 Rust 代码工作正常！**
- ✅ examples/basic.rs 正常运行
- ✅ examples/jpg2bmp.rs 能正确生成 BMP 文件
- ✅ 与 C 代码输出对比一致

**因此问题不在于 Rust 实现本身，而在于：**
1. **API 使用方式** - ESP32 代码可能没有正确调用 API
2. **内存分配策略** - ESP32 上的缓冲区分配可能有问题
3. **数据加载方式** - ESP32 可能没有正确加载完整的 JPEG 数据

## 问题诊断

经过对比 C 代码和 Rust 代码，发现 **Rust 版本的 `prepare()` 方法与 C 版本的 `jd_prepare()` 函数在设计理念上有根本性差异**。这种差异本身不是问题（PC上工作正常），但在 ESP32 上使用时需要特别注意。

## 关键差异对比

### C 代码 (`jd_prepare`) 的设计：

```c
JRESULT jd_prepare(
    JDEC* jd,
    size_t (*infunc)(JDEC*, uint8_t*, size_t),  // 流式输入回调
    void* pool,                                  // 内存池指针
    size_t sz_pool,                             // 内存池大小
    void* dev                                   // 设备标识符
)
```

**C 代码在 prepare 阶段做的事情：**

1. ✅ **清空 JDEC 结构** - `memset(jd, 0, sizeof(JDEC))`
2. ✅ **设置内存池** - 保存 `pool` 和 `sz_pool`
3. ✅ **分配输入缓冲区** - `alloc_pool(jd, JD_SZBUF)` (512字节)
4. ✅ **流式读取和解析头部** - 通过 `infunc` 回调按需读取数据
5. ✅ **在内存池中分配量化表** - 每个表 256 字节 (64 * sizeof(int32_t))
6. ✅ **在内存池中分配霍夫曼表** - bits, code, data 数组
7. ✅ **在 SOS 段完成后分配工作缓冲区** - `alloc_pool(jd, len)` 
8. ✅ **分配 MCU 缓冲区** - `alloc_pool(jd, (n + 2) * 64 * sizeof(jd_yuv_t))`
9. ✅ **预读取并对齐输入流** - 准备好解压缩所需的数据

**内存分配顺序 (在同一个内存池中)：**
```
[输入缓冲区 512B] -> [量化表] -> [霍夫曼表] -> [工作缓冲区] -> [MCU缓冲区]
<---------------------------- 总计约 9644 字节 (FASTDECODE=2) ---------------------------->
```

### Rust 代码 (`prepare`) 的设计：

```rust
pub fn prepare(&mut self, data: &[u8]) -> Result<()>
```

**Rust 代码在 prepare 阶段做的事情：**

1. ✅ **解析 SOI 标记**
2. ✅ **解析各个段** (SOF, DHT, DQT, DRI, SOS)
3. ✅ **将量化表和霍夫曼表存储在堆上** (`Box`, `HuffmanTable`)
4. ❌ **没有分配工作缓冲区**
5. ❌ **没有分配 MCU 缓冲区**
6. ❌ **没有输入流管理**
7. ❌ **需要完整的 JPEG 数据在内存中**

## 问题根源

### 1. 内存分配策略完全不同

**C 代码：**
- 使用用户提供的内存池（通常在栈上或静态分配）
- 所有内存在 `jd_prepare` 时从内存池分配
- 内存对齐到 4 字节边界：`ndata = (ndata + 3) & ~3`
- 总内存使用可预测且固定（~9644 字节）

**Rust 代码：**
- 量化表和霍夫曼表使用堆分配 (`Box`, `Vec`)
- 工作缓冲区和 MCU 缓冲区在 `decompress` 时才分配
- 在 ESP32 上，大量小堆分配可能导致内存碎片
- 没有内存池的概念

### 2. 流式输入 vs 完整数据

**C 代码：**
- 通过 `infunc` 回调函数流式读取数据
- 只需要 512 字节的输入缓冲区
- 适合从 SD 卡、串口等流式源读取

**Rust 代码：**
- 需要完整的 JPEG 数据在内存中：`data: &[u8]`
- 在 ESP32 上，完整的 JPEG 图片可能很大（几十 KB 到几百 KB）
- 可能导致内存不足

### 3. 执行流程差异

**C 代码典型流程：**
```c
uint8_t work[9644];  // 在栈上或静态分配
JDEC jdec;

// prepare 时分配所有内部结构
jd_prepare(&jdec, in_func, work, sizeof(work), &dev);

// decomp 时使用已分配的缓冲区
jd_decomp(&jdec, out_func, 0);
```

**Rust 代码典型流程（当前实现）：**
```rust
let jpeg_data = read_entire_file();  // ⚠️ 需要完整数据在内存中
let mut decoder = JpegDecoder::new();

// 只解析头部，不分配缓冲区
decoder.prepare(&jpeg_data)?;

// 在这里才分配缓冲区（在堆上）
decoder.decompress(&jpeg_data, 0, callback)?;
```

## 在 ESP32 上导致卡死的可能原因

### 1. **API 使用不正确** ⚠️ 最可能的原因

ESP32 代码可能这样调用（错误）：
```rust
// ❌ 错误：只调用了 prepare，没有调用 decompress_with_buffers
decoder.prepare(&jpeg_data)?;
// ... 然后直接尝试使用解码器？
```

**正确的调用方式**（参考 examples/basic.rs）：
```rust
// ✅ 正确流程
let mut decoder = JpegDecoder::new();

// 1. 准备（解析头部）
decoder.prepare(&jpeg_data)?;

// 2. 分配缓冲区
let mcu_size = decoder.mcu_buffer_size();
let work_size = decoder.work_buffer_size();
let mut mcu_buffer = vec![0i16; mcu_size];
let mut work_buffer = vec![0u8; work_size];

// 3. 解压
decoder.decompress_with_buffers(
    &jpeg_data, 
    0,
    &mut mcu_buffer,
    &mut work_buffer,
    &mut |decoder, bitmap, rect| {
        // 输出回调
        Ok(true)
    }
)?;
```

### 2. **完整 JPEG 数据加载失败**
   - ESP32 RAM 有限（通常 320KB）
   - 加载大图片到内存可能失败或导致 OOM
   - 需要检查 `jpeg_data` 是否完整加载

### 3. **缓冲区分配失败**
   - 在 ESP32 上分配 Vec 可能失败（OOM）
   - 应该使用静态缓冲区或预分配内存池

### 4. **栈溢出**
   - ESP32 栈通常只有 4KB-8KB
   - 如果在栈上分配大数组会溢出

## 解决方案

### 方案 1：为 Rust 添加流式输入和内存池支持（推荐）

修改 `JpegDecoder` 以匹配 C 代码的设计：

```rust
pub struct JpegDecoder<'pool> {
    // 现有字段...
    
    // 新增字段（匹配 C 结构）
    pool: Option<&'pool mut [u8]>,     // 内存池
    pool_offset: usize,                 // 当前分配偏移
    input_buffer: Option<&'pool mut [u8]>, // 输入缓冲区（从pool分配）
    work_buffer_ptr: Option<&'pool mut [u8]>, // 工作缓冲区（从pool分配）
    mcu_buffer_ptr: Option<&'pool mut [i16]>,  // MCU缓冲区（从pool分配）
}

impl<'pool> JpegDecoder<'pool> {
    /// 创建带内存池的解码器（匹配 C 的 jd_prepare）
    pub fn prepare_with_pool<F>(
        &mut self,
        pool: &'pool mut [u8],
        input_callback: F,
    ) -> Result<()>
    where
        F: FnMut(&mut [u8]) -> usize,
    {
        // 1. 初始化内存池
        self.pool = Some(pool);
        self.pool_offset = 0;
        
        // 2. 从内存池分配输入缓冲区 (512字节)
        let input_buf = self.alloc_from_pool(512)?;
        self.input_buffer = Some(input_buf);
        
        // 3. 流式解析头部（使用 input_callback 读取数据）
        self.parse_headers_streaming(input_callback)?;
        
        // 4. 分配量化表（在内存池中）
        self.allocate_qtables_in_pool()?;
        
        // 5. 分配霍夫曼表（在内存池中）
        self.allocate_huffman_in_pool()?;
        
        // 6. 在 SOS 后分配工作缓冲区和 MCU 缓冲区
        let work_size = self.calculate_work_buffer_size();
        let work_buf = self.alloc_from_pool(work_size)?;
        self.work_buffer_ptr = Some(work_buf);
        
        let mcu_size = self.calculate_mcu_buffer_size();
        let mcu_buf = self.alloc_from_pool_i16(mcu_size)?;
        self.mcu_buffer_ptr = Some(mcu_buf);
        
        Ok(())
    }
    
    /// 从内存池分配（4字节对齐，匹配 C 代码）
    fn alloc_from_pool(&mut self, size: usize) -> Result<&'pool mut [u8]> {
        // 对齐到 4 字节边界
        let aligned_size = (size + 3) & !3;
        
        let pool = self.pool.as_mut().ok_or(Error::InsufficientMemory)?;
        
        if self.pool_offset + aligned_size > pool.len() {
            return Err(Error::InsufficientMemory);
        }
        
        let start = self.pool_offset;
        let end = start + size;
        self.pool_offset += aligned_size;
        
        // 使用 unsafe 来分割可变引用（这是安全的，因为我们确保不重叠）
        unsafe {
            let ptr = pool.as_mut_ptr().add(start);
            Ok(core::slice::from_raw_parts_mut(ptr, size))
        }
    }
}
```

### 方案 2：添加验证和更清晰的错误信息

如果暂时不实现流式输入，至少要确保：

```rust
pub fn prepare(&mut self, data: &[u8]) -> Result<()> {
    // 验证数据大小
    if data.len() < 1024 {  // JPEG 头部至少需要几百字节
        return Err(Error::Input);
    }
    
    // 解析头部...
    
    // 在 SOS 后验证所有必需的表都已加载
    self.validate_tables()?;
    
    Ok(())
}

fn validate_tables(&self) -> Result<()> {
    // 检查所有必需的霍夫曼表
    for i in 0..self.num_components as usize {
        let table_id = if i == 0 { 0 } else { 1 };
        if self.huff_dc[table_id].is_none() {
            return Err(Error::FormatError);  // 添加更详细的错误信息
        }
        if self.huff_ac[table_id].is_none() {
            return Err(Error::FormatError);
        }
    }
    
    // 检查量化表
    for i in 0..self.num_components as usize {
        if self.qtables[self.qtable_ids[i] as usize].is_none() {
            return Err(Error::FormatError);
        }
    }
    
    Ok(())
}
```

### 方案 3：使用静态分配的工作区（ESP32 友好）

```rust
// 在应用代码中
static mut JPEG_WORK_AREA: [u8; 9644] = [0; 9644];

unsafe {
    let mut decoder = JpegDecoder::new();
    decoder.prepare_with_pool(&mut JPEG_WORK_AREA, |buf| {
        // 从 SD 卡或其他源读取数据到 buf
        read_from_source(buf)
    })?;
}
```

## 内存使用对比

### C 代码内存布局
```
栈/静态:
  └─ work[9644]               9644 字节
      ├─ input_buffer[512]     512 字节
      ├─ qtables               ~1024 字节
      ├─ huffman_tables        ~2000 字节
      ├─ work_buffer           ~3000 字节
      └─ mcu_buffer            ~3000 字节
总计: 约 9644 字节（可预测、连续）
```

### Rust 代码当前内存布局
```
堆:
  ├─ qtables (Box)             ~1024 字节
  ├─ huffman_tables (Box)      ~2000 字节
  ├─ mcu_buffer (Vec)          ~3000 字节
  └─ work_buffer (Vec)         ~3000 字节

栈:
  └─ jpeg_data (Vec)           可能几十到几百 KB！
总计: 可能超过 100KB，且分散（内存碎片风险）
```

## 推荐的修复优先级

1. **立即修复**：实现内存池分配机制（方案 1）
2. **短期改进**：添加流式输入支持
3. **长期优化**：完全匹配 C 代码的内存布局和执行流程

## 测试建议

修复后，使用以下测试验证：

```rust
#[test]
fn test_memory_pool_allocation() {
    let mut work_area = [0u8; 9644];
    let mut decoder = JpegDecoder::new();
    
    // 应该成功分配所有内部结构
    decoder.prepare_with_pool(&mut work_area, input_callback).unwrap();
    
    // 验证内存池使用量
    assert!(decoder.pool_offset <= 9644);
}
```

## 总结

当前 Rust 代码的设计是合理的且在 PC 上工作正常：

- ✅ `prepare()`: 解析头部，提取图片信息
- ✅ `mcu_buffer_size()` / `work_buffer_size()`: 计算所需缓冲区大小
- ✅ `decompress_with_buffers()`: 执行解压，使用外部提供的缓冲区

**ESP32 上卡死的真正原因需要检查：**

### 检查清单

1. **ESP32 代码是否完整调用了 API？**
   ```rust
   // 检查是否有这个完整流程
   decoder.prepare()?;
   // ... 分配缓冲区 ...
   decoder.decompress_with_buffers()?; // ← 是否调用了这个？
   ```

2. **JPEG 数据是否完整加载？**
   ```rust
   println!("JPEG data size: {}", jpeg_data.len());
   // 应该看到完整的文件大小
   ```

3. **缓冲区分配是否成功？**
   ```rust
   let mcu_size = decoder.mcu_buffer_size();
   println!("MCU size: {}", mcu_size);
   let mut mcu_buf = vec![0i16; mcu_size]; // ← 这里可能 OOM
   println!("MCU buffer allocated");
   ```

4. **是否启用了 alloc-buffers feature？**
   - 如果使用 `decompress()` 而不是 `decompress_with_buffers()`
   - 需要启用 feature: `tjpgd = { features = ["alloc-buffers"] }`

### ESP32 推荐使用方式

```rust
// 在 ESP32 上使用静态缓冲区（避免堆分配）
static mut MCU_BUFFER: [i16; 256] = [0; 256]; // 根据图片调整大小
static mut WORK_BUFFER: [u8; 256] = [0; 256];

unsafe {
    decoder.decompress_with_buffers(
        &jpeg_data,
        0,
        &mut MCU_BUFFER,
        &mut WORK_BUFFER,
        &mut |decoder, bitmap, rect| {
            // 输出到显示器
            Ok(true)
        }
    )?;
}
```

### 下一步调试

**请在 ESP32 代码中添加调试输出：**

```rust
println!("Step 1: Creating decoder");
let mut decoder = JpegDecoder::new();

println!("Step 2: Loading JPEG data ({} bytes)", jpeg_data.len());

println!("Step 3: Calling prepare()");
decoder.prepare(&jpeg_data)?;
println!("Step 3 OK - Image: {}x{}", decoder.width(), decoder.height());

println!("Step 4: Calculating buffer sizes");
let mcu_size = decoder.mcu_buffer_size();
let work_size = decoder.work_buffer_size();
println!("Step 4 OK - MCU: {}, Work: {}", mcu_size, work_size);

println!("Step 5: Allocating buffers");
let mut mcu_buf = vec![0i16; mcu_size];
let mut work_buf = vec![0u8; work_size];
println!("Step 5 OK");

println!("Step 6: Decompressing");
decoder.decompress_with_buffers(&jpeg_data, 0, &mut mcu_buf, &mut work_buf, callback)?;
println!("Step 6 OK - DONE!");
```

**在哪一步卡住，就能确定真正的问题所在。**
