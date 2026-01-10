# ESP32-S2 专用烧录脚本

Write-Host "正在为 ESP32-S2 构建和烧录..."

$chip = "esp32s2"
$target = "xtensa-esp32s2-espidf"
$feature = "esp32s2"

# 设置环境变量 - 使用绝对路径
$env:MCU = "esp32s2"
$projectRoot = $PSScriptRoot
$env:ESP_IDF_SDKCONFIG_DEFAULTS = "$projectRoot\sdkconfig.defaults.esp32s2"
$env:ESP_IDF_VERSION = "v5.3.2"

# 使用 --target 和 --features 参数构建
cargo build --release --target $target --no-default-features --features "$feature,experimental"

if ($LASTEXITCODE -ne 0) {
    Write-Host "构建失败!" -ForegroundColor Red
    exit 1
}

# 从 .cargo/config.toml 读取目标路径
$configContent = Get-Content ".cargo\config.toml" -Raw
if ($configContent -match 'target-dir\s*=\s*"([^"]+)"') {
    $targetDir = $matches[1].Replace('/', '\\')
    Write-Host "Target directory: $targetDir"
} else {
    $targetDir = "target"
    Write-Host "Using default target directory: $targetDir"
}

$binaryPath = "$targetDir\\$target\\release\\esp32-wifi-screen"
$binOutputPath = "$targetDir\\$target\\release\\esp32-wifi-screen-$chip.bin"

if (-not (Test-Path $binaryPath)) {
    Write-Host "Error: Binary not found at $binaryPath" -ForegroundColor Red
    exit 1
}

espflash save-image --chip $chip --partition-table partitions.csv $binaryPath $binOutputPath

if ($LASTEXITCODE -ne 0) {
    Write-Host "生成镜像失败!" -ForegroundColor Red
    exit 1
}

# pip install esptool -i https://pypi.tuna.tsinghua.edu.cn/simple

$tool = ".\esptool.exe"

$availablePorts = [System.IO.Ports.SerialPort]::getportnames()

$portsToCheck = @("COM3", "COM10", "COM6", "COM5")
$selectPort = $portsToCheck[0];

foreach ($port in $portsToCheck) {
    if ($availablePorts -contains $port) {
        Write-Host "Port $port Exist" -ForegroundColor Green
        $selectPort = $port;
        break
    } else {
        Write-Host "Port $port Not Exist." -ForegroundColor Yellow
    }
}

Write-Host "使用端口: $selectPort" -ForegroundColor Cyan

& $tool -p $selectPort --before default_reset --after hard_reset --chip $chip write_flash --flash_mode dio --flash_size detect 0x1000 .\bootloader.bin 0x8000 .\partitions.bin 0x10000 $binOutputPath

if ($LASTEXITCODE -eq 0) {
    Write-Host "烧录完成!" -ForegroundColor Green
} else {
    Write-Host "烧录失败!" -ForegroundColor Red
    exit 1
}
