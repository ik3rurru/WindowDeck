#Requires -RunAsAdministrator
[CmdletBinding()]
param()
$ErrorActionPreference = 'Stop'
$iddRoot = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '../..'))
$iddExe = Join-Path $iddRoot 'target/windows-idd/windowdeck-display.exe'
$iddLog = Join-Path $iddRoot 'target/windows-idd-test'
if (!(Test-Path -LiteralPath $iddExe)) { throw 'Run build.ps1 and install-test.ps1 first.' }
if (Get-Process windowdeck-display -ErrorAction SilentlyContinue) { throw 'Close windowdeck-display before testing.' }

function Get-DisplayDevices {
    @(Get-PnpDevice -Class Display,Monitor -PresentOnly)
}
function Get-VirtualDevices($iddDevices) {
    @($iddDevices | Where-Object { $_.InstanceId -eq 'SWD\WINDOWDECK\WINDOWDECKDISPLAY' -or $_.InstanceId -like 'DISPLAY\WND0001\*' })
}

$iddBaseline = @(Get-DisplayDevices)
if (@(Get-VirtualDevices $iddBaseline).Count) { throw 'WindowDeck is already present; refusing to interfere with another activation.' }
New-Item -ItemType Directory -Force $iddLog | Out-Null
Start-Transcript -Path (Join-Path $iddLog 'device-cycles.log') -Append
$iddProbe = $null
try {
    Write-Host 'Each probe checks PnP and an active 1280x800@60 desktop before normal removal. Frame delivery needs a separate IddCx trace.'
    for ($iddCycle = 1; $iddCycle -le 10; ++$iddCycle) {
        $iddProbe = Start-Process -FilePath $iddExe -ArgumentList '--probe' -WindowStyle Hidden -PassThru -RedirectStandardOutput (Join-Path $iddLog "pnp-$iddCycle.stdout.log") -RedirectStandardError (Join-Path $iddLog "pnp-$iddCycle.stderr.log")
        $null = $iddProbe.Handle # Preserve ExitCode after the redirected process exits in Windows PowerShell.
        $iddReady = $false
        $iddDeadline = (Get-Date).AddSeconds(15)
        do {
            $iddDuring = @(Get-DisplayDevices)
            $iddVirtual = @(Get-VirtualDevices $iddDuring)
            if ($iddVirtual.Count -eq 2 -and @($iddVirtual | Where-Object Status -ne 'OK').Count -eq 0) {
                $iddReady = $true
                break
            }
            Start-Sleep -Milliseconds 100
        } while (!$iddProbe.HasExited -and (Get-Date) -lt $iddDeadline)
        if (!$iddReady) { throw "Cycle ${iddCycle}: adapter and monitor did not both become ready." }
        foreach ($iddPhysical in $iddBaseline) {
            if ($iddPhysical.InstanceId -notin $iddDuring.InstanceId) { throw 'A baseline display device disappeared during activation.' }
        }
        if (!$iddProbe.WaitForExit(15000)) { throw 'Probe did not exit normally.' }
        if ($iddProbe.ExitCode -ne 0) { throw "Probe failed: $($iddProbe.ExitCode)" }
        $iddDeadline = (Get-Date).AddSeconds(10)
        do {
            $iddAfter = @(Get-DisplayDevices)
            if (@(Get-VirtualDevices $iddAfter).Count -eq 0) { break }
            Start-Sleep -Milliseconds 100
        } while ((Get-Date) -lt $iddDeadline)
        if (@(Get-VirtualDevices $iddAfter).Count) { throw 'Virtual devices remained after normal closure.' }
        foreach ($iddPhysical in $iddBaseline) {
            if ($iddPhysical.InstanceId -notin $iddAfter.InstanceId) { throw 'A baseline display device was not preserved after closure.' }
        }
        Write-Host "PASS display cycle ${iddCycle}: adapter/monitor ready, active 1280x800@60 desktop, normal removal, baseline devices preserved."
    }
} catch {
    Write-Error $_ -ErrorAction Continue
    throw
} finally {
    if ($iddProbe -and !$iddProbe.HasExited) { Stop-Process -Id $iddProbe.Id -Force; $iddProbe.WaitForExit() }
    Stop-Transcript
}
