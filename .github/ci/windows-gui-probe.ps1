[CmdletBinding()]
param(
    [string]$Executable,
    [Parameter(Mandatory)][string]$OutputDirectory,
    [ValidateSet('default', 'software')][string]$Backend = 'default',
    [switch]$DesktopOnly
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
if (-not $IsWindows) { throw 'This probe needs a Windows desktop.' }

$OutputDirectory = [IO.Path]::GetFullPath($OutputDirectory)
$null = New-Item -ItemType Directory -Force -Path $OutputDirectory
$record = [ordered]@{
    schema_version = 1
    status = 'failed'
    visual_review_required = $true
    source_commit = $env:PROBE_SOURCE_SHA
    run_id = $env:GITHUB_RUN_ID
    run_attempt = $env:GITHUB_RUN_ATTEMPT
    backend_requested = $Backend
    started_at = [DateTime]::UtcNow.ToString('o')
    phases = [Collections.Generic.List[object]]::new()
}
$previousBackend = $env:SLINT_BACKEND
$previousLog = $env:RUST_LOG

function Save-Capture([Drawing.Rectangle]$Bounds, [string]$Path) {
    if ($Bounds.Width -lt 100 -or $Bounds.Height -lt 100) { throw 'No usable display rectangle.' }
    $bitmap = [Drawing.Bitmap]::new($Bounds.Width, $Bounds.Height)
    $graphics = [Drawing.Graphics]::FromImage($bitmap)
    try {
        $graphics.CopyFromScreen($Bounds.Location, [Drawing.Point]::Empty, $Bounds.Size)
        $bitmap.Save($Path, [Drawing.Imaging.ImageFormat]::Png)
        $colors = [Collections.Generic.HashSet[int]]::new()
        for ($y = 0; $y -lt $Bounds.Height; $y += 13) {
            for ($x = 0; $x -lt $Bounds.Width; $x += 13) {
                $null = $colors.Add($bitmap.GetPixel($x, $y).ToArgb())
            }
        }
        return [ordered]@{
            file = [IO.Path]::GetFileName($Path)
            sha256 = (Get-FileHash $Path -Algorithm SHA256).Hash.ToLowerInvariant()
            width = $Bounds.Width
            height = $Bounds.Height
            sampled_colors = $colors.Count
        }
    }
    finally {
        $graphics.Dispose()
        $bitmap.Dispose()
    }
}

function Invoke-WindowProbe([string]$Name, [string[]]$ExtraArguments) {
    $phase = [ordered]@{ name = $Name; status = 'failed'; captures = @() }
    $record.phases.Add($phase)
    $process = $null
    try {
        $config = Join-Path $OutputDirectory "$Name-settings"
        $arguments = @('--windowed', '--config-dir', ('"' + $config + '"')) + $ExtraArguments
        if ($Backend -eq 'software') { $arguments += @('--renderer', 'raster') }
        $phase.arguments = $arguments
        $process = Start-Process -FilePath $Executable -ArgumentList $arguments -PassThru `
            -WorkingDirectory ([IO.Path]::GetDirectoryName($Executable)) `
            -RedirectStandardOutput "$OutputDirectory/$Name-stdout.log" `
            -RedirectStandardError "$OutputDirectory/$Name-stderr.log"
        $phase.pid = $process.Id
        $watch = [Diagnostics.Stopwatch]::StartNew()
        do {
            $process.Refresh()
            if ($process.HasExited) { throw "$Name exited before a window appeared (exit $($process.ExitCode))." }
            $handle = $process.MainWindowHandle
            if ($handle -ne [IntPtr]::Zero) { break }
            Start-Sleep -Milliseconds 200
        } while ($watch.Elapsed.TotalSeconds -lt 45)
        if ($handle -eq [IntPtr]::Zero) { throw "$Name did not create a window within 45 seconds." }
        if (-not [ProbeWindow]::IsWindowVisible($handle)) { throw "$Name window is not visible." }
        $screen = [Windows.Forms.SystemInformation]::VirtualScreen
        $width = [Math]::Min(1100, $screen.Width - 40)
        $height = [Math]::Min(780, $screen.Height - 40)
        if (-not [ProbeWindow]::SetWindowPos($handle, [IntPtr]::Zero, $screen.X + 20, $screen.Y + 20, $width, $height, 0x0040)) {
            throw 'Could not position the application window.'
        }
        $phase.foreground_requested = [ProbeWindow]::SetForegroundWindow($handle)
        $phase.window_title = $process.MainWindowTitle
        $phase.modules = @($process.Modules | ForEach-Object {
            [ordered]@{ name = $_.ModuleName; path = $_.FileName; version = $_.FileVersionInfo.FileVersion }
        })
        foreach ($frame in 1..2) {
            Start-Sleep -Seconds 3
            $process.Refresh()
            if ($process.HasExited) { throw "$Name exited while capturing (exit $($process.ExitCode))." }
            $rect = [ProbeWindow+Rect]::new()
            $point = [ProbeWindow+Point]::new()
            if (-not [ProbeWindow]::GetClientRect($handle, [ref]$rect) -or -not [ProbeWindow]::ClientToScreen($handle, [ref]$point)) {
                throw 'Could not locate the application client area.'
            }
            $bounds = [Drawing.Rectangle]::new($point.X, $point.Y, $rect.Right, $rect.Bottom)
            if (-not $screen.Contains($bounds)) { throw 'The application client area is outside the desktop.' }
            $capture = Save-Capture $bounds "$OutputDirectory/$Name-$frame.png"
            $phase.captures += $capture
            if ($capture.sampled_colors -lt 4) { throw "$Name capture appears blank." }
        }
        $phase.status = 'captured'
    }
    catch {
        $phase.error = $_.Exception.Message
        throw
    }
    finally {
        if ($null -ne $process) {
            if (-not $process.HasExited) {
                $null = $process.CloseMainWindow()
                if (-not $process.WaitForExit(5000)) {
                    $process.Kill($true)
                    if (-not $process.WaitForExit(5000)) { throw "Could not stop $Name process." }
                    $phase.forced_cleanup = $true
                }
            }
            $phase.exit_code = $process.ExitCode
            $process.Dispose()
        }
    }
    $stderr = Get-Content "$OutputDirectory/$Name-stderr.log" -Raw
    if ($stderr -match 'direct scenario launch failed') {
        $phase.status = 'failed'
        throw 'Scenario launch fell back to the launcher; captures do not establish scenario startup.'
    }
}

try {
    Add-Type -AssemblyName System.Windows.Forms
    Add-Type -AssemblyName System.Drawing
    Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
public static class ProbeWindow {
    [StructLayout(LayoutKind.Sequential)] public struct Rect { public int Left, Top, Right, Bottom; }
    [StructLayout(LayoutKind.Sequential)] public struct Point { public int X, Y; }
    [DllImport("user32.dll")] public static extern bool SetProcessDPIAware();
    [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr handle);
    [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr handle);
    [DllImport("user32.dll")] public static extern bool GetClientRect(IntPtr handle, out Rect rect);
    [DllImport("user32.dll")] public static extern bool ClientToScreen(IntPtr handle, ref Point point);
    [DllImport("user32.dll")] public static extern bool SetWindowPos(IntPtr handle, IntPtr after, int x, int y, int width, int height, uint flags);
}
'@
    $null = [ProbeWindow]::SetProcessDPIAware()
    $record.environment = [ordered]@{
        os = [Runtime.InteropServices.RuntimeInformation]::OSDescription
        architecture = [Runtime.InteropServices.RuntimeInformation]::OSArchitecture.ToString()
        powershell = $PSVersionTable.PSVersion.ToString()
        image_os = $env:ImageOS
        image_version = $env:ImageVersion
        user_interactive = [Environment]::UserInteractive
        session_id = [Diagnostics.Process]::GetCurrentProcess().SessionId
        display = @((Get-CimInstance Win32_VideoController) | Select-Object Name, DriverVersion, VideoModeDescription)
    }
    $commit = (git rev-parse HEAD).Trim()
    if ($LASTEXITCODE -ne 0 -or $commit -ne $env:PROBE_SOURCE_SHA) { throw 'Checkout differs from the expected candidate commit.' }
    $record.desktop = Save-Capture ([Windows.Forms.SystemInformation]::VirtualScreen) "$OutputDirectory/desktop.png"
    if ($DesktopOnly) {
        $record.status = 'desktop_captured'
        return
    }
    $Executable = (Resolve-Path $Executable).Path
    $workspace = [IO.Path]::GetFullPath($env:GITHUB_WORKSPACE).TrimEnd('\', '/') + [IO.Path]::DirectorySeparatorChar
    if ($Executable.StartsWith($workspace, [StringComparison]::OrdinalIgnoreCase)) { throw 'The candidate must run outside the source checkout.' }
    $record.executable = $Executable
    $record.executable_sha256 = (Get-FileHash $Executable -Algorithm SHA256).Hash.ToLowerInvariant()
    $manifest = Get-Content (Join-Path ([IO.Path]::GetDirectoryName($Executable)) 'probe-manifest.json') -Raw | ConvertFrom-Json
    if ($manifest.source_commit -ne $commit -or $manifest.executable_sha256 -ne $record.executable_sha256) { throw 'Extracted executable does not match its candidate manifest.' }
    $record.profile = $manifest.profile
    $record.target = $manifest.target
    if ($Backend -eq 'software') { $env:SLINT_BACKEND = 'winit-software' }
    else { $env:SLINT_BACKEND = $null }
    $env:RUST_LOG = 'info'
    $record.slint_backend = $env:SLINT_BACKEND
    $help = Start-Process -FilePath $Executable -ArgumentList '--help' -PassThru `
        -WorkingDirectory ([IO.Path]::GetDirectoryName($Executable)) `
        -RedirectStandardOutput "$OutputDirectory/help.txt" -RedirectStandardError "$OutputDirectory/help-stderr.log"
    try {
        if (-not $help.WaitForExit(15000)) {
            $help.Kill($true)
            $null = $help.WaitForExit(5000)
            throw '--help timed out.'
        }
        $record.help_exit_code = $help.ExitCode
        if ($help.ExitCode -ne 0) { throw "--help failed with exit $($help.ExitCode)." }
    }
    finally { $help.Dispose() }
    $errors = [Collections.Generic.List[string]]::new()
    foreach ($phase in @(@{ name = 'launcher'; arguments = @() }, @{ name = 'falling'; arguments = @('--scenario', 'falling') })) {
        try { Invoke-WindowProbe $phase.name $phase.arguments }
        catch { $errors.Add($_.Exception.Message) }
    }
    if ($errors.Count -gt 0) { throw ($errors -join '; ') }
    $record.status = 'captured'
}
catch {
    $record.error = $_.Exception.Message
    throw
}
finally {
    $env:SLINT_BACKEND = $previousBackend
    $env:RUST_LOG = $previousLog
    $record.finished_at = [DateTime]::UtcNow.ToString('o')
    $record | ConvertTo-Json -Depth 12 | Set-Content "$OutputDirectory/result.json" -Encoding utf8
}
