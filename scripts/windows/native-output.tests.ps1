# Real native processes and nested PS5 stream redirection; no external tools or Pester required.
$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'common.ps1')
function Assert-True([bool]$Condition, [string]$Message) {
    if (-not $Condition) { throw "FAIL: $Message" }
}
$directory = Join-Path $env:TEMP ('entwurf-native-output-' + [guid]::NewGuid().ToString('N'))
New-Item $directory -ItemType Directory | Out-Null
$helper = Join-Path $PSScriptRoot 'common.ps1'
$fixture = Join-Path $directory 'fixture.cmd'
$invalid = Join-Path $directory 'invalid.exe'
$stage = Join-Path $directory 'build-fixture.ps1'
$log = Join-Path $directory 'build.log'
Set-Content $fixture -Encoding Ascii -Value @'
@echo off
echo FIXTURE_STDOUT
echo FIXTURE_STDERR 1>&2
exit /b %1
'@
Set-Content $invalid -Encoding Ascii -Value 'this is not an executable'
Set-Content $stage -Encoding Ascii -Value @'
param([string]$Helper, [string]$Native, [int]$Code)
. $Helper
Invoke-CheckedNative $Native @([string]$Code)
Write-Output 'FIXTURE_AFTER_NATIVE'
'@
Push-Location $directory
try {
    # This is the build invocation that failed on normal Tauri stderr output.
    & $stage -Helper $helper -Native $fixture -Code 0 *>&1 | Tee-Object -FilePath $log | Out-Null
    $output = Get-Content $log -Raw
    Assert-True ($output -match 'FIXTURE_STDOUT' -and $output -match 'FIXTURE_STDERR' -and $output -match 'FIXTURE_AFTER_NATIVE') 'exit 0 preserves both streams and continues under outer Stop/Tee-Object'
    Assert-True ($ErrorActionPreference -eq 'Stop') 'caller Stop preference survives success'

    $failure = ''
    try { & $stage -Helper $helper -Native $fixture -Code 23 *>&1 | Tee-Object -FilePath $log | Out-Null }
    catch { $failure = $_.Exception.Message }
    Assert-True ($failure -match 'exit code 23') 'exit 23 throws the real native exit code rather than stderr text'
    $output = Get-Content $log -Raw
    Assert-True ($output -match 'FIXTURE_STDOUT' -and $output -match 'FIXTURE_STDERR' -and $output -notmatch 'FIXTURE_AFTER_NATIVE') 'failed native output is retained and later build steps do not run'
    Assert-True ($ErrorActionPreference -eq 'Stop') 'caller Stop preference survives nonzero exit'

    foreach ($unavailable in @((Join-Path $directory 'missing-tool.exe'), $invalid)) {
        # A stale successful LASTEXITCODE must never make launch failures succeed.
        & $env:ComSpec /d /c 'exit 0'
        $failure = ''
        try { & $stage -Helper $helper -Native $unavailable -Code 0 *>&1 | Tee-Object -FilePath $log | Out-Null }
        catch { $failure = $_.Exception.Message }
        Assert-True (-not [string]::IsNullOrWhiteSpace($failure)) 'missing or unlaunchable commands throw'
        Assert-True ((Get-Content $log -Raw) -notmatch 'FIXTURE_AFTER_NATIVE') 'launch failure cannot continue the build'
        Assert-True ($ErrorActionPreference -eq 'Stop') 'caller Stop preference survives launch failure'
    }
    foreach ($preference in @('Stop', 'Continue')) {
        $ErrorActionPreference = $preference
        Invoke-CheckedNative $env:ComSpec @('/d', '/c', 'exit 0')
        Assert-True ($ErrorActionPreference -eq $preference) 'the original caller preference survives a successful call'
        $failure = ''
        try { Invoke-CheckedNative $env:ComSpec @('/d', '/c', 'exit 17') } catch { $failure = $_.Exception.Message }
        Assert-True ($failure -match 'exit code 17') 'nonzero exits terminate even for a Continue caller'
        Assert-True ($ErrorActionPreference -eq $preference) 'the original caller preference survives a failed call'
    }
    Write-Output ('PASS: PowerShell {0}: stderr success/failure, nested Tee, missing/unlaunchable command, preference restoration' -f $PSVersionTable.PSVersion)
} finally {
    $ErrorActionPreference = 'Stop'
    Pop-Location
    Remove-Item $directory -Recurse -Force
}
