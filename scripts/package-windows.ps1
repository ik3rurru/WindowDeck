[CmdletBinding()]
param([switch]$SkipBuild)
$ErrorActionPreference = 'Stop'
$packageRoot = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
if (!$SkipBuild) {
    & (Join-Path $PSScriptRoot 'build-media.ps1')
    & (Join-Path $packageRoot 'driver/windows-idd/build.ps1')
}
$packagePath = Join-Path $packageRoot ('target/WindowDeck-0.2.0-' + [guid]::NewGuid().ToString('N'))
foreach ($folder in @('bin', 'scripts', 'assets', 'licenses')) { New-Item -ItemType Directory -Path (Join-Path $packagePath $folder) -Force | Out-Null }
foreach ($binary in @('windowdeck-host.exe', 'windowdeck-client.exe')) {
    Copy-Item -LiteralPath (Join-Path $packageRoot "target/release/$binary") -Destination (Join-Path $packagePath 'bin')
}
Copy-Item -LiteralPath (Join-Path $packageRoot 'target/windows-idd/windowdeck-display.exe') -Destination (Join-Path $packagePath 'bin')
Get-ChildItem -LiteralPath (Join-Path $packageRoot 'target/release') -Filter '*.dll' -File | Copy-Item -Destination (Join-Path $packagePath 'bin')
$packageSdk = Join-Path $packageRoot 'target/media-sdk/ffmpeg-9.0.1-full_build-shared'
foreach ($binary in @('ffmpeg.exe', 'ffplay.exe', 'ffprobe.exe')) {
    Copy-Item -LiteralPath (Join-Path $packageSdk "bin/$binary") -Destination (Join-Path $packagePath 'bin')
}
foreach ($script in @('WindowDeck.ps1', 'inventory.ps1')) { Copy-Item -LiteralPath (Join-Path $PSScriptRoot $script) -Destination (Join-Path $packagePath 'scripts') }
Copy-Item -LiteralPath (Join-Path $packageRoot 'WindowDeck.vbs') -Destination $packagePath
Copy-Item -LiteralPath (Join-Path $packageRoot 'assets/WindowDeck.ico') -Destination (Join-Path $packagePath 'assets')
Copy-Item -LiteralPath (Join-Path $packageSdk 'LICENSE') -Destination (Join-Path $packagePath 'licenses/FFmpeg-GPLv3.txt')
Copy-Item -LiteralPath (Join-Path $packageSdk 'README.txt') -Destination (Join-Path $packagePath 'licenses/FFmpeg-build.txt')
Copy-Item -LiteralPath (Join-Path $packageRoot 'target/media-sdk/SDL2-2.32.10/LICENSE.txt') -Destination (Join-Path $packagePath 'licenses/SDL2.txt')
Copy-Item -LiteralPath (Join-Path $packageRoot 'driver/windows-idd/LICENSE') -Destination (Join-Path $packagePath 'licenses/driver-MS-PL.txt')
foreach ($license in @('LICENSE-MIT', 'LICENSE-APACHE')) { Copy-Item -LiteralPath (Join-Path $packageRoot $license) -Destination (Join-Path $packagePath 'licenses') }
Copy-Item -LiteralPath (Join-Path $packageRoot 'docs/mejoras-implementadas.md') -Destination (Join-Path $packagePath 'README.md')
$sourcePath = Join-Path $packagePath 'source'
New-Item -ItemType Directory -Path $sourcePath | Out-Null
foreach ($folder in @('crates', 'driver', 'scripts', 'docs', 'packaging', 'assets', '.github')) {
    Copy-Item -LiteralPath (Join-Path $packageRoot $folder) -Destination $sourcePath -Recurse
}
foreach ($file in @('Cargo.toml', 'Cargo.lock', 'README.md', 'WindowDeck.vbs', 'LICENSE-MIT', 'LICENSE-APACHE')) {
    Copy-Item -LiteralPath (Join-Path $packageRoot $file) -Destination $sourcePath
}
$manifest = [ordered]@{
    version = '0.2.0'
    commit = (& git -C $packageRoot rev-parse HEAD)
    working_tree_modified = [bool](& git -C $packageRoot status --porcelain)
    host = @(& (Join-Path $packagePath 'bin/windowdeck-host.exe') --version)
    client = @(& (Join-Path $packagePath 'bin/windowdeck-client.exe') --version)
    helper = @(& (Join-Path $packagePath 'bin/windowdeck-display.exe') --version)
    ffmpeg_source = 'https://github.com/FFmpeg/FFmpeg/commit/bf1b838f2a'
    files = @(Get-ChildItem -LiteralPath (Join-Path $packagePath 'bin') -File | ForEach-Object {
        @{ name = $_.Name; sha256 = (Get-FileHash -LiteralPath $_.FullName -Algorithm SHA256).Hash }
    })
}
$manifest | ConvertTo-Json -Depth 5 | Set-Content -LiteralPath (Join-Path $packagePath 'manifest.json') -Encoding UTF8
$packageZip = $packagePath + '.zip'
Compress-Archive -LiteralPath $packagePath -DestinationPath $packageZip
Write-Output $packageZip
