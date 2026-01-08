# PowerShell test script for TJpgDec Windows PC version

Write-Host "====================================" -ForegroundColor Cyan
Write-Host "Testing TJpgDec" -ForegroundColor Cyan
Write-Host "====================================" -ForegroundColor Cyan
Write-Host ""

# Check if executable exists
if (-not (Test-Path "tjpgd_test.exe")) {
    Write-Host "Error: tjpgd_test.exe not found" -ForegroundColor Red
    Write-Host "Please run build.ps1 or build.bat first" -ForegroundColor Yellow
    Read-Host "Press Enter to exit"
    exit 1
}

# Check if test image exists
if (-not (Test-Path "test.jpg")) {
    Write-Host "Warning: test.jpg not found" -ForegroundColor Yellow
    Write-Host "Please provide a test JPEG image named test.jpg" -ForegroundColor Yellow
    Write-Host ""
    Write-Host "You can:" -ForegroundColor Yellow
    Write-Host "  1. Create or copy a test.jpg file to this directory" -ForegroundColor Yellow
    Write-Host "  2. Run: .\tjpgd_test.exe your_image.jpg" -ForegroundColor Yellow
    Write-Host ""
    Read-Host "Press Enter to exit"
    exit 1
}

Write-Host "Running test with test.jpg..." -ForegroundColor Green
Write-Host ""
.\tjpgd_test.exe test.jpg test_output.ppm

if ($LASTEXITCODE -eq 0) {
    Write-Host ""
    Write-Host "====================================" -ForegroundColor Green
    Write-Host "Test completed successfully!" -ForegroundColor Green
    Write-Host "Output file: test_output.ppm" -ForegroundColor Green
    Write-Host "====================================" -ForegroundColor Green
} else {
    Write-Host ""
    Write-Host "====================================" -ForegroundColor Red
    Write-Host "Test failed!" -ForegroundColor Red
    Write-Host "====================================" -ForegroundColor Red
}

Write-Host ""
Read-Host "Press Enter to exit"

