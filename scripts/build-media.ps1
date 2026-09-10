[CmdletBinding()]
param([switch]$Download, [switch]$PrepareOnly)
$ErrorActionPreference = 'Stop'
$mediaRoot = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
$mediaSdk = Join-Path $mediaRoot 'target/media-sdk'
$mediaFfmpeg = Join-Path $mediaSdk 'ffmpeg-9.0.1-full_build-shared'
$mediaSdl = Join-Path $mediaSdk 'SDL2-2.32.10'
New-Item -ItemType Directory -Force -Path $mediaSdk | Out-Null
$archives = @(
    @{ File = 'ffmpeg.7z'; Url = 'https://www.gyan.dev/ffmpeg/builds/packages/ffmpeg-9.0.1-full_build-shared.7z'; Sha256 = 'CB4D5E8DB6A3353BFFDB2100D3EB4B76733457FA443215E236F57C99F9FFDCA4'; Folder = $mediaFfmpeg },
    @{ File = 'sdl.zip'; Url = 'https://github.com/libsdl-org/SDL/releases/download/release-2.32.10/SDL2-devel-2.32.10-VC.zip'; Sha256 = 'AF347939395A58B365846AAEA27391E69F9EC9D4DD650D6AC40802159B418A6E'; Folder = $mediaSdl }
)
foreach ($archive in $archives) {
    $mediaFile = Join-Path $mediaSdk $archive.File
    if (!(Test-Path -LiteralPath $mediaFile)) {
        if (!$Download) { throw "Falta $mediaFile. Ejecuta scripts/build-media.ps1 -Download para obtener los SDK." }
        $client = New-Object Net.WebClient
        try { $client.DownloadFile($archive.Url, $mediaFile) } finally { $client.Dispose() }
    }
    if ((Get-FileHash -LiteralPath $mediaFile -Algorithm SHA256).Hash -ne $archive.Sha256) { throw "SHA256 incorrecto: $mediaFile" }
    if (!(Test-Path -LiteralPath $archive.Folder)) {
        if ($archive.File.EndsWith('.zip')) { Expand-Archive -LiteralPath $mediaFile -DestinationPath $mediaSdk }
        else {
            & tar -xf $mediaFile -C $mediaSdk
            if ($LASTEXITCODE) { throw 'No se pudo extraer FFmpeg.' }
        }
    }
}
$env:WINDOWDECK_FFMPEG_DIR = $mediaFfmpeg
$env:WINDOWDECK_SDL_DIR = $mediaSdl
$env:Path = "$mediaFfmpeg/bin;$mediaSdl/lib/x64;" + $env:Path
if ($PrepareOnly) { return }
Push-Location $mediaRoot
try {
    & cargo build --locked --release --workspace --all-features
    if ($LASTEXITCODE) { throw 'No se pudo compilar la integracion multimedia.' }
    # Keep the regular launcher usable directly from target/release.
    Get-ChildItem -LiteralPath (Join-Path $mediaFfmpeg 'bin') -File | Where-Object { $_.Extension -eq '.dll' } |
        Copy-Item -Destination (Join-Path $mediaRoot 'target/release')
    Copy-Item -LiteralPath (Join-Path $mediaSdl 'lib/x64/SDL2.dll') -Destination (Join-Path $mediaRoot 'target/release')
    foreach ($binary in @('ffmpeg.exe', 'ffplay.exe', 'ffprobe.exe')) {
        Copy-Item -LiteralPath (Join-Path $mediaFfmpeg "bin/$binary") -Destination (Join-Path $mediaRoot 'target/release')
    }
    & (Join-Path $mediaRoot 'target/release/windowdeck-client.exe') --media-self-test
    if ($LASTEXITCODE) { throw 'La prueba integrada de video ha fallado.' }
} finally { Pop-Location }
