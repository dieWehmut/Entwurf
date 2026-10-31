[CmdletBinding()]
param()
Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
# Load named function definitions only. Never dot-source the installation loop.
$driver = Join-Path $PSScriptRoot 'reinstall-acceptance.ps1'
$tokens = $null; $parseErrors = $null
$ast = [Management.Automation.Language.Parser]::ParseFile($driver, [ref]$tokens, [ref]$parseErrors)
if ($parseErrors.Count -ne 0) { throw 'Acceptance driver did not parse.' }
$names = @('Get-PreservedDataSnapshot', 'Assert-PreservedDataUnchanged', 'Get-AcceptanceBridgeChildren', 'Wait-AcceptanceBridgeExit', 'Stop-AcceptanceApp')
foreach ($name in $names) {
    $definition = @($ast.FindAll({ param($node) $node -is [Management.Automation.Language.FunctionDefinitionAst] -and $node.Name -eq $name }, $true))
    if ($definition.Count -ne 1) { throw "Expected one preservation function: $name" }
    . ([ScriptBlock]::Create($definition[0].Extent.Text))
}
function Assert-Throws([ScriptBlock]$Action, [string]$Message) {
    $caught = $false
    try { & $Action } catch { $caught = $true }
    if (-not $caught) { throw $Message }
}
function Assert-True([bool]$Condition, [string]$Message) { if (-not $Condition) { throw $Message } }

