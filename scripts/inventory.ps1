[CmdletBinding()]
param([string]$Output)
$ErrorActionPreference = 'Stop'
$inventoryRoot = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
$inventoryBin = Join-Path $inventoryRoot 'bin'
if (!(Test-Path -LiteralPath $inventoryBin)) { $inventoryBin = Join-Path $inventoryRoot 'target/release' }
function Read-Binary([string]$Path) {
    if (!(Test-Path -LiteralPath $Path)) { return @{ path = $Path; available = $false } }
    return @{ path = $Path; sha256 = (Get-FileHash -LiteralPath $Path -Algorithm SHA256).Hash; version = @(& $Path --version) }
}
function Read-DriverPackage([string]$Folder) {
    $dll = Join-Path $Folder 'WindowDeckDisplay.dll'
    $inf = Join-Path $Folder 'WindowDeckDisplay.inf'
    $driverVersion = if (Test-Path -LiteralPath $inf) { (Select-String -LiteralPath $inf -Pattern '^\s*DriverVer\s*=.*').Line.Trim() } else { $null }
    return @{ path = $dll; sha256 = (Get-FileHash -LiteralPath $dll -Algorithm SHA256).Hash; driver_version = $driverVersion; file_version = (Get-Item -LiteralPath $dll).VersionInfo.FileVersion }
}
$installed = @(Get-CimInstance Win32_PnPSignedDriver -Filter "DeviceName = 'WindowDeck Display'" | Select-Object DeviceName, DriverVersion, InfName, DriverDate, DeviceID)
$driverFiles = @(Get-ChildItem -LiteralPath "$env:windir/System32/DriverStore/FileRepository" -Filter 'windowdeckdisplay.inf_*' -Directory -ErrorAction SilentlyContinue | ForEach-Object {
    $dll = Join-Path $_.FullName 'WindowDeckDisplay.dll'
    if (Test-Path -LiteralPath $dll) { Read-DriverPackage $_.FullName }
})
$helperPath = Join-Path $inventoryBin 'windowdeck-display.exe'
if (!(Test-Path -LiteralPath $helperPath)) { $helperPath = Join-Path $inventoryRoot 'target/windows-idd/windowdeck-display.exe' }
$data = [ordered]@{
    recorded_utc = [DateTime]::UtcNow.ToString('o')
    host = Read-Binary (Join-Path $inventoryBin 'windowdeck-host.exe')
    client = Read-Binary (Join-Path $inventoryBin 'windowdeck-client.exe')
    helper = Read-Binary $helperPath
    installed_driver_devices = $installed
    staged_driver_dlls = $driverFiles
    note = 'DLLs en DriverStore no prueban cual esta cargada. Relacionar InfName del dispositivo instalado con el paquete; reconstruir no instala.'
}
$json = $data | ConvertTo-Json -Depth 6
if ($Output) { $json | Set-Content -LiteralPath $Output -Encoding UTF8 } else { $json }
