[CmdletBinding()]
param()
. (Join-Path $PSScriptRoot 'common.ps1')
$root = (Resolve-Path (Join-Path $PSScriptRoot '..\..')).Path
$destination = Join-Path $root 'apps\desktop\src-tauri\resources\gpt-bridge'
$project = Join-Path $root 'sidecars\gpt-bridge\Entwurf.Bridge\Entwurf.Bridge.csproj'
$stage = Join-Path ([IO.Path]::GetTempPath()) ('entwurf-bridge-publish-' + [Guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $stage | Out-Null
try {
    Invoke-CheckedNative 'dotnet' @('publish', $project, '-c', 'Release', '-r', 'win-x64', '--self-contained', 'true', '--nologo', '-o', $stage)
    foreach ($required in @('Entwurf.Bridge.exe', 'Entwurf.Bridge.dll', 'coreclr.dll', 'hostfxr.dll')) {
        if (-not (Test-Path (Join-Path $stage $required) -PathType Leaf)) { throw "Self-contained bridge publish omitted $required" }
    }
    New-Item -ItemType Directory -Path $destination -Force | Out-Null
    # Remove stale generated DLLs after a successful publish; preserve tracked docs.
    Get-ChildItem $destination -Force | Where-Object { $_.Name -notin @('.gitignore', 'README.md') } | Remove-Item -Recurse -Force
    Copy-Item (Join-Path $stage '*') $destination -Recurse -Force
    Write-Output 'Prepared self-contained Windows x64 ChatGPT adapter for Tauri packaging. No ChatGPT prompt was sent.'
} finally { Remove-Item $stage -Recurse -Force }
