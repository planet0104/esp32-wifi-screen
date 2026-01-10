# ESP32-S2 专用烧录脚本

param(
    [string]$Port = $null
)

Write-Host "正在为 ESP32-S2 构建和烧录..."

$chip = "esp32s2"
$target = "xtensa-esp32s2-espidf"
$feature = "esp32s2"

# 设置环境变量 - 使用绝对路径
$env:MCU = "esp32s2"
$projectRoot = $PSScriptRoot
$env:ESP_IDF_SDKCONFIG_DEFAULTS = "$projectRoot\sdkconfig.defaults.esp32s2"
$env:ESP_IDF_VERSION = "v5.3.2"

Write-Host "环境变量设置:"
Write-Host "  MCU: $env:MCU"
Write-Host "  ESP_IDF_SDKCONFIG_DEFAULTS: $env:ESP_IDF_SDKCONFIG_DEFAULTS"
Write-Host "  ESP_IDF_VERSION: $env:ESP_IDF_VERSION"

# 使用 --target 和 --features 参数构建
cargo build --release --target $target --no-default-features --features "$feature,experimental"

if ($LASTEXITCODE -ne 0) {
    Write-Host "构建失败!" -ForegroundColor Red
    exit 1
}

# 从 .cargo/config.toml 读取目标路径并转换为绝对路径
$configContent = Get-Content ".cargo\config.toml" -Raw
if ($configContent -match 'target-dir\s*=\s*"([^"]+)"') {
    $targetDir = $matches[1].Replace('/', '\\')
    # 如果不是绝对路径，则转换为绝对路径
    if (-not [System.IO.Path]::IsPathRooted($targetDir)) {
        $targetDir = Join-Path $projectRoot $targetDir
    }
    Write-Host "Target directory: $targetDir" -ForegroundColor Cyan
} else {
    $targetDir = Join-Path $projectRoot "target"
    Write-Host "Using default target directory: $targetDir" -ForegroundColor Yellow
}

$binaryPath = "$targetDir\\$target\\release\\esp32-wifi-screen"
$binOutputPath = "esp32-wifi-screen-$chip-merged.bin"

if (-not (Test-Path $binaryPath)) {
    Write-Host "Error: Binary not found at $binaryPath" -ForegroundColor Red
    exit 1
}

# 生成完整合并镜像（包含bootloader、partition和应用）
Write-Host "正在生成完整镜像: $binOutputPath" -ForegroundColor Cyan
espflash save-image --chip $chip --merge --partition-table partitions.csv $binaryPath $binOutputPath

if ($LASTEXITCODE -ne 0) {
    Write-Host "生成镜像失败!" -ForegroundColor Red
    exit 1
}

Write-Host "镜像生成成功!" -ForegroundColor Green

# 合并镜像从0x0地址开始包含所有内容，无需单独烧录bootloader和partition
# pip install esptool -i https://pypi.tuna.tsinghua.edu.cn/simple

$tool = ".\esptool.exe"

# 列出并记录可用串口，便于调试
$availablePorts = [System.IO.Ports.SerialPort]::GetPortNames()
Write-Host "Detected serial ports: $($availablePorts -join ', ')" -ForegroundColor Cyan

# 选择端口逻辑：优先使用用户提供的 -Port 参数，其次优先使用 COM6，再回退到第一个可用端口
$selectPort = $null
if ($Port -and $availablePorts -contains $Port) {
    Write-Host "Using user-specified port: $Port" -ForegroundColor Green
    $selectPort = $Port
} elseif ($availablePorts -contains 'COM6') {
    Write-Host "Preferring COM6" -ForegroundColor Green
    $selectPort = 'COM6'
} elseif ($availablePorts.Count -gt 0) {
    Write-Host "No COM6 found; using first available port: $($availablePorts[0])" -ForegroundColor Yellow
    $selectPort = $availablePorts[0]
} else {
    Write-Host "No serial ports detected. Please connect device and try again." -ForegroundColor Red
    exit 1
}

Write-Host "Using port: $selectPort" -ForegroundColor Cyan
Write-Host "烧录完整镜像（包含bootloader、partition和应用）..." -ForegroundColor Cyan

& $tool -p $selectPort --before default_reset --after hard_reset --chip $chip write_flash --flash_mode dio --flash_size 4MB 0x0 $binOutputPath

if ($LASTEXITCODE -eq 0) {
    Write-Host "烧录完成!" -ForegroundColor Green
    Write-Host "正在启动串口监视器..." -ForegroundColor Cyan
    & ".\monitor.ps1" -p $selectPort
} else {
    Write-Host "烧录失败!" -ForegroundColor Red
    exit 1
}
