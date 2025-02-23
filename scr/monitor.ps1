# 定义脚本参数
param (
    [string]$p  # 接收串口名称参数
)

# 检查是否提供了串口名称
if (-not $p) {
    Write-Host "Usage: .\monitor.ps1 -p COMx"
    exit
}

while (1){
    try {

        Write-Host "Opening $p..."

        $port = New-Object System.IO.Ports.SerialPort $p, 115200, None, 8, one
        
        $port.Open()

        Write-Host "Listening $p..."
        
        for(;;)
        {
            if ($port.IsOpen)
            {
                $data = $port.ReadLine()
                Write-Output $data
            }
        }
    }
    catch {
        try {
            $port.Close();
        }
        catch {
            
        }
        Write-Host "Error: $_"
        Start-Sleep -Milliseconds 3000
    }
}