$ErrorActionPreference = 'Stop'
$helper = (Join-Path $PSScriptRoot 'common.ps1').Replace("'", "''")
# Loading reviewed helper text avoids a script-file policy dependency in the child.
# A .ps1 pnpm shim remains subject to Restricted; a .cmd shim is usable.
$command = ". ([scriptblock]::Create([IO.File]::ReadAllText('$helper'))); Get-ToolOutput 'pnpm' @('--version')"
$version = & powershell.exe -NoProfile -ExecutionPolicy Restricted -Command $command
if ($LASTEXITCODE -ne 0 -or ($version | Out-String) -notmatch '\d+\.\d+\.\d+') {
    throw 'FAIL: native pnpm is unavailable under Restricted when its .ps1 shim is present'
}
Write-Output 'PASS: pnpm uses its CMD wrapper under Restricted execution policy'
