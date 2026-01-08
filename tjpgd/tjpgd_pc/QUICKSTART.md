# 快速入门指南

## 第一步：安装 GCC

如果你还没有安装 GCC 编译器，请按照以下步骤安装：

1. 下载 TDM-GCC (推荐): https://jmeubank.github.io/tdm-gcc/
2. 运行安装程序
3. 选择默认安装选项
4. 确保勾选"添加到 PATH"选项

验证安装：
```
gcc --version
```

## 第二步：编译程序

### 使用批处理脚本（推荐）

双击 `build.bat` 文件

或在命令提示符中运行：
```
build.bat
```

### 使用 PowerShell

```powershell
.\build.ps1
```

### 使用 Make

```
make
```

## 第三步：准备测试图片

将一个 JPEG 图片复制到当前目录并命名为 `test.jpg`

## 第四步：运行测试

### 使用批处理脚本

双击 `test.bat` 或运行：
```
test.bat
```

### 使用 PowerShell

```powershell
.\test.ps1
```

### 手动运行

```
tjpgd_test.exe test.jpg output.ppm
```

## 第五步：查看结果

输出文件 `output.ppm` 可以使用以下工具打开：

- GIMP (免费): https://www.gimp.org/
- IrfanView (免费): https://www.irfanview.com/

## 故障排除

### 问题：找不到 gcc 命令

**解决方案：**
1. 确保已安装 MinGW 或 TDM-GCC
2. 重启命令提示符/PowerShell
3. 手动添加 GCC 到 PATH：
   - 右键"此电脑" -> "属性"
   - "高级系统设置" -> "环境变量"
   - 在"系统变量"中找到"Path"
   - 添加 GCC 的 bin 目录（例如：C:\TDM-GCC-64\bin）

### 问题：编译时出现错误

**解决方案：**
1. 确保所有文件都在同一目录
2. 检查文件名是否正确（区分大小写）
3. 尝试清理后重新编译：
   ```
   del *.o
   build.bat
   ```

### 问题：无法打开输出的 PPM 文件

**解决方案：**
1. 下载并安装 GIMP 或 IrfanView
2. 或者使用在线 PPM 查看器


