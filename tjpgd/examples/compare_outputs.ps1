# JPEG Decode Comparison Script
# Compare C and Rust JPEG decoder outputs across all JD_FASTDECODE modes
#
# Modes tested:
# - fast-decode-0: Basic optimization (8/16-bit MCU)
# - fast-decode-1: 32-bit barrel shifter
# - fast-decode-2: + Huffman LUT

param(
    [string]$TestDir = "test_images",
    [string]$OutputDir = "compare_output",
    [switch]$Verbose,
    [string]$Mode = "all"  # "all", "0", "1", "2"
)

$ErrorActionPreference = "Stop"

Write-Host "======================================" -ForegroundColor Cyan
Write-Host "JPEG Decode Comparison (C vs Rust)" -ForegroundColor Cyan
Write-Host "Testing JD_FASTDECODE modes" -ForegroundColor Cyan
Write-Host "======================================" -ForegroundColor Cyan
Write-Host ""

# Get script directory and project root
$ScriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
if (-not $ScriptDir) { $ScriptDir = Get-Location }

# Project root is parent of examples folder
$ProjectRoot = Split-Path -Parent $ScriptDir
if (-not $ProjectRoot) { $ProjectRoot = Get-Location }

# Path settings
$TestImagesPath = Join-Path $ProjectRoot $TestDir
$OutputPath = Join-Path $ProjectRoot $OutputDir
$CExePath = Join-Path $ProjectRoot "tjpgd_pc\tjpgd_test.exe"
$RustExampleName = "jpg2bmp"

# Define modes to test
$ModesToTest = @()
if ($Mode -eq "all") {
    $ModesToTest = @(
        @{ Level = 0; Feature = "fast-decode-0"; Name = "Basic (8/16-bit MCU)" },
        @{ Level = 1; Feature = "fast-decode-1"; Name = "32-bit barrel shifter" },
        @{ Level = 2; Feature = "fast-decode-2"; Name = "+ Huffman LUT" }
    )
} else {
    switch ($Mode) {
        "0" { $ModesToTest = @(@{ Level = 0; Feature = "fast-decode-0"; Name = "Basic (8/16-bit MCU)" }) }
        "1" { $ModesToTest = @(@{ Level = 1; Feature = "fast-decode-1"; Name = "32-bit barrel shifter" }) }
        "2" { $ModesToTest = @(@{ Level = 2; Feature = "fast-decode-2"; Name = "+ Huffman LUT" }) }
        default { 
            Write-Host "Invalid mode: $Mode. Use 'all', '0', '1', or '2'" -ForegroundColor Red
            exit 1
        }
    }
}

# Create output directories
if (-not (Test-Path $OutputPath)) {
    New-Item -ItemType Directory -Path $OutputPath | Out-Null
}
$COutputDir = Join-Path $OutputPath "c_output"
if (-not (Test-Path $COutputDir)) { New-Item -ItemType Directory -Path $COutputDir | Out-Null }

