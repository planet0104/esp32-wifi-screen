# PowerShell benchmark script for TJpgDec

Write-Host "====================================" -ForegroundColor Cyan
Write-Host "TJpgDec Performance Benchmark" -ForegroundColor Cyan
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
    Write-Host "Error: test.jpg not found" -ForegroundColor Red
    Write-Host "Please provide a test image" -ForegroundColor Yellow
    Read-Host "Press Enter to exit"
    exit 1
}

$iterations = 10
Write-Host "Running benchmark ($iterations iterations)..." -ForegroundColor Green
Write-Host ""

$times = @()

for ($i = 1; $i -le $iterations; $i++) {
    Write-Host "Iteration $i..." -NoNewline
    
    $stopwatch = [System.Diagnostics.Stopwatch]::StartNew()
    .\tjpgd_test.exe test.jpg benchmark_output.ppm 2>&1 | Out-Null
    $stopwatch.Stop()
    
    $elapsed = $stopwatch.ElapsedMilliseconds
    $times += $elapsed
    
    Write-Host " $elapsed ms" -ForegroundColor Yellow
}

Write-Host ""
Write-Host "====================================" -ForegroundColor Cyan
Write-Host "Benchmark Results" -ForegroundColor Cyan
Write-Host "====================================" -ForegroundColor Cyan

$avgTime = ($times | Measure-Object -Average).Average
$minTime = ($times | Measure-Object -Minimum).Minimum
$maxTime = ($times | Measure-Object -Maximum).Maximum

Write-Host "Iterations: $iterations"
Write-Host "Average time: $([math]::Round($avgTime, 2)) ms" -ForegroundColor Green
Write-Host "Minimum time: $minTime ms" -ForegroundColor Green
Write-Host "Maximum time: $maxTime ms" -ForegroundColor Green
Write-Host "====================================" -ForegroundColor Cyan
Write-Host ""

# Clean up benchmark output
if (Test-Path "benchmark_output.ppm") {
    Remove-Item "benchmark_output.ppm"
}

Read-Host "Press Enter to exit"


