[CmdletBinding()]
param([string]$Destination)
$ErrorActionPreference = 'Stop'
$shortcutRoot = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
$shortcutBinary = Join-Path $shortcutRoot 'WindowDeck.exe'
if (!(Test-Path -LiteralPath $shortcutBinary)) { $shortcutBinary = Join-Path $shortcutRoot 'target/release/windowdeck-launcher.exe' }
if (!(Test-Path -LiteralPath $shortcutBinary)) { throw 'Falta el lanzador Rust. Compila release o usa el paquete completo.' }
if (!$Destination) { $Destination = Join-Path ([Environment]::GetFolderPath('Desktop')) 'WindowDeck.lnk' }
$Destination = [IO.Path]::GetFullPath($Destination)
if ([IO.Path]::GetExtension($Destination) -ne '.lnk') { throw 'El destino debe ser un acceso directo .lnk.' }
$shortcutShell = New-Object -ComObject WScript.Shell
try {
    $shortcut = $shortcutShell.CreateShortcut($Destination)
    $shortcut.TargetPath = $shortcutBinary
    $shortcut.Arguments = ''
    $shortcut.WorkingDirectory = Split-Path $shortcutBinary -Parent
    $shortcut.IconLocation = $shortcutBinary + ',0'
    $shortcut.Description = 'WindowDeck'
    $shortcut.WindowStyle = 1
    $shortcut.Save()
    Write-Output $Destination
} finally { [void][Runtime.InteropServices.Marshal]::FinalReleaseComObject($shortcutShell) }
