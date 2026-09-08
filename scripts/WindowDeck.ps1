param([switch]$Broker, [string]$Session, [int]$OwnerId)
$ErrorActionPreference = 'Stop'
$root = Split-Path $PSScriptRoot -Parent
$display = Join-Path $root 'target/windows-idd/windowdeck-display.exe'
if ($Broker) {
    $child = $null
    try {
        $hostBinary = Join-Path $root 'target/debug/windowdeck-host.exe'
        if (!(Get-NetFirewallRule -Name 'WindowDeck-mDNS' -ErrorAction SilentlyContinue)) {
            New-NetFirewallRule -Name 'WindowDeck-mDNS' -DisplayName 'WindowDeck discovery (private LAN)' -Direction Inbound -Action Allow -Protocol UDP -LocalPort 5353 -Program $hostBinary -Profile Private -RemoteAddress LocalSubnet | Out-Null
        }
        $owner = Get-Process -Id $OwnerId
        $null = $owner.Handle
        $child = Start-Process $display -ArgumentList '--frame-broker' -WindowStyle Hidden -PassThru -RedirectStandardOutput "$Session/broker.log" -RedirectStandardError "$Session/broker.err"
        $null = $child.Handle
        Start-Sleep -Milliseconds 500
        if ($child.HasExited) { throw 'No se pudo iniciar el broker; consulta broker.err.' }
        Set-Content "$Session/ready" 'ready'
        while (!$owner.HasExited -and !$child.HasExited -and !(Test-Path "$Session/stop")) { Start-Sleep -Milliseconds 250 }
        # Allow the host to release the display lease before stopping the broker.
        Start-Sleep -Seconds 8
    } catch { $_ | Out-String | Set-Content "$Session/error" }
    finally { if ($child -and !$child.HasExited) { $child.Kill(); $child.WaitForExit() } }
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
    if ($script:hostChild -and !$script:hostChild.HasExited) { $script:hostChild.Kill(); $script:hostChild.WaitForExit() }
    if ($script:sessionPath) { Set-Content "$script:sessionPath/stop" 'stop' }
    $stop.Enabled = $false
    $status.Text = 'Deteniendo y recuperando las ventanas...'
}
$start.Add_Click({
    try {
        $principal = New-Object Security.Principal.WindowsPrincipal([Security.Principal.WindowsIdentity]::GetCurrent())
        if ($principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)) { throw 'Abre WindowDeck sin ejecutar como administrador. Solo el broker necesita elevacion.' }
        $script:hostExe = Join-Path $root 'target/debug/windowdeck-host.exe'
        foreach ($file in @($script:hostExe,$display)) { if (!(Test-Path $file)) { throw "Falta $file. Compila el proyecto antes de iniciar." } }
        if (!(Get-Command ffmpeg -ErrorAction SilentlyContinue)) { throw 'FFmpeg no esta en PATH. Instala FFmpeg y vuelve a abrir WindowDeck.' }
        if (Get-NetTCPConnection -LocalPort 48150 -State Listen -ErrorAction SilentlyContinue) { throw 'El puerto 48150 esta ocupado. Cierra la prueba anterior antes de iniciar.' }
        $script:sessionPath = Join-Path $root ('target/launcher-' + [guid]::NewGuid().ToString('N'))
        New-Item -ItemType Directory $script:sessionPath | Out-Null
        $args = '-NoProfile -ExecutionPolicy Bypass -File "{0}" -Broker -Session "{1}" -OwnerId {2}' -f $PSCommandPath,$script:sessionPath,$PID
        $script:manager = Start-Process powershell.exe -Verb RunAs -WindowStyle Hidden -ArgumentList $args -PassThru
        $null = $script:manager.Handle
        $script:startTime = Get-Date
        $script:starting = $true
        $start.Enabled = $false; $stop.Enabled = $true
        $status.Text = 'Iniciando...'
    } catch { $status.Text = $_.Exception.Message }
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
                $env:WINDOWDECK_H264_ENCODER = 'libx264'
                $script:hostChild = Start-Process $script:hostExe -ArgumentList '--driver-h264 0.0.0.0:48150' -WindowStyle Hidden -PassThru -RedirectStandardError "$script:sessionPath/host.log" -RedirectStandardOutput "$script:sessionPath/host.out"
                $null = $script:hostChild.Handle
                $script:starting = $false
            } elseif (((Get-Date) - $script:startTime).TotalSeconds -gt 15) { throw 'Tiempo de espera agotado al iniciar el broker.' }
        } elseif ($script:hostChild -and !$script:hostChild.HasExited -and $stop.Enabled) {
            if ($script:manager.HasExited) { throw 'El broker se ha cerrado. Consulta los registros.' }
            $connections = Get-NetTCPConnection -LocalPort 48150 -State Established -ErrorAction SilentlyContinue
            $status.Text = if ($connections) { 'Deck conectada.' } else { 'Esperando Deck. Abre WindowDeck en la Steam Deck.' }
        } elseif ($stop.Enabled) { throw 'El host se ha cerrado. Consulta los registros.' }
        elseif (!$script:manager -or $script:manager.HasExited) { $start.Enabled = $true }
    } catch { Stop-Session; $status.Text = $_.Exception.Message }
})
$form.Add_FormClosing({ Stop-Session })
try { $timer.Start(); [void]$form.ShowDialog() }
finally { $timer.Stop(); $timer.Dispose(); $mutex.ReleaseMutex(); $mutex.Dispose(); $form.Dispose() }
