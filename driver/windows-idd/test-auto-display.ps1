[CmdletBinding()]
param()
$ErrorActionPreference = 'Stop'
$testRoot = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '../..'))
$displayExe = Join-Path $testRoot 'target/windows-idd/windowdeck-display.exe'
$testLog = Join-Path $testRoot 'target/windows-idd/auto-display-test.log'

function Get-DisplayCode {
    $info = New-Object Diagnostics.ProcessStartInfo
    $info.FileName = $displayExe
    $info.Arguments = '--verify'
    $info.UseShellExecute = $false
    $info.CreateNoWindow = $true
    $info.RedirectStandardOutput = $true
    $info.RedirectStandardError = $true
    $process = [Diagnostics.Process]::Start($info)
    $result = $process.StandardOutput.ReadToEnd() + $process.StandardError.ReadToEnd()
    $process.WaitForExit()
    $code = $process.ExitCode
    $process.Dispose()
    Add-Content -LiteralPath $testLog -Value "verify=$code $result"
    return $code
}

function Assert-Display([int]$Expected) {
    $code = Get-DisplayCode
    if ($code -ne $Expected) { throw "Expected display status $Expected, got $code" }
}

function Start-Lease {
    $info = New-Object Diagnostics.ProcessStartInfo
    $info.FileName = $displayExe
    $info.Arguments = '--lease'
    $info.UseShellExecute = $false
    $info.CreateNoWindow = $true
    $info.RedirectStandardInput = $true
    $info.RedirectStandardOutput = $true
    $info.RedirectStandardError = $true
    $process = [Diagnostics.Process]::Start($info)
    return $process
}

function Read-Ready($Process) {
    $line = $Process.StandardOutput.ReadLineAsync()
    if (!$line.Wait(10000) -or $line.Result -ne 'READY') {
        if ($Process.HasExited) { Add-Content -LiteralPath $testLog -Value $Process.StandardError.ReadToEnd() }
        throw 'Lease did not report READY'
    }
}

function Wait-Removed {
    $deadline = [DateTime]::UtcNow.AddSeconds(8)
    do {
        if ((Get-DisplayCode) -eq 4) { return }
        Start-Sleep -Milliseconds 100
    } while ([DateTime]::UtcNow -lt $deadline)
    throw 'Display remained active'
}

# Run with an already started elevated --broker in this logon. No installation.
$physical = @(Get-PnpDevice -PresentOnly | Where-Object {
    $_.Class -in @('Display', 'Monitor') -and $_.InstanceId -notmatch 'SWD\\WindowDeck|DISPLAY\\WND0001'
} | Select-Object -ExpandProperty InstanceId)
$lease = $null
$second = $null
Add-Content -LiteralPath $testLog -Value "Start $([DateTime]::UtcNow.ToString('o'))"
try {
    Assert-Display 4
    foreach ($cycle in 1..3) {
        $lease = Start-Lease
        Read-Ready $lease
        Assert-Display 0
        $second = Start-Lease
        if (!$second.WaitForExit(2000) -or $second.ExitCode -eq 0) { throw 'Second lease was not rejected' }
        Add-Content -LiteralPath $testLog -Value $second.StandardError.ReadToEnd()
        $second.Dispose()
        $second = $null
        Assert-Display 0
        if ($cycle -eq 2) { $lease.Kill() } else { $lease.StandardInput.Close() }
        if (!$lease.WaitForExit(8000)) { throw 'Lease failed to exit' }
        if ($cycle -ne 2 -and $lease.ExitCode -ne 0) { throw 'Normal lease release failed' }
        $lease.Dispose()
        $lease = $null
        Wait-Removed
        Assert-Display 4
        Add-Content -LiteralPath $testLog -Value "PASS cycle=$cycle (cycle 2 terminates lease process)"
    }
    $after = @(Get-PnpDevice -PresentOnly | Where-Object { $_.Class -in @('Display', 'Monitor') })
    if (@($after | Where-Object { $_.InstanceId -match 'SWD\\WindowDeck|DISPLAY\\WND0001' }).Count) { throw 'Virtual PnP device remained' }
    foreach ($id in $physical) {
        if (!($after | Where-Object { $_.InstanceId -eq $id -and $_.Status -eq 'OK' })) { throw "Physical device changed: $id" }
    }
    Add-Content -LiteralPath $testLog -Value 'PASS automatic lifetime, exclusive lease and physical device preservation'
    Write-Output "PASS: automatic display lifecycle. Log: $testLog"
} finally {
    foreach ($process in @($second, $lease)) {
        if ($null -ne $process) {
            if (!$process.HasExited) { $process.Kill(); $process.WaitForExit() }
            $process.Dispose()
        }
    }
}