$fixtureRoot = Join-Path $env:TEMP ('entwurf-preservation-test-' + [Guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $fixtureRoot | Out-Null
try {
    $localWeb = Join-Path $fixtureRoot 'local-web'
    $generated = Join-Path $localWeb 'projects\one\generated'
    $automation = Join-Path $localWeb 'automation'
    New-Item -ItemType Directory -Path $generated, $automation -Force | Out-Null
    [IO.File]::WriteAllBytes((Join-Path $localWeb 'workspace.sqlite'), [byte[]](1, 2, 3))
    [IO.File]::WriteAllBytes((Join-Path $generated 'fixture.png'), [byte[]](4, 5, 6))
    [IO.File]::WriteAllText((Join-Path $automation 'native-state.json'), '{"fixture":true}')
    # A sibling profile must never become part of this exact subtree snapshot.
    New-Item -ItemType Directory -Path (Join-Path $fixtureRoot 'EBWebView') | Out-Null
    [IO.File]::WriteAllText((Join-Path $fixtureRoot 'EBWebView\fixture'), 'unrelated fixture')
    $before = Get-PreservedDataSnapshot $localWeb
    Assert-True ($before.files.Count -eq 3) 'Snapshot crossed the owned local-web boundary.'
    Assert-PreservedDataUnchanged $before (Get-PreservedDataSnapshot $localWeb) 'fixture'
    [IO.File]::WriteAllBytes((Join-Path $localWeb 'workspace.sqlite'), [byte[]](3, 2, 1))
    Assert-Throws { Assert-PreservedDataUnchanged $before (Get-PreservedDataSnapshot $localWeb) 'fixture' } 'Same-size SQLite modification was not detected.'
    [IO.File]::WriteAllBytes((Join-Path $localWeb 'workspace.sqlite'), [byte[]](1, 2, 3))
    Rename-Item -LiteralPath (Join-Path $generated 'fixture.png') -NewName 'renamed.png'
    Assert-Throws { Assert-PreservedDataUnchanged $before (Get-PreservedDataSnapshot $localWeb) 'fixture' } 'Same-count image rename was not detected.'
    Rename-Item -LiteralPath (Join-Path $generated 'renamed.png') -NewName 'fixture.png'
    Remove-Item -LiteralPath (Join-Path $automation 'native-state.json')
    Assert-Throws { Assert-PreservedDataUnchanged $before (Get-PreservedDataSnapshot $localWeb) 'fixture' } 'Native state removal was not detected.'
    $missing = Join-Path $fixtureRoot 'not-created'
    $absent = Get-PreservedDataSnapshot $missing
    Assert-PreservedDataUnchanged $absent (Get-PreservedDataSnapshot $missing) 'fixture'
    New-Item -ItemType Directory -Path $missing | Out-Null
    Assert-Throws { Assert-PreservedDataUnchanged (Get-PreservedDataSnapshot $missing) $absent 'fixture' } 'Empty data root removal was not detected.'
    $junction = Join-Path $localWeb 'redirect'
    New-Item -ItemType Junction -Path $junction -Target (Join-Path $fixtureRoot 'EBWebView') | Out-Null
    Assert-Throws { Get-PreservedDataSnapshot $localWeb | Out-Null } 'Snapshot followed a junction to a profile.'
    [IO.Directory]::Delete($junction)

    # Process fixtures replace all process discovery/stopping; no live process is stopped.
    $executable = Join-Path $fixtureRoot 'installed\entwurf-desktop.exe'
    $bridgeExecutable = Join-Path $fixtureRoot 'installed\resources\gpt-bridge\Entwurf.Bridge.exe'
    $created = [DateTime]::UtcNow.AddMinutes(-1)
    $parent = [pscustomobject]@{ ProcessId = 101; ExecutablePath = $executable; CreationDate = $created }
    $otherParent = [pscustomobject]@{ ProcessId = 102; ExecutablePath = 'C:\unowned\entwurf-desktop.exe'; CreationDate = $created }
    $child = [pscustomobject]@{ ProcessId = 201; ParentProcessId = 101; ExecutablePath = $bridgeExecutable; CreationDate = $created.AddSeconds(1) }
    $unownedChild = [pscustomobject]@{ ProcessId = 202; ParentProcessId = 102; ExecutablePath = $bridgeExecutable; CreationDate = $created.AddSeconds(1) }
    $foreignBinary = [pscustomobject]@{ ProcessId = 203; ParentProcessId = 101; ExecutablePath = 'C:\unowned\Entwurf.Bridge.exe'; CreationDate = $created.AddSeconds(1) }
    $oldIdentity = [pscustomobject]@{ ProcessId = 204; ParentProcessId = 101; ExecutablePath = $bridgeExecutable; CreationDate = $created.AddMinutes(-1) }
    $script:stopped = @(); $script:parentStopped = $false; $script:bridgeQueries = 0; $script:mode = 'exits'
    function Get-CimInstance([string]$ClassName, [string]$Filter) {
        if ($Filter -eq "Name='entwurf-desktop.exe'") { if ($script:mode -eq 'no-owned-parent') { return $otherParent }; return @($parent, $otherParent) }
        if ($Filter -ne "Name='Entwurf.Bridge.exe'") { throw 'Unexpected process query.' }
        $script:bridgeQueries++
        if ($script:mode -eq 'reused') { return [pscustomobject]@{ ProcessId = 201; ParentProcessId = 999; ExecutablePath = $bridgeExecutable; CreationDate = $created.AddSeconds(20) } }
        if ($script:mode -eq 'late') { if ($script:parentStopped) { return $child }; return @() }
        if ($script:mode -eq 'lingers' -or -not $script:parentStopped) { return @($child, $unownedChild, $foreignBinary, $oldIdentity) }
        return @($unownedChild, $foreignBinary)
    }
    function Stop-Process([int]$Id, [switch]$Force, [string]$ErrorAction) { $script:stopped += $Id; $script:parentStopped = $true }
    Stop-AcceptanceApp
    Assert-True ($script:stopped.Count -eq 1 -and $script:stopped[0] -eq 101) 'An unowned process was stopped.'
    Assert-True ($script:bridgeQueries -ge 3) 'Bridge exit was not checked after parent shutdown.'
    $script:mode = 'lingers'
    Assert-Throws { Wait-AcceptanceBridgeExit @($child) 20 } 'A lingering native child was ignored.'
    Assert-True ($script:stopped.Count -eq 1) 'A lingering child was force-stopped.'
    $script:mode = 'reused'
    Wait-AcceptanceBridgeExit @($child) 20
    $script:mode = 'late'; $script:parentStopped = $false
    Assert-True (@(Get-AcceptanceBridgeChildren @($parent)).Count -eq 0) 'Late-child fixture was not initially empty.'
    $script:parentStopped = $true
    Assert-True (@(Get-AcceptanceBridgeChildren @($parent)).Count -eq 1) 'A child born during shutdown was missed.'
    $script:mode = 'no-owned-parent'
    Stop-AcceptanceApp
    Assert-True ($script:stopped.Count -eq 1) 'Stopping with no owned desktop affected another process.'

    # Exercise original refusal paths with temporary roots and inert files only.
    # Any unexpected attempt to launch an installer is trapped before execution.
    $savedLocalAppData = $env:LOCALAPPDATA; $savedAppData = $env:APPDATA; $savedPath = $env:PATH
    $script:launchAttempted = $false
    function Start-Process { $script:launchAttempted = $true; throw 'Unexpected process launch in a refusal fixture.' }
    function Assert-PreflightRefusal([string]$Evidence, [string]$Expected) {
        $message = ''
        try { & $driver -Installer $fixtureInstaller -EvidenceRoot $Evidence -StartCycle 3 -LastCycle 4 | Out-Null }
        catch { $message = $_.Exception.Message }
        Assert-True ($message -eq $Expected) 'Original acceptance ownership/preflight refusal changed.'
    }
    try {
        $env:LOCALAPPDATA = Join-Path $fixtureRoot 'temporary-local-appdata'
        $env:APPDATA = Join-Path $fixtureRoot 'temporary-roaming-appdata'
        $fixtureInstaller = Join-Path $fixtureRoot 'inert-installer.exe'
        [IO.File]::WriteAllText($fixtureInstaller, 'not an executable')
        $fixtureInstallDirectory = Join-Path $env:LOCALAPPDATA 'Programs\Entwurf'
        New-Item -ItemType Directory -Path $fixtureInstallDirectory -Force | Out-Null
        Assert-PreflightRefusal $fixtureInstallDirectory 'Acceptance evidence and installation directories must not overlap.'
        $fixtureExecutable = Join-Path $fixtureInstallDirectory 'entwurf-desktop.exe'
        [IO.File]::WriteAllText($fixtureExecutable, 'not an executable')
        Assert-PreflightRefusal (Join-Path $fixtureRoot 'unowned-installation') 'Existing Entwurf installation is not owned by this acceptance run.'
        Remove-Item -LiteralPath $fixtureExecutable
        $nonempty = Join-Path $fixtureRoot 'nonempty-evidence'
        New-Item -ItemType Directory -Path $nonempty | Out-Null
        [IO.File]::WriteAllText((Join-Path $nonempty 'fixture'), 'owned test data')
        Assert-PreflightRefusal $nonempty 'A new acceptance run requires an empty evidence directory with no existing project.'
        $mismatched = Join-Path $fixtureRoot 'mismatched-owner'
        New-Item -ItemType Directory -Path $mismatched | Out-Null
        [IO.File]::WriteAllText((Join-Path $mismatched 'acceptance-owner.json'), (@{install_directory=$fixtureInstallDirectory;project_directory='not-this-project'} | ConvertTo-Json))
        Assert-PreflightRefusal $mismatched 'Acceptance ownership does not match these application/project paths.'
        Assert-True (-not $script:launchAttempted) 'A refusal fixture reached an installer invocation.'
    } finally { $env:LOCALAPPDATA = $savedLocalAppData; $env:APPDATA = $savedAppData; $env:PATH = $savedPath }
    Write-Output 'PASS: owned data preservation, redirected-path refusal, child exit/timeout, process ownership, and four original preflight refusals. No installer or live application was invoked.'
} finally {
    Remove-Item -LiteralPath $fixtureRoot -Recurse -Force
}
