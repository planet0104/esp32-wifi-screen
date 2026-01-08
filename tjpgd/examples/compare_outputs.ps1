# JPEG Decode Comparison Script
# Compare C and Rust JPEG decoder outputs

param(
    [string]$TestDir = "test_images",
    [string]$OutputDir = "compare_output",
    [switch]$Verbose
)

$ErrorActionPreference = "Stop"

Write-Host "======================================" -ForegroundColor Cyan
Write-Host "JPEG Decode Comparison (C vs Rust)" -ForegroundColor Cyan
Write-Host "======================================" -ForegroundColor Cyan
Write-Host ""

# Get script directory
$ScriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
if (-not $ScriptDir) { $ScriptDir = Get-Location }

# Path settings
$TestImagesPath = Join-Path $ScriptDir $TestDir
$OutputPath = Join-Path $ScriptDir $OutputDir
$CExePath = Join-Path $ScriptDir "tjpgd_pc\tjpgd_test.exe"
$RustExampleName = "jpg2bmp"

# Create output directories
if (-not (Test-Path $OutputPath)) {
    New-Item -ItemType Directory -Path $OutputPath | Out-Null
}
$COutputDir = Join-Path $OutputPath "c_output"
$RustOutputDir = Join-Path $OutputPath "rust_output"
if (-not (Test-Path $COutputDir)) { New-Item -ItemType Directory -Path $COutputDir | Out-Null }
if (-not (Test-Path $RustOutputDir)) { New-Item -ItemType Directory -Path $RustOutputDir | Out-Null }

# Check C executable
if (-not (Test-Path $CExePath)) {
    Write-Host "C executable not found, compiling..." -ForegroundColor Yellow
    Push-Location (Join-Path $ScriptDir "tjpgd_pc")
    try {
        $vsPath = & "${env:ProgramFiles(x86)}\Microsoft Visual Studio\Installer\vswhere.exe" -latest -property installationPath 2>$null
        if ($vsPath) {
            $vcvars = Join-Path $vsPath "VC\Auxiliary\Build\vcvars64.bat"
            $null = cmd /c "`"$vcvars`" && cl /O2 /W3 main.c tjpgd.c /Fe:tjpgd_test.exe" 2>&1
        }
    } finally {
        Pop-Location
    }
    
    if (-not (Test-Path $CExePath)) {
        Write-Host "Error: Cannot compile C version" -ForegroundColor Red
        exit 1
    }
    Write-Host "C version compiled successfully" -ForegroundColor Green
}

# Compile Rust version
Write-Host "Compiling Rust version..." -ForegroundColor Yellow
Push-Location $ScriptDir
$ErrorActionPreference = "Continue"
cmd /c "cargo build --example $RustExampleName --release 2>&1" | Out-Null
$ErrorActionPreference = "Stop"
$RustExePath = Join-Path $ScriptDir "target\release\examples\jpg2bmp.exe"
if (-not (Test-Path $RustExePath)) {
    $ErrorActionPreference = "Continue"
    cmd /c "cargo build --example $RustExampleName 2>&1" | Out-Null
    $ErrorActionPreference = "Stop"
    $RustExePath = Join-Path $ScriptDir "target\debug\examples\jpg2bmp.exe"
}
Pop-Location

if (-not (Test-Path $RustExePath)) {
    Write-Host "Error: Cannot compile Rust version" -ForegroundColor Red
    exit 1
}
Write-Host "Rust version compiled successfully" -ForegroundColor Green
Write-Host ""

# Get all JPG files
$JpgFiles = Get-ChildItem -Path $TestImagesPath -Filter "*.jpg" | Sort-Object Name

if ($JpgFiles.Count -eq 0) {
    Write-Host "Error: No JPG files found in $TestImagesPath" -ForegroundColor Red
    exit 1
}

Write-Host "Found $($JpgFiles.Count) test images" -ForegroundColor Green
Write-Host ""

# Statistics
$Results = @()
$TotalFiles = $JpgFiles.Count
$PassedFiles = 0
$FailedFiles = 0
$ErrorFiles = 0

# Process each file
foreach ($jpg in $JpgFiles) {
    $BaseName = [System.IO.Path]::GetFileNameWithoutExtension($jpg.Name)
    $CBmpPath = Join-Path $COutputDir "$BaseName.bmp"
    $RustBmpPath = Join-Path $RustOutputDir "$BaseName.bmp"
    
    Write-Host "Processing: $($jpg.Name)" -ForegroundColor White -NoNewline
    
    $Result = @{
        FileName = $jpg.Name
        CSuccess = $false
        RustSuccess = $false
        Identical = $false
        DiffBytes = 0
        DiffPercent = 0
        Error = ""
    }
    
    try {
        # Run C version
        $cOutput = & $CExePath $jpg.FullName $CBmpPath 2>&1
        if ($LASTEXITCODE -eq 0 -and (Test-Path $CBmpPath)) {
            $Result.CSuccess = $true
        }
        
        # Run Rust version
        $rustOutput = & $RustExePath $jpg.FullName $RustBmpPath 2>&1
        if ($LASTEXITCODE -eq 0 -and (Test-Path $RustBmpPath)) {
            $Result.RustSuccess = $true
        }
        
        # Check if both failed (invalid file)
        if (-not $Result.CSuccess -and -not $Result.RustSuccess) {
            throw "Both C and Rust failed (invalid JPEG)"
        }
        
        # Check if only C failed
        if (-not $Result.CSuccess -and $Result.RustSuccess) {
            throw "C failed, Rust succeeded (unsupported format in C)"
        }
        
        # Check if only Rust failed
        if ($Result.CSuccess -and -not $Result.RustSuccess) {
            throw "C succeeded, Rust failed (bug in Rust)"
        }
        
        # Compare files
        $CFile = Get-Item $CBmpPath
        $RustFile = Get-Item $RustBmpPath
        
        if ($CFile.Length -ne $RustFile.Length) {
            $Result.Error = "Size mismatch: C=$($CFile.Length), Rust=$($RustFile.Length)"
        } else {
            # Binary comparison
            $fcOutput = cmd /c "fc /b `"$CBmpPath`" `"$RustBmpPath`"" 2>&1
            if ($LASTEXITCODE -eq 0) {
                $Result.Identical = $true
                $Result.DiffBytes = 0
                $Result.DiffPercent = 0
            } else {
                # Count differences
                $diffLines = @($fcOutput | Where-Object { $_ -match "^[0-9A-F]+:" }).Count
                $Result.DiffBytes = $diffLines
                # Pixel data size = file size - 54 (BMP header)
                $pixelBytes = $CFile.Length - 54
                if ($pixelBytes -gt 0) {
                    $Result.DiffPercent = [math]::Round($diffLines / $pixelBytes * 100, 2)
                }
                
                # Check if all differences are small (within +/-2, common for IDCT rounding)
                $allSmallDiff = $true
                $maxDiff = 0
                foreach ($line in $fcOutput) {
                    # fc output format: "00000536: FF FE" (offset: val1 val2)
                    if ($line -match "^[0-9A-Fa-f]+:\s*([0-9A-Fa-f]{2})\s+([0-9A-Fa-f]{2})") {
                        $val1 = [Convert]::ToInt32($Matches[1], 16)
                        $val2 = [Convert]::ToInt32($Matches[2], 16)
                        $diff = [Math]::Abs($val1 - $val2)
                        if ($diff -gt $maxDiff) { $maxDiff = $diff }
                        if ($diff -gt 2) {
                            $allSmallDiff = $false
                        }
                    }
                }
                
                # Accept as rounding error if max diff <= 2 and total diff < 10%
                if ($allSmallDiff -and $Result.DiffPercent -lt 10) {
                    $Result.Identical = $true
                }
            }
        }
        
        if ($Result.Identical) {
            Write-Host " [OK]" -ForegroundColor Green -NoNewline
            if ($Result.DiffBytes -gt 0) {
                Write-Host " (rounding diff: $($Result.DiffBytes) bytes, $($Result.DiffPercent)%)" -ForegroundColor DarkGray
            } else {
                Write-Host " (identical)" -ForegroundColor DarkGray
            }
            $PassedFiles++
        } else {
            Write-Host " [DIFF]" -ForegroundColor Yellow -NoNewline
            Write-Host " diff: $($Result.DiffBytes) bytes, $($Result.DiffPercent)%" -ForegroundColor Yellow
            $FailedFiles++
        }
        
    } catch {
        Write-Host " [ERROR]" -ForegroundColor Red -NoNewline
        Write-Host " $($_.Exception.Message)" -ForegroundColor Red
        $Result.Error = $_.Exception.Message
        $ErrorFiles++
    }
    
    $Results += $Result
}

