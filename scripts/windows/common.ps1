Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
# Include newly installed user tools when this session inherited an older PATH.
$env:PATH += ';' + [Environment]::GetEnvironmentVariable('PATH', 'User')
$env:DOTNET_CLI_TELEMETRY_OPTOUT = '1'

function Invoke-CheckedNative {
    param([Parameter(Mandatory = $true)][string]$Command, [string[]]$Arguments = @())
    if ($Command -eq 'pnpm') { $Command = 'pnpm.cmd' }
    $oldPreference = $ErrorActionPreference
    try {
        # Resolve failures must terminate even when the caller uses Continue.
        $native = Get-Command $Command -CommandType Application -ErrorAction Stop
        # Windows PowerShell 5 turns redirected native stderr into ErrorRecords.
        # Build tools write ordinary progress there; only their exit code fails a build.
        $ErrorActionPreference = 'Continue'
        # A local LASTEXITCODE would shadow the global value native processes update.
        $global:LASTEXITCODE = $null
        & $native.Source @Arguments
        $exitCode = $global:LASTEXITCODE
        if ($null -eq $exitCode) { throw "$Command could not be started; no native exit code was returned" }
        if ($exitCode -ne 0) { throw "$Command failed with exit code $exitCode" }
    } finally {
        $ErrorActionPreference = $oldPreference
    }
}

function Get-ToolOutput {
    param([string]$Name, [string[]]$Arguments)
    if ($Name -eq 'pnpm') { $Name = 'pnpm.cmd' }
    $command = Get-Command $Name -ErrorAction SilentlyContinue
    if (-not $command) { return '' }
    $changedLocation = $Name -eq 'pnpm.cmd'
    if ($changedLocation) { Push-Location $env:TEMP }
    try {
        # Some tools (Tesseract) write their version to stderr even on success.
        $oldPreference = $ErrorActionPreference
        $ErrorActionPreference = 'Continue'
        $output = & $command.Source @Arguments 2>&1
        $code = $LASTEXITCODE
        $ErrorActionPreference = $oldPreference
        if ($code -eq 0) { return ($output | Out-String).Trim() }
    } catch { return '' } finally {
        $ErrorActionPreference = 'Stop'
        if ($changedLocation) { Pop-Location }
    }
    return ''
}

function Get-RequirementResults {
    param([hashtable]$Snapshot)
    $node = [version]'0.0'
    if ($Snapshot.NodeVersion -match 'v?(\d+\.\d+\.\d+)') { $node = [version]$Matches[1] }
    $pnpm = [version]'0.0'
    if ($Snapshot.PnpmVersion -match '^10\.\d+\.\d+$') { $pnpm = [version]$Snapshot.PnpmVersion }
    $rust = [version]'0.0'
    if ($Snapshot.RustVersion -match '^\d+\.\d+\.\d+$') { $rust = [version]$Snapshot.RustVersion }
    $webview = [version]'0.0'
    if ($Snapshot.WebViewVersion -match '^\d+(\.\d+)+$') { $webview = [version]$Snapshot.WebViewVersion }
    $specs = @(
        ,@('Windows x64', ($Snapshot.WindowsVersion.Major -eq 10 -and $Snapshot.WindowsVersion.Build -ge 10240 -and $Snapshot.Architecture -eq 'AMD64'), $true, 'Windows 10/11 x64; ARM64 and WSL builds are not supported.')
        ,@('Node.js', ($node -ge [version]'22.12.0'), $true, 'Install Node.js 22.12+ (LTS recommended) and reopen the terminal.')
        ,@('pnpm', ($pnpm.Major -eq 10), $true, 'Install supported pnpm 10: npm install --global pnpm@10.34.9')
        ,@('Rust MSVC', ($rust -ge [version]'1.92.0' -and $Snapshot.RustHost -eq 'x86_64-pc-windows-msvc' -and $Snapshot.RustTargets -contains 'x86_64-pc-windows-msvc' -and -not [string]::IsNullOrWhiteSpace($Snapshot.CargoVersion)), $true, 'Install Rust 1.92+ for the pinned Typst renderer; rustup update stable; rustup default stable-x86_64-pc-windows-msvc; rustup target add x86_64-pc-windows-msvc')
        ,@('MSVC C++ tools', [bool]$Snapshot.MsvcAvailable, $true, 'Install Visual Studio 2022 Build Tools with Desktop development with C++.')
        ,@('Windows SDK', [bool]$Snapshot.WindowsSdkAvailable, $true, 'Install a Windows 10/11 SDK with headers and x64 libraries through Visual Studio Installer.')
        ,@('.NET 10 SDK', (@($Snapshot.DotnetSdks | Where-Object { $_ -match '^10\.' }).Count -gt 0), $true, 'Install the x64 .NET 10 SDK; a runtime alone is insufficient.')
        ,@('WebView2 runtime', ($webview -gt [version]'0.0'), $true, 'Install Microsoft Edge WebView2 Evergreen Runtime for x64.')
        ,@('Tesseract', (-not [string]::IsNullOrWhiteSpace($Snapshot.TesseractVersion) -and $Snapshot.OcrLanguages -contains 'eng' -and $Snapshot.OcrLanguages -contains 'chi_sim'), $false, 'Optional: install Tesseract with eng and chi_sim traineddata, then add it to PATH.')
    )
    foreach ($spec in $specs) {
        [pscustomobject]@{ Name = $spec[0]; Available = [bool]$spec[1]; Required = [bool]$spec[2]; Help = $spec[3] }
    }
}

