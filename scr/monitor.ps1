# 定义脚本参数
param (
    [string]$p  # 接收串口名称参数
)

# 检查是否提供了串口名称
if (-not $p) {
    Write-Host "Usage: .\monitor.ps1 -p COMx"
    exit
}

# 设置 CTRL+C 处理
$port = $null
$running = $true

# 注册 CTRL+C 处理器
[Console]::TreatControlCAsInput = $false
$cleanupScript = {
    param($sender, $e)
    $script:running = $false
    if ($null -ne $script:port -and $script:port.IsOpen) {
        $script:port.Close()
    }
    Write-Host "`nMonitor stopped."
    exit 0
}

# 注册取消事件
try {
    [Console]::CancelKeyPress.Add($cleanupScript)
} catch {
    # 忽略注册失败
}

while ($running){
    try {
        Write-Host "Opening $p..."

        $port = New-Object System.IO.Ports.SerialPort $p, 115200, None, 8, one
        $port.ReadTimeout = 1000  # 1秒超时，允许响应CTRL+C
        $port.Open()

        Write-Host "Listening $p... (Press CTRL+C to stop)"
        
        while($running -and $port.IsOpen)
        {
            try {
                $data = $port.ReadLine()
                Write-Output $data
            }
            catch [System.TimeoutException] {
                # 超时是正常的，继续循环以检查 $running 状态
                continue
            }
        }
    }
    catch {
        if ($null -ne $port -and $port.IsOpen) {
            try {
                $port.Close()
            }
            catch {
                # 忽略关闭错误
            }
        }
        if ($running) {
            Write-Host "Error: $_"
            Start-Sleep -Milliseconds 3000
        }
    }
}

# 清理
if ($null -ne $port -and $port.IsOpen) {
    $port.Close()
}