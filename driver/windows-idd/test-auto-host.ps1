[CmdletBinding()]
param([int]$Port = 48151, [switch]$DriverCpu)
$ErrorActionPreference = 'Stop'
$testRoot = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '../..'))
$displayExe = Join-Path $testRoot 'target/windows-idd/windowdeck-display.exe'
$hostExe = Join-Path $testRoot 'target/debug/windowdeck-host.exe'
$testDirectory = Join-Path $testRoot ('target/auto-host-test-' + [guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $testDirectory | Out-Null

function Display-Code {
    $info = New-Object Diagnostics.ProcessStartInfo
    $info.FileName = $displayExe
    $info.Arguments = '--verify'
    $info.UseShellExecute = $false
    $info.CreateNoWindow = $true
    $info.RedirectStandardOutput = $true
    $info.RedirectStandardError = $true
    $process = [Diagnostics.Process]::Start($info)
    $null = $process.StandardOutput.ReadToEnd()
    $null = $process.StandardError.ReadToEnd()
    $process.WaitForExit()
    $code = $process.ExitCode
    $process.Dispose()
    return $code
}

function Wait-Inactive {
    $deadline = [DateTime]::UtcNow.AddSeconds(10)
    do {
        if ((Display-Code) -eq 4) { return }
        Start-Sleep -Milliseconds 100
    } while ([DateTime]::UtcNow -lt $deadline)
    throw 'Virtual monitor remained active'
}

function Read-Message($Stream) {
    $header = New-Object byte[] 4
    Read-Exact $Stream $header
    [Array]::Reverse($header)
    $size = [BitConverter]::ToUInt32($header, 0)
    if ($size -lt 3 -or $size -gt 65536) { throw "Invalid message size $size" }
    $payload = New-Object byte[] $size
    Read-Exact $Stream $payload
    return ,$payload
}

function Read-Exact($Stream, [byte[]]$Buffer) {
    $offset = 0
    while ($offset -lt $Buffer.Length) {
        $count = $Stream.Read($Buffer, $offset, $Buffer.Length - $offset)
        if ($count -eq 0) { throw 'Unexpected EOF' }
        $offset += $count
    }
}

$server = $null
$client = $null
$encoderIds = @()
try {
    if ((Display-Code) -ne 4) { throw 'Close the active session before running this test' }
    $physical = @(Get-PnpDevice -PresentOnly | Where-Object { $_.Class -in @('Display','Monitor') } | Select-Object -ExpandProperty InstanceId)
    foreach ($scenario in @('encoder-missing', 'disconnect-reconnect', 'host-terminated')) {
        $info = New-Object Diagnostics.ProcessStartInfo
        $info.FileName = $hostExe
        $mode = if ($DriverCpu) { '--driver-h264' } else { '--auto-virtual-h264' }
        $info.Arguments = "$mode 127.0.0.1:$Port"
        $info.UseShellExecute = $false
        $info.CreateNoWindow = $true
        $info.RedirectStandardError = $true
        $info.RedirectStandardOutput = $true
        if ($scenario -eq 'encoder-missing') { $info.EnvironmentVariables['PATH'] = $testDirectory }
        $server = [Diagnostics.Process]::Start($info)
        $serverLog = $server.StandardError.ReadToEndAsync()
        Start-Sleep -Milliseconds 300
        if ($server.HasExited) { throw "Host did not start: $($serverLog.Result)" }
        $client = New-Object Net.Sockets.TcpClient
        $client.Connect('127.0.0.1', $Port)
        $stream = $client.GetStream()
        $stream.ReadTimeout = 10000
        $stream.WriteTimeout = 10000
        # Protocol v3: Hello with an empty version, then 1280x800@60, H.264.
        [byte[]]$hello = @(0,0,0,5,0,3,1,0,0)
        $stream.Write($hello, 0, $hello.Length)
        if ((Read-Message $stream)[2] -ne 1) { throw 'Expected Hello' }
        [byte[]]$caps = @(0,0,0,10,0,3,2,5,0,3,32,0,60,2)
        $stream.Write($caps, 0, $caps.Length)
        if ((Read-Message $stream)[2] -ne 3) { throw 'Expected SessionConfig' }
        if ((Read-Message $stream)[2] -ne 4) { throw 'Expected Start' }
        if ($scenario -eq 'encoder-missing') {
            if ($stream.ReadByte() -ne -1) { throw 'Unexpected video with missing encoder' }
            Wait-Inactive
            if ($server.HasExited) { throw 'Encoder failure stopped the host' }
            # A new Hello proves cleanup completed and the accept loop resumed.
            $probe = New-Object Net.Sockets.TcpClient
            try {
                $probe.Connect('127.0.0.1', $Port)
                $probeStream = $probe.GetStream()
                $probeStream.ReadTimeout = 10000
                $probeStream.Write($hello, 0, $hello.Length)
                if ((Read-Message $probeStream)[2] -ne 1) { throw 'Host did not accept a new connection after encoder failure' }
            } finally { $probe.Close() }
        } elseif ($scenario -eq 'disconnect-reconnect') {
            foreach ($cycle in 1..2) {
                if ($cycle -eq 2) {
                    $client = New-Object Net.Sockets.TcpClient
                    $client.Connect('127.0.0.1', $Port)
                    $stream = $client.GetStream()
                    $stream.ReadTimeout = 10000
                    $stream.Write($hello, 0, $hello.Length)
                    if ((Read-Message $stream)[2] -ne 1) { throw 'Expected reconnect Hello' }
                    $stream.Write($caps, 0, $caps.Length)
                    if ((Read-Message $stream)[2] -ne 3) { throw 'Expected reconnect configuration' }
                    if ((Read-Message $stream)[2] -ne 4) { throw 'Expected reconnect Start' }
                }
                $videoPath = Join-Path $testDirectory "video-$cycle.ts"
                $video = [IO.File]::Create($videoPath)
                try {
                    $until = [DateTime]::UtcNow.AddSeconds(8)
                    do {
                        $packet = Read-Message $stream
                        if ($packet[2] -ne 10 -or $packet.Length -le 32) { throw 'Expected video chunk' }
                        $video.Write($packet, 32, $packet.Length - 32)
                    } while ([DateTime]::UtcNow -lt $until)
                } finally { $video.Dispose() }
                $children = @(Get-CimInstance Win32_Process | Where-Object {
                    $_.ParentProcessId -eq $server.Id -and $_.Name -in @('ffmpeg.exe','windowdeck-display.exe')
                } | Select-Object -ExpandProperty ProcessId)
                $client.Close()
                Wait-Inactive
                foreach ($childId in $children) {
                    $child = Get-Process -Id $childId -ErrorAction SilentlyContinue
                    if ($child -and !$child.WaitForExit(10000)) { throw 'Session child survived disconnect' }
                }
                # Decode complete early frames; the final TCP chunk may end mid-packet.
                $decodeProgress = Join-Path $testDirectory "decode-$cycle.log"
                & ffmpeg -hide_banner -loglevel error -xerror -progress $decodeProgress -i $videoPath -frames:v 120 -f null -
                if ($LASTEXITCODE) { throw 'H.264 decoding failed' }
                if (!(Select-String -LiteralPath $decodeProgress -Pattern '^frame=120$' -Quiet)) { throw 'Fewer than 120 decoded frames' }
                $metadata = & ffprobe -v error -select_streams v:0 -show_entries stream=codec_name,width,height,r_frame_rate -of json $videoPath
                $metadata | Set-Content (Join-Path $testDirectory "video-$cycle.json")
                $format = ($metadata | ConvertFrom-Json).streams[0]
                if ($format.codec_name -ne 'h264' -or $format.width -ne 1280 -or $format.height -ne 800 -or $format.r_frame_rate -ne '60/1') {
                    throw 'Unexpected encoded format'
                }
            }
        } else {
            if ((Read-Message $stream)[2] -ne 10) { throw 'Expected video' }
            if ((Display-Code) -ne 0) { throw 'Monitor not active during video' }
            $encoderIds = @(Get-CimInstance Win32_Process | Where-Object {
                $_.Name -eq 'ffmpeg.exe' -and $_.ParentProcessId -eq $server.Id
            } | Select-Object -ExpandProperty ProcessId)
            if ($encoderIds.Count -ne 1) { throw 'Expected one encoder belonging to this host' }
            $server.Kill()
            $server.WaitForExit()
            Wait-Inactive
            foreach ($encoderId in $encoderIds) {
                $encoder = Get-Process -Id $encoderId -ErrorAction SilentlyContinue
                if ($encoder -and !$encoder.WaitForExit(10000)) { throw 'Encoder survived host termination' }
            }
        }
        $client.Close()
        $client = $null
        if (!$server.HasExited) { $server.Kill(); $server.WaitForExit() }
        Set-Content -LiteralPath (Join-Path $testDirectory "$scenario.log") -Value $serverLog.Result
        $server.Dispose()
        $server = $null
        Write-Output "PASS: $scenario"
    }
    $after = @(Get-PnpDevice -PresentOnly | Where-Object { $_.Class -in @('Display','Monitor') })
    $after | Select-Object InstanceId,Status | ConvertTo-Json | Set-Content (Join-Path $testDirectory 'devices-after.json')
    foreach ($id in $physical) {
        if (!($after | Where-Object { $_.InstanceId -eq $id -and $_.Status -eq 'OK' })) { throw "Physical device changed: $id" }
    }
    if ($after | Where-Object { $_.InstanceId -match '^SWD\\WindowDeck\\|^DISPLAY\\WND0001\\' }) { throw 'Virtual PnP device survived tests' }
    Write-Output "Evidence: $testDirectory"
} finally {
    if ($client) { $client.Close() }
    if ($server) {
        if (!$server.HasExited) { $server.Kill(); $server.WaitForExit() }
        if ($serverLog.Wait(2000)) { Set-Content -LiteralPath (Join-Path $testDirectory 'failure.log') -Value $serverLog.Result }
        $server.Dispose()
    }
    foreach ($encoderId in $encoderIds) {
        $encoder = Get-Process -Id $encoderId -ErrorAction SilentlyContinue
        if ($encoder -and $encoder.ProcessName -eq 'ffmpeg') { $encoder.Kill(); $encoder.WaitForExit() }
    }
}
