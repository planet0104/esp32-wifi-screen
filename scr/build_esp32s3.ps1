# ESP32-S3 Build Script

Write-Host "Building project for ESP32-S3..." -ForegroundColor Green

$chip = "esp32s3"
$target = "xtensa-esp32s3-espidf"
$feature = "esp32s3"

# Set environment variables with absolute paths
$env:MCU = "esp32s3"
$projectRoot = $PSScriptRoot
$env:ESP_IDF_VERSION = "v5.3.4"

# Read target directory from config (before build)
$configContent = Get-Content ".cargo\config.toml" -Raw -ErrorAction SilentlyContinue
if ($configContent -match 'target-dir\s*=\s*"([^"]+)"') {
    $targetDir = $matches[1].Replace('/', '\')
    if (-not [System.IO.Path]::IsPathRooted($targetDir)) {
        $targetDir = Join-Path $projectRoot $targetDir
    }
} else {
    $targetDir = Join-Path $projectRoot "target"
}

# Generate temporary sdkconfig with absolute partition path
$srcSdkconfig = Join-Path $projectRoot "sdkconfig.defaults.esp32s3"
$tempSdkconfig = Join-Path $projectRoot "sdkconfig.defaults.esp32s3.tmp"
$srcPartitions = Join-Path $projectRoot "partitions.csv"
# Use forward slash format (CMake/ESP-IDF compatible)
$absPartitionsPath = $srcPartitions.Replace('\', '/')

Write-Host "Generating temporary sdkconfig with absolute partition path..." -ForegroundColor Cyan
$sdkconfigContent = Get-Content $srcSdkconfig -Raw
$sdkconfigContent = $sdkconfigContent -replace 'CONFIG_PARTITION_TABLE_CUSTOM_FILENAME="[^"]*"', "CONFIG_PARTITION_TABLE_CUSTOM_FILENAME=`"$absPartitionsPath`""
Set-Content -Path $tempSdkconfig -Value $sdkconfigContent -NoNewline
Write-Host "  Partition path: $absPartitionsPath" -ForegroundColor DarkCyan

$env:ESP_IDF_SDKCONFIG_DEFAULTS = $tempSdkconfig

Write-Host "Environment variables:" -ForegroundColor Cyan
Write-Host "  MCU: $env:MCU"
Write-Host "  ESP_IDF_SDKCONFIG_DEFAULTS: $env:ESP_IDF_SDKCONFIG_DEFAULTS"
Write-Host "  ESP_IDF_VERSION: $env:ESP_IDF_VERSION"

# Build with --target and --features
cargo build --release --target $target --no-default-features --features "$feature,experimental"

# Clean up temporary file
if (Test-Path $tempSdkconfig) {
    Remove-Item $tempSdkconfig -Force
}

if ($LASTEXITCODE -ne 0) {
    Write-Host "Build failed!" -ForegroundColor Red
    exit 1
}

Write-Host "ESP32-S3 compilation successful!" -ForegroundColor Green

$binaryPath = "$targetDir\$target\release\esp32-wifi-screen"
# Output merged bin to project root directory
$binOutputPath = Join-Path $projectRoot "esp32-wifi-screen-$chip-merged.bin"

# Print absolute paths for partitions.csv and bootloader.bin for debugging
$partitionsCsv = Join-Path $projectRoot 'partitions.csv'
if (Test-Path $partitionsCsv) {
    Write-Host "Partitions CSV: $partitionsCsv" -ForegroundColor Cyan
} else {
    Write-Host "Partitions CSV not found: $partitionsCsv" -ForegroundColor Red
}

$bootCandidates = @(
    "$projectRoot\build\bootloader\bootloader.bin",
    "$projectRoot\build\bootloader.bin",
    "$projectRoot\build\$chip\bootloader.bin",
    "$targetDir\bootloader.bin",
    "$projectRoot\bootloader.bin"
)
$firstBootPath = $null
foreach ($p in $bootCandidates) {
    if (Test-Path $p) { $firstBootPath = $p; break }
}

if (-not $firstBootPath) {
    # Silent search in common roots (no printing) to locate a bootloader if present
    $scanRoots = @()
    $scanRoots += (Join-Path $projectRoot 'build')
    if ($targetDir) { $scanRoots += $targetDir }
    $scanRoots += 'C:\esp\.embuild'
    if ($env:IDF_PATH) { $scanRoots += $env:IDF_PATH }
    if ($env:USERPROFILE) { $scanRoots += (Join-Path $env:USERPROFILE '.espressif') }

    foreach ($root in $scanRoots | Select-Object -Unique) {
        if (-not (Test-Path $root)) { continue }
        $scan = Get-ChildItem -Path $root -Recurse -Filter '*boot*.bin' -ErrorAction SilentlyContinue
        if ($scan -and $scan.Count -gt 0) { $firstBootPath = $scan[0].FullName; break }
    }
}

# Prefer the bootloader produced under the target's release folder if present
$preferredBoot = Join-Path $targetDir "${target}\release\bootloader.bin"
if (Test-Path $preferredBoot) { $firstBootPath = $preferredBoot }

# If we've found a bootloader, ensure espflash will pick the correct file by copying
# it to projectRoot\build\bootloader\bootloader.bin (overwrite). We will report
# the path under project build as the used bootloader for consistent output.
if ($firstBootPath) {
    $buildBootDir = Join-Path $projectRoot 'build\bootloader'
    if (-not (Test-Path $buildBootDir)) { New-Item -ItemType Directory -Path $buildBootDir | Out-Null }
    $usedBootPath = Join-Path $buildBootDir ("bootloader-{0}.bin" -f $chip)
    Copy-Item -Path $firstBootPath -Destination $usedBootPath -Force
    $firstBootPath = $usedBootPath
}

# Check if application binary exists
if (-not (Test-Path $binaryPath)) {
    Write-Host "Error: application binary not found at: $binaryPath" -ForegroundColor Red
    Write-Host "Ensure the target build produced the binary and rerun the script." -ForegroundColor Yellow
    exit 1
}

# If binary is ELF, try to find generated .bin under target directory (esp-idf/embuild output)
$magic = @()
try { $magic = [System.IO.File]::ReadAllBytes($binaryPath)[0..3] } catch { $magic = @() }
if ($magic -and ($magic -join ' ' ) -eq "127 69 76 70") {
    Write-Host "Detected ELF executable at $binaryPath; searching for generated .bin under $targetDir..." -ForegroundColor Yellow
    # Improved selection: prefer project/app .bin, exclude dep-graph, bootloader, libespidf and incremental build artifacts
    $filtered = @()
    $targetReleasePath = Join-Path $targetDir "$target\release"
    $targetDebugPath = Join-Path $targetDir "$target\debug"
    $preferPaths = @($targetReleasePath, $targetDebugPath) | Where-Object { Test-Path $_ }
    foreach ($p in $preferPaths) {
        $filtered += Get-ChildItem -Path $p -Recurse -Filter '*.bin' -ErrorAction SilentlyContinue | Where-Object {
            $_.Name -ne 'dep-graph.bin' -and
            $_.Name -ne 'bootloader.bin' -and
            $_.Name -ne 'libespidf.bin' -and
            $_.FullName -notlike '*incremental*' -and
            $_.FullName -notlike '*build\esp-idf-sys*' -and
            $_.Length -gt 32768
        }
    }
    if (-not $filtered -or $filtered.Count -eq 0) {
        $filtered = Get-ChildItem -Path $targetDir -Recurse -Filter '*.bin' -ErrorAction SilentlyContinue | Where-Object {
            $_.Name -ne 'dep-graph.bin' -and
            $_.Name -ne 'bootloader.bin' -and
            $_.Name -ne 'libespidf.bin' -and
            $_.FullName -notlike '*incremental*' -and
            $_.FullName -notlike '*build\esp-idf-sys*' -and
            $_.Length -gt 32768
        }
    }
    if ($filtered -and $filtered.Count -gt 0) {
        $projBase = [System.IO.Path]::GetFileNameWithoutExtension($binaryPath)
        # Prefer files that match project base name or common app names
        $preferredNameMatches = $filtered | Where-Object { $_.Name -match $projBase -or ($_.Name -match '(?i)app|factory') }

        if ($preferredNameMatches -and $preferredNameMatches.Count -gt 0) {
            $best = $preferredNameMatches | Sort-Object Length -Descending | Select-Object -First 1
        } else {
            $releasePref = $filtered | Where-Object { $_.FullName -match '\\release\\' }
            if ($releasePref -and $releasePref.Count -gt 0) {
                $best = $releasePref | Sort-Object Length -Descending | Select-Object -First 1
            } else {
                $best = $filtered | Sort-Object Length -Descending | Select-Object -First 1
            }
        }
        Write-Host "Found candidate app bin: $($best.FullName) (size $($best.Length) bytes)" -ForegroundColor Cyan
        $binaryPath = $best.FullName
    } else {
        Write-Host "No suitable .bin found under $targetDir; continuing with ELF path (may fail)." -ForegroundColor Yellow
    }
}

$binSize = (Get-Item $binaryPath).Length
if ($binSize -lt 32768) {
    Write-Host "Error: application binary looks too small ($binSize bytes): $binaryPath" -ForegroundColor Red
    exit 1
}

# Generate merged image (including bootloader, partition and application)
Write-Host "Generating merged image: $binOutputPath" -ForegroundColor Cyan
# Use absolute path for partitions.csv
$espflashArgs = @("save-image", "--chip", $chip, "--merge", "--partition-table", "`"$partitionsCsv`"", $binaryPath, $binOutputPath)
Write-Host "Running: espflash $($espflashArgs -join ' ')" -ForegroundColor DarkCyan
$procInfo = New-Object System.Diagnostics.ProcessStartInfo
$procInfo.FileName = "espflash"
$procInfo.Arguments = $espflashArgs -join ' '
$procInfo.RedirectStandardOutput = $true
$procInfo.RedirectStandardError = $true
$procInfo.UseShellExecute = $false
$procInfo.CreateNoWindow = $true

$proc = [System.Diagnostics.Process]::Start($procInfo)
$stdOut = $proc.StandardOutput.ReadToEnd()
$stdErr = $proc.StandardError.ReadToEnd()
$proc.WaitForExit()

if ($firstBootPath) {
    Write-Host "Using bootloader: $firstBootPath" -ForegroundColor Cyan
    try {
        if (Test-Path $firstBootPath) {
            $bootMd5 = (Get-FileHash $firstBootPath -Algorithm MD5).Hash
            Write-Host ("Bootloader MD5: {0}" -f $bootMd5) -ForegroundColor Cyan
        } else {
            Write-Host "Bootloader path does not exist to compute MD5." -ForegroundColor Yellow
        }
    } catch {
        Write-Host ("Warning: failed to compute bootloader MD5: {0}" -f $_) -ForegroundColor Yellow
    }
} else { Write-Host "No local bootloader file was chosen (espflash may use embuild cache)." -ForegroundColor Yellow }

Write-Host "--- espflash stdout ---" -ForegroundColor DarkCyan
Write-Host $stdOut
if ($stdErr) {
   Write-Host "--- espflash stderr ---" -ForegroundColor DarkCyan
   Write-Host $stdErr
}

if ($proc.ExitCode -eq 0) {
    # Optional: if local esptool.exe or esptool exists in system path, validate merged.bin with image_info
    $esptoolCmd = $null
    if (Test-Path (Join-Path $PSScriptRoot 'esptool.exe')) { $esptoolCmd = Join-Path $PSScriptRoot 'esptool.exe' }
    elseif (Get-Command esptool -ErrorAction SilentlyContinue) { $esptoolCmd = 'esptool' }
    if ($esptoolCmd) {
        # Create temp slice if merged image has leading 0xFF and contains ESP magic 0xE9
        $tempSlice = $null
        try {
            $bytes = [System.IO.File]::ReadAllBytes($binOutputPath)
            if ($bytes.Length -gt 0 -and $bytes[0] -eq 0xFF) {
                $foundOffset = $null
                $maxSearch = [Math]::Min($bytes.Length, 4 * 1024 * 1024)
                for ($i = 0; $i -lt $maxSearch; $i++) {
                    if ($bytes[$i] -eq 0xE9) { $foundOffset = $i; break }
                }
                if ($foundOffset -ne $null) {
                    Write-Host ('Detected ESP image magic at offset 0x{0:X} (decimal {1}). Extracting slice to temp file for esptool validation.' -f $foundOffset,$foundOffset) -ForegroundColor Cyan
                    $tempSlice = Join-Path $PSScriptRoot ('temp_image_offset_{0:X}.bin' -f $foundOffset)
                    try {
                        $outStream = [System.IO.File]::OpenWrite($tempSlice)
                        $outStream.Write($bytes,$foundOffset,$bytes.Length - $foundOffset)
                        $outStream.Close()
                    } catch {
                        Write-Host ('Warning: failed to write temporary slice file: {0}' -f $_) -ForegroundColor Yellow
                        $tempSlice = $null
                    }
                } else {
                    Write-Host "No ESP image magic (0xE9) found within first 4MB; running image_info on full merged image." -ForegroundColor Yellow
                }
            }
        } catch {
            Write-Host ('Warning: failed to inspect merged image for magic bytes: {0}' -f $_) -ForegroundColor Yellow
        }

        $imgArgs = if ($tempSlice) { "image_info $tempSlice" } else { "image_info $binOutputPath" }
        Write-Host ('Running image validation: {0} {1}' -f $esptoolCmd,$imgArgs) -ForegroundColor DarkCyan
        $imgInfoProc = New-Object System.Diagnostics.ProcessStartInfo
        $imgInfoProc.FileName = $esptoolCmd
        $imgInfoProc.Arguments = $imgArgs
        $imgInfoProc.RedirectStandardOutput = $true
        $imgInfoProc.RedirectStandardError = $true
        $imgInfoProc.UseShellExecute = $false
        $imgInfoProc.CreateNoWindow = $true
        $imgProc = [System.Diagnostics.Process]::Start($imgInfoProc)
        $imgOut = $imgProc.StandardOutput.ReadToEnd()
        $imgErr = $imgProc.StandardError.ReadToEnd()
        $imgProc.WaitForExit()
        if ($imgProc.ExitCode -ne 0) {
            Write-Host "Image validation failed! esptool returned exit code $($imgProc.ExitCode)" -ForegroundColor Red
            Write-Host "esptool stdout:" -ForegroundColor DarkCyan
            Write-Host $imgOut
            if ($imgErr) { Write-Host "esptool stderr:" -ForegroundColor DarkCyan; Write-Host $imgErr }
            if ($tempSlice) { Remove-Item $tempSlice -ErrorAction SilentlyContinue }
            exit $imgProc.ExitCode
        } else {
            Write-Host $imgOut
            if ($tempSlice) { Remove-Item $tempSlice -ErrorAction SilentlyContinue }
        }
    }
    Write-Host "Using bootloader: $firstBootPath" -ForegroundColor Cyan
    try {
        if (Test-Path $firstBootPath) {
            $bootMd5 = (Get-FileHash $firstBootPath -Algorithm MD5).Hash
            Write-Host ("Bootloader MD5: {0}" -f $bootMd5) -ForegroundColor Cyan
        } else {
            Write-Host "Bootloader path does not exist to compute MD5." -ForegroundColor Yellow
        }
    } catch {
        Write-Host ("Warning: failed to compute bootloader MD5: {0}" -f $_) -ForegroundColor Yellow
    }
    Write-Host "Partitions CSV: $partitionsCsv" -ForegroundColor Cyan
    Write-Host "Merged image: $binOutputPath" -ForegroundColor Cyan
    Write-Host "ESP32-S3 image generated successfully: $binOutputPath" -ForegroundColor Green
} else {
    Write-Host "Image generation failed! Exit code: $($proc.ExitCode)" -ForegroundColor Red
    exit $proc.ExitCode
}
