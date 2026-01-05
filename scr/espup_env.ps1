# ESP32 Rust Development Environment Setup Script
# Usage: . .\espup_env.ps1

# Set ESP-IDF sdkconfig defaults path
$env:ESP_IDF_SDKCONFIG_DEFAULTS = "$PSScriptRoot\sdkconfig.defaults"

# Method 1: Use official espup generated script (recommended)
$espupScript = "$HOME\export-esp.ps1"
if (Test-Path $espupScript) {
    Write-Host "Loading espup environment variables..." -ForegroundColor Green
    . $espupScript
    Write-Host "ESP32 development environment activated!" -ForegroundColor Green
} else {
    # Method 2: Manual setup using rustup toolchain path
    $clangBinPath = "$HOME\.rustup\toolchains\esp\xtensa-esp32-elf-clang\esp-clang\bin"
    
    if (Test-Path $clangBinPath) {
        $env:LIBCLANG_PATH = $clangBinPath
        $env:PATH = "$clangBinPath;$env:PATH"
        Write-Host "ESP32 development environment activated!" -ForegroundColor Green
    } else {
        Write-Host "Error: ESP toolchain not found at $clangBinPath" -ForegroundColor Red
        Write-Host "Please run: espup install" -ForegroundColor Yellow
    }
}

# Verify environment variables
if ($env:LIBCLANG_PATH) {
    Write-Host "`n[OK] LIBCLANG_PATH = $env:LIBCLANG_PATH" -ForegroundColor Cyan
} else {
    Write-Host "`n[ERROR] LIBCLANG_PATH not set" -ForegroundColor Red
}
