# TJpgDec Windows PC 版本

这是 TJpgDec (Tiny JPEG Decompressor) 的 Windows PC 测试版本。

## 功能

- 在 Windows PC 上解码 JPEG 图像
- 输出为 PPM (Portable Pixmap) 格式
- 支持灰度和彩色 JPEG 图像
- 命令行工具，易于测试和调试

## 环境要求

- Windows 操作系统
- GCC 编译器 (MinGW 或 TDM-GCC)

### 安装 GCC

如果你还没有安装 GCC，可以选择以下任一选项：

1. **MinGW**: http://www.mingw.org/
2. **TDM-GCC**: https://jmeubank.github.io/tdm-gcc/ (推荐，更容易安装)

安装后，确保将 GCC 的 bin 目录添加到系统 PATH 环境变量中。

## 文件说明

- `tjpgd.c` - TJpgDec 核心解码器实现
- `tjpgd.h` - TJpgDec 头文件
- `tjpgdcnf.h` - TJpgDec 配置文件
- `main.c` - Windows PC 测试主程序
- `build.bat` - Windows 批处理编译脚本
- `build.ps1` - PowerShell 编译脚本
- `test.bat` - Windows 批处理测试脚本
- `test.ps1` - PowerShell 测试脚本

## 编译方法

### 方法 1: 使用批处理脚本

双击运行 `build.bat` 或在命令提示符中执行：
```
build.bat
```

### 方法 2: 使用 PowerShell 脚本

在 PowerShell 中执行：
```powershell
.\build.ps1
```

### 方法 3: 手动编译

在命令提示符或 PowerShell 中执行：
```
gcc -o tjpgd_test.exe main.c tjpgd.c -O2 -Wall
```

## 使用方法

### 基本用法

```
tjpgd_test.exe input.jpg [output.ppm]
```

参数说明：
- `input.jpg` - 输入的 JPEG 文件（必需）
- `output.ppm` - 输出的 PPM 文件（可选，默认为 output.ppm）

### 示例

```
# 使用默认输出文件名
tjpgd_test.exe photo.jpg

# 指定输出文件名
tjpgd_test.exe photo.jpg decoded.ppm
```

## 运行测试

### 准备测试

1. 将一个 JPEG 图像文件重命名为 `test.jpg` 并放在当前目录
2. 运行测试脚本

### 方法 1: 使用批处理脚本

双击运行 `test.bat` 或在命令提示符中执行：
```
test.bat
```

### 方法 2: 使用 PowerShell 脚本

在 PowerShell 中执行：
```powershell
.\test.ps1
```

## 查看输出

生成的 PPM 文件可以使用以下工具查看：

- **GIMP** (免费): https://www.gimp.org/
- **IrfanView** (免费): https://www.irfanview.com/
- **XnView** (免费): https://www.xnview.com/
- 在线查看器: 搜索 "PPM viewer online"

## 配置选项

可以通过编辑 `tjpgdcnf.h` 文件来调整解码器的配置：

- `JD_SZBUF` - 输入缓冲区大小（默认 512 字节）
- `JD_FORMAT` - 输出像素格式
  - 0: RGB888 (24位/像素)
  - 1: RGB565 (16位/像素)
  - 2: 灰度 (8位/像素)
- `JD_USE_SCALE` - 缩放功能
  - 0: 禁用
  - 1: 启用
- `JD_TBLCLIP` - 使用查表法进行饱和运算（更快但增加 1KB 代码）
  - 0: 禁用
  - 1: 启用
- `JD_FASTDECODE` - 优化级别
  - 0: 基本优化（工作区 3100 字节）
  - 1: + 32位桶形移位器（工作区 3480 字节）
  - 2: + 霍夫曼解码表转换（工作区 9644 字节）

## 故障排除

### 编译错误

1. **找不到 gcc 命令**
   - 确保已安装 MinGW 或 TDM-GCC
   - 将 GCC 的 bin 目录添加到 PATH 环境变量

2. **链接错误**
   - 确保所有源文件（main.c, tjpgd.c）都在同一目录
   - 检查头文件（tjpgd.h, tjpgdcnf.h）是否存在

### 运行时错误

1. **无法打开输入文件**
   - 检查文件路径是否正确
   - 确保文件存在且有读取权限

2. **解码失败**
   - 确认输入文件是有效的 JPEG 文件
   - 尝试用其他工具打开该 JPEG 文件验证其完整性

## 许可证

TJpgDec 是自由软件，遵循 ChaN 的许可政策。详见源代码文件头部的版权声明。

## 版本历史

- R0.03 - 2021年7月 - 添加 JD_FASTDECODE 选项及性能改进
- R0.02 - 2021年5月 - 支持灰度图像
- R0.01 - 2011年10月 - 首次发布

## 作者

- TJpgDec 原作者: ChaN
- Windows PC 移植: 2026

## 相关链接

- TJpgDec 官方页面: http://elm-chan.org/fsw/tjpgd/00index.html







