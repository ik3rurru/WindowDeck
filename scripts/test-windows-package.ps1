[CmdletBinding()]
param([Parameter(Mandatory = $true)][string]$Archive)
$ErrorActionPreference = 'Stop'
$packageTestRoot = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
$packageTestStage = Join-Path $packageTestRoot ('target/package-check-' + [guid]::NewGuid().ToString('N'))
Expand-Archive -LiteralPath $Archive -DestinationPath $packageTestStage
$folders = @(Get-ChildItem -LiteralPath $packageTestStage -Directory)
if ($folders.Count -ne 1) { throw 'El ZIP debe contener una carpeta de aplicacion.' }
$app = $folders[0].FullName
$manifest = Get-Content -LiteralPath (Join-Path $app 'manifest.json') -Raw | ConvertFrom-Json
foreach ($file in $manifest.files) {
    if ((Get-FileHash -LiteralPath (Join-Path $app $file.name) -Algorithm SHA256).Hash -ne $file.sha256) {
        throw "Hash incorrecto: $($file.name)"
    }
}
foreach ($required in @('WindowDeck.exe', 'bin/windowdeck-host.exe', 'bin/windowdeck-client.exe', 'bin/windowdeck-display.exe', 'bin/ffmpeg.exe', 'bin/ffplay.exe', 'bin/SDL2.dll', 'scripts/install-shortcut.ps1', 'source/Cargo.lock', 'source/crates/windowdeck-launcher/src/main.rs', 'licenses/FFmpeg-GPLv3.txt', 'licenses/SDL2.txt')) {
    if (!(Test-Path -LiteralPath (Join-Path $app $required))) { throw "Falta $required" }
}
foreach ($obsolete in @('WindowDeck.vbs', 'scripts/WindowDeck.ps1')) {
    if (Test-Path -LiteralPath (Join-Path $app $obsolete)) { throw "Entrada obsoleta: $obsolete" }
}
$testPath = $env:Path
try {
    # Detect missing runtime DLLs without the developer's FFmpeg/SDL PATH.
    $env:Path = "$(Join-Path $app 'bin');$env:SystemRoot/System32;$env:SystemRoot"
    foreach ($binary in @('WindowDeck.exe', 'bin/windowdeck-host.exe', 'bin/windowdeck-client.exe')) {
        $version = @(& (Join-Path $app $binary) --version | Out-String -Stream)
        if ($LASTEXITCODE -ne 0 -or !($version -match [regex]::Escape($manifest.version))) { throw "Version incorrecta: $binary" }
    }
    & (Join-Path $app 'bin/windowdeck-client.exe') --media-self-test
    if ($LASTEXITCODE) { throw 'Fallo del reproductor integrado en el paquete.' }
    & (Join-Path $app 'bin/windowdeck-display.exe') --self-test
    if ($LASTEXITCODE) { throw 'Fallo de la autoprueba del auxiliar.' }
    $shortcutPath = Join-Path $packageTestStage 'WindowDeck.lnk'
    & (Join-Path $app 'scripts/install-shortcut.ps1') -Destination $shortcutPath | Out-Null
    $shell = New-Object -ComObject WScript.Shell
    try {
        $shortcut = $shell.CreateShortcut($shortcutPath)
        if ($shortcut.TargetPath -ne (Join-Path $app 'WindowDeck.exe')) { throw 'Destino incorrecto del acceso directo.' }
    } finally { [void][Runtime.InteropServices.Marshal]::FinalReleaseComObject($shell) }
} finally { $env:Path = $testPath }
Write-Host "Paquete verificado: $Archive"
