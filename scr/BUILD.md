# 构建说明（Windows / PowerShell 支持）

本文件汇总了在 Windows（PowerShell）上为本仓库 `scr` 子项目构建、烧录、调试所需的依赖、环境设置、常用脚本和常见问题排错步骤。

**适用范围**：本说明针对使用 ESP-IDF v5.x、Rust `esp-idf-sys` 绑定，并以 `xtensa-esp32s3-espidf` 目标构建的场景。请根据需要调整 MCU（`esp32s2` / `esp32s3`）和 `sdkconfig.defaults` 文件。

---

## 目录
- 先决依赖
- 环境准备
- 常用脚本（构建 / 清理 / 烧录 / 监视）
- 日志收集与详细构建
- 常见问题与排错步骤
- 额外说明（测试测速脚本）

---

## 先决依赖
- Python 3.8+（用于 esp-idf 的工具管理器和部分脚本）
- Git
- Rust toolchain（包含为 `esp` 交叉构建配置的 toolchain）
  - 推荐：已通过 `rustup` 安装并配置 `esp` toolchain（项目使用的工具链在 `rust-toolchain.toml` 中指定）
- ESP-IDF（本仓库示例使用 esp-idf v5.3.4，示例路径：`C:\esp\.embuild\espressif\esp-idf\v5.3.4`）
- esp-idf 工具集合（xtensa 编译器、cmake、ninja、openocd 等），通过 esp-idf 自带的 `idf_tools.py` 安装或 espup/esp-idf 工具管理器安装

---

## 环境准备（PowerShell）
1. 打开 PowerShell，定位到项目 `scr` 目录：
```powershell
cd C:\Users\planet\Documents\GitHub\esp32-wifi-screen\scr
```

2. 载入 ESP-IDF 环境导出脚本（按你本机路径调整）：
```powershell
& 'C:\esp\.embuild\espressif\esp-idf\v5.3.4\export.ps1'
```
此脚本会设置 `IDF_PATH`、将 esp-idf 工具路径加入 `PATH`，并导出一些辅助环境变量。若首次运行提示缺少工具，请按输出提示运行 `idf_tools.py install`（见下节）。

3. 如需显式指向工具安装目录（espup 安装位置等），设置：
```powershell
$env:ESP_IDF_TOOLS_INSTALL_DIR = 'custom:C:\Users\planet\.espressif'
```
4. 设置项目默认 sdkconfig 文件（可选，针对 S3）
```powershell
$env:ESP_IDF_SDKCONFIG_DEFAULTS = 'C:\Users\planet\Documents\GitHub\esp32-wifi-screen\scr\sdkconfig.defaults.esp32s3'
```
5. 设置 MCU（可选，esp-idf-sys 构建脚本会读取）
```powershell
$env:MCU = 'esp32s3'
```
6. 启用 Rust 回溯（用于调试构建脚本错误）：
```powershell
$env:RUST_BACKTRACE = '1'
```

---

## 安装 esp-idf 工具（如果 export.ps1 提示缺失）
在 PowerShell 中（示例）：
```powershell
python C:\esp\.embuild\espressif\esp-idf\v5.3.4\tools\idf_tools.py install
```
安装完成后重复 `export.ps1`。

---

## 常用脚本和构建命令
请在 `scr` 根目录下执行以下命令。

- 简单开发构建（可指定特性）：
```powershell
cargo build --target xtensa-esp32s3-espidf --features "esp32s3,experimental"
```

- 详细并保存日志（推荐用于调试构建脚本问题）：
```powershell
cargo build -vv --release --target xtensa-esp32s3-espidf --no-default-features --features "esp32s3,experimental" 2>&1 | Tee-Object -FilePath build-output.log
```
说明：`-vv` 会显示构建脚本的详细输出；`Tee-Object` 将输出同时写入文件。

- 清理：
```powershell
cargo clean
```

- 烧录（仓库已经内置脚本，按硬件和目标选择）：
```powershell
# 示例脚本位于仓库根或 scr 下，按实际使用：
.\flash_esp32s3.ps1
```

- 串口监视：
```powershell
.\monitor.ps1
# 或使用 idf.py monitor / esptool 等工具
```

---

## 日志收集与定位构建脚本错误
- 生成完整构建日志（示例）：
```powershell
cargo build -vv --release --target xtensa-esp32s3-espidf --no-default-features --features "esp32s3,experimental" 2>&1 | Tee-Object -FilePath build-output.log
```
- 出错时搜索关键字（示例）：
  - `esp-idf-sys`（crate 名）
  - `Error: Matching variant not found`（envy 反序列化错误）
  - `Stack backtrace`（获取详细回溯）

---

## 常见问题与排错步骤
1. export.ps1 报错 "tool XXX has no installed versions"
   - 运行：
```powershell
python C:\esp\.embuild\espressif\esp-idf\v5.3.4\tools\idf_tools.py install
& 'C:\esp\.embuild\espressif\esp-idf\v5.3.4\export.ps1'
```

2. 构建失败并出现 `esp-idf-sys` build script 报错：`Error: Matching variant not found`
   - 该错误通常来自 `esp-idf-sys` 的构建脚本使用 `envy` 从环境变量反序列化一个 enum/variant，但某个 env 值格式不符合预期。
   - 检查并打印相关 env：
```powershell
Get-ChildItem env:ESP_IDF_TOOLS_INSTALL_DIR,ESP_IDF_SDKCONFIG_DEFAULTS,ESP_IDF_SDKCONFIG,MCU
```
   - 若 `ESP_IDF_TOOLS_INSTALL_DIR` 格式不对，可尝试删除或设为 `custom:<path>`：
```powershell
Remove-Item env:ESP_IDF_TOOLS_INSTALL_DIR
# 或
$env:ESP_IDF_TOOLS_INSTALL_DIR = 'custom:C:\Users\planet\.espressif'
```
   - 确保 `MCU` 为期望的字符串（例如 `esp32s3`）
   - 重新运行带 `-vv` 的构建并查看 `build-output.log` 中 `esp-idf-sys` 的回溯行，定位失败点（我可以代为解析日志并给出精确改法）。

3. 无法找到 `sdkconfig.defaults` 文件
   - 检查 `$env:ESP_IDF_SDKCONFIG_DEFAULTS` 是否指向正确路径。

4. 链接/交叉编译器找不到
   - 确保 esp-idf 的工具（xtensa-esp-elf 等）安装成功并在 `PATH` 中（`export.ps1` 通常会处理）。

---

## 测速脚本与说明
- 仓库示例 `esp32_s3_seritalTest` 提供了主机端脚本用于测速（`scr/esp32_s3_seritalTest/tools/serial_speed_test.js` 或类似位置）。
- 我们已经把 S3 的测速帧（`SPDTEST1`, `SPDEND!!` 等）集成进 `scr/src/usb_reader.rs`；构建并烧录 S3 固件后，可使用示例 host 脚本进行速度测试。

---

## 联系我/我能代劳的事
- 我可以替你解析 `build-output.log`，定位 `envy` 报错的确切 env key/value 并给出修复命令。
- 我也可以直接在本仓库内添加一个小脚本（例如 `scripts/validate_env.ps1`）来自动打印并校验关键信息。

---

文件创建于仓库 `scr` 下。若要我现在生成 `scripts/validate_env.ps1` 并测试，请回复“生成验证脚本”。
