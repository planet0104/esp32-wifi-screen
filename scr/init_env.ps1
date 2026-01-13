# init_env.ps1
# Initialize environment variables for building the esp32s3 firmware on Windows (PowerShell).
# Usage examples:
#   .\init_env.ps1
#   . .\init_env.ps1        # dot-source to persist in the current session
#   .\init_env.ps1 -EspIdfPath 'C:\esp\\.embuild\\espressif\\esp-idf\\v5.3.4' -Mcu esp32s3

[CmdletBinding()]
param(
    [string]$EspIdfPath = "C:\esp\\.embuild\\espressif\\esp-idf\\v5.3.4",
    [string]$EspIdfToolsInstallDir = "custom:C:\Users\$env:USERNAME\\.espressif",
    [string]$SdkconfigDefaults = "$PSScriptRoot\sdkconfig.defaults.esp32s3",
    [ValidateSet('esp32s2','esp32s3','esp32')]
    [string]$Mcu = 'esp32s3',
    [switch]$NoExport,
    [switch]$VerboseOutput
)

function Write-Info {
    param($msg)
    Write-Host "[init_env] $msg"
}

Write-Info "Initializing environment for MCU='$Mcu'"

# 1) Run export.ps1 if available and not disabled
if (-not $NoExport) {
    $exportScript = Join-Path -Path $EspIdfPath -ChildPath 'export.ps1'
    if (Test-Path $exportScript) {
        Write-Info "Found export.ps1 at $exportScript — running it (this may modify PATH)."
        try {
            & $exportScript
            Write-Info "Sourced export.ps1 successfully."
        } catch {
            Write-Host "[init_env] Warning: running export.ps1 failed: $_" -ForegroundColor Yellow
        }
    } else {
        Write-Info "export.ps1 not found at $exportScript. Skipping automatic export."
    }
} else {
    Write-Info "Skipping export.ps1 as requested by -NoExport." 
}

# 2) Set environment variables used by the build (process scope)
[System.Environment]::SetEnvironmentVariable('ESP_IDF_TOOLS_INSTALL_DIR', $EspIdfToolsInstallDir, 'Process')
[System.Environment]::SetEnvironmentVariable('ESP_IDF_SDKCONFIG_DEFAULTS', $SdkconfigDefaults, 'Process')
[System.Environment]::SetEnvironmentVariable('MCU', $Mcu, 'Process')
[System.Environment]::SetEnvironmentVariable('RUST_BACKTRACE', '1', 'Process')

if ($VerboseOutput) {
    Write-Info "Environment variables set (process scope):"
    Get-ChildItem Env:ESP_IDF_TOOLS_INSTALL_DIR,Env:ESP_IDF_SDKCONFIG_DEFAULTS,Env:MCU,Env:RUST_BACKTRACE | ForEach-Object { Write-Host "  $_" }
}

Write-Host "`nEnvironment initialized. You can now run the build command for esp32s3:" -ForegroundColor Green
Write-Host "  cargo build --target xtensa-esp32s3-espidf --features \"esp32s3,experimental\"`n"

Write-Host "Tips:"
Write-Host " - To persist these vars for the current PowerShell session, call this script with dot-sourcing:`n    . .\init_env.ps1`n"
Write-Host " - If your esp-idf is installed elsewhere, pass -EspIdfPath to override the default."

Write-Info "Done."
