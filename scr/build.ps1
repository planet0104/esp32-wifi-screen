param(
    [string]$chip = "esp32s2"  # 默认为 esp32s2，可以传递 esp32s3
)

# 验证芯片参数
if ($chip -ne "esp32s2" -and $chip -ne "esp32s3") {
    Write-Host "错误: 不支持的芯片类型 '$chip'。请使用 'esp32s2' 或 'esp32s3'。"
    Write-Host "用法: .\build.ps1 -chip esp32s2"
    Write-Host "      .\build.ps1 -chip esp32s3"
    exit 1
}

Write-Host "正在为 $chip 构建项目..."

$projectRoot = $PSScriptRoot

# 根据芯片类型设置目标和 feature
if ($chip -eq "esp32s3") {
    $target = "xtensa-esp32s3-espidf"
    $feature = "esp32s3"
    $env:MCU = "esp32s3"
    $env:ESP_IDF_SDKCONFIG_DEFAULTS = "$projectRoot\sdkconfig.defaults.esp32s3"
    $env:ESP_IDF_VERSION = "v5.3.2"
} else {
    $target = "xtensa-esp32s2-espidf"
    $feature = "esp32s2"
    $env:MCU = "esp32s2"
    $env:ESP_IDF_SDKCONFIG_DEFAULTS = "$projectRoot\sdkconfig.defaults.esp32s2"
    $env:ESP_IDF_VERSION = "v5.3.2"
}

# 使用 --target 和 --features 参数构建
cargo build --release --target $target --no-default-features --features "$feature,experimental"

espflash save-image --chip $chip --partition-table partitions.csv target/$target/release/esp32-wifi-screen "esp32-wifi-screen-$chip.bin"

Write-Host "构建完成: esp32-wifi-screen-$chip.bin"