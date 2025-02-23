cargo build --release

espflash save-image --chip esp32s2 --partition-table partitions.csv target/xtensa-esp32s2-espidf/release/esp32-wifi-screen target/xtensa-esp32s2-espidf/release/esp32-wifi-screen.bin

# pip install esptool -i https://pypi.tuna.tsinghua.edu.cn/simple

$tool = "./esptool.exe"

$availablePorts = [System.IO.Ports.SerialPort]::getportnames()

$portsToCheck = @("COM3", "COM10")
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

& $tool -p $selectPort --before default_reset --after hard_reset --chip esp32s2 write_flash --flash_mode dio --flash_size detect 0x10000 target/xtensa-esp32s2-espidf/release/esp32-wifi-screen.bin

& ".\monitor.ps1" $selectPort