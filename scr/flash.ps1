cargo build --release

# 从 .cargo/config.toml 读取目标路径
$configContent = Get-Content ".cargo\config.toml" -Raw
if ($configContent -match 'target-dir\s*=\s*"([^"]+)"') {
    $targetDir = $matches[1].Replace('/', '\\')
    Write-Host "Target directory: $targetDir"
} else {
    $targetDir = "target"
    Write-Host "Using default target directory: $targetDir"
}

if ($configContent -match 'target\s*=\s*"([^"]+)"') {
    $targetTriple = $matches[1]
    Write-Host "Target triple: $targetTriple"
} else {
    $targetTriple = "xtensa-esp32s2-espidf"
    Write-Host "Using default target triple: $targetTriple"
}

$binaryPath = "$targetDir\\$targetTriple\\release\\esp32-wifi-screen"
$binOutputPath = "$targetDir\\$targetTriple\\release\\esp32-wifi-screen.bin"

if (-not (Test-Path $binaryPath)) {
    Write-Host "Error: Binary not found at $binaryPath"
    exit 1
}

espflash save-image --chip esp32s2 --partition-table partitions.csv $binaryPath $binOutputPath

# pip install esptool -i https://pypi.tuna.tsinghua.edu.cn/simple

$tool = ".\esptool.exe"

$availablePorts = [System.IO.Ports.SerialPort]::getportnames()

$portsToCheck = @("COM3", "COM10", "COM6", "COM5")
$selectPort = $portsToCheck[0];

foreach ($port in $portsToCheck) {
    if ($availablePorts -contains $port) {
        Write-Host "Port $port Exist"
        $selectPort = $port;
        break
    } else {
        Write-Host "Port $port Not Exist."
    }
}

& $tool -p $selectPort --before default_reset --after hard_reset --chip esp32s2 write_flash --flash_mode dio --flash_size detect 0x1000 .\bootloader.bin 0x8000 .\partitions.bin 0x10000 $binOutputPath

& ".\monitor.ps1" -p $selectPort