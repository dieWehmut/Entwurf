[CmdletBinding()]
param([switch]$Json, [switch]$Strict)
. (Join-Path $PSScriptRoot 'common.ps1')
try {
    $snapshot = Get-PrerequisiteSnapshot
    $checks = @(Get-RequirementResults $snapshot)
    $ready = @($checks | Where-Object { $_.Required -and -not $_.Available }).Count -eq 0
    if ($Json) {
        [pscustomobject]@{ Ready = $ready; Checks = $checks; Detected = $snapshot } | ConvertTo-Json -Depth 5
    } else {
        foreach ($check in $checks) {
            $status = if ($check.Available) { 'OK' } elseif ($check.Required) { 'MISSING' } else { 'OPTIONAL' }
            Write-Output ("[{0}] {1}" -f $status, $check.Name)
            if (-not $check.Available) { Write-Output ("  {0}" -f $check.Help) }
        }
        Write-Output ("Windows build prerequisites ready: {0}" -f $ready)
    }
    if ($Strict -and -not $ready) { exit 1 }
    $global:LASTEXITCODE = 0
} catch {
    Write-Error $_
    exit 1
}
