## 安装步骤

### 方式一：使用 espup 安装（推荐）

1. **安装 espup 工具**
   ```powershell
   cargo +stable install espup
   ```

2. **安装 ldproxy 链接器**
   ```powershell
   # 临时取消项目的工具链覆盖
   rustup override unset
   
   # 安装 ldproxy
   cargo install ldproxy
   
   # 恢复项目的 esp 工具链
   rustup override set esp
   ```

3. **安装 ESP Rust 工具链**
   ```powershell
   espup install
   ```
   注意：如果安装过程卡住超过5分钟，按 Ctrl+C 中断即可，工具链已经安装成功。

4. **配置项目**
   
   a) **设置超短路径编译目录**（**必需**，解决ESP-IDF路径过长错误）
   
   编辑 `.cargo/config.toml`：
   ```toml
   [build]
   target = "xtensa-esp32s2-espidf"
   target-dir = "C:/t"  # 使用超短路径，ESP-IDF 要求总路径不超过特定长度
   ```
   
   创建目录：
   ```powershell
   New-Item -ItemType Directory -Path "C:/t" -Force
   ```

   b) **配置 sdkconfig 路径**
   
   在 `.cargo/config.toml` 的 `[env]` 部分添加：
   ```toml
   [env]
   MCU="esp32s2"
   ESP_IDF_VERSION = "v5.3.2"
   ESP_IDF_SDKCONFIG_DEFAULTS = { value = "sdkconfig.defaults", relative = true }
   ```

   c) **配置库文件路径**
   
   在 `.cargo/config.toml` 的 `rustflags` 中，将库路径设置为项目的实际路径：
   ```toml
   [target.xtensa-esp32s2-espidf]
   rustflags = [
       "--cfg",  "espidf_time64",
       "-L", "C:/Users/你的用户名/Documents/GitHub/esp32-wifi-screen/scr/library",
       "-l", "tjpgd"
   ]
   ```
   
   > **注意**：将路径中的"你的用户名"替换为实际的用户名，或使用相对路径

   **注意**：由于本项目已经使用纯 Rust 实现的 `tjpg-decoder` 库，不再需要链接 C 库。可以移除 rustflags 中的 `-L` 和 `-l` 参数：
   ```toml
   [target.xtensa-esp32s2-espidf]
   rustflags = [
       "--cfg",  "espidf_time64",
   ]
   ```

5. **安装 espflash 工具**（用于烧写固件）
   ```powershell
   cargo install espflash
   ```

6. **每次编译前设置环境变量**
   ```powershell
   . .\espup_env.ps1
   ```

6. **编译和烧写**
   ```powershell
   # 方式 1：使用 build.ps1 脚本编译并生成固件文件
   . .\build.ps1
   
   # 方式 2：分步执行
   # 编译
   cargo build --release
   
   # 生成固件文件
   espflash save-image --chip esp32s2 --partition-table partitions.csv target/xtensa-esp32s2-espidf/release/esp32-wifi-screen esp32-wifi-screen.bin
   
   # 烧写固件到设备（需要修改串口名称）
   espflash flash --monitor target/xtensa-esp32s2-espidf/release/esp32-wifi-screen
   
   # 或使用 flash.ps1 脚本烧写
   .\flash.ps1
   ```
   
   **espflash 常用命令**：
   ```powershell
   # 查看可用串口
   espflash board-info
   
   # 烧写并监控串口输出
   espflash flash --monitor target/xtensa-esp32s2-espidf/release/esp32-wifi-screen
   
   # 指定串口烧写
   espflash flash --port COM3 target/xtensa-esp32s2-espidf/release/esp32-wifi-screen
   
   # 擦除闪存
   espflash erase-flash
   ```
   
   **首次编译注意**：
   - 首次执行 `cargo build` 会从 GitHub 克隆相关库到 `C:/esp/.embuild` 文件夹
   - **必须开启 Clash 全局代理**，否则会克隆失败（错误：`Recv failure: Connection was reset`）
   - 如果遇到 `could not find Cargo.toml` 错误，请注释掉 `.cargo/config.toml` 中的 `target-dir` 配置
   - 首次编译耗时较久（10-30分钟），请耐心等待

### 方式二：使用 ESP-IDF 官方安装器

1. **下载并安装 ESP-IDF 5.3**
   - 官网：https://dl.espressif.com.cn/dl/esp-idf/index.html
   - 可选择安装 Rust 相关组件（需要全局代理）
   - 或不选择 Rust 组件，手动安装（推荐方式一）

2. **使用 ESP-IDF 控制台**
   - 打开 ESP-IDF 5.3 CMD 控制台
   - 进入项目目录
   - 执行 `code .` 打开项目

### 注意事项

- **纯 Rust 实现**：本项目已使用纯 Rust 的 `tjpg-decoder` 库，不再需要 C 库链接
- **路径长度限制**：ESP-IDF 要求编译输出路径极短，**必须**使用 `target-dir = "C:/t"` 等超短路径
- **网络代理**：首次编译必须开启全局代理访问 GitHub
- **sdkconfig 配置**：`ESP_IDF_SDKCONFIG_DEFAULTS` 需要正确指向 `sdkconfig.defaults` 文件
- **串口配置**：烧写时可通过 `espflash board-info` 查看可用串口

### 构建脚本说明

所有构建脚本都支持自动从 `.cargo/config.toml` 读取 `target-dir` 配置，无需手动修改路径。

- **build.ps1**：编译项目并生成 `.bin` 固件文件
  - 自动检测 target-dir 配置
  - 编译 release 版本
  - 生成 `esp32-wifi-screen.bin` 固件文件
  
- **flash.ps1**：编译、生成固件并烧写到设备
  - 自动检测 target-dir 配置
  - 自动检测可用串口（COM3, COM10, COM6）
  - 使用 esptool 烧写固件
  - 烧写完成后自动启动串口监控
  
- **monitor.ps1**：监控串口输出
  - 用法：`.\monitor.ps1 -p COM3`

**推荐工作流**：
```powershell
# 仅编译和生成固件文件
. .\build.ps1

# 或 编译 + 烧写 + 监控（一步到位）
. .\flash.ps1
```