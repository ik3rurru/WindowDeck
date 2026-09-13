[CmdletBinding()]
param([switch]$SkipBuild, [switch]$Release)
$ErrorActionPreference = 'Stop'
$packageRoot = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
$versionMatch = Select-String -LiteralPath (Join-Path $packageRoot 'Cargo.toml') -Pattern '^version = "([0-9]+\.[0-9]+\.[0-9]+)"$'
if (!$versionMatch) { throw 'No se pudo leer la version del workspace.' }
$packageVersion = $versionMatch.Matches[0].Groups[1].Value
$packageCommit = & git -C $packageRoot rev-parse HEAD
if ($LASTEXITCODE) { throw 'No se pudo identificar el commit.' }
$packageDirty = [bool](& git -C $packageRoot status --porcelain)
if ($LASTEXITCODE) { throw 'No se pudo comprobar el estado de Git.' }
if ($Release -and $packageDirty) { throw 'Una release requiere un arbol de trabajo limpio.' }
if (!$SkipBuild) {
    & (Join-Path $PSScriptRoot 'build-media.ps1')
    & (Join-Path $packageRoot 'driver/windows-idd/build.ps1') -ControlOnly
}
$packageStage = Join-Path $packageRoot ('target/package-' + [guid]::NewGuid().ToString('N'))
$packagePath = Join-Path $packageStage "WindowDeck-$packageVersion-windows-x64"
foreach ($folder in @('bin', 'scripts', 'assets', 'licenses')) { New-Item -ItemType Directory -Path (Join-Path $packagePath $folder) -Force | Out-Null }
foreach ($binary in @('windowdeck-host.exe', 'windowdeck-client.exe')) {
    Copy-Item -LiteralPath (Join-Path $packageRoot "target/release/$binary") -Destination (Join-Path $packagePath 'bin')
}
Copy-Item -LiteralPath (Join-Path $packageRoot 'target/release/windowdeck-launcher.exe') -Destination (Join-Path $packagePath 'WindowDeck.exe')
Copy-Item -LiteralPath (Join-Path $packageRoot 'target/windows-idd/windowdeck-display.exe') -Destination (Join-Path $packagePath 'bin')
$packageSdk = Join-Path $packageRoot 'target/media-sdk/ffmpeg-9.0.1-full_build-shared'
# Use only the pinned SDK runtime DLLs, not leftovers in target/release.
Get-ChildItem -LiteralPath (Join-Path $packageSdk 'bin') -Filter '*.dll' -File | Copy-Item -Destination (Join-Path $packagePath 'bin')
Copy-Item -LiteralPath (Join-Path $packageRoot 'target/media-sdk/SDL2-2.32.10/lib/x64/SDL2.dll') -Destination (Join-Path $packagePath 'bin')
foreach ($binary in @('ffmpeg.exe', 'ffplay.exe', 'ffprobe.exe')) {
    Copy-Item -LiteralPath (Join-Path $packageSdk "bin/$binary") -Destination (Join-Path $packagePath 'bin')
}
foreach ($script in @('inventory.ps1', 'install-shortcut.ps1')) { Copy-Item -LiteralPath (Join-Path $PSScriptRoot $script) -Destination (Join-Path $packagePath 'scripts') }
Copy-Item -LiteralPath (Join-Path $packageRoot 'assets/WindowDeck.ico') -Destination (Join-Path $packagePath 'assets')
Copy-Item -LiteralPath (Join-Path $packageSdk 'LICENSE') -Destination (Join-Path $packagePath 'licenses/FFmpeg-GPLv3.txt')
Copy-Item -LiteralPath (Join-Path $packageSdk 'README.txt') -Destination (Join-Path $packagePath 'licenses/FFmpeg-build.txt')
Copy-Item -LiteralPath (Join-Path $packageRoot 'target/media-sdk/SDL2-2.32.10/LICENSE.txt') -Destination (Join-Path $packagePath 'licenses/SDL2.txt')
Copy-Item -LiteralPath (Join-Path $packageRoot 'driver/windows-idd/LICENSE') -Destination (Join-Path $packagePath 'licenses/driver-MS-PL.txt')
foreach ($license in @('LICENSE-MIT', 'LICENSE-APACHE')) { Copy-Item -LiteralPath (Join-Path $packageRoot $license) -Destination (Join-Path $packagePath 'licenses') }
Copy-Item -LiteralPath (Join-Path $packageRoot 'packaging/windows/README.md') -Destination (Join-Path $packagePath 'README.md')
$sourcePath = Join-Path $packagePath 'source'
New-Item -ItemType Directory -Path $sourcePath | Out-Null
if ($Release) {
    $sourceZip = Join-Path $packageStage 'source.zip'
    & git -C $packageRoot archive --format=zip "--output=$sourceZip" $packageCommit
    if ($LASTEXITCODE) { throw 'No se pudieron empaquetar las fuentes del commit.' }
    Expand-Archive -LiteralPath $sourceZip -DestinationPath $sourcePath
} else {
    $sourceFiles = @(& git -c core.quotepath=false -C $packageRoot ls-files --cached --others --exclude-standard)
    if ($LASTEXITCODE) { throw 'No se pudieron enumerar las fuentes.' }
    foreach ($file in ($sourceFiles | Select-Object -Unique)) {
        $sourceFile = Join-Path $packageRoot $file
        if (!(Test-Path -LiteralPath $sourceFile -PathType Leaf)) { continue }
        $destination = Join-Path $sourcePath $file
        New-Item -ItemType Directory -Path (Split-Path $destination -Parent) -Force | Out-Null
        Copy-Item -LiteralPath $sourceFile -Destination $destination
    }
}
# PowerShell only waits for a GUI-subsystem executable when its output is piped.
$launcherVersion = @(& (Join-Path $packagePath 'WindowDeck.exe') --version | Out-String -Stream)
if ($LASTEXITCODE -ne 0 -or !$launcherVersion.Count) { throw 'No se pudo leer la version del lanzador.' }
$manifest = [ordered]@{
    version = $packageVersion
    commit = $packageCommit
    working_tree_modified = $packageDirty
    launcher = $launcherVersion
    host = @(& (Join-Path $packagePath 'bin/windowdeck-host.exe') --version)
    client = @(& (Join-Path $packagePath 'bin/windowdeck-client.exe') --version)
    helper = @(& (Join-Path $packagePath 'bin/windowdeck-display.exe') --version)
    ffmpeg_source = 'https://github.com/FFmpeg/FFmpeg/commit/bf1b838f2a'
    files = @(@(Get-Item -LiteralPath (Join-Path $packagePath 'WindowDeck.exe'); Get-ChildItem -LiteralPath (Join-Path $packagePath 'bin') -File) | ForEach-Object {
        @{ name = $_.FullName.Substring($packagePath.Length + 1).Replace('\', '/'); sha256 = (Get-FileHash -LiteralPath $_.FullName -Algorithm SHA256).Hash }
    })
}
$manifest | ConvertTo-Json -Depth 5 | Set-Content -LiteralPath (Join-Path $packagePath 'manifest.json') -Encoding UTF8
$packageZip = $packagePath + '.zip'
Compress-Archive -LiteralPath $packagePath -DestinationPath $packageZip
Write-Output $packageZip
