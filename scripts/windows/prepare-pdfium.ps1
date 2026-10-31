[CmdletBinding()]
param([string]$ArchivePath)
$ErrorActionPreference = 'Stop'
$root = (Resolve-Path (Join-Path $PSScriptRoot '..\..')).Path
$destination = Join-Path $root 'apps\desktop\src-tauri\resources\pdfium'
$manifest = Get-Content (Join-Path $destination 'manifest.json') -Raw | ConvertFrom-Json
$dll = Join-Path $destination 'pdfium.dll'
$expectedDll = '79d4676b656cfb1abcea88f9ade3b4b0826c5200382db5f4ec72a636c598c118'
if ((Test-Path $dll) -and ((Get-FileHash $dll -Algorithm SHA256).Hash.ToLowerInvariant() -eq $expectedDll)) {
    Write-Output 'Pinned PDFium 7881 is already prepared.'
    return
}
$temp = Join-Path ([IO.Path]::GetTempPath()) ('entwurf-pdfium-' + [Guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $temp | Out-Null
try {
    if (-not $ArchivePath) {
        $ArchivePath = Join-Path $temp 'pdfium-win-x64.tgz'
        Invoke-WebRequest -UseBasicParsing -Uri $manifest.windows_x64_archive -OutFile $ArchivePath
    }
    if ((Get-FileHash $ArchivePath -Algorithm SHA256).Hash.ToLowerInvariant() -ne $manifest.windows_x64_sha256) {
        throw 'PDFium archive SHA-256 mismatch.'
    }
    & tar.exe -xzf $ArchivePath -C $temp
    if ($LASTEXITCODE -ne 0) { throw "PDFium extraction failed: $LASTEXITCODE" }
    $extracted = Join-Path $temp 'bin\pdfium.dll'
    if ((Get-FileHash $extracted -Algorithm SHA256).Hash.ToLowerInvariant() -ne $expectedDll) { throw 'PDFium DLL SHA-256 mismatch.' }
    Copy-Item $extracted $dll -Force
    # Preserve all upstream notices in the packaged resources.
    Copy-Item (Join-Path $temp 'LICENSE') (Join-Path $destination 'LICENSE') -Force
    Copy-Item (Join-Path $temp 'licenses\*') (Join-Path $destination 'licenses') -Force
    Write-Output 'Prepared verified PDFium 151.0.7881.0 for offline runtime use.'
} finally { Remove-Item $temp -Recurse -Force }
