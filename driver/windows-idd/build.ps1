[CmdletBinding()]
param([switch]$ControlOnly, [string]$Toolset = '', [string]$SdkVersion = '')
$ErrorActionPreference = 'Stop'
$iddRoot = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '../..'))
$iddTools = Join-Path $iddRoot 'target/idd-tools'
$iddOutput = Join-Path $iddRoot 'target/windows-idd'
$iddVswhere = Join-Path ${env:ProgramFiles(x86)} 'Microsoft Visual Studio/Installer/vswhere.exe'
if (!(Test-Path $iddVswhere)) { throw 'Install Visual Studio 2026 Build Tools with C++ and Windows Driver Kit build tools.' }
$iddVs = if ($ControlOnly) {
    & $iddVswhere -latest -products '*' -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath
} else {
    & $iddVswhere -latest -products '*' -version '[18.0,19.0)' -requires Component.Microsoft.Windows.DriverKit.BuildTools -property installationPath
}
if (!$iddVs) { throw 'Add Windows Driver Kit build tools in Visual Studio Installer (Component.Microsoft.Windows.DriverKit.BuildTools).' }
$iddMsbuild = Join-Path $iddVs 'MSBuild/Current/Bin/amd64/MSBuild.exe'

[xml]$iddManifest = Get-Content -LiteralPath (Join-Path $PSScriptRoot 'packages.config')
$iddMissing = @($iddManifest.packages.package | Where-Object {
    !(Test-Path (Join-Path $iddTools "packages/$($_.id).$($_.version)/build/native/$($_.id).props"))
})
if (!$ControlOnly -and $iddMissing.Count) {
    New-Item -ItemType Directory -Force $iddTools | Out-Null
    $iddNuget = Join-Path $iddTools 'nuget.exe'
    if (!(Test-Path $iddNuget)) {
        Invoke-WebRequest -UseBasicParsing 'https://dist.nuget.org/win-x86-commandline/v6.14.0/nuget.exe' -OutFile $iddNuget
    }
    $iddSignature = Get-AuthenticodeSignature -LiteralPath $iddNuget
    if ($iddSignature.Status -ne 'Valid' -or $iddSignature.SignerCertificate.Subject -notmatch 'O=Microsoft Corporation') {
        throw 'NuGet does not have a valid Microsoft signature.'
    }
    & $iddNuget restore (Join-Path $PSScriptRoot 'packages.config') -PackagesDirectory (Join-Path $iddTools 'packages') -Source https://www.nuget.org/api/v2 -NonInteractive -DirectDownload -NoHttpCache
    if ($LASTEXITCODE) { throw "NuGet restore failed: $LASTEXITCODE" }
}

# Some launchers supply both Path and PATH; .NET Framework MSBuild rejects duplicates.
$iddBuildPath = $env:Path
[Environment]::SetEnvironmentVariable('PATH', $null, 'Process')
[Environment]::SetEnvironmentVariable('Path', $null, 'Process')
[Environment]::SetEnvironmentVariable('Path', $iddBuildPath, 'Process')
$iddProjects = if ($ControlOnly) { @('Control.vcxproj') } else { @('Control.vcxproj', 'WindowDeckDisplay.vcxproj') }
$iddOptions = @('/p:Configuration=Release', '/p:Platform=x64', '/p:PreferredToolArchitecture=x64', '/v:minimal', '/nologo')
if ($ControlOnly) { $iddOptions += '/p:WindowDeckControlOnly=true' }
if ($Toolset) { $iddOptions += "/p:PlatformToolset=$Toolset" }
if ($SdkVersion) { $iddOptions += "/p:WindowsTargetPlatformVersion=$SdkVersion" }
foreach ($iddProject in $iddProjects) {
    & $iddMsbuild (Join-Path $PSScriptRoot $iddProject) @iddOptions
    if ($LASTEXITCODE) { throw "$iddProject failed: $LASTEXITCODE" }
}
& (Join-Path $iddOutput 'windowdeck-display.exe') --self-test
if ($LASTEXITCODE) { throw 'Display self-test failed.' }
Copy-Item -LiteralPath (Join-Path $PSScriptRoot 'LICENSE') -Destination $iddOutput
if (!$ControlOnly) { Copy-Item -LiteralPath (Join-Path $PSScriptRoot 'LICENSE') -Destination (Join-Path $iddOutput 'WindowDeckDisplay') }
Write-Host "Unsigned prototype built in $iddOutput. No certificate trusted, driver installed or display activated."
