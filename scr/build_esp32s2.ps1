# ESP32-S2 专用构建脚本

Write-Host "Building project for ESP32-S2..." -ForegroundColor Green

$chip = "esp32s2"
$target = "xtensa-esp32s2-espidf"
$feature = "esp32s2"

# 设置环境变量 - 使用绝对路径
$env:MCU = "esp32s2"
$projectRoot = $PSScriptRoot
$env:ESP_IDF_SDKCONFIG_DEFAULTS = "$projectRoot\sdkconfig.defaults.esp32s2"
$env:ESP_IDF_VERSION = "v5.3.4"

Write-Host "Environment variables:" -ForegroundColor Cyan
Write-Host "  MCU: $env:MCU"
Write-Host "  ESP_IDF_SDKCONFIG_DEFAULTS: $env:ESP_IDF_SDKCONFIG_DEFAULTS"
Write-Host "  ESP_IDF_VERSION: $env:ESP_IDF_VERSION"

# 使用 --target 和 --features 参数构建
cargo build --release --target $target --no-default-features --features "$feature,experimental"

if ($LASTEXITCODE -ne 0) {
    Write-Host "Build failed!" -ForegroundColor Red
    exit 1
}

Write-Host "ESP32-S2 compilation successful!" -ForegroundColor Green

# 读取target目录配置
$configContent = Get-Content ".cargo\config.toml" -Raw
if ($configContent -match 'target-dir\s*=\s*"([^"]+)"') {
    $targetDir = $matches[1].Replace('/', '\\')
    if (-not [System.IO.Path]::IsPathRooted($targetDir)) {
        $targetDir = Join-Path $projectRoot $targetDir
    }
} else {
    $targetDir = Join-Path $projectRoot "target"
}

$binaryPath = "$targetDir\\$target\\release\\esp32-wifi-screen"
$binOutputPath = "esp32-wifi-screen-$chip-merged.bin"

# 打印 partitions.csv 与 bootloader.bin 的绝对路径，便于排查
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
    if (Test-Path $p) {
        $firstBootPath = $p
        break
    }
}

# Prefer the bootloader produced under the target's release folder if present
$preferredBoot = Join-Path $targetDir "${target}\release\bootloader.bin"
if (Test-Path $preferredBoot) { $firstBootPath = $preferredBoot }

# If we've found a bootloader, copy it to project build/bootloader to ensure espflash uses correct file
if ($firstBootPath) {
    $buildBootDir = Join-Path $projectRoot 'build\bootloader'
    if (-not (Test-Path $buildBootDir)) { New-Item -ItemType Directory -Path $buildBootDir | Out-Null }
    $usedBootPath = Join-Path $buildBootDir ("bootloader-{0}.bin" -f $chip)
    Copy-Item -Path $firstBootPath -Destination $usedBootPath -Force
    $firstBootPath = $usedBootPath
}

if (-not (Test-Path $partitionsCsv)) {
    Write-Host "Partitions CSV not found: $partitionsCsv" -ForegroundColor Red
}

# 检查应用二进制是否存在
if (-not (Test-Path $binaryPath)) {
    Write-Host "Error: application binary not found at: $binaryPath" -ForegroundColor Red
    Write-Host "Ensure the target build produced the binary and rerun the script." -ForegroundColor Yellow
    exit 1
}

