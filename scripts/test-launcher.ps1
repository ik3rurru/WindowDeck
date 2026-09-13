[CmdletBinding()]
param([string]$Launcher, [string]$EvidenceDirectory, [switch]$Session, [switch]$TerminatePanel)
$ErrorActionPreference = 'Stop'
# Windows PowerShell Start-Process rejects environments containing both spellings.
$launcherTestPath = $env:Path
[Environment]::SetEnvironmentVariable('PATH', $null, 'Process')
[Environment]::SetEnvironmentVariable('Path', $null, 'Process')
[Environment]::SetEnvironmentVariable('Path', $launcherTestPath, 'Process')
if ($TerminatePanel -and !$Session) { throw '-TerminatePanel requiere -Session.' }
$testRoot = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
if (!$Launcher) { $Launcher = Join-Path $testRoot 'target/release/windowdeck-launcher.exe' }
$Launcher = (Resolve-Path -LiteralPath $Launcher).Path
if (!$EvidenceDirectory) { $EvidenceDirectory = Join-Path $testRoot ('target/launcher-smoke-' + [guid]::NewGuid().ToString('N')) }
$EvidenceDirectory = [IO.Path]::GetFullPath($EvidenceDirectory)
New-Item -ItemType Directory -Path $EvidenceDirectory -Force | Out-Null
Add-Type -AssemblyName UIAutomationClient
Add-Type -AssemblyName UIAutomationTypes
Add-Type -AssemblyName System.Drawing
Add-Type -ReferencedAssemblies System.Drawing -TypeDefinition @'
using System;
using System.Drawing;
using System.Drawing.Imaging;
using System.Runtime.InteropServices;
public static class WindowDeckPanelProbe {
    delegate bool EnumWindow(IntPtr window, IntPtr context);
    [DllImport("user32.dll")] static extern bool EnumWindows(EnumWindow callback, IntPtr context);
    [DllImport("user32.dll")] static extern uint GetWindowThreadProcessId(IntPtr window, out uint process);
    [DllImport("user32.dll")] static extern int GetWindowTextLength(IntPtr window);
    [DllImport("user32.dll")] public static extern bool ShowWindow(IntPtr window, int command);
    [DllImport("user32.dll")] public static extern bool PostMessage(IntPtr window, uint message, IntPtr wparam, IntPtr lparam);
    public static IntPtr FindPanel(int process) {
        IntPtr found = IntPtr.Zero;
        EnumWindows((window, context) => {
            uint owner;
            GetWindowThreadProcessId(window, out owner);
            if (owner == process && GetWindowTextLength(window) > 0) { found = window; return false; }
            return true;
        }, IntPtr.Zero);
        return found;
    }
    [StructLayout(LayoutKind.Sequential)] public struct Rect { public int Left, Top, Right, Bottom; }
    [DllImport("user32.dll")] static extern bool GetWindowRect(IntPtr window, out Rect rect);
    [DllImport("user32.dll")] static extern bool PrintWindow(IntPtr window, IntPtr dc, uint flags);
    [DllImport("user32.dll")] public static extern uint GetDpiForWindow(IntPtr window);
    public static void Capture(IntPtr window, string path) {
        Rect rect;
        if (!GetWindowRect(window, out rect)) throw new Exception("No se puede medir el panel");
        using (var bitmap = new Bitmap(rect.Right - rect.Left, rect.Bottom - rect.Top))
        using (var graphics = Graphics.FromImage(bitmap)) {
            var dc = graphics.GetHdc();
            try { if (!PrintWindow(window, dc, 2)) throw new Exception("No se puede capturar el panel"); }
            finally { graphics.ReleaseHdc(dc); }
            bitmap.Save(path, ImageFormat.Png);
        }
    }
}
'@
$watch = [Diagnostics.Stopwatch]::StartNew()
if ($Session -and (Get-Process -Name windowdeck-host,windowdeck-display,windowdeck-launcher,WindowDeck -ErrorAction SilentlyContinue)) { throw 'Ya hay una instancia de WindowDeck. Cierra su sesion antes de probar el panel.' }
$previousLogDirectory = $env:WINDOWDECK_LOG_DIR
try {
    $env:WINDOWDECK_LOG_DIR = Join-Path $EvidenceDirectory 'sessions'
    $launchOptions = @{ FilePath = $Launcher; WindowStyle = 'Hidden'; PassThru = $true; RedirectStandardOutput = (Join-Path $EvidenceDirectory 'stdout.txt'); RedirectStandardError = (Join-Path $EvidenceDirectory 'stderr.txt') }
    if (!$Session) { $launchOptions['ArgumentList'] = ('--ui-smoke-test "{0}"' -f $EvidenceDirectory) }
    $panel = Start-Process @launchOptions
} finally { $env:WINDOWDECK_LOG_DIR = $previousLogDirectory }
$null = $panel.Handle
try {
    while (($Session -and [WindowDeckPanelProbe]::FindPanel($panel.Id) -eq [IntPtr]::Zero) -or (!$Session -and !(Test-Path -LiteralPath (Join-Path $EvidenceDirectory 'startup.txt')))) {
        $panel.Refresh()
        if ($panel.HasExited) { throw ('El panel terminó antes de abrir: ' + (Get-Content -LiteralPath (Join-Path $EvidenceDirectory 'stderr.txt') -Raw)) }
        if ($watch.Elapsed.TotalSeconds -gt 10) { throw 'El panel no abrió en diez segundos.' }
        Start-Sleep -Milliseconds 50
    }
    $panel.Refresh()
    $window = [WindowDeckPanelProbe]::FindPanel($panel.Id)
    if ($window -eq [IntPtr]::Zero) { throw 'No se encuentra la ventana del panel.' }
    [void][WindowDeckPanelProbe]::ShowWindow($window, 4)
    Start-Sleep -Milliseconds 100
    $element = [Windows.Automation.AutomationElement]::FromHandle($window)
    $controls = @($element.FindAll([Windows.Automation.TreeScope]::Children, [Windows.Automation.Condition]::TrueCondition) | ForEach-Object {
        @{ name = $_.Current.Name; enabled = $_.Current.IsEnabled; bounds = $_.Current.BoundingRectangle.ToString() }
    })
    $names = @($controls | ForEach-Object { $_.name })
    $controls | ConvertTo-Json -Depth 5 | Set-Content -LiteralPath (Join-Path $EvidenceDirectory 'controls.json') -Encoding UTF8
    [WindowDeckPanelProbe]::Capture($window, (Join-Path $EvidenceDirectory 'panel.png'))
    foreach ($name in @('Iniciar', 'Detener', 'Ver registros')) {
        if ($names -notcontains $name) { throw "Falta el control accesible $name." }
    }
    $metrics = [ordered]@{
        launcher = $Launcher
        binary_bytes = (Get-Item -LiteralPath $Launcher).Length
        window_ready_ms = $watch.Elapsed.TotalMilliseconds
        working_set_bytes = $panel.WorkingSet64
        peak_working_set_bytes = $panel.PeakWorkingSet64
        dpi = [WindowDeckPanelProbe]::GetDpiForWindow($window)
        controls = $controls
    }
    if ($Session) {
        $startButton = $element.FindFirst([Windows.Automation.TreeScope]::Children, (New-Object Windows.Automation.PropertyCondition([Windows.Automation.AutomationElement]::NameProperty, 'Iniciar')))
        $stopButton = $element.FindFirst([Windows.Automation.TreeScope]::Children, (New-Object Windows.Automation.PropertyCondition([Windows.Automation.AutomationElement]::NameProperty, 'Detener')))
        if (![WindowDeckPanelProbe]::PostMessage([IntPtr]$startButton.Current.NativeWindowHandle, 0x00F5, [IntPtr]::Zero, [IntPtr]::Zero)) { throw 'No se pudo pulsar Iniciar.' }
        $sessionWatch = [Diagnostics.Stopwatch]::StartNew()
        $hostOutput = $null
        while ($true) {
            $hostOutput = Get-ChildItem -LiteralPath (Join-Path $EvidenceDirectory 'sessions') -Filter 'host.out' -Recurse -ErrorAction SilentlyContinue | Select-Object -First 1
            if ($hostOutput -and (Get-Content -LiteralPath $hostOutput.FullName -Tail 1) -eq 'windowdeck_state=listening') { break }
            if ($sessionWatch.Elapsed.TotalSeconds -gt 2 -and $startButton.Current.IsEnabled) { throw 'El inicio falló. Consulta los registros de sessions.' }
            if ($sessionWatch.Elapsed.TotalSeconds -gt 130) { throw 'No se confirmó el inicio del host.' }
            Start-Sleep -Milliseconds 100
        }
        $metrics['host_listening_ms'] = $sessionWatch.Elapsed.TotalMilliseconds
        [WindowDeckPanelProbe]::Capture($window, (Join-Path $EvidenceDirectory 'listening.png'))
        $ownedChildren = @(Get-Process -Name windowdeck-host,windowdeck-display,([IO.Path]::GetFileNameWithoutExtension($Launcher)) -ErrorAction SilentlyContinue | Where-Object { $_.Id -ne $panel.Id })
        foreach ($child in $ownedChildren) { $null = $child.Handle }
        $stopWatch = [Diagnostics.Stopwatch]::StartNew()
        if ($TerminatePanel) {
            $panel.Kill()
            $panel.WaitForExit()
        } else {
            if (![WindowDeckPanelProbe]::PostMessage([IntPtr]$stopButton.Current.NativeWindowHandle, 0x00F5, [IntPtr]::Zero, [IntPtr]::Zero)) { throw 'No se pudo pulsar Detener.' }
            while (!$startButton.Current.IsEnabled) {
                if ($stopWatch.Elapsed.TotalSeconds -gt 25) { throw 'El panel no completo Detener.' }
                Start-Sleep -Milliseconds 100
            }
        }
        foreach ($child in $ownedChildren) { if (!$child.WaitForExit(15000)) { throw "El proceso $($child.ProcessName) no termino." }; $child.Dispose() }
        $metrics['stop_ms'] = $stopWatch.Elapsed.TotalMilliseconds
        if (Get-ChildItem -LiteralPath (Join-Path $EvidenceDirectory 'sessions') -Filter 'launcher-error.txt' -Recurse) { throw 'La sesión registró un fallo.' }
        if (Get-Process -Name windowdeck-host -ErrorAction SilentlyContinue) { throw 'El host sigue activo después de Detener.' }
        if (!$TerminatePanel) {
            [WindowDeckPanelProbe]::Capture($window, (Join-Path $EvidenceDirectory 'stopped.png'))
            [void]$panel.CloseMainWindow()
        }
    }
    if (!$Session) {
        $duplicate = Start-Process -FilePath $Launcher -ArgumentList ('--ui-smoke-test "{0}"' -f (Join-Path $EvidenceDirectory 'duplicate')) -WindowStyle Hidden -PassThru -RedirectStandardError (Join-Path $EvidenceDirectory 'duplicate.err')
        $null = $duplicate.Handle
        try {
            if (!$duplicate.WaitForExit(5000)) { throw 'La segunda instancia no terminó.' }
            if ($duplicate.ExitCode -eq 0) { throw 'La segunda instancia no fue rechazada.' }
        } finally { if (!$duplicate.HasExited) { $duplicate.Kill(); $duplicate.WaitForExit() }; $duplicate.Dispose() }
    }
    if (!$panel.WaitForExit(10000)) { throw 'El panel no cerró normalmente.' }
    if (!$TerminatePanel -and $panel.ExitCode -ne 0) { throw "El panel terminó con código $($panel.ExitCode)." }
    if (!$Session -and !(Test-Path -LiteralPath (Join-Path $EvidenceDirectory 'closed.txt'))) { throw 'Falta la confirmación de cierre.' }
    $metrics['close_exit_code'] = $panel.ExitCode
    $metrics['terminated_panel'] = [bool]$TerminatePanel
    $metrics | ConvertTo-Json -Depth 5 | Set-Content -LiteralPath (Join-Path $EvidenceDirectory 'metrics.json') -Encoding UTF8
    Write-Output $EvidenceDirectory
} finally {
    if (!$panel.HasExited) {
        if ($Session) { [void]$panel.CloseMainWindow(); [void]$panel.WaitForExit(25000) }
        if (!$panel.HasExited) { $panel.Kill(); $panel.WaitForExit() }
    }
    $panel.Dispose()
}
