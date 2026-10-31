[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)][string]$Installer,
    [Parameter(Mandatory = $true)][string]$EvidenceRoot,
    [int]$StartCycle = 0,
    [int]$LastCycle = 2
)
. (Join-Path $PSScriptRoot 'common.ps1')
[Console]::OutputEncoding = [Text.UTF8Encoding]::new()
if ($StartCycle -lt 0 -or $LastCycle -lt $StartCycle) { throw 'Invalid acceptance cycle range.' }
$root = (Resolve-Path (Join-Path $PSScriptRoot '..\..')).Path
$Installer = (Resolve-Path -LiteralPath $Installer).Path
$EvidenceRoot = [IO.Path]::GetFullPath($EvidenceRoot).TrimEnd('\')
# Keep installed application files separate from user projects and build/OCR tools.
$installDirectory = Join-Path $env:LOCALAPPDATA 'Programs\Entwurf'
$executable = Join-Path $installDirectory 'entwurf-desktop.exe'
$projectFolder = ConvertFrom-Json '"\u4e2d\u6587 \u9879\u76ee\u6570\u636e"'
$projectDirectory = Join-Path $EvidenceRoot $projectFolder
$ownerFile = Join-Path $EvidenceRoot 'acceptance-owner.json'
$comparison = [StringComparison]::OrdinalIgnoreCase
if ($EvidenceRoot.Equals($installDirectory, $comparison) -or
    $EvidenceRoot.StartsWith($installDirectory + '\', $comparison) -or
    $installDirectory.StartsWith($EvidenceRoot + '\', $comparison)) {
    throw 'Acceptance evidence and installation directories must not overlap.'
}
if (Test-Path $ownerFile) {
    $owner = Get-Content $ownerFile -Raw -Encoding UTF8 | ConvertFrom-Json
    if ($owner.install_directory -ne $installDirectory -or $owner.project_directory -ne $projectDirectory) {
        throw 'Acceptance ownership does not match these application/project paths.'
    }
} else {
    if ((Test-Path $projectDirectory) -or
        ((Test-Path -LiteralPath $EvidenceRoot) -and @(Get-ChildItem -LiteralPath $EvidenceRoot -Force).Count -gt 0)) {
        throw 'A new acceptance run requires an empty evidence directory with no existing project.'
    }
    if (Test-Path $executable) { throw 'Existing Entwurf installation is not owned by this acceptance run.' }
    New-Item -ItemType Directory -Path $EvidenceRoot -Force | Out-Null
    $owner = @{ install_directory = $installDirectory; project_directory = $projectDirectory }
    [IO.File]::WriteAllText($ownerFile, ($owner | ConvertTo-Json), [Text.UTF8Encoding]::new($false))
}
function Stop-AcceptanceApp {
    foreach ($candidate in @(Get-CimInstance Win32_Process -Filter "Name='entwurf-desktop.exe'")) {
        if ($candidate.ExecutablePath -eq $executable) { Stop-Process -Id $candidate.ProcessId -Force -ErrorAction SilentlyContinue }
    }
}
function Invoke-Installer([string]$Path, [string]$Arguments) {
    $process = Start-Process -FilePath $Path -ArgumentList $Arguments -PassThru
    if (-not $process.WaitForExit(180000)) { throw 'Installer exceeded the three-minute acceptance deadline.' }
    if ($process.ExitCode -ne 0) { throw "Installer exited with $($process.ExitCode)." }
}
function Get-ProjectHashes {
    $files = @{}
    foreach ($file in @(Get-ChildItem -LiteralPath $projectDirectory -Recurse -File -Force)) {
        $relative = $file.FullName.Substring($projectDirectory.Length).TrimStart('\')
        $files[$relative] = (Get-FileHash -LiteralPath $file.FullName -Algorithm SHA256).Hash
    }
    return $files
}
for ($cycle = $StartCycle; $cycle -le $LastCycle; $cycle++) {
    $cycleDirectory = Join-Path $EvidenceRoot "cycle-$cycle"
    New-Item -ItemType Directory -Path $cycleDirectory -Force | Out-Null
    Stop-AcceptanceApp
    $record = @{ cycle = $cycle; started_at = [DateTime]::UtcNow.ToString('o'); installer_sha256 = (Get-FileHash -LiteralPath $Installer -Algorithm SHA256).Hash.ToLowerInvariant(); installer_applied_this_cycle = $false; reused_existing_installation = $false }
    if ($cycle -gt 0) {
        $mirror = Join-Path $projectDirectory 'project.json'
        if (-not (Test-Path $mirror)) { throw 'Existing acceptance project must be present before uninstalling.' }
        $before = Get-ProjectHashes
        $uninstallers = @(Get-ChildItem $installDirectory -Filter '*uninstall*.exe' -File)
        if ($uninstallers.Count -ne 1) { throw 'Could not uniquely locate this installation uninstaller.' }
        # NSIS documented flags: /S; _?= must be last and unquoted even with spaces.
        # https://nsis.sourceforge.io/Docs/Chapter3.html#3.2
        Invoke-Installer $uninstallers[0].FullName "/S _?=$installDirectory"
        if (Test-Path $executable) { throw 'Uninstaller left the application executable installed.' }
        $after = Get-ProjectHashes
        if ($before.Count -ne $after.Count) { throw 'Uninstall changed the user project file count.' }
        foreach ($relative in $before.Keys) {
            if ($after[$relative] -ne $before[$relative]) { throw 'Uninstall modified a user project file.' }
        }
        $record.uninstalled = $true
        $record.project_unchanged_after_uninstall = $true
        $record.project_files_verified_after_uninstall = $before.Count
    }
    if (-not (Test-Path $executable)) {
        # NSIS /D must be the final unquoted parameter.
        Invoke-Installer $Installer "/S /D=$installDirectory"
        $record.installer_applied_this_cycle = $true
    } else {
        $record.reused_existing_installation = $true
    }
    if (-not (Test-Path $executable)) { throw 'Installed desktop executable is missing.' }
    if (-not (Test-Path (Join-Path $installDirectory 'resources\pdfium\pdfium.dll'))) { throw 'Installed PDFium is missing.' }
    if (-not (Test-Path (Join-Path $installDirectory 'resources\gpt-bridge\Entwurf.Bridge.exe'))) { throw 'Installed bridge is missing.' }
    $noticeDirectory = Join-Path $installDirectory 'resources\licenses'
    if (-not (Test-Path (Join-Path $noticeDirectory 'THIRD-PARTY-NOTICES.txt'))) { throw 'Installed third-party notices are missing.' }
    foreach ($notice in @('THIRD-PARTY-NOTICES.txt', 'dependency-inventory.json')) {
        $installedHash = (Get-FileHash (Join-Path $noticeDirectory $notice) -Algorithm SHA256).Hash
        $sourceNotice = Join-Path $root "apps\desktop\src-tauri\resources\licenses\$notice"
        if ($installedHash -ne (Get-FileHash $sourceNotice -Algorithm SHA256).Hash) { throw 'Installed notices differ from the verified inventory.' }
    }
    $record.installed_notices_match_source = $true
    $record.installed = $true
    $record.executable_sha256 = (Get-FileHash $executable -Algorithm SHA256).Hash.ToLowerInvariant()
    $port = 19340 + $cycle
    $info = [Diagnostics.ProcessStartInfo]::new($executable)
    $info.WorkingDirectory = $installDirectory
    $info.UseShellExecute = $false
    # Debug endpoint exists only in this test-launched WebView, bound to localhost.
    $info.EnvironmentVariables['WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS'] = "--remote-debugging-address=127.0.0.1 --remote-debugging-port=$port --force-renderer-accessibility"
    $info.EnvironmentVariables['WEBVIEW2_USER_DATA_FOLDER'] = Join-Path $cycleDirectory 'webview-profile'
    $process = [Diagnostics.Process]::Start($info)
    try {
        $configuration = @{
            endpoint = "http://127.0.0.1:$port"; process_id = $process.Id; cycle = $cycle
            evidence = $cycleDirectory; project = $projectDirectory
            source = Join-Path $root 'fixtures\public\probability-chapter.md'
        }
        $configurationPath = Join-Path $cycleDirectory 'configuration.json'
        [IO.File]::WriteAllText($configurationPath, ($configuration | ConvertTo-Json), [Text.UTF8Encoding]::new($false))
        Invoke-CheckedNative 'node' @((Join-Path $PSScriptRoot 'accept-desktop.mjs'), $configurationPath)
        $record.ui_acceptance_passed = $true
    } finally {
        Stop-AcceptanceApp
        $record.finished_at = [DateTime]::UtcNow.ToString('o')
        [IO.File]::WriteAllText((Join-Path $cycleDirectory 'installation.json'), ($record | ConvertTo-Json -Depth 5), [Text.UTF8Encoding]::new($false))
    }
    Write-Output "PASS: installed desktop acceptance cycle $cycle"
}
Write-Output "Desktop acceptance completed for cycles $StartCycle through $LastCycle. Evidence: $EvidenceRoot"