# 如果二进制是 ELF（静态可执行），尝试在 target 目录里查找生成的 .bin（esp-idf/embuild 输出）
$magic = @()
try { $magic = [System.IO.File]::ReadAllBytes($binaryPath)[0..3] } catch { $magic = @() }
if ($magic -and ($magic -join ' ' ) -eq "127 69 76 70") {
    Write-Host "Detected ELF executable at $binaryPath; searching for generated .bin under $targetDir..." -ForegroundColor Yellow
    
    # 改进的搜索逻辑：优先查找正确的应用程序 bin 文件
    $filtered = @()
    $targetReleasePath = Join-Path $targetDir "$target\release"
    
    # 首先尝试查找 esp32-wifi-screen.bin（项目名称的 bin）
    $projectBinPath = Join-Path $targetReleasePath "esp32-wifi-screen.bin"
    if (Test-Path $projectBinPath) {
        Write-Host "Found project binary: $projectBinPath" -ForegroundColor Green
        $binaryPath = $projectBinPath
    } else {
        # 如果找不到，搜索符合条件的 bin 文件，但排除 libespidf.bin
        if (Test-Path $targetReleasePath) {
            $filtered = Get-ChildItem -Path $targetReleasePath -Recurse -Filter '*.bin' -ErrorAction SilentlyContinue | Where-Object { 
                $_.Name -ne 'dep-graph.bin' -and 
                $_.Name -ne 'bootloader.bin' -and 
                $_.Name -ne 'libespidf.bin' -and  # 排除 ESP-IDF 库文件
                $_.FullName -notlike '*incremental*' -and 
                $_.FullName -notlike '*build\esp-idf-sys*' -and  # 排除 esp-idf-sys 构建产物
                $_.Length -gt 32768 
            }
        }
        
        if (-not $filtered -or $filtered.Count -eq 0) {
            # 如果还是找不到，扩大搜索范围但保持排除规则
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
            # 优先匹配项目名称
            $preferredNameMatches = $filtered | Where-Object { $_.Name -match $projBase }
            
            if ($preferredNameMatches -and $preferredNameMatches.Count -gt 0) {
                $best = $preferredNameMatches | Sort-Object Length -Descending | Select-Object -First 1
            } else {
                # 否则选择最大的 bin 文件
                $best = $filtered | Sort-Object Length -Descending | Select-Object -First 1
            }
            
            Write-Host "Found candidate app bin: $($best.FullName) (size $($best.Length) bytes)" -ForegroundColor Cyan
            $binaryPath = $best.FullName
        } else {
            Write-Host "No suitable .bin found under $targetDir; will try to use ELF directly (may fail)." -ForegroundColor Yellow
        }
    }
}

$binSize = (Get-Item $binaryPath).Length
if ($binSize -lt 32768) {
    Write-Host "Error: application binary looks too small ($binSize bytes): $binaryPath" -ForegroundColor Red
    exit 1
}

Write-Host "Using application binary: $binaryPath (size: $binSize bytes)" -ForegroundColor Green

# 生成完整合并镜像
Write-Host "Generating merged image: $binOutputPath" -ForegroundColor Cyan
$espflashArgs = @("save-image", "--chip", $chip, "--merge", "--partition-table", "partitions.csv", $binaryPath, $binOutputPath)
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

Write-Host "Partitions CSV: $partitionsCsv" -ForegroundColor Cyan
Write-Host "Merged image: $binOutputPath" -ForegroundColor Cyan

Write-Host "--- espflash stdout ---" -ForegroundColor DarkCyan
Write-Host $stdOut
if ($stdErr) {
   Write-Host "--- espflash stderr ---" -ForegroundColor DarkCyan
   Write-Host $stdErr
}

if ($proc.ExitCode -eq 0) {
    $esptoolCmd = $null
    if (Test-Path (Join-Path $PSScriptRoot 'esptool.exe')) { $esptoolCmd = Join-Path $PSScriptRoot 'esptool.exe' }
    elseif (Get-Command esptool -ErrorAction SilentlyContinue) { $esptoolCmd = 'esptool' }
    if ($esptoolCmd) {
            # If the merged image starts with 0xFF, esptool image_info may reject it because
            # flash images can have 0xFF padding before the actual ESP image. Search the merged
            # file for the first ESP image magic byte 0xE9. If found, create a temporary slice
            # starting at that offset and run esptool on the slice (avoids passing unrecognized
            # --offset flags to different esptool versions).
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
                        $msg = ('Detected ESP image magic at offset 0x{0:X} (decimal {1}). Extracting slice to temp file for esptool validation.' -f $foundOffset,$foundOffset)
                        Write-Host $msg -ForegroundColor Cyan
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
    Write-Host "ESP32-S2 image generated successfully: $binOutputPath" -ForegroundColor Green
} else {
    Write-Host "Image generation failed! Exit code: $($proc.ExitCode)" -ForegroundColor Red
    exit $proc.ExitCode
}