[CmdletBinding()]
param()
$ErrorActionPreference = 'Stop'
$testRoot = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '../..'))
$displayExe = Join-Path $testRoot 'target/windows-idd/windowdeck-display.exe'
$hostExe = Join-Path $testRoot 'target/debug/windowdeck-host.exe'
$evidence = Join-Path $testRoot ('target/driver-frames-test-' + [guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $evidence | Out-Null
Write-Output "Evidence: $evidence"

function Display-Code {
    $info = New-Object Diagnostics.ProcessStartInfo
    $info.FileName = $displayExe; $info.Arguments = '--verify'
    $info.UseShellExecute = $false; $info.CreateNoWindow = $true
    $info.RedirectStandardOutput = $true; $info.RedirectStandardError = $true
    $process = [Diagnostics.Process]::Start($info)
    $null = $process.StandardOutput.ReadToEnd(); $null = $process.StandardError.ReadToEnd()
    $process.WaitForExit(); $code = $process.ExitCode; $process.Dispose()
    return $code
}
function Wait-Inactive {
    $deadline = [DateTime]::UtcNow.AddSeconds(10)
    do {
        $virtual = @(Get-PnpDevice -PresentOnly | Where-Object { $_.InstanceId -match '^SWD\\WindowDeck\\|^DISPLAY\\WND0001\\' })
        if ((Display-Code) -eq 4 -and $virtual.Count -eq 0) { return }
        Start-Sleep -Milliseconds 100
    } while ([DateTime]::UtcNow -lt $deadline)
    throw 'Display or virtual PnP devices remained'
}
if ((Display-Code) -ne 4) { throw 'Close the active display before the frame probe' }
$physical = @(Get-PnpDevice -PresentOnly | Where-Object { $_.Class -in @('Display','Monitor') } | Select-Object -ExpandProperty InstanceId)
$probe = $null
try {
    foreach ($cycle in 1..3) {
        $probe = Start-Process -FilePath $hostExe -ArgumentList '--driver-frame-test' -WindowStyle Hidden -RedirectStandardOutput (Join-Path $evidence "probe-$cycle.stdout.log") -RedirectStandardError (Join-Path $evidence "probe-$cycle.log") -PassThru
        $null = $probe.Handle
        $deadline = [DateTime]::UtcNow.AddSeconds(8)
        while ((Display-Code) -ne 0 -and !$probe.HasExited -and [DateTime]::UtcNow -lt $deadline) { Start-Sleep -Milliseconds 100 }
        if ((Display-Code) -ne 0) { throw 'Probe did not activate the monitor' }
        $properties = @(Get-PnpDeviceProperty -InstanceId 'SWD\WindowDeck\WindowDeckDisplay' -KeyName 'DEVPKEY_Device_DriverVersion','DEVPKEY_Device_DriverInfPath')
        $properties | Select-Object KeyName,Data | ConvertTo-Json | Set-Content (Join-Path $evidence "driver-$cycle.json")
        if (($properties | Where-Object KeyName -eq 'DEVPKEY_Device_DriverVersion').Data -ne '0.1.0.8') { throw 'Wrong installed driver version' }
        if ($cycle -eq 2) {
            Start-Sleep -Milliseconds 500
            $probe.Kill()
        }
        if (!$probe.WaitForExit(30000)) { throw 'Probe timeout' }
        if ($cycle -ne 2 -and $probe.ExitCode -ne 0) {
            Get-Content (Join-Path $evidence "probe-$cycle.log")
            throw 'Frame validation failed'
        }
        $probe.Dispose(); $probe = $null
        Wait-Inactive
        if ($cycle -ne 2) {
            $result = Get-Content (Join-Path $evidence "probe-$cycle.log") | Select-String -Pattern 'driver_frame_probe_passed'
            if (!$result) { throw 'Missing Rust frame validation result' }
            Write-Output $result.Line
        }
        Write-Output "PASS cycle=$cycle (cycle 2 terminates the host)"
    }
    $after = @(Get-PnpDevice -PresentOnly | Where-Object { $_.Class -in @('Display','Monitor') })
    foreach ($id in $physical) {
        if (!($after | Where-Object { $_.InstanceId -eq $id -and $_.Status -eq 'OK' })) { throw "Physical device changed: $id" }
    }
    Write-Output 'PASS frame transfer, host termination, reconnection and physical devices'
} finally {
    if ($probe) { if (!$probe.HasExited) { $probe.Kill(); $probe.WaitForExit() }; $probe.Dispose() }
}
