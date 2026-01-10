# ESP32-S2 专用构建脚本

Write-Host "正在为 ESP32-S2 构建项目..."

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

if ($LASTEXITCODE -eq 0) {
    Write-Host "ESP32-S2 构建成功!" -ForegroundColor Green
    Write-Host "可执行文件: target/$target/release/esp32-wifi-screen" -ForegroundColor Cyan
} else {
    Write-Host "构建失败!" -ForegroundColor Red
    exit 1
}