function Get-PrerequisiteSnapshot {
    $architecture = $env:PROCESSOR_ARCHITECTURE
    if ($env:PROCESSOR_ARCHITEW6432) { $architecture = $env:PROCESSOR_ARCHITEW6432 }
    $rustInfo = Get-ToolOutput 'rustc' @('-vV')
    $rustHost = ''
    if ($rustInfo -match '(?m)^host:\s*(\S+)') { $rustHost = $Matches[1] }
    $rustVersion = ''
    if ($rustInfo -match '(?m)^release:\s*(\S+)') { $rustVersion = $Matches[1] }
    $rustTargets = @( (Get-ToolOutput 'rustup' @('target', 'list', '--installed')) -split '\r?\n' )
    $vswhere = Join-Path ${env:ProgramFiles(x86)} 'Microsoft Visual Studio\Installer\vswhere.exe'
    $msvc = $false
    if (Test-Path $vswhere) {
        $installations = @(& $vswhere -products '*' -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath)
        foreach ($installation in $installations) {
            $toolsRoot = Join-Path $installation 'VC\Tools\MSVC'
            foreach ($version in @(Get-ChildItem $toolsRoot -Directory -ErrorAction SilentlyContinue)) {
                $compiler = Join-Path $version.FullName 'bin\Hostx64\x64\cl.exe'
                $linker = Join-Path $version.FullName 'bin\Hostx64\x64\link.exe'
                if ((Test-Path $compiler) -and (Test-Path $linker)) { $msvc = $true }
            }
        }
    }
    $sdk = $false
    $kits = Join-Path ${env:ProgramFiles(x86)} 'Windows Kits\10'
    foreach ($version in @(Get-ChildItem (Join-Path $kits 'Include') -Directory -ErrorAction SilentlyContinue)) {
        $header = Join-Path $version.FullName 'um\Windows.h'
        $lib = Join-Path $kits ("Lib\{0}\um\x64\kernel32.lib" -f $version.Name)
        $ucrt = Join-Path $kits ("Lib\{0}\ucrt\x64\ucrt.lib" -f $version.Name)
        if ((Test-Path $header) -and (Test-Path $lib) -and (Test-Path $ucrt)) { $sdk = $true }
    }
    $webview = ''
    foreach ($key in @('HKCU:\SOFTWARE\Microsoft\EdgeUpdate\Clients', 'HKLM:\SOFTWARE\Microsoft\EdgeUpdate\Clients', 'HKLM:\SOFTWARE\WOW6432Node\Microsoft\EdgeUpdate\Clients')) {
        foreach ($client in @(Get-ChildItem $key -ErrorAction SilentlyContinue)) {
            $values = Get-ItemProperty $client.PSPath -ErrorAction SilentlyContinue
            if ($values -and $values.PSObject.Properties['name'] -and $values.name -match 'WebView2' -and $values.PSObject.Properties['pv']) { $webview = [string]$values.pv }
        }
    }
    return @{
        WindowsVersion = [Environment]::OSVersion.Version; Architecture = $architecture
        NodeVersion = Get-ToolOutput 'node' @('--version'); PnpmVersion = Get-ToolOutput 'pnpm' @('--version')
        CargoVersion = Get-ToolOutput 'cargo' @('--version'); RustVersion = $rustVersion; RustHost = $rustHost; RustTargets = $rustTargets
        DotnetSdks = @((Get-ToolOutput 'dotnet' @('--list-sdks')) -split '\r?\n')
        MsvcAvailable = $msvc; WindowsSdkAvailable = $sdk; WebViewVersion = $webview
        TesseractVersion = Get-ToolOutput 'tesseract' @('--version')
        OcrLanguages = @((Get-ToolOutput 'tesseract' @('--list-langs')) -split '\r?\n')
    }
}
