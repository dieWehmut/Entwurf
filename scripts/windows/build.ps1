[CmdletBinding()]
param([switch]$Bundle)
. (Join-Path $PSScriptRoot 'common.ps1')
# This script builds in the current native Windows checkout. It does not install tools.
& (Join-Path $PSScriptRoot 'check-prerequisites.ps1') -Strict
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
$root = (Resolve-Path (Join-Path $PSScriptRoot '..\..')).Path
Push-Location $root
try {
    & (Join-Path $PSScriptRoot 'prepare-pdfium.ps1')
    & (Join-Path $PSScriptRoot 'prepare-gpt-bridge.ps1')
    Push-Location (Join-Path $root 'apps\desktop')
    try {
        Invoke-CheckedNative 'pnpm' @('install', '--frozen-lockfile')
        Invoke-CheckedNative 'pnpm' @('build')
    } finally { Pop-Location }
    Invoke-CheckedNative 'cargo' @('build', '--locked', '--workspace', '--release', '--target', 'x86_64-pc-windows-msvc')
    Push-Location (Join-Path $root 'apps\desktop')
    try {
        $tauriArguments = @('tauri', 'build', '--target', 'x86_64-pc-windows-msvc')
        if ($Bundle) { $tauriArguments += @('--bundles', 'nsis') } else { $tauriArguments += '--no-bundle' }
        $tauriArguments += @('--', '--locked')
        Invoke-CheckedNative 'pnpm' $tauriArguments
    } finally { Pop-Location }
    Write-Output 'Build completed with the self-contained GPT bridge bundled as resources. ChatGPT automation remains disabled pending all four real acceptance gates.'
} finally { Pop-Location }
