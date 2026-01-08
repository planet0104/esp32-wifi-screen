# Rust与C版本差异分析文档

**日期**: 2026年1月9日  
**状态**: 🔴 BMP输出不一致，需继续修复  
**差异位置**: 字节294处 (Rust: 0xD5, C: 0xD6)

---

## 一、已完成的修复工作

### 1.1 Debug输出对齐 ✅
已在以下5个位置添加与C版本完全一致的debug输出：

**位置1**: `decode()` 函数entry clearing (huffman.rs:128-149)
```rust
if bits.bits_in_buffer > 0 && bits.bits_in_buffer < 32 {
    let mask = (1u32 << bits.bits_in_buffer) - 1;
    let old_buffer = bits.bit_buffer;
    bits.bit_buffer &= mask;
    if bits.call_count < 500 {
        println!("Rust huffext #{}: CLEAR wbit={}, old_wreg={:08X}, mask={:08X}, new_w={:08X}",
            bits.call_count, bits.bits_in_buffer, old_buffer, mask, bits.bit_buffer);
    }
}
```

**位置2**: `read_bits()` 函数entry clearing (huffman.rs:298-302)
```rust
if self.bits_in_buffer > 0 && self.bits_in_buffer < 32 {
    let mask = (1u32 << self.bits_in_buffer) - 1;
    self.bit_buffer &= mask;  // 匹配C的bitext()行为
}
```

**位置3**: `refill()` 函数读取字节 (huffman.rs:369-374)
```rust
if self.call_count < 500 {
    println!("Rust huffext #{}: READ byte={:02X}, dc={}",
        self.call_count, byte, self.data_counter);
}
```

**位置4**: `refill()` 函数添加字节后 (huffman.rs:387-392)
```rust
if self.call_count < 500 {
    println!("Rust huffext #{}: After adding byte: w={:08X}, wbit={}",
        self.call_count, self.bit_buffer, self.bits_in_buffer);
}
```

**位置5**: `refill()` 函数FF-escape处理 (huffman.rs:378-382)
```rust
if self.call_count < 500 {
    println!("Rust huffext #{}: 0xFF {:02X} -> escaped 0xFF",
        self.call_count, next_byte);
}
```

### 1.2 位清除逻辑对齐 ✅

**核心发现**: C语言采用"延迟清除"(lazy clearing)策略
- ✅ **仅在函数入口清除**: `huffext()`和`bitext()`均在入口执行`w = jd->wreg & mask`
- ✅ **refill时允许垃圾**: 高位可能包含未清除的垃圾数据
- ✅ **退出时保存垃圾**: `jd->wreg = w;` 可能保存含垃圾的值

**已修复**:
- ✅ 移除了`decode_slow()`的post-consumption clearing
- ✅ 移除了`read_bit()`的post-consumption clearing  
- ✅ 移除了`skip()`的post-skip clearing
- ✅ 在`decode()`入口添加clearing (匹配C的huffext)
- ✅ 在`read_bits()`入口添加clearing (匹配C的bitext)

---

## 二、当前发现的不一致

### 2.1 BMP文件差异
```
文件在字节 294 处首次不同
- Rust生成: 0xD5
- C生成:    0xD6
- 差值: 1 (可能是某个系数解码错误)
```

### 2.2 调用计数差异 🔴 **关键问题**

**C版本**:
```
huffext调用总数: 约468次 (1406行输出 / 3行每次)
#328处: dc=79
```

**Rust版本**:
```
huffext调用总数: 499次
#328处: dc=821
```

**分析**:
- ❌ **调用次数相近但dc值差异巨大** (821 vs 79)
- ❌ **dc计数逻辑可能存在根本性差异**
- ⚠️  C版本的dc可能是向上计数（从0开始）
- ⚠️  Rust版本的dc可能是向下计数（从总数递减）

### 2.3 Debug输出对比

**#328位置的old_wreg值** (✅ 一致):
```
C:    CLEAR wbit=9, old_wreg=000004D5, mask=000001FF, new_w=000000D5
Rust: CLEAR wbit=9, old_wreg=000004D5, mask=000001FF, new_w=000000D5
```

**#328位置的dc值** (❌ 不一致):
```
C:    READ byte=E1, dc=79
Rust: READ byte=E1, dc=821
```

### 2.4 关键序列对比

**#199-#201序列**:
```
C #199:    old_wreg=00005B46 → new_w=00001B46, dc=208
Rust #199: old_wreg=00005B46 → new_w=00001B46, dc=950 ❌

C #200:    old_wreg=000B46E3 → new_w=000346E3
Rust #200: old_wreg=000B46E3 → new_w=000346E3 ✅

C #201:    old_wreg=000006E3 → new_w=000006E3, dc=207
Rust #201: old_wreg=000006E3 → new_w=000006E3, dc=949 ❌
```

**结论**: 
- ✅ wreg位操作逻辑完全一致
- ❌ dc计数器逻辑存在差异

---

## 三、疑似问题根源

### 3.1 data_counter初始化差异

**需检查的代码位置**:

**C版本** (tjpgd.c):
```c
// prepare() 函数中
jd->dctr = ???  // 需查看初始值
```

**Rust版本** (huffman.rs):
```rust
pub fn new(input: F, size_hint: usize) -> Self {
    Self {
        // ...
        data_counter: size_hint,  // 可能初始化不正确
        // ...
    }
}
```

### 3.2 data_counter递减逻辑

