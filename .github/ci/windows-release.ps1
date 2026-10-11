[CmdletBinding()]
param(
    [Parameter(Mandatory)][string]$SourceCommit,
    [Parameter(Mandatory)][string]$OutputDirectory,
    [switch]$FunctionsOnly
)
Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

function Get-Sha256([string]$Path) {
    (Get-FileHash -LiteralPath $Path -Algorithm SHA256).Hash.ToLowerInvariant()
}
function Write-Checksums([string]$Directory) {
    $lines = @(Get-ChildItem -LiteralPath $Directory -File -Recurse | Where-Object Name -ne 'SHA256SUMS' | Sort-Object FullName | ForEach-Object {
        $relative = [IO.Path]::GetRelativePath($Directory, $_.FullName).Replace('\', '/')
        "$(Get-Sha256 $_.FullName)  $relative"
    })
    $lines | Set-Content (Join-Path $Directory 'SHA256SUMS') -Encoding utf8
}
function Write-SoftwareLauncher([string]$Path) {
    $wrapper = @'
@echo off
setlocal
cd /d "%~dp0"
set "SLINT_BACKEND=winit-software"
"%~dp0engine-client.exe" --renderer raster %*
exit /b %errorlevel%
'@
    $wrapper.Replace("`r`n", "`n").Replace("`n", "`r`n") + "`r`n" | Set-Content $Path -Encoding ascii -NoNewline
}
function Assert-NativeExit([int]$Code, [string]$Operation) {
    if ($Code -ne 0) { throw "$Operation failed with exit $Code." }
}
function Write-Json($Value, [string]$Path) {
    $Value | ConvertTo-Json -Depth 20 | Set-Content -LiteralPath $Path -Encoding utf8
}
function Write-EvidenceManifest($Identity, [string]$Directory) {
    $files = [ordered]@{}
    $size = 0L
    foreach ($file in Get-ChildItem -LiteralPath $Directory -Recurse -File | Where-Object Name -ne 'manifest.json') {
        $size += $file.Length
        $files[[IO.Path]::GetRelativePath($Directory, $file.FullName).Replace('\', '/')] = Get-Sha256 $file.FullName
    }
    if ($size -gt 256MB) { throw 'Expanded evidence exceeds 256 MiB.' }
    $manifest = [ordered]@{} + $Identity
    $manifest.files = $files
    Write-Json $manifest (Join-Path $Directory 'manifest.json')
}
if ($FunctionsOnly) { return }
if (-not $IsWindows) { throw 'Release validation requires Windows.' }
if ($SourceCommit -cnotmatch '^[0-9a-f]{40}$') { throw 'Expected a full lowercase source SHA.' }
$head = (git rev-parse HEAD).Trim()
Assert-NativeExit $LASTEXITCODE 'Resolve checkout'
if ($head -cne $SourceCommit) { throw 'Checkout differs from selected source.' }
$OutputDirectory = [IO.Path]::GetFullPath($OutputDirectory)
if (Test-Path $OutputDirectory) { throw 'Evidence directory must be fresh.' }
$null = New-Item -ItemType Directory -Path $OutputDirectory
$identity = [ordered]@{
    schema_version = 1; repository = 'aortez/space-wars'
    source_commit = $head; tested_commit = $head; source_kind = 'head'
    run_id = [long]$env:GITHUB_RUN_ID; run_attempt = [int]$env:GITHUB_RUN_ATTEMPT
}
$record = [ordered]@{} + $identity
$record.required_mode = 'software'
$work = Join-Path $env:RUNNER_TEMP ('windows-release-' + [Guid]::NewGuid())
$stage = Join-Path $work 'stage'
$extract = Join-Path $work 'extracted'
$previousRevision = $env:SPACEWARS_BUILD_REVISION
$previousBackend = $env:SLINT_BACKEND
try {
    $null = New-Item -ItemType Directory -Path $stage -Force
    $env:SPACEWARS_BUILD_REVISION = $head
    $command = @('build', '--locked', '--release', '-p', 'engine-client', '--target', 'x86_64-pc-windows-msvc')
    & cargo @command 2>&1 | Tee-Object -FilePath "$OutputDirectory/build.log" | Out-Host
    $buildExit = $LASTEXITCODE
    $rustc = (& rustc -Vv) -join "`n"
    Assert-NativeExit $LASTEXITCODE 'rustc version'
    $record.build = [ordered]@{ command = @('cargo') + $command; exit_code = $buildExit; profile = 'release'; target = 'x86_64-pc-windows-msvc'; rustc = $rustc }
    Assert-NativeExit $buildExit 'Cargo release build'
    Copy-Item 'target/x86_64-pc-windows-msvc/release/engine-client.exe' "$stage/engine-client.exe"
    Write-SoftwareLauncher "$stage/run-software.cmd"
    # Conservative supported baseline, including dynamic MSVC and Windows UCRT.
    # Loaded modules are audited below; never infer clean-machine support from CI.
    $minimumRuntime = [version]'14.44.35211'
    $osInstructions = 'Use Windows 10 version 1809 or newer (x64), or Windows 11, with the Windows Universal CRT and current graphics drivers.'
    $vswhere = Join-Path ${env:ProgramFiles(x86)} 'Microsoft Visual Studio/Installer/vswhere.exe'
    $vs = (& $vswhere -latest -products '*' -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath)
    Assert-NativeExit $LASTEXITCODE 'Locate MSVC dependency tools'
    $dumpbin = Get-ChildItem "$vs/VC/Tools/MSVC/*/bin/Hostx64/x64/dumpbin.exe" | Sort-Object FullName -Descending | Select-Object -First 1
    if (-not $dumpbin) { throw 'MSVC dumpbin is required for the runtime inventory.' }
    $toolsetVersion = [version]([regex]::Match($dumpbin.FullName.Replace('\', '/'), '/MSVC/([0-9.]+)/').Groups[1].Value)
    if ($toolsetVersion -gt $minimumRuntime) { $minimumRuntime = $toolsetVersion }
    $vcInstructions = "Install Microsoft Visual C++ v14 Redistributable (x64), version $minimumRuntime or newer: https://aka.ms/vs/17/release/vc_redist.x64.exe"
    $imports = & $dumpbin.FullName /DEPENDENTS "$stage/engine-client.exe" 2>&1
    $importExit = $LASTEXITCODE
    $imports | Set-Content "$OutputDirectory/pe-dependencies.txt" -Encoding utf8
    Assert-NativeExit $importExit 'PE dependency inventory'
    foreach ($line in $imports) {
        if ([string]$line -match '^\s+([A-Za-z0-9_.-]+\.dll)\s*$') {
            $dll = $Matches[1]
            if ($dll -match '^(vcruntime|msvcp|concrt|vcomp|mfc|msvcr)' -and $dll -notin @('vcruntime140.dll', 'vcruntime140_1.dll', 'msvcp140.dll', 'msvcp140_1.dll', 'msvcp140_2.dll', 'msvcp140_atomic_wait.dll', 'msvcp140_codecvt_ids.dll', 'concrt140.dll')) {
                throw "Undeclared MSVC dependency: $dll"
            }
            if ($dll -notmatch '^api-ms-' -and -not (Test-Path (Join-Path $env:SystemRoot "System32/$dll"))) {
                throw "Missing unbundled import: $dll"
            }
        }
    }
    $info = [ordered]@{} + $identity
    $info.profile = 'release'; $info.target = 'x86_64-pc-windows-msvc'
    $info.required_runtime_files = @('engine-client.exe')
    $info.runtime_prerequisites = @(
        @{ name = 'Microsoft Visual C++ v14 x64'; dlls = @('vcruntime140.dll', 'vcruntime140_1.dll', 'msvcp140.dll', 'msvcp140_1.dll', 'msvcp140_2.dll', 'msvcp140_atomic_wait.dll', 'msvcp140_codecvt_ids.dll', 'concrt140.dll'); instructions = $vcInstructions },
        @{ name = 'Windows x64 and Universal CRT'; dlls = @('ucrtbase.dll'); instructions = $osInstructions }
    )
    Write-Json $info "$stage/build-info.json"
    @"
Space Wars Windows x64 nightly — source $head
Extract all files, then double-click engine-client.exe to open the launcher.
If default OpenGL startup fails, double-click run-software.cmd for software rendering.
From a terminal: run-software.cmd --scenario falling
No Rust toolchain or source checkout is needed. CLI/agent are not included.
$vcInstructions
$osInstructions
The UI is compiled by slint-build; Falling's ROM is embedded with include_bytes!.
System fonts are supplied by Windows. User-added NES ROMs are optional external content.
The hosted Windows probe validates launcher and Falling startup with software rendering;
it does not certify gameplay, audio, default OpenGL support or a clean installation.
SHA256SUMS covers every bundled file except itself. build-info.json records provenance.
"@ | Set-Content "$stage/README.txt" -Encoding utf8
    Write-Checksums $stage
    Compress-Archive -Path "$stage/*" -DestinationPath "$OutputDirectory/windows-client.zip"
    Expand-Archive -LiteralPath "$OutputDirectory/windows-client.zip" -DestinationPath $extract
    $exe = Join-Path $extract 'engine-client.exe'
    if ((Get-Sha256 $exe) -ne (Get-Sha256 "$stage/engine-client.exe")) { throw 'Extracted executable mismatch.' }
    $record.package = @{ file = 'windows-client.zip'; sha256 = Get-Sha256 "$OutputDirectory/windows-client.zip"; size_bytes = (Get-Item "$OutputDirectory/windows-client.zip").Length; executable_sha256 = Get-Sha256 $exe }
    $env:SLINT_BACKEND = $null
    $help = Start-Process -FilePath $env:ComSpec -ArgumentList @('/d', '/c', ('""' + $extract + '\run-software.cmd" --help"')) -WorkingDirectory $extract -PassThru -NoNewWindow -RedirectStandardOutput "$OutputDirectory/software-launch-help.txt" -RedirectStandardError "$OutputDirectory/software-launch-help-stderr.txt"
    try {
        if (-not $help.WaitForExit(15000)) {
            $help.Kill($true); $null = $help.WaitForExit(5000)
            throw 'Software wrapper help timed out.'
        }
        $record.software_launch = @{ help_exit_code = $help.ExitCode; sha256 = Get-Sha256 "$extract/run-software.cmd" }
        Assert-NativeExit $help.ExitCode 'Software wrapper help'
    } finally { $help.Dispose() }
    $env:PROBE_SOURCE_SHA = $head
    foreach ($mode in @('default', 'software')) {
        try {
            & "$PSScriptRoot/windows-gui-probe.ps1" -Executable $exe -OutputDirectory "$OutputDirectory/$mode" -Backend $mode -SettingsDirectory "$work/$mode-settings"
        } catch {
            if ($mode -eq 'software') { throw }
            $record.default_error = $_.Exception.Message
        }
    }
    # Inventory every loaded non-OS module. Unknown dependencies fail packaging
    # rather than silently relying on tools installed on the hosted runner.
    $audit = @()
    foreach ($mode in @('default', 'software')) {
        $probe = Get-Content "$OutputDirectory/$mode/result.json" -Raw | ConvertFrom-Json
        foreach ($phase in $probe.phases) {
            if (-not $phase.PSObject.Properties['modules']) { continue }
            foreach ($module in $phase.modules) {
                $audit += $module
                if ($module.name -match '^(vcruntime|msvcp|concrt)') {
                    $version = [version]([regex]::Match($module.version, '\d+\.\d+\.\d+(\.\d+)?').Value)
                    if ($version -lt $minimumRuntime) { throw "Runtime below documented minimum: $($module.name)" }
                }
                if ($module.path -ine $exe -and -not $module.path.StartsWith($env:SystemRoot + '\', [StringComparison]::OrdinalIgnoreCase)) {
                    throw "Unbundled dependency outside Windows: $($module.path)"
                }
            }
        }
    }
    Write-Json $audit "$OutputDirectory/runtime-modules.json"
    $record.status = 'captured'
} catch {
    $record.status = 'failed'; $record.error = $_.Exception.Message
    throw
} finally {
    $env:SPACEWARS_BUILD_REVISION = $previousRevision
    $env:SLINT_BACKEND = $previousBackend
    try {
        Write-Json $record "$OutputDirectory/release.json"
        Write-EvidenceManifest $identity $OutputDirectory
        if ($env:GITHUB_STEP_SUMMARY) {
            $sizeText = if ($record.Contains('package')) { "$($record.package.size_bytes) bytes" } else { 'not produced' }
            $defaultText = if ($record.Contains('default_error')) { $record.default_error } else { 'See default/result.json (separate diagnostic)' }
            @"
### Windows release
Source: $head; target: x86_64-pc-windows-msvc; profile: release
ZIP: $sizeText; required software path: $($record.status)
Default renderer: $defaultText
Visual review still required; hosted startup is not a gameplay/audio or clean-machine test.
"@ | Add-Content $env:GITHUB_STEP_SUMMARY
        }
    } finally {
        if (Test-Path $work) { Remove-Item -LiteralPath $work -Recurse -Force }
    }
}
