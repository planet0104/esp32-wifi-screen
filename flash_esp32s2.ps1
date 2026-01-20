## Flash script simplified: only flash merged image and open serial monitor.

param(
    [string]$Port = $null
)

Write-Host "Flashing merged image for ESP32-S2..." -ForegroundColor Green

$chip = "esp32s2"
# merged image and project root
$binOutputPath = "esp32-wifi-screen-$chip-merged.bin"
$projectRoot = $PSScriptRoot

# partitions CSV path (used to find user/data partitions to preserve)
$partitionsCsv = Join-Path $projectRoot 'partitions.csv'

# Ensure merged image exists
if (-not (Test-Path $binOutputPath)) {
    Write-Host "Error: merged image not found: $binOutputPath" -ForegroundColor Red
    Write-Host "Run build_esp32s2.ps1 first to generate the merged image." -ForegroundColor Yellow
    exit 1
}

# Optional: validate image with esptool (handle 0xFF padding by slicing)
$esptoolCmd = $null
if (Test-Path (Join-Path $PSScriptRoot 'esptool.exe')) { $esptoolCmd = Join-Path $PSScriptRoot 'esptool.exe' }
elseif (Get-Command esptool -ErrorAction SilentlyContinue) { $esptoolCmd = 'esptool' }
if ($esptoolCmd) {
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

Write-Host "Merged image: $binOutputPath" -ForegroundColor Cyan

# Helper: parse partition table inside merged image at 0x8000 to find offset/size for a label
function Get-PartitionOffsetSize {
    param(
        [string]$imagePath,
        [string]$label
    )
    try {
        $fs = [System.IO.File]::OpenRead($imagePath)
        $fs.Position = 0x8000
        $buf = New-Object byte[] 4096
        $read = $fs.Read($buf,0,$buf.Length)
        $fs.Close()
    } catch {
        return $null
    }
    for ($i=0; $i -lt $read-16; $i++) {
        if ($buf[$i] -eq 0xAA -and $buf[$i+1] -eq 0x50) {
            $entryOffset = [BitConverter]::ToUInt32($buf,$i+4)
            $entrySize = [BitConverter]::ToUInt32($buf,$i+8)
            # read label bytes until null
            $j = $i + 12
            $lblBytes = @()
            while ($j -lt $read -and $buf[$j] -ne 0) { $lblBytes += $buf[$j]; $j++ }
            $lbl = ([System.Text.Encoding]::ASCII).GetString($lblBytes)
            if ($lbl -and ($lbl -ieq $label -or $lbl -imatch $label)) {
                return @{ offset = ('0x{0:X}' -f $entryOffset); size = ('0x{0:X}' -f $entrySize) }
            }
        }
    }
    return $null
}

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

$tool = ".\esptool.exe"

# ESP32-S2使用原生USB-OTG，操作后需要等待设备重新枚举
function Wait-ForPort {
    param(
        [string]$TargetPort,
        [int]$TimeoutSeconds = 15
    )
    $elapsed = 0
    $sleepMs = 500
    Write-Host "Waiting for port $TargetPort to become available..." -ForegroundColor Cyan
    while ($elapsed -lt $TimeoutSeconds) {
        Start-Sleep -Milliseconds $sleepMs
        $elapsed += ($sleepMs / 1000)
        $ports = [System.IO.Ports.SerialPort]::GetPortNames()
        if ($ports -contains $TargetPort) {
            # 额外等待端口稳定
            Start-Sleep -Milliseconds 500
            Write-Host "Port $TargetPort is available." -ForegroundColor Green
            return $true
        }
    }
    Write-Host "Timeout waiting for port $TargetPort" -ForegroundColor Yellow
    return $false
}

# 默认要保留的分区名片段（不区分大小写），可按需调整
$preserveNames = @('nvs','storage','spiffs','fat','littlefs','filesystem','ota','data')
$preserveBackups = @()

if (Test-Path $partitionsCsv) {
    Write-Host "Reading partition table from: $partitionsCsv" -ForegroundColor Cyan
    $lines = Get-Content $partitionsCsv | Where-Object { -not ($_ -match '^\s*#') -and ($_ -match '\S') }
    foreach ($line in $lines) {
        $cols = $line -split ','
        if ($cols.Length -ge 5) {
            $pname = $cols[0].Trim()
            $poffset = $cols[3].Trim()
            $psize = $cols[4].Trim()
            foreach ($pat in $preserveNames) {
                if ($pname -imatch $pat) {
                    if ((-not $poffset) -or ($poffset -eq '')) {
                        $resolved = Get-PartitionOffsetSize -imagePath $binOutputPath -label $pname
                        if ($resolved) { $poffset = $resolved.offset; $psize = $resolved.size }
                    }
                    $backupFile = Join-Path $PSScriptRoot ("preserve_{0}.bin" -f $pname)
                    Write-Host ("Backing up partition '{0}' offset {1} size {2} -> {3}" -f $pname,$poffset,$psize,$backupFile) -ForegroundColor Yellow
                    # ESP32-S2使用no_reset避免USB断开
                    $procInfo = New-Object System.Diagnostics.ProcessStartInfo
                    $procInfo.FileName = $tool
                    $procInfo.Arguments = ('-p {0} --before default_reset --after no_reset --chip {1} read_flash {2} {3} {4}' -f $selectPort,$chip,$poffset,$psize,$backupFile)
                    $procInfo.RedirectStandardOutput = $true
                    $procInfo.RedirectStandardError = $true
                    $procInfo.UseShellExecute = $false
                    $procInfo.CreateNoWindow = $true
                    $proc = [System.Diagnostics.Process]::Start($procInfo)
                    $out = $proc.StandardOutput.ReadToEnd()
                    $err = $proc.StandardError.ReadToEnd()
                    $proc.WaitForExit()
                    if ($proc.ExitCode -ne 0) {
                        Write-Host ("Warning: failed to back up partition {0}: exit {1}" -f $pname,$proc.ExitCode) -ForegroundColor Yellow
                        Write-Host $out
                        if ($err) { Write-Host $err }
                    } else {
                        $preserveBackups += @{ name = $pname; offset = $poffset; file = $backupFile }
                    }
                    # 等待端口稳定
                    Start-Sleep -Milliseconds 300
                    break
                }
            }
        }
    }
} else {
    Write-Host "No partitions.csv found; cannot auto-detect partitions to preserve." -ForegroundColor Yellow
}

Write-Host "Flashing merged image (bootloader, partition and app) to 0x0..." -ForegroundColor Cyan
# 使用 --no-compress 避免压缩写入错误
& $tool -p $selectPort --before default_reset --after no_reset --chip $chip write_flash --no-compress --flash_mode dio --flash_size 4MB 0x0 $binOutputPath

if ($LASTEXITCODE -ne 0) {
    Write-Host "Flashing failed!" -ForegroundColor Red
    exit 1
}

# 等待设备稳定
Start-Sleep -Milliseconds 500

# 恢复备份的分区
foreach ($b in $preserveBackups) {
    $roffset = $b.offset
    if (-not $roffset -or $roffset -eq '') {
        $resolved = Get-PartitionOffsetSize -imagePath $binOutputPath -label $b.name
        if ($resolved) { $roffset = $resolved.offset }
    }
    Write-Host ("Restoring partition '{0}' from {1} to offset {2}..." -f $b.name,$b.file,$roffset) -ForegroundColor Yellow
    # 使用no_reset避免每次恢复后USB断开
    $procInfo = New-Object System.Diagnostics.ProcessStartInfo
    $procInfo.FileName = $tool
    $procInfo.Arguments = ('-p {0} --before default_reset --after no_reset --chip {1} write_flash --no-compress {2} {3}' -f $selectPort,$chip,$roffset,$b.file)
    $procInfo.RedirectStandardOutput = $true
    $procInfo.RedirectStandardError = $true
    $procInfo.UseShellExecute = $false
    $procInfo.CreateNoWindow = $true
    $proc = [System.Diagnostics.Process]::Start($procInfo)
    $out = $proc.StandardOutput.ReadToEnd()
    $err = $proc.StandardError.ReadToEnd()
    $proc.WaitForExit()
    if ($proc.ExitCode -ne 0) {
        Write-Host ("Warning: failed to restore partition {0}: exit {1}" -f $b.name,$proc.ExitCode) -ForegroundColor Yellow
        Write-Host $out
        if ($err) { Write-Host $err }
    }
    # 等待设备稳定
    Start-Sleep -Milliseconds 300
}

# 所有操作完成后执行硬复位启动设备
Write-Host "Performing hard reset to boot the device..." -ForegroundColor Cyan
& $tool -p $selectPort --before default_reset --after hard_reset --chip $chip read_mac 2>$null | Out-Null
Start-Sleep -Milliseconds 1000

# 等待端口重新枚举
$portReady = Wait-ForPort -TargetPort $selectPort -TimeoutSeconds 15
if (-not $portReady) {
    Write-Host "Warning: Port not available after reset, monitor may fail." -ForegroundColor Yellow
}

Write-Host "Flashing completed!" -ForegroundColor Green
Write-Host "Starting serial monitor..." -ForegroundColor Cyan
& ".\monitor.ps1" -p $selectPort
