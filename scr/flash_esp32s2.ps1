# ESP32-S2 专用烧录脚本

param(
    [string]$Port = $null
)

Write-Host "Building and flashing for ESP32-S2..." -ForegroundColor Green

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

# 从 .cargo/config.toml 读取目标路径并转换为绝对路径
$configContent = Get-Content ".cargo\config.toml" -Raw
if ($configContent -match 'target-dir\s*=\s*"([^"]+)"') {
    $targetDir = $matches[1].Replace('/', '\\')
    # 如果不是绝对路径，则转换为绝对路径
    if (-not [System.IO.Path]::IsPathRooted($targetDir)) {
        $targetDir = Join-Path $projectRoot $targetDir
    }
    Write-Host "Target directory: $targetDir" -ForegroundColor Cyan
} else {
    $targetDir = Join-Path $projectRoot "target"
    Write-Host "Using default target directory: $targetDir" -ForegroundColor Yellow
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
    if (Test-Path $p) { $firstBootPath = $p; break }
}

# Prefer the bootloader produced under the target's release folder if present
$preferredBoot = Join-Path $targetDir "${target}\release\bootloader.bin"
if (Test-Path $preferredBoot) { $firstBootPath = $preferredBoot }

# Copy to build\bootloader to ensure espflash picks the expected bootloader
if ($firstBootPath) {
    $buildBootDir = Join-Path $projectRoot 'build\bootloader'
    if (-not (Test-Path $buildBootDir)) { New-Item -ItemType Directory -Path $buildBootDir | Out-Null }
    $usedBootPath = Join-Path $buildBootDir 'bootloader.bin'
    Copy-Item -Path $firstBootPath -Destination $usedBootPath -Force
    $firstBootPath = $usedBootPath
}

if (-not (Test-Path $binaryPath)) {
    Write-Host "Error: Binary not found at $binaryPath" -ForegroundColor Red
    exit 1
}

