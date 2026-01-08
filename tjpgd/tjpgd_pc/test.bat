@echo off
REM Test script for TJpgDec Windows PC version

echo ====================================
echo Testing TJpgDec
echo ====================================
echo.

REM Check if executable exists
if not exist "tjpgd_test.exe" (
    echo Error: tjpgd_test.exe not found
    echo Please run build.bat first
    pause
    exit /b 1
)

REM Check if test image exists
if not exist "test.jpg" (
    echo Warning: test.jpg not found
    echo Please provide a test JPEG image named test.jpg
    echo.
    echo You can:
    echo   1. Create or copy a test.jpg file to this directory
    echo   2. Run: tjpgd_test.exe your_image.jpg
    echo.
    pause
    exit /b 1
)

echo Running test with test.jpg...
echo.
tjpgd_test.exe test.jpg test_output.ppm

if %ERRORLEVEL% EQU 0 (
    echo.
    echo ====================================
    echo Test completed successfully!
    echo Output file: test_output.ppm
    echo ====================================
) else (
    echo.
    echo ====================================
    echo Test failed!
    echo ====================================
)

echo.
pause