# Summary
Write-Host ""
Write-Host "======================================" -ForegroundColor Cyan
Write-Host "Test Summary" -ForegroundColor Cyan
Write-Host "======================================" -ForegroundColor Cyan
Write-Host "Total files:  $TotalFiles"
Write-Host "Passed:       $PassedFiles" -ForegroundColor Green
Write-Host "Large diff:   $FailedFiles" -ForegroundColor Yellow
Write-Host "Errors:       $ErrorFiles" -ForegroundColor Red
Write-Host ""

if ($PassedFiles -eq $TotalFiles) {
    Write-Host "All tests passed! C and Rust outputs are consistent." -ForegroundColor Green
} elseif ($ErrorFiles -gt 0) {
    Write-Host "$ErrorFiles file(s) failed to decode" -ForegroundColor Red
} else {
    Write-Host "$FailedFiles file(s) have significant differences" -ForegroundColor Yellow
}

Write-Host ""
Write-Host "Output directories:"
Write-Host "  C version:    $COutputDir"
Write-Host "  Rust version: $RustOutputDir"

# Detailed report
if ($Verbose -or $FailedFiles -gt 0 -or $ErrorFiles -gt 0) {
    Write-Host ""
    Write-Host "Detailed results:" -ForegroundColor Cyan
    foreach ($r in $Results) {
        $status = if ($r.Identical) { "[OK]" } elseif ($r.Error) { "[ERROR]" } else { "[DIFF]" }
        $color = if ($r.Identical) { "Green" } elseif ($r.Error) { "Red" } else { "Yellow" }
        Write-Host "  $($r.FileName): " -NoNewline
        Write-Host $status -ForegroundColor $color -NoNewline
        if ($r.DiffBytes -gt 0) {
            Write-Host " ($($r.DiffBytes) bytes diff, $($r.DiffPercent)%)" -ForegroundColor DarkGray
        } elseif ($r.Error) {
            Write-Host " $($r.Error)" -ForegroundColor Red
        } else {
            Write-Host ""
        }
    }
}

# Return exit code
if ($ErrorFiles -gt 0) {
    exit 2
} elseif ($FailedFiles -gt 0) {
    exit 1
} else {
    exit 0
}