# Check C executable
if (-not (Test-Path $CExePath)) {
    Write-Host "C executable not found, compiling..." -ForegroundColor Yellow
    Push-Location (Join-Path $ProjectRoot "tjpgd_pc")
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

# Get all JPG files
$JpgFiles = Get-ChildItem -Path $TestImagesPath -Filter "*.jpg" | Sort-Object Name

if ($JpgFiles.Count -eq 0) {
    Write-Host "Error: No JPG files found in $TestImagesPath" -ForegroundColor Red
    exit 1
}

Write-Host "Found $($JpgFiles.Count) test images" -ForegroundColor Green
Write-Host "Testing $($ModesToTest.Count) mode(s)" -ForegroundColor Green
Write-Host ""

# Generate C reference outputs first (only once)
Write-Host "Generating C reference outputs..." -ForegroundColor Yellow
foreach ($jpg in $JpgFiles) {
    $BaseName = [System.IO.Path]::GetFileNameWithoutExtension($jpg.Name)
    $CBmpPath = Join-Path $COutputDir "$BaseName.bmp"
    
    if (-not (Test-Path $CBmpPath)) {
        $null = & $CExePath $jpg.FullName $CBmpPath 2>&1
    }
}
Write-Host "C reference outputs ready" -ForegroundColor Green
Write-Host ""

# Overall statistics
$AllModeResults = @{}
$TotalPassed = 0
$TotalFailed = 0
$TotalErrors = 0

# Test each mode
foreach ($ModeInfo in $ModesToTest) {
    $Level = $ModeInfo.Level
    $Feature = $ModeInfo.Feature
    $ModeName = $ModeInfo.Name
    
    Write-Host "======================================" -ForegroundColor Magenta
    Write-Host "Testing JD_FASTDECODE = $Level ($ModeName)" -ForegroundColor Magenta
    Write-Host "======================================" -ForegroundColor Magenta
    
    # Create output directory for this mode
    $RustOutputDir = Join-Path $OutputPath "rust_output_level$Level"
    if (-not (Test-Path $RustOutputDir)) { New-Item -ItemType Directory -Path $RustOutputDir | Out-Null }
    
    # Compile Rust version with specific feature
    Write-Host "Compiling Rust version with --features $Feature..." -ForegroundColor Yellow
    Push-Location $ProjectRoot
    
    # Clean and rebuild
    $ErrorActionPreference = "Continue"
    cmd /c "cargo build --example $RustExampleName --release --no-default-features --features $Feature 2>&1" | Out-Null
    $BuildResult = $LASTEXITCODE
    $ErrorActionPreference = "Stop"
    
    $RustExePath = Join-Path $ProjectRoot "target\release\examples\jpg2bmp.exe"
    
    if ($BuildResult -ne 0 -or -not (Test-Path $RustExePath)) {
        # Try debug build
        $ErrorActionPreference = "Continue"
        cmd /c "cargo build --example $RustExampleName --no-default-features --features $Feature 2>&1" | Out-Null
        $ErrorActionPreference = "Stop"
        $RustExePath = Join-Path $ProjectRoot "target\debug\examples\jpg2bmp.exe"
    }
    Pop-Location
    
    if (-not (Test-Path $RustExePath)) {
        Write-Host "Error: Cannot compile Rust version for level $Level" -ForegroundColor Red
        $AllModeResults[$Level] = @{ Passed = 0; Failed = 0; Errors = $JpgFiles.Count }
        $TotalErrors += $JpgFiles.Count
        continue
    }
    Write-Host "Rust version compiled (level $Level)" -ForegroundColor Green
    Write-Host ""
    
    # Statistics for this mode
    $ModeResults = @()
    $PassedFiles = 0
    $FailedFiles = 0
    $ErrorFiles = 0
    
    # Process each file
    foreach ($jpg in $JpgFiles) {
        $BaseName = [System.IO.Path]::GetFileNameWithoutExtension($jpg.Name)
        $CBmpPath = Join-Path $COutputDir "$BaseName.bmp"
        $RustBmpPath = Join-Path $RustOutputDir "$BaseName.bmp"
        
        Write-Host "  $($jpg.Name): " -NoNewline
        
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
            # Check C output
            if (Test-Path $CBmpPath) {
                $Result.CSuccess = $true
            }
            
            # Run Rust version
            $rustOutput = & $RustExePath $jpg.FullName $RustBmpPath 2>&1
            if ($LASTEXITCODE -eq 0 -and (Test-Path $RustBmpPath)) {
                $Result.RustSuccess = $true
            }
            
            # Check results
            if (-not $Result.CSuccess -and -not $Result.RustSuccess) {
                throw "Both C and Rust failed (invalid JPEG)"
            }
            
            if (-not $Result.CSuccess -and $Result.RustSuccess) {
                throw "C failed, Rust succeeded"
            }
            
            if ($Result.CSuccess -and -not $Result.RustSuccess) {
                throw "C succeeded, Rust failed"
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
                } else {
                    # Count differences
                    $diffLines = @($fcOutput | Where-Object { $_ -match "^[0-9A-F]+:" }).Count
                    $Result.DiffBytes = $diffLines
                    $pixelBytes = $CFile.Length - 54
                    if ($pixelBytes -gt 0) {
                        $Result.DiffPercent = [math]::Round($diffLines / $pixelBytes * 100, 2)
                    }
                    
                    # Check if all differences are within acceptable range (±3)
                    $allSmallDiff = $true
                    foreach ($line in $fcOutput) {
                        if ($line -match "^[0-9A-Fa-f]+:\s*([0-9A-Fa-f]{2})\s+([0-9A-Fa-f]{2})") {
                            $val1 = [Convert]::ToInt32($Matches[1], 16)
                            $val2 = [Convert]::ToInt32($Matches[2], 16)
                            $diff = [Math]::Abs($val1 - $val2)
                            if ($diff -gt 3) {
                                $allSmallDiff = $false
                                break
                            }
                        }
                    }
                    
                    if ($allSmallDiff) {
                        $Result.Identical = $true
                    }
                }
            }
            
            if ($Result.Identical) {
                Write-Host "[OK]" -ForegroundColor Green -NoNewline
                if ($Result.DiffBytes -gt 0) {
                    Write-Host " (rounding: $($Result.DiffBytes) bytes)" -ForegroundColor DarkGray
                } else {
                    Write-Host " (identical)" -ForegroundColor DarkGray
                }
                $PassedFiles++
            } else {
                Write-Host "[DIFF]" -ForegroundColor Yellow -NoNewline
                Write-Host " $($Result.DiffBytes) bytes ($($Result.DiffPercent)%)" -ForegroundColor Yellow
                $FailedFiles++
            }
            
        } catch {
            Write-Host "[ERROR]" -ForegroundColor Red -NoNewline
            Write-Host " $($_.Exception.Message)" -ForegroundColor Red
            $Result.Error = $_.Exception.Message
            $ErrorFiles++
        }
        
        $ModeResults += $Result
    }
    
    # Mode summary
    Write-Host ""
    Write-Host "Level $Level Summary: " -NoNewline
    Write-Host "$PassedFiles passed" -ForegroundColor Green -NoNewline
    Write-Host ", $FailedFiles diff" -ForegroundColor Yellow -NoNewline
    Write-Host ", $ErrorFiles errors" -ForegroundColor Red
    Write-Host ""
    
    $AllModeResults[$Level] = @{ 
        Passed = $PassedFiles
        Failed = $FailedFiles
        Errors = $ErrorFiles
        Results = $ModeResults
    }
    
    $TotalPassed += $PassedFiles
    $TotalFailed += $FailedFiles
    $TotalErrors += $ErrorFiles
}