# 生成合并镜像并捕获 espflash 输出（只打印最终使用的 paths）
# 检查应用二进制
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
    # Prefer .bin under the specific target's release/debug folders, exclude incremental and dep-graph
    $filtered = @()
    $targetReleasePath = Join-Path $targetDir "$target\release"
    $targetDebugPath = Join-Path $targetDir "$target\debug"
    $preferPaths = @($targetReleasePath, $targetDebugPath) | Where-Object { Test-Path $_ }
    foreach ($p in $preferPaths) {
        $filtered += Get-ChildItem -Path $p -Recurse -Filter '*.bin' -ErrorAction SilentlyContinue | Where-Object { $_.Name -ne 'dep-graph.bin' -and $_.FullName -notlike '*incremental*' -and $_.Name -notlike 'bootloader.bin' -and $_.Length -gt 32768 }
    }
    if (-not $filtered -or $filtered.Count -eq 0) {
        $filtered = Get-ChildItem -Path $targetDir -Recurse -Filter '*.bin' -ErrorAction SilentlyContinue | Where-Object { $_.Name -ne 'dep-graph.bin' -and $_.FullName -notlike '*incremental*' -and $_.Name -notlike 'bootloader.bin' -and $_.Length -gt 32768 }
    }
    if ($filtered -and $filtered.Count -gt 0) {
        $projBase = [System.IO.Path]::GetFileNameWithoutExtension($binaryPath)
        $preferredNameMatches = $filtered | Where-Object { ($_.Name -match $projBase) -or ($_.Name -match '(?i)app|factory') }
        if ($preferredNameMatches -and $preferredNameMatches.Count -gt 0) {
            $best = $preferredNameMatches | Sort-Object Length -Descending | Select-Object -First 1
        } else {
            $releasePref = $filtered | Where-Object { $_.FullName -match '\\release\\' }
            if ($releasePref -and $releasePref.Count -gt 0) { $best = $releasePref | Sort-Object Length -Descending | Select-Object -First 1 } else { $best = $filtered | Sort-Object Length -Descending | Select-Object -First 1 }
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

Write-Host "Generating merged image: $binOutputPath" -ForegroundColor Cyan
$espflashArgs = @("save-image", "--chip", $chip, "--merge", "--partition-table", "partitions.csv", $binaryPath, $binOutputPath)
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

if ($proc.ExitCode -ne 0) {
    Write-Host "Image generation failed! Exit code: $($proc.ExitCode)" -ForegroundColor Red
    Write-Host "--- espflash stdout ---" -ForegroundColor DarkCyan
    Write-Host $stdOut
    if ($stdErr) { Write-Host "--- espflash stderr ---" -ForegroundColor DarkCyan; Write-Host $stdErr }
    exit $proc.ExitCode
}

# post-merge validation
$esptoolCmd = $null
if (Test-Path (Join-Path $PSScriptRoot 'esptool.exe')) { $esptoolCmd = Join-Path $PSScriptRoot 'esptool.exe' }
elseif (Get-Command esptool -ErrorAction SilentlyContinue) { $esptoolCmd = 'esptool' }
if ($esptoolCmd) {
    Write-Host "Running image validation: $esptoolCmd image_info $binOutputPath" -ForegroundColor DarkCyan
    $imgInfoProc = New-Object System.Diagnostics.ProcessStartInfo
    $imgInfoProc.FileName = $esptoolCmd
    $imgInfoProc.Arguments = "image_info $binOutputPath"
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
        exit $imgProc.ExitCode
    } else {
        Write-Host $imgOut
    }
}

if ($firstBootPath) { Write-Host "Using bootloader: $firstBootPath" -ForegroundColor Cyan }
else { Write-Host "No local bootloader file was chosen (espflash may use embuild cache)." -ForegroundColor Yellow }

Write-Host "Partitions CSV: $partitionsCsv" -ForegroundColor Cyan
Write-Host "Merged image: $binOutputPath" -ForegroundColor Cyan

Write-Host "--- espflash stdout ---" -ForegroundColor DarkCyan
Write-Host $stdOut
if ($stdErr) { Write-Host "--- espflash stderr ---" -ForegroundColor DarkCyan; Write-Host $stdErr }

Write-Host "Image generated successfully!" -ForegroundColor Green

# 合并镜像从0x0地址开始包含所有内容，无需单独烧录bootloader和partition
# pip install esptool -i https://pypi.tuna.tsinghua.edu.cn/simple

$tool = ".\esptool.exe"

# 列出并记录可用串口，便于调试
$availablePorts = [System.IO.Ports.SerialPort]::GetPortNames()
Write-Host "Detected serial ports: $($availablePorts -join ', ')" -ForegroundColor Cyan

# 选择端口逻辑：优先使用用户提供的 -Port 参数，其次优先使用 COM6，再回退到第一个可用端口
$selectPort = $null
if ($Port -and $availablePorts -contains $Port) {
    Write-Host "Using user-specified port: $Port" -ForegroundColor Green
    $selectPort = $Port
} elseif ($availablePorts -contains 'COM6') {
    Write-Host "Preferring COM6" -ForegroundColor Green
    $selectPort = 'COM6'
} elseif ($availablePorts.Count -gt 0) {
    Write-Host "No COM6 found; using first available port: $($availablePorts[0])" -ForegroundColor Yellow
    $selectPort = $availablePorts[0]
} else {
    Write-Host "No serial ports detected. Please connect device and try again." -ForegroundColor Red
    exit 1
}

Write-Host "Using port: $selectPort" -ForegroundColor Cyan
Write-Host "Flashing merged image (bootloader, partition and app)..." -ForegroundColor Cyan

& $tool -p $selectPort --before default_reset --after hard_reset --chip $chip write_flash --flash_mode dio --flash_size 4MB 0x0 $binOutputPath

if ($LASTEXITCODE -eq 0) {
    Write-Host "Flashing completed!" -ForegroundColor Green
    Write-Host "Starting serial monitor..." -ForegroundColor Cyan
    & ".\monitor.ps1" -p $selectPort
} else {
    Write-Host "Flashing failed!" -ForegroundColor Red
    exit 1
}
