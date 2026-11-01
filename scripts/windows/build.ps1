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
    # Tauri builds the shipped desktop with its custom-protocol feature below.
    # Restore metadata/notices without a second LTO build of the internal CLI
    # and the desktop's development-protocol variant.
    Invoke-CheckedNative 'cargo' @('fetch', '--locked')
    Invoke-CheckedNative 'node' @('scripts/release/generate-notices.mjs')
    Push-Location (Join-Path $root 'apps\desktop')
    try {
        $tauriArguments = @('tauri', 'build', '--target', 'x86_64-pc-windows-msvc')
        if ($Bundle) { $tauriArguments += @('--bundles', 'nsis') } else { $tauriArguments += '--no-bundle' }
        $tauriArguments += @('--', '--locked')
        Invoke-CheckedNative 'pnpm' $tauriArguments
    } finally { Pop-Location }
    Write-Output 'Build completed with the self-contained ChatGPT adapter. Installed-desktop and real-account acceptance are separate release checks.'
} finally { Pop-Location }