# Final summary
Write-Host ""
Write-Host "======================================" -ForegroundColor Cyan
Write-Host "Final Test Summary" -ForegroundColor Cyan
Write-Host "======================================" -ForegroundColor Cyan
Write-Host ""

Write-Host "Mode Results:" -ForegroundColor White
foreach ($ModeInfo in $ModesToTest) {
    $Level = $ModeInfo.Level
    $Result = $AllModeResults[$Level]
    $Status = if ($Result.Errors -eq 0 -and $Result.Failed -eq 0) { "[PASS]" } 
              elseif ($Result.Errors -gt 0) { "[ERROR]" } 
              else { "[WARN]" }
    $Color = if ($Result.Errors -eq 0 -and $Result.Failed -eq 0) { "Green" }
             elseif ($Result.Errors -gt 0) { "Red" }
             else { "Yellow" }
    
    Write-Host "  Level $Level ($($ModeInfo.Name)): " -NoNewline
    Write-Host $Status -ForegroundColor $Color -NoNewline
    Write-Host " - $($Result.Passed)/$($JpgFiles.Count) passed"
}

Write-Host ""
Write-Host "Total: " -NoNewline
$TotalTests = $TotalPassed + $TotalFailed + $TotalErrors
Write-Host "$TotalPassed/$TotalTests passed" -ForegroundColor $(if ($TotalPassed -eq $TotalTests) { "Green" } else { "Yellow" })

Write-Host ""
Write-Host "Output directories:"
Write-Host "  C version:    $COutputDir"
foreach ($ModeInfo in $ModesToTest) {
    $Level = $ModeInfo.Level
    Write-Host "  Rust level $Level`: $(Join-Path $OutputPath "rust_output_level$Level")"
}

# Exit code
$ExitCode = 0
if ($TotalErrors -gt 0) {
    $ExitCode = 2
} elseif ($TotalFailed -gt 0) {
    $ExitCode = 1
}

if ($ExitCode -eq 0) {
    Write-Host ""
    Write-Host "All tests passed! All JD_FASTDECODE modes are consistent with C version." -ForegroundColor Green
}

exit $ExitCode
