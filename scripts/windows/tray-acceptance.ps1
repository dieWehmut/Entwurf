[CmdletBinding()]
param(
    [Parameter(Mandatory)][ValidateRange(1,2147483647)][int]$AppProcessId,
    [Parameter(Mandatory)][string]$ExecutablePath,
    [Parameter(Mandatory)][string]$Created,
    [ValidateSet('inspect','restore','exit')][string]$Action = 'inspect',
    [string]$ExitLabel = ([string][char]0x9000 + [char]0x51FA)
)
# Acceptance only: actual Shell UI; never inject a Tauri tray callback.
$ErrorActionPreference = 'Stop'
[Console]::OutputEncoding = [Text.UTF8Encoding]::new()
Add-Type -AssemblyName UIAutomationClient
Add-Type -AssemblyName UIAutomationTypes
Add-Type -TypeDefinition @'
using System; using System.Runtime.InteropServices;
public static class EntwurfTrayInput {
 [DllImport("user32.dll")] public static extern IntPtr OpenInputDesktop(uint f,bool i,uint a);
 [DllImport("user32.dll")] public static extern bool CloseDesktop(IntPtr d);
 [DllImport("user32.dll")] public static extern bool SetCursorPos(int x,int y);
 [DllImport("user32.dll")] public static extern void mouse_event(uint f,uint x,uint y,uint data,UIntPtr extra);
 public static void Click(int x,int y,bool right) {
  if (!SetCursorPos(x,y)) throw new Exception("Could not position pointer on Entwurf control.");
  mouse_event(right?8u:2u,0,0,0,UIntPtr.Zero); mouse_event(right?16u:4u,0,0,0,UIntPtr.Zero);
 }
}
'@
function Assert-Owner {
 $p = Get-CimInstance Win32_Process -Filter "ProcessId=$AppProcessId"
 if (-not $p -or $p.ExecutablePath -ine $ExecutablePath -or $p.CreationDate.ToUniversalTime().ToString('o') -ne $Created) { throw 'Entwurf process identity changed.' }
 if ($p.SessionId -ne [Diagnostics.Process]::GetCurrentProcess().SessionId) { throw 'Entwurf is in another Windows session.' }
}
Assert-Owner
$d = [EntwurfTrayInput]::OpenInputDesktop(0,$false,1)
if ($d -eq [IntPtr]::Zero) { throw 'Real Shell acceptance requires an accessible unlocked input desktop.' }
[EntwurfTrayInput]::CloseDesktop($d) | Out-Null
$root = [Windows.Automation.AutomationElement]::RootElement
function Find-ShellRoots {
 $all = $root.FindAll([Windows.Automation.TreeScope]::Children,[Windows.Automation.Condition]::TrueCondition)
 foreach ($w in $all) {
  if ($w.Current.ClassName -notin @('Shell_TrayWnd','Shell_SecondaryTrayWnd','NotifyIconOverflowWindow','TopLevelWindowForOverflowXamlIsland')) { continue }
  $p = Get-Process -Id $w.Current.ProcessId -ErrorAction Stop
  if ($p.ProcessName -eq 'explorer' -and $p.SessionId -eq [Diagnostics.Process]::GetCurrentProcess().SessionId) { $w }
 }
}
function Find-Icon {
 $condition = [Windows.Automation.AndCondition]::new(
  [Windows.Automation.PropertyCondition]::new([Windows.Automation.AutomationElement]::NameProperty,'Entwurf'),
  [Windows.Automation.PropertyCondition]::new([Windows.Automation.AutomationElement]::ControlTypeProperty,[Windows.Automation.ControlType]::Button))
 $matches = @(foreach ($w in Find-ShellRoots) { foreach($e in $w.FindAll([Windows.Automation.TreeScope]::Descendants,$condition)) { if (-not $e.Current.IsOffscreen) { $e } } })
 if ($matches.Count -gt 1) { throw 'Multiple Entwurf notification icons; refusing ambiguous Shell action.' }
 if ($matches.Count -eq 1) { return $matches[0] }
}
function Invoke-Control($Element) {
 $pattern = $null
 if ($Element.TryGetCurrentPattern([Windows.Automation.InvokePattern]::Pattern,[ref]$pattern)) { $pattern.Invoke(); return }
 $point = $Element.GetClickablePoint()
 [EntwurfTrayInput]::Click([int]$point.X,[int]$point.Y,$false)
}
$icon = Find-Icon
$overflowOpened = $false
if (-not $icon) {
 $expand = @(foreach ($w in Find-ShellRoots) {
  foreach($id in @('SystemTrayIcon','OverflowButton')) {
   $c = [Windows.Automation.PropertyCondition]::new([Windows.Automation.AutomationElement]::AutomationIdProperty,$id)
   foreach($e in $w.FindAll([Windows.Automation.TreeScope]::Descendants,$c)) { if (-not $e.Current.IsOffscreen -and $e.Current.IsEnabled) { $e } }
  }
 })
 if ($expand.Count -ne 1) { throw 'Shell overflow button unavailable or ambiguous; real tray coverage pending.' }
 Assert-Owner
 Invoke-Control $expand[0]
 $overflowOpened = $true
 $deadline = [DateTime]::UtcNow.AddSeconds(5)
 do { Start-Sleep -Milliseconds 100; $icon = Find-Icon } while (-not $icon -and [DateTime]::UtcNow -lt $deadline)
}
if (-not $icon) { throw 'Entwurf notification icon not found in Shell or overflow.' }
Assert-Owner
if ($Action -eq 'restore') { Invoke-Control $icon }
if ($Action -eq 'exit') {
 $point = $icon.GetClickablePoint()
 [EntwurfTrayInput]::Click([int]$point.X,[int]$point.Y,$true)
 $deadline = [DateTime]::UtcNow.AddSeconds(5)
 $items = @()
 do {
  Start-Sleep -Milliseconds 100
  $owned = [Windows.Automation.PropertyCondition]::new([Windows.Automation.AutomationElement]::ProcessIdProperty,$AppProcessId)
  $condition = [Windows.Automation.AndCondition]::new(
   [Windows.Automation.PropertyCondition]::new([Windows.Automation.AutomationElement]::NameProperty,$ExitLabel),
   [Windows.Automation.PropertyCondition]::new([Windows.Automation.AutomationElement]::ControlTypeProperty,[Windows.Automation.ControlType]::MenuItem))
  $items = @(foreach($w in $root.FindAll([Windows.Automation.TreeScope]::Children,$owned)) {
   foreach($e in $w.FindAll([Windows.Automation.TreeScope]::Descendants,$condition)) { if(-not $e.Current.IsOffscreen -and $e.Current.IsEnabled) { $e } }
  })
 } while ($items.Count -eq 0 -and [DateTime]::UtcNow -lt $deadline)
 if ($items.Count -ne 1) { throw 'Owned Entwurf Exit menu item unavailable or ambiguous.' }
 Assert-Owner
 Invoke-Control $items[0]
}
[pscustomobject]@{ action=$Action; icon='Entwurf'; overflow_opened=$overflowOpened; real_shell=$true } | ConvertTo-Json -Compress
