# ESP32-S3 专用构建脚本

Write-Host "正在为 ESP32-S3 构建项目..."

$chip = "esp32s3"
$target = "xtensa-esp32s3-espidf"
$feature = "esp32s3"

# 设置环境变量 - 使用绝对路径
$env:MCU = "esp32s3"
$projectRoot = $PSScriptRoot
$env:ESP_IDF_SDKCONFIG_DEFAULTS = "$projectRoot\sdkconfig.defaults.esp32s3"
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

Write-Host "ESP32-S3 编译成功!" -ForegroundColor Green

# 读取target目录配置
$configContent = Get-Content ".cargo\config.toml" -Raw
if ($configContent -match 'target-dir\s*=\s*"([^"]+)"') {
    $targetDir = $matches[1].Replace('/', '\\')
    if (-not [System.IO.Path]::IsPathRooted($targetDir)) {
        $targetDir = Join-Path $projectRoot $targetDir
    }
} else {
    $targetDir = Join-Path $projectRoot "target"
}

$binaryPath = "$targetDir\\$target\\release\\esp32-wifi-screen"
$binOutputPath = "esp32-wifi-screen-$chip-merged.bin"

# 生成完整合并镜像（包含bootloader、partition和应用）
Write-Host "正在生成完整镜像: $binOutputPath" -ForegroundColor Cyan
espflash save-image --chip $chip --merge --partition-table partitions.csv $binaryPath $binOutputPath

if ($LASTEXITCODE -eq 0) {
    Write-Host "ESP32-S3 镜像生成成功: $binOutputPath" -ForegroundColor Green
} else {
    Write-Host "镜像生成失败!" -ForegroundColor Red
    exit 1
}
