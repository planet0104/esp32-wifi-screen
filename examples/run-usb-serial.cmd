@echo off
REM Run examples crate with usb-serial feature to test ESP32 via serial

REM Get the script directory (ends with backslash)
SET SCRIPT_DIR=%~dp0

echo === Listing serial ports ===
powershell -NoProfile -Command "Get-WmiObject Win32_SerialPort | Select-Object -Property DeviceID,Caption,PNPDeviceID | Format-Table -AutoSize"

echo.
echo === Building examples (feature: usb-serial, no-default-features) ===
cargo build --manifest-path "%SCRIPT_DIR%Cargo.toml" --no-default-features --features "usb-serial" --release
IF %ERRORLEVEL% NEQ 0 (
  echo Build failed.
  pause
  exit /b %ERRORLEVEL%
)

echo.
echo === Running examples (usb-serial, no-default-features) ===
cargo run --manifest-path "%SCRIPT_DIR%Cargo.toml" --no-default-features --features "usb-serial" --release
IF %ERRORLEVEL% NEQ 0 (
  echo Run failed.
  pause
  exit /b %ERRORLEVEL%
)

echo Program finished.
pause
