# No test framework installation is needed; compatible with Windows PowerShell 5.1.
$ErrorActionPreference = 'Stop'
$helper = Join-Path $PSScriptRoot 'common.ps1'
if (-not (Test-Path $helper)) { throw 'FAIL: prerequisite evaluation is not implemented' }
. $helper
function Assert-True([bool]$Condition, [string]$Message) {
    if (-not $Condition) { throw "FAIL: $Message" }
}
$snapshot = @{
    WindowsVersion = [version]'10.0.19045'; Architecture = 'AMD64'
    NodeVersion = 'v22.12.0'; PnpmVersion = '10.0.0'; CargoVersion = 'cargo 1.99.1'; RustVersion = '1.99.1'
    RustHost = 'x86_64-pc-windows-msvc'; RustTargets = @('x86_64-pc-windows-msvc')
    DotnetSdks = @('10.0.100 [C:\sdk]'); MsvcAvailable = $true; WindowsSdkAvailable = $true
    WebViewVersion = '130.0.0.0'; TesseractVersion = ''; OcrLanguages = @()
}
$results = @(Get-RequirementResults $snapshot)
Assert-True (@($results | Where-Object { $_.Required -and -not $_.Available }).Count -eq 0) 'valid Windows build prerequisites are accepted'
Assert-True (-not ($results | Where-Object Name -eq 'Tesseract').Available) 'missing OCR is reported without blocking builds'
foreach ($version in @('1.91.1', '1.80.0', '', 'invalid')) {
    $snapshot.RustVersion = $version
    Assert-True (-not (Get-RequirementResults $snapshot | Where-Object Name -eq 'Rust MSVC').Available) "Rust $version is rejected before compiling the pinned renderer"
}
$snapshot.RustVersion = '1.92.0'
Assert-True ((Get-RequirementResults $snapshot | Where-Object Name -eq 'Rust MSVC').Available) 'the pinned renderer minimum Rust 1.92 is accepted'
$snapshot.RustVersion = '1.99.1'
foreach ($version in @('8.15.0', '9.15.0', '11.0.0', 'invalid', '10.34.9-rc.1')) {
    $snapshot.PnpmVersion = $version
    $results = @(Get-RequirementResults $snapshot)
    Assert-True (-not ($results | Where-Object Name -eq 'pnpm').Available) "pnpm $version is rejected before frozen install"
    Assert-True (@($results | Where-Object { $_.Required -and -not $_.Available }).Count -gt 0) 'unsupported pnpm prevents readiness'
}
$snapshot.PnpmVersion = '10.34.9'
Assert-True ((Get-RequirementResults $snapshot | Where-Object Name -eq 'pnpm').Available) 'verified pnpm 10.34.9 is accepted'
$snapshot.NodeVersion = 'v20.19.0'
$snapshot.DotnetSdks = @('9.0.300 [C:\sdk]')
$snapshot.RustHost = 'x86_64-pc-windows-gnu'
$snapshot.Architecture = 'ARM64'
$results = @(Get-RequirementResults $snapshot)
foreach ($name in @('Windows x64', 'Node.js', '.NET 10 SDK', 'Rust MSVC')) {
    Assert-True (-not ($results | Where-Object Name -eq $name).Available) "$name rejects incompatible prerequisites"
}
$snapshot.TesseractVersion = 'tesseract 5.5.0'
$snapshot.OcrLanguages = @('chi_sim', 'eng')
Assert-True ((Get-RequirementResults $snapshot | Where-Object Name -eq 'Tesseract').Available) 'OCR accepts both required languages'
$snapshot.OcrLanguages = @('eng')
Assert-True (-not (Get-RequirementResults $snapshot | Where-Object Name -eq 'Tesseract').Available) 'OCR reports missing Chinese language data'
# Prove failed native commands stop the build, rather than allowing later commands.
$failed = $false
Push-Location $env:TEMP
try {
    try { Invoke-CheckedNative $env:ComSpec @('/d', '/c', 'exit 23') } catch { $failed = $_.Exception.Message -match '23' }
} finally { Pop-Location }
Assert-True $failed 'nonzero native exit codes propagate as terminating errors'
Write-Output 'PASS: prerequisite compatibility, optional OCR, and native failure propagation'
