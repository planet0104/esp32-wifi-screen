# PowerShell build script for TJpgDec Windows PC version

Write-Host "====================================" -ForegroundColor Cyan
Write-Host "Building TJpgDec for Windows PC" -ForegroundColor Cyan
Write-Host "====================================" -ForegroundColor Cyan
Write-Host ""

# Check for available C compiler
$compiler = $null
$compilerName = ""

$clangPath = Get-Command clang -ErrorAction SilentlyContinue
if ($clangPath) {
    $compiler = "clang"
    $compilerName = "Clang"
}

if (-not $compiler) {
    $gccPath = Get-Command gcc -ErrorAction SilentlyContinue
    if ($gccPath) {
        $compiler = "gcc"
        $compilerName = "GCC"
    }
}

if (-not $compiler) {
    $clPath = Get-Command cl -ErrorAction SilentlyContinue
    if ($clPath) {
        $compiler = "cl"
        $compilerName = "MSVC"
    }
}

if (-not $compiler) {
    Write-Host "Error: No C compiler found in PATH" -ForegroundColor Red
    Write-Host "Please install one of the following:" -ForegroundColor Yellow
    Write-Host "  LLVM/Clang: https://releases.llvm.org/" -ForegroundColor Yellow
    Write-Host "  MinGW: http://www.mingw.org/" -ForegroundColor Yellow
    Write-Host "  TDM-GCC: https://jmeubank.github.io/tdm-gcc/" -ForegroundColor Yellow
    Read-Host "Press Enter to exit"
    exit 1
}

Write-Host "Using $compilerName compiler..." -ForegroundColor Cyan
Write-Host "Compiling..." -ForegroundColor Green
& $compiler -o tjpgd_test.exe main.c tjpgd.c -O2 -Wall

if ($LASTEXITCODE -eq 0) {
    Write-Host ""
    Write-Host "====================================" -ForegroundColor Green
    Write-Host "Build completed successfully!" -ForegroundColor Green
    Write-Host "Executable: tjpgd_test.exe" -ForegroundColor Green
    Write-Host "====================================" -ForegroundColor Green
    Write-Host ""
    Write-Host "Usage: .\tjpgd_test.exe input.jpg [output.ppm]" -ForegroundColor Cyan
} else {
    Write-Host ""
    Write-Host "====================================" -ForegroundColor Red
    Write-Host "Build failed!" -ForegroundColor Red
    Write-Host "====================================" -ForegroundColor Red
}

Write-Host ""
Read-Host "Press Enter to exit"

