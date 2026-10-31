[CmdletBinding()]
param([switch]$E2E)
. (Join-Path $PSScriptRoot 'common.ps1')
& (Join-Path $PSScriptRoot 'check-prerequisites.ps1') -Strict
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
& (Join-Path $PSScriptRoot 'scripts.tests.ps1')
& (Join-Path $PSScriptRoot 'pnpm-restricted.tests.ps1')
$root = (Resolve-Path (Join-Path $PSScriptRoot '..\..')).Path
Push-Location $root
try {
    Invoke-CheckedNative 'cargo' @('fmt', '--all', '--', '--check')
    Invoke-CheckedNative 'cargo' @('test', '--locked', '--workspace', '--target', 'x86_64-pc-windows-msvc')
    Invoke-CheckedNative 'dotnet' @('test', 'sidecars/gpt-bridge/Entwurf.Bridge.Tests/Entwurf.Bridge.Tests.csproj', '-c', 'Release', '--logger', 'trx', '--results-directory', (Join-Path $root 'artifacts\test-results'))
    Push-Location (Join-Path $root 'apps\desktop')
    try {
        Invoke-CheckedNative 'pnpm' @('install', '--frozen-lockfile')
        Invoke-CheckedNative 'pnpm' @('test')
        Invoke-CheckedNative 'pnpm' @('build')
        if ($E2E) { Invoke-CheckedNative 'pnpm' @('test:e2e') }
    } finally { Pop-Location }
} finally { Pop-Location }
