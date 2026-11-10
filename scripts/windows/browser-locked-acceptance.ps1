[CmdletBinding()]
param(
    [switch]$ProbeOnly,
    [int]$BrowserProcessId,
    [string]$NodeExecutable,
    [string]$ChromiumExecutable,
    [string]$WebSocketModule,
    [string]$TlsCertificate,
    [string]$TlsKey,
    [string]$ArtifactDirectory,
    [string]$ExtensionDirectory,
    [string]$FixtureImage,
    [string]$BackendUrl,
    [string]$BackendDataDirectory,
    [ValidateRange(65000,540000)][int]$GenerationWaitMilliseconds = 65000
)
$ErrorActionPreference = 'Stop'
[Console]::OutputEncoding = [Text.UTF8Encoding]::new($false)
Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
public static class EntwurfLockedBrowserProbe {
 [DllImport("wtsapi32.dll", SetLastError=true)] public static extern bool WTSQuerySessionInformation(IntPtr server, int session, int cls, out IntPtr buffer, out int bytes);
 [DllImport("wtsapi32.dll")] public static extern void WTSFreeMemory(IntPtr memory);
 [DllImport("user32.dll", SetLastError=true)] public static extern IntPtr OpenInputDesktop(uint flags, bool inherit, uint access);
 [DllImport("user32.dll")] public static extern bool CloseDesktop(IntPtr desktop);
}
'@
function Get-LockedBrowserSession {
    $probeSession = (Get-Process -Id $PID).SessionId
    $session = if ($BrowserProcessId) { (Get-Process -Id $BrowserProcessId -ErrorAction Stop).SessionId } else { $probeSession }
    $buffer = [IntPtr]::Zero; $bytes = 0
    if (-not [EntwurfLockedBrowserProbe]::WTSQuerySessionInformation([IntPtr]::Zero, $session, 25, [ref]$buffer, [ref]$bytes)) { throw 'WTSInfoEx query failed' }
    try {
        if ($bytes -lt 20 -or [Runtime.InteropServices.Marshal]::ReadInt32($buffer, 0) -ne 1) { throw 'Unexpected WTSInfoEx layout' }
        $id = [Runtime.InteropServices.Marshal]::ReadInt32($buffer, 8)
        $state = [Runtime.InteropServices.Marshal]::ReadInt32($buffer, 12)
        $flags = [Runtime.InteropServices.Marshal]::ReadInt32($buffer, 16)
    } finally { [EntwurfLockedBrowserProbe]::WTSFreeMemory($buffer) }
    $desktop = [EntwurfLockedBrowserProbe]::OpenInputDesktop(0, $false, 1)
    $desktopError = [Runtime.InteropServices.Marshal]::GetLastWin32Error()
    if ($desktop -ne [IntPtr]::Zero) { [void][EntwurfLockedBrowserProbe]::CloseDesktop($desktop) }
    # Windows 10/11: WTS_SESSIONSTATE_LOCK=0. No desktop switch or input is used.
    [pscustomobject]@{ utc = [DateTime]::UtcNow.ToString('o'); session_id = $id; probe_session = $probeSession; browser_process_id = $BrowserProcessId; browser_session = if ($BrowserProcessId) { $session } else { $null }; connection_state = $state; session_flags = $flags; locked = ($flags -eq 0); input_desktop_accessible = ($desktop -ne [IntPtr]::Zero); input_desktop_error = $desktopError }
}
$probe = Get-LockedBrowserSession
if ($ProbeOnly) { $probe | ConvertTo-Json -Compress; exit 0 }
if (-not $probe.locked -or $probe.input_desktop_accessible) { throw 'This acceptance requires an already locked Windows session; it never locks or unlocks Windows itself.' }
if (-not $ExtensionDirectory) { $ExtensionDirectory = Join-Path $PSScriptRoot '../../sidecars/chatgpt-browser' }
if (-not $FixtureImage) { $FixtureImage = Join-Path $PSScriptRoot '../../fixtures/public/ingestion/bilingual.png' }
foreach ($required in @('NodeExecutable','ChromiumExecutable','WebSocketModule','TlsCertificate','TlsKey','ArtifactDirectory')) {
    if (-not (Get-Variable -Name $required -ValueOnly)) { throw "Missing -$required" }
}
$arguments = @(
    (Join-Path $PSScriptRoot 'browser-locked-acceptance.mjs'),
    '--chromium', $ChromiumExecutable, '--websocket-module', $WebSocketModule,
    '--tls-cert', $TlsCertificate, '--tls-key', $TlsKey,
    '--artifacts', $ArtifactDirectory, '--extension', $ExtensionDirectory,
    '--image', $FixtureImage, '--windows-probe', $PSCommandPath,
    '--generation-wait-ms', [string]$GenerationWaitMilliseconds
)
if ($BackendUrl) { $arguments += @('--backend-url', $BackendUrl, '--backend-data-dir', $BackendDataDirectory) }
elseif ($BackendDataDirectory) { throw '-BackendDataDirectory requires -BackendUrl' }
& $NodeExecutable @arguments
if ($LASTEXITCODE -ne 0) { throw "Locked browser acceptance failed (Node exit $LASTEXITCODE)." }