**C版本** (tjpgd.c huffext函数):
```c
// 需查看是 jd->dctr++ 还是 jd->dctr--
```

**Rust版本** (huffman.rs refill函数):
```rust
// 当前实现 (line 364):
self.data_counter = self.data_counter.saturating_sub(1);
```

**疑问**:
- ⚠️  C版本是否也是递减？
- ⚠️  还是C版本是递增计数？
- ⚠️  初始值是0还是文件大小？

---

## 四、下一步调试计划

### 4.1 立即需要检查的内容

1. **查看C版本的prepare()函数**
   - `jd->dctr` 的初始值是什么？
   - 文件位置: `tjpgd_pc/tjpgd.c` prepare函数

2. **查看C版本的huffext()函数**
   - `jd->dctr` 是如何变化的？(++ or --)
   - 何时修改dctr？

3. **对比Rust的BitStream::new()**
   - `data_counter` 初始值是否正确？
   - 文件位置: `src/huffman.rs` line ~240

### 4.2 验证步骤

```bash
# 1. 查看C版本第一次huffext调用的dc值
./tjpgd_pc/tjpgd_test.exe test_images/test1.jpg tjpgd_pc/output.bmp 2>&1 | Select-String "huffext #" | Select-Object -First 5

# 2. 查看Rust版本第一次huffext调用的dc值
cargo run --example jpg2bmp test_images/test1.jpg test_output/test.bmp 2>&1 | Select-String "Rust huffext #" | Select-Object -First 5

# 3. 对比dc的变化趋势（递增/递减）
```

### 4.3 可能的修复方向

**假设1**: 如果C版本dc向上计数
```rust
// 修改 data_counter 初始化
data_counter: 0,  // 从0开始

// 修改 refill 中的递减为递增
self.data_counter = self.data_counter + 1;
```

**假设2**: 如果C版本dc初始值不同
```rust
// 可能需要从prepare()传入正确的初始值
// 而不是使用size_hint
```

---

## 五、相关文件清单

### 5.1 Rust源码
- `src/huffman.rs` (462 lines) - BitStream实现，包含decode/read_bits/refill
- `src/decoder.rs` (821 lines) - 调用read_bits获取系数
- `examples/jpg2bmp.rs` - 测试程序

### 5.2 C参考实现
- `tjpgd_pc/tjpgd.c` (1202 lines) - 完整实现
  - `prepare()` 函数 - 初始化（需重点查看dctr初始化）
  - `huffext()` 函数 (lines 336-442) - Huffman解码
  - `bitext()` 函数 (lines 448-528) - 位提取
- `tjpgd_pc/tjpgd.h` - 头文件定义

### 5.3 测试文件
- `test_images/test1.jpg` - 测试图片
- `test_output/rust_final2.bmp` - Rust生成（字节294=0xD5）
- `tjpgd_pc/output.bmp` - C生成（字节294=0xD6）

---

## 六、技术备注

### 6.1 JD_FASTDECODE模式
- 使用32位工作寄存器 (`jd->wreg` / `bit_buffer`)
- 8位refill操作
- 共享缓冲区（huffext和bitext）

### 6.2 位缓冲区管理策略
```
Entry:  w = jd->wreg & mask;  // 清除高位垃圾
Refill: w = (w << 8) | byte;  // 可能产生高位垃圾
Exit:   jd->wreg = w;         // 保存（可能含垃圾）
```

### 6.3 已排除的问题
- ✅ 位清除时机 - 已对齐C版本的lazy clearing
- ✅ Debug输出格式 - 已完全匹配
- ✅ refill逻辑 - FF-escape处理正确
- ✅ Entry clearing - decode()和read_bits()均已实现

---

## 七、编译和测试命令

```powershell
# 编译
cargo build --example jpg2bmp

# 运行Rust版本
cargo run --example jpg2bmp test_images/test1.jpg test_output/test.bmp 2>$null

# 运行C版本
./tjpgd_pc/tjpgd_test.exe test_images/test1.jpg tjpgd_pc/output.bmp 2>$null

# 比较BMP文件
$rust = [System.IO.File]::ReadAllBytes("test_output/test.bmp")
$c = [System.IO.File]::ReadAllBytes("tjpgd_pc/output.bmp")
for ($i = 0; $i -lt $rust.Length; $i++) {
    if ($rust[$i] -ne $c[$i]) {
        Write-Host "字节 $i 不同: Rust=0x$($rust[$i].ToString('X2')), C=0x$($c[$i].ToString('X2'))"
        break
    }
}

# 提取关键debug输出
cargo run --example jpg2bmp test_images/test1.jpg test_output/test.bmp 2>&1 | Select-String "huffext #(1|2|3|199|200|328)" | Select-Object -First 20
```

---

## 八、待解决问题清单

- [ ] **P0 - 确认dc计数器初始值和递增/递减逻辑**
- [ ] **P0 - 查看C版本prepare()中dctr的初始化**
- [ ] **P0 - 查看C版本huffext()中dctr的更新方式**
- [ ] **P1 - 对比第一次huffext调用时的dc值**
- [ ] **P1 - 修正Rust的data_counter逻辑**
- [ ] **P2 - 验证修复后BMP文件是否一致**
- [ ] **P2 - 运行完整测试套件**
- [ ] **P3 - 清理debug输出或添加feature flag**

---

**最后更新**: 2026年1月9日 01:00
**下次任务**: 检查C版本的dctr初始化和更新逻辑
