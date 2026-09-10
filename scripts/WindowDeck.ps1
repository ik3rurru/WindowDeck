param([switch]$Broker, [string]$Session, [int]$OwnerId, [switch]$Native, [switch]$Legacy)
$ErrorActionPreference = 'Stop'
$root = Split-Path $PSScriptRoot -Parent
$display = Join-Path $root 'bin/windowdeck-display.exe'
if (!(Test-Path -LiteralPath $display)) { $display = Join-Path $root 'target/windows-idd/windowdeck-display.exe' }
$hostBinary = Join-Path $root 'bin/windowdeck-host.exe'
if (!(Test-Path -LiteralPath $hostBinary)) { $hostBinary = Join-Path $root 'target/release/windowdeck-host.exe' }
$env:Path = (Split-Path $hostBinary -Parent) + ';' + $env:Path
if ($Broker) {
    $child = $null
    $compatibilityChild = $null
    try {
        foreach ($rule in @(@{ Name = 'WindowDeck-mDNS'; Protocol = 'UDP'; Port = 5353 }, @{ Name = 'WindowDeck-Video'; Protocol = 'TCP'; Port = 48150 })) {
            $existing = Get-NetFirewallRule -Name $rule.Name -ErrorAction SilentlyContinue
            if ($existing) {
                $existing | Get-NetFirewallApplicationFilter | Set-NetFirewallApplicationFilter -Program $hostBinary | Out-Null
            } else {
                New-NetFirewallRule -Name $rule.Name -DisplayName ($rule.Name + ' (private LAN)') -Direction Inbound -Action Allow -Protocol $rule.Protocol -LocalPort $rule.Port -Program $hostBinary -Profile Private -RemoteAddress LocalSubnet | Out-Null
            }
        }
        $owner = Get-Process -Id $OwnerId
        $null = $owner.Handle
        $brokerMode = if ($Native) { '--gpu-frame-broker' } else { '--frame-broker' }
        $child = Start-Process $display -ArgumentList $brokerMode -WindowStyle Hidden -PassThru -RedirectStandardOutput "$Session/broker.log" -RedirectStandardError "$Session/broker.err"
        $null = $child.Handle
        if ($Native) {
            $compatibilityChild = Start-Process $display -ArgumentList '--frame-broker' -WindowStyle Hidden -PassThru -RedirectStandardOutput "$Session/cpu-broker.log" -RedirectStandardError "$Session/cpu-broker.err"
            $null = $compatibilityChild.Handle
        }
        Start-Sleep -Milliseconds 500
        if ($child.HasExited) { throw 'No se pudo iniciar el broker; consulta broker.err.' }
        if ($compatibilityChild -and $compatibilityChild.HasExited) { throw 'No se pudo iniciar el respaldo CPU; consulta cpu-broker.err.' }
        Set-Content "$Session/ready" 'ready'
        while (!$owner.HasExited -and !$child.HasExited -and (!$compatibilityChild -or !$compatibilityChild.HasExited) -and !(Test-Path "$Session/stop")) { Start-Sleep -Milliseconds 250 }
        # Allow the host to release the display lease before stopping the broker.
        Start-Sleep -Seconds 8
    } catch { $_ | Out-String | Set-Content "$Session/error" }
    finally {
        foreach ($process in @($child, $compatibilityChild)) {
            if ($process -and !$process.HasExited) { $process.Kill(); $process.WaitForExit() }
        }
    }
    exit
}
Add-Type -AssemblyName System.Windows.Forms
Add-Type -AssemblyName System.Drawing
$mutex = New-Object Threading.Mutex($false, 'Local\WindowDeck.Launcher')
if (!$mutex.WaitOne(0)) { [Windows.Forms.MessageBox]::Show('WindowDeck ya esta abierto.','WindowDeck') | Out-Null; exit }
$script:hostChild = $null
$script:manager = $null
$script:sessionPath = $null
$script:starting = $false
$script:stoppingSince = $null
$script:lastFailure = $null
$script:startTime = Get-Date
$form = New-Object Windows.Forms.Form
$form.Text = 'WindowDeck'
$form.ClientSize = New-Object Drawing.Size(480,210)
$form.StartPosition = 'CenterScreen'
$form.FormBorderStyle = 'FixedDialog'
$form.MaximizeBox = $false
$form.Icon = New-Object Drawing.Icon (Join-Path $root 'assets/WindowDeck.ico')
$status = New-Object Windows.Forms.Label
$status.SetBounds(20,20,440,65)
$status.Text = 'Detenido. Pulsa Iniciar y abre WindowDeck en la Steam Deck.'
$start = New-Object Windows.Forms.Button
$start.Text = 'Iniciar'; $start.SetBounds(20,95,130,40)
$stop = New-Object Windows.Forms.Button
$stop.Text = 'Detener'; $stop.SetBounds(170,95,130,40); $stop.Enabled = $false
$logs = New-Object Windows.Forms.Button
$logs.Text = 'Ver registros'; $logs.SetBounds(320,95,140,40)
$address = New-Object Windows.Forms.Label
$address.SetBounds(20,155,440,45)
$ips = @(Get-NetIPAddress -AddressFamily IPv4 -ErrorAction SilentlyContinue | Where-Object { $_.IPAddress -notmatch '^(127\.|169\.254\.)' } | Select-Object -ExpandProperty IPAddress)
$address.Text = 'Direccion del PC: ' + (($ips | ForEach-Object { "${_}:48150" }) -join ', ')
$form.Controls.AddRange(@($status,$start,$stop,$logs,$address))
function Stop-Session {
    $script:starting = $false
    if (!$script:stoppingSince) { $script:stoppingSince = Get-Date }
    if ($script:sessionPath) { Set-Content "$script:sessionPath/stop" 'stop' }
    $stop.Enabled = $false
    $status.Text = 'Deteniendo y recuperando las ventanas...'
}
$start.Add_Click({
    try {
        $principal = New-Object Security.Principal.WindowsPrincipal([Security.Principal.WindowsIdentity]::GetCurrent())
        if ($principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)) { throw 'Abre WindowDeck sin ejecutar como administrador. Solo el broker necesita elevacion.' }
        $script:hostExe = $hostBinary
        foreach ($file in @($script:hostExe,$display)) { if (!(Test-Path $file)) { throw "Falta $file. Compila el proyecto antes de iniciar." } }
        $version = & $script:hostExe --version
        if ($LASTEXITCODE) { throw 'No se pudo abrir el host. Comprueba que sus DLL estan junto al ejecutable.' }
        $script:useNative = !$Legacy -and ($version -contains 'native_media=true')
        if (!$script:useNative -and !(Get-Command ffmpeg -ErrorAction SilentlyContinue)) { throw 'FFmpeg no esta en PATH. Usa el paquete completo de WindowDeck.' }
        $script:sessionPath = Join-Path $root ('target/launcher-' + [guid]::NewGuid().ToString('N'))
        New-Item -ItemType Directory $script:sessionPath | Out-Null
        $script:stoppingSince = $null
        $script:lastFailure = $null
        $version | Set-Content -LiteralPath "$script:sessionPath/host-version.txt"
        $args = '-NoProfile -ExecutionPolicy Bypass -File "{0}" -Broker -Session "{1}" -OwnerId {2}' -f $PSCommandPath,$script:sessionPath,$PID
        if ($script:useNative) { $args += ' -Native' }
        $script:manager = Start-Process powershell.exe -Verb RunAs -WindowStyle Hidden -ArgumentList $args -PassThru
        $null = $script:manager.Handle
        $script:startTime = Get-Date
        $script:starting = $true
        $start.Enabled = $false; $stop.Enabled = $true
        $status.Text = 'Iniciando...'
    } catch { $script:lastFailure = $_.Exception.Message; $status.Text = $script:lastFailure }
})
$stop.Add_Click({ Stop-Session })
$logs.Add_Click({ if ($script:sessionPath) { Start-Process explorer.exe -ArgumentList ('"' + $script:sessionPath + '"') } })
$timer = New-Object Windows.Forms.Timer
$timer.Interval = 500
$timer.Add_Tick({
    try {
        if ($script:starting) {
            if (Test-Path "$script:sessionPath/error") { throw (Get-Content "$script:sessionPath/error" -Raw) }
            if ($script:manager.HasExited) { throw 'El broker se ha cerrado.' }
            if (Test-Path "$script:sessionPath/ready") {
                $env:WINDOWDECK_DISPLAY_EXE = $display
                $env:WINDOWDECK_STOP_FILE = "$script:sessionPath/stop"
                $hostMode = if ($script:useNative) { '--driver-native-h264 0.0.0.0:48150' } else { '--driver-h264 0.0.0.0:48150' }
                $script:hostChild = Start-Process $script:hostExe -ArgumentList $hostMode -WindowStyle Hidden -PassThru -RedirectStandardError "$script:sessionPath/host.log" -RedirectStandardOutput "$script:sessionPath/host.out"
                $null = $script:hostChild.Handle
                $script:starting = $false
            } elseif (((Get-Date) - $script:startTime).TotalSeconds -gt 15) { throw 'Tiempo de espera agotado al iniciar el broker.' }
        } elseif ($script:hostChild -and !$script:hostChild.HasExited -and $stop.Enabled) {
            if ($script:manager.HasExited) { throw 'El broker se ha cerrado. Consulta los registros.' }
            $stateLine = Get-Content -LiteralPath "$script:sessionPath/host.out" -Tail 1 -ErrorAction SilentlyContinue
            $status.Text = switch ($stateLine) {
                'windowdeck_state=streaming' { 'Deck conectada.' }
                'windowdeck_state=negotiating' { 'Preparando la conexion con la Deck...' }
                'windowdeck_state=listening' { 'Esperando Deck. Abre WindowDeck en la Steam Deck.' }
                default { 'Iniciando el host...' }
            }
        } elseif ($stop.Enabled) { throw 'El host se ha cerrado. Consulta los registros.' }
        elseif ($script:hostChild -and !$script:hostChild.HasExited) {
            if ($script:stoppingSince -and ((Get-Date) - $script:stoppingSince).TotalSeconds -gt 8) { $script:hostChild.Kill() }
        }
        elseif (!$script:manager -or $script:manager.HasExited) {
            $start.Enabled = $true
            if (!$script:lastFailure -and $script:stoppingSince) { $status.Text = 'Detenido. Pulsa Iniciar para conectar de nuevo.' }
        }
    } catch { Stop-Session; $script:lastFailure = $_.Exception.Message; $status.Text = $script:lastFailure }
})
$form.Add_FormClosing({ Stop-Session })
try { $timer.Start(); [void]$form.ShowDialog() }
finally {
    $timer.Stop(); $timer.Dispose()
    if ($script:hostChild -and !$script:hostChild.HasExited) {
        if (!$script:hostChild.WaitForExit(8000)) { $script:hostChild.Kill() }
    }
    $mutex.ReleaseMutex(); $mutex.Dispose(); $form.Dispose()
}
