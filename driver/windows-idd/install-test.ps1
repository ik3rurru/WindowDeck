#Requires -RunAsAdministrator
[CmdletBinding()]
param()
$ErrorActionPreference = 'Stop'
$iddRoot = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '../..'))
$iddSource = Join-Path $iddRoot 'target/windows-idd/WindowDeckDisplay'
$iddOutput = Join-Path $iddRoot 'target/windows-idd-test'
$iddSigning = Join-Path $iddRoot 'target/idd-signing'
$iddPackages = Join-Path $iddRoot 'target/idd-tools/packages'
$iddSignTool = Join-Path $iddPackages 'Microsoft.Windows.SDK.CPP.10.0.28000.2526/c/bin/10.0.28000.0/x64/signtool.exe'
$iddInf2Cat = Join-Path $iddPackages 'Microsoft.Windows.WDK.x64.10.0.28000.2526/c/bin/10.0.28000.0/x86/Inf2Cat.exe'
foreach ($iddFile in @($iddSignTool, $iddInf2Cat, "$iddSource/WindowDeckDisplay.dll", "$iddSource/WindowDeckDisplay.inf", "$iddSource/LICENSE")) {
    if (!(Test-Path -LiteralPath $iddFile)) { throw "Missing $iddFile. Run build.ps1 first." }
}
if (Get-Process windowdeck-display -ErrorAction SilentlyContinue) { throw 'Close windowdeck-display before updating its driver.' }
New-Item -ItemType Directory -Force $iddOutput, $iddSigning | Out-Null
Start-Transcript -Path (Join-Path $iddOutput 'install.log') -Append
try {
    # Read-only checks: this script never changes boot configuration or Secure Boot.
    try { Write-Host "Secure Boot: $(Confirm-SecureBootUEFI)" } catch { Write-Warning $_.Exception.Message }
    & bcdedit /enum '{current}'
    if ($LASTEXITCODE) { Write-Warning 'Could not query the current boot entry.' }

    $iddCer = Join-Path $iddSigning 'WindowDeckDisplay.cer'
    if (Test-Path -LiteralPath $iddCer) {
        $iddPublic = [Security.Cryptography.X509Certificates.X509Certificate2]::new($iddCer)
        $iddCert = Get-Item -LiteralPath "Cert:/CurrentUser/My/$($iddPublic.Thumbprint)"
    } else {
        $iddCert = New-SelfSignedCertificate -Type CodeSigningCert -Subject 'CN=WindowDeck Display Development' -FriendlyName 'WindowDeck Display local test only' -CertStoreLocation Cert:/CurrentUser/My -KeyAlgorithm RSA -KeyLength 3072 -HashAlgorithm SHA256 -KeyExportPolicy NonExportable -NotAfter (Get-Date).AddDays(90)
        Export-Certificate -Cert $iddCert -FilePath $iddCer | Out-Null
    }
    if ($iddCert.Subject -ne 'CN=WindowDeck Display Development' -or !$iddCert.HasPrivateKey -or $iddCert.NotAfter -le (Get-Date)) {
        throw 'The saved WindowDeck test certificate is not usable. Do not replace it silently.'
    }
    Write-Host "Test certificate: $($iddCert.Thumbprint); expires $($iddCert.NotAfter.ToString('s'))"
    # Only the public certificate is exported. The private key stays non-exportable in the user's store.
    foreach ($iddStore in @('Cert:/LocalMachine/Root', 'Cert:/LocalMachine/TrustedPublisher')) {
        if (!(Test-Path -LiteralPath "$iddStore/$($iddCert.Thumbprint)")) {
            Import-Certificate -FilePath $iddCer -CertStoreLocation $iddStore | Out-Null
        }
    }
    foreach ($iddName in @('WindowDeckDisplay.dll', 'WindowDeckDisplay.inf', 'LICENSE')) {
        Copy-Item -LiteralPath (Join-Path $iddSource $iddName) -Destination $iddOutput
    }
    $iddDll = Join-Path $iddOutput 'WindowDeckDisplay.dll'
    $iddCat = Join-Path $iddOutput 'WindowDeckDisplay.cat'
    & $iddSignTool sign /fd SHA256 /sha1 $iddCert.Thumbprint /s My $iddDll
    if ($LASTEXITCODE) { throw 'DLL signing failed.' }
    # Regenerate after signing the DLL so the catalog covers its final contents.
    & $iddInf2Cat "/driver:$iddOutput" /os:10_CO_X64 /uselocaltime
    if ($LASTEXITCODE) { throw 'Catalog generation failed.' }
    & $iddSignTool sign /fd SHA256 /sha1 $iddCert.Thumbprint /s My $iddCat
    if ($LASTEXITCODE) { throw 'Catalog signing failed.' }
    foreach ($iddSigned in @($iddDll, $iddCat)) {
        $iddSignature = Get-AuthenticodeSignature -LiteralPath $iddSigned
        if ($iddSignature.Status -ne 'Valid' -or $iddSignature.SignerCertificate.Thumbprint -ne $iddCert.Thumbprint) {
            throw "Signature verification failed: $iddSigned"
        }
        & $iddSignTool verify /pa /v $iddSigned
        if ($LASTEXITCODE) { throw "SignTool verification failed: $iddSigned" }
    }
    & $iddSignTool verify /pa /v /c $iddCat $iddDll
    if ($LASTEXITCODE) { throw 'Catalog/DLL verification failed.' }
    & $iddSignTool verify /pa /v /c $iddCat (Join-Path $iddOutput 'WindowDeckDisplay.inf')
    if ($LASTEXITCODE) { throw 'Catalog/INF verification failed.' }

    & pnputil /add-driver (Join-Path $iddOutput 'WindowDeckDisplay.inf') /install
    $iddInstallResult = $LASTEXITCODE
    if ($iddInstallResult -notin @(0, 3010)) { throw "PnPUtil failed: $iddInstallResult" }
    if ($iddInstallResult -eq 3010) { Write-Warning 'Windows requests a reboot; this script will not restart the PC.' }
    Write-Host 'Signed package staged. Device loading still needs a separate activation test.'
} finally {
    Stop-Transcript
}
