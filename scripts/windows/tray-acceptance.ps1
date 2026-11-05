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
 [StructLayout(LayoutKind.Sequential)] public struct RECT { public int Left,Top,Right,Bottom; }
 [StructLayout(LayoutKind.Sequential)] public struct POINT { public int X,Y; public POINT(int x,int y){X=x;Y=y;} }
 [StructLayout(LayoutKind.Sequential)] public struct MENUBARINFO { public uint cbSize; public RECT rcBar; public IntPtr hMenu,hwndMenu; public uint flags; }
 [StructLayout(LayoutKind.Sequential, CharSet=CharSet.Unicode)] public struct MENUITEMINFO { public uint cbSize,fMask,fType,fState,wID; public IntPtr hSubMenu,hbmpChecked,hbmpUnchecked,dwItemData; public IntPtr dwTypeData; public uint cch; public IntPtr hbmpItem; }
 public delegate bool EnumWindowsProc(IntPtr h,IntPtr p);
 [DllImport("user32.dll")] static extern bool EnumWindows(EnumWindowsProc cb,IntPtr p);
 [DllImport("user32.dll")] static extern bool IsWindowVisible(IntPtr h);
 [DllImport("user32.dll",CharSet=CharSet.Unicode)] static extern int GetClassName(IntPtr h,System.Text.StringBuilder b,int n);
 [DllImport("user32.dll")] static extern uint GetWindowThreadProcessId(IntPtr h,out uint p);
 [DllImport("user32.dll")] static extern bool GetMenuBarInfo(IntPtr h,int o,int i,ref MENUBARINFO b);
 [DllImport("user32.dll")] static extern int GetMenuItemCount(IntPtr m);
 [DllImport("user32.dll",CharSet=CharSet.Unicode)] static extern bool GetMenuItemInfoW(IntPtr m,uint i,bool pos,ref MENUITEMINFO info);
 [DllImport("user32.dll")] static extern bool GetMenuItemRect(IntPtr h,IntPtr m,uint i,out RECT r);
 [DllImport("user32.dll")] static extern IntPtr WindowFromPoint(POINT p);
 [DllImport("user32.dll")] static extern IntPtr GetAncestor(IntPtr h,uint f);
 [DllImport("user32.dll")] static extern IntPtr SetThreadDpiAwarenessContext(IntPtr c);
 // Documented popup handle and visible menu bounds:
 // https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-getmenubarinfo
 // https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-getmenuitemrect
 public static int[] ExitPoint(uint pid,string label) {
  IntPtr prior=SetThreadDpiAwarenessContext(new IntPtr(-4));
  if(prior==IntPtr.Zero) throw new Exception("Cannot establish physical menu coordinates.");
  try {
   var popups=new System.Collections.Generic.List<IntPtr>();
   EnumWindows((h,p)=>{uint owner; GetWindowThreadProcessId(h,out owner); var c=new System.Text.StringBuilder(128); GetClassName(h,c,c.Capacity); if(owner==pid && IsWindowVisible(h) && c.ToString()=="#32768") popups.Add(h); return true;},IntPtr.Zero);
   if(popups.Count==0) return null;
   if(popups.Count!=1) throw new Exception("Owned native popup ambiguous: count="+popups.Count);
   IntPtr popup=popups[0]; var bar=new MENUBARINFO(); bar.cbSize=(uint)Marshal.SizeOf(bar);
   if(!GetMenuBarInfo(popup,-4,0,ref bar) || bar.hMenu==IntPtr.Zero) throw new Exception("Owned popup menu handle unavailable.");
   int count=GetMenuItemCount(bar.hMenu), found=-1;
   if(count<1 || count>100) throw new Exception("Invalid popup menu item count.");
   for(int i=0;i<count;i++) {
    var item=new MENUITEMINFO(); item.cbSize=(uint)Marshal.SizeOf(item); item.fMask=0x40|1;
    if(!GetMenuItemInfoW(bar.hMenu,(uint)i,true,ref item) || item.cch>4096) throw new Exception("Cannot inspect owned menu item.");
    item.cch++; item.dwTypeData=Marshal.AllocHGlobal(checked((int)item.cch*2));
    try {
     if(!GetMenuItemInfoW(bar.hMenu,(uint)i,true,ref item)) throw new Exception("Cannot read owned menu label.");
     if(Marshal.PtrToStringUni(item.dwTypeData)==label && (item.fState & 3)==0) { if(found!=-1) throw new Exception("Ambiguous native Exit item."); found=i; }
    } finally { Marshal.FreeHGlobal(item.dwTypeData); }
   }
   if(found<0) throw new Exception("Enabled native Exit item missing.");
   RECT rect; if(!GetMenuItemRect(IntPtr.Zero,bar.hMenu,(uint)found,out rect) || rect.Right<=rect.Left || rect.Bottom<=rect.Top) throw new Exception("Invalid native menu bounds.");
   int x=(int)(((long)rect.Left+rect.Right)/2),y=(int)(((long)rect.Top+rect.Bottom)/2);
   IntPtr hit=WindowFromPoint(new POINT(x,y)); uint actual; GetWindowThreadProcessId(popup,out actual);
   if(actual!=pid || !IsWindowVisible(popup) || (hit!=popup && GetAncestor(hit,2)!=popup)) throw new Exception("Owned native menu moved or is covered.");
   return new[]{x,y};
  } finally { SetThreadDpiAwarenessContext(prior); }
 }
 [StructLayout(LayoutKind.Sequential)] public struct MOUSEINPUT {public int dx,dy;public uint mouseData,dwFlags,time;public UIntPtr extra;}
 [StructLayout(LayoutKind.Sequential)] public struct INPUT {public uint type;public MOUSEINPUT mi;}
 [DllImport("user32.dll",SetLastError=true)] static extern uint SendInput(uint n,INPUT[] inputs,int size);
 [DllImport("user32.dll")] static extern int GetSystemMetrics(int n);
 public static void Click(int x,int y,bool right) {
  IntPtr prior=SetThreadDpiAwarenessContext(new IntPtr(-4));
  if(prior==IntPtr.Zero)throw new Exception("No physical input context.");
  try {
   int left=GetSystemMetrics(76),top=GetSystemMetrics(77),width=GetSystemMetrics(78),height=GetSystemMetrics(79);
   if(width<2||height<2||x<left||y<top||x>=left+width||y>=top+height)throw new Exception("Pointer outside virtual desktop.");
   var inputs=new INPUT[3];
   inputs[0].mi.dx=(int)Math.Round((x-left)*65535.0/(width-1));inputs[0].mi.dy=(int)Math.Round((y-top)*65535.0/(height-1));inputs[0].mi.dwFlags=0x8000|0x4000|1;
   inputs[1].mi.dwFlags=right?8u:2u; inputs[2].mi.dwFlags=right?16u:4u;
   // SendInput inserts this movement/button sequence serially.
   // https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-sendinput
   uint sent=SendInput(3,inputs,Marshal.SizeOf(typeof(INPUT)));
   if(sent!=3) {
    int error=Marshal.GetLastWin32Error();
    if(sent>1) SendInput(1,new[]{inputs[2]},Marshal.SizeOf(typeof(INPUT)));
    throw new Exception("SendInput incomplete: "+sent+" error="+error);
   }
  } finally {SetThreadDpiAwarenessContext(prior);}
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
function Test-NotificationIcon($Element, $ShellRoot) {
 # Windows 11 exposes icons beneath the shared XAML input site, alongside
 # taskbar controls. Require the notification-item identity there.
 if ($Element.Current.AutomationId -eq 'NotifyItemIcon' -and
     $Element.Current.ClassName -eq 'SystemTray.NormalButton') { return $true }
 # Legacy Shell icons must live inside the notification area or overflow.
 if ($ShellRoot.Current.ClassName -in @('NotifyIconOverflowWindow','TopLevelWindowForOverflowXamlIsland')) { return $true }
 $ancestor = [Windows.Automation.TreeWalker]::ControlViewWalker.GetParent($Element)
 while ($ancestor -and -not [Windows.Automation.Automation]::Compare($ancestor,$ShellRoot)) {
  if ($ancestor.Current.ClassName -eq 'TrayNotifyWnd') { return $true }
  $ancestor = [Windows.Automation.TreeWalker]::ControlViewWalker.GetParent($ancestor)
 }
 return $false
}
function Click-Control($Element, [bool]$Right = $false) {
 # UIA coordinates are physical pixels; use the matching Win32 API.
 # Re-resolve at the point and require the intended element or its child.
 Assert-Owner
 try { $point = $Element.GetClickablePoint() } catch [System.Management.Automation.MethodInvocationException] {
  # The Windows 11 overflow provider may expose bounds but no clickable point.
  $rect = $Element.Current.BoundingRectangle
  if ($rect.IsEmpty -or $rect.Width -le 0 -or $rect.Height -le 0) { throw 'No usable Shell control bounds.' }
  foreach ($value in @($rect.X,$rect.Y,$rect.Width,$rect.Height)) {
   if ([double]::IsNaN($value) -or [double]::IsInfinity($value)) { throw 'Invalid Shell control bounds.' }
  }
  $point = [Windows.Point]::new($rect.X + $rect.Width / 2, $rect.Y + $rect.Height / 2)
 }
 $hit = [Windows.Automation.AutomationElement]::FromPoint($point)
 $matched = $false
 for ($depth = 0; $hit -and $depth -lt 8; $depth++) {
  if ([Windows.Automation.Automation]::Compare($hit,$Element)) { $matched = $true; break }
  $hit = [Windows.Automation.TreeWalker]::ControlViewWalker.GetParent($hit)
 }
 if (-not $matched -and $Element.Current.AutomationId -eq 'NotifyItemIcon' -and $Element.Current.ClassName -eq 'SystemTray.NormalButton') {
  # Windows 11 hit testing can stop at the overflow XAML host. Accept only
  # that exact host when it is an ancestor of this notification item.
  $hostAt = [Windows.Automation.AutomationElement]::FromPoint($point)
  if ($hostAt.Current.ClassName -eq 'TopLevelWindowForOverflowXamlIsland') {
   $parent = [Windows.Automation.TreeWalker]::ControlViewWalker.GetParent($Element)
   for ($depth = 0; $parent -and $depth -lt 12; $depth++) {
    if ([Windows.Automation.Automation]::Compare($parent,$hostAt)) { $matched = $true; break }
    $parent = [Windows.Automation.TreeWalker]::ControlViewWalker.GetParent($parent)
   }
  }
 }
 if (-not $matched) { throw 'Shell control moved or is covered; refusing pointer input.' }
 [EntwurfTrayInput]::Click([int]$point.X,[int]$point.Y,$Right)
}
function Find-Icon {
 $condition = [Windows.Automation.AndCondition]::new(
  [Windows.Automation.PropertyCondition]::new([Windows.Automation.AutomationElement]::NameProperty,'Entwurf'),
  [Windows.Automation.PropertyCondition]::new([Windows.Automation.AutomationElement]::ControlTypeProperty,[Windows.Automation.ControlType]::Button))
 $matches = @(foreach ($w in Find-ShellRoots) { foreach($e in $w.FindAll([Windows.Automation.TreeScope]::Descendants,$condition)) { if (-not $e.Current.IsOffscreen -and (Test-NotificationIcon $e $w)) { $e } } })
 if ($matches.Count -gt 1) { throw 'Multiple Entwurf notification icons; refusing ambiguous Shell action.' }
 if ($matches.Count -eq 1) { return $matches[0] }
}
function Invoke-Control($Element) {
 $pattern = $null
 if ($Element.TryGetCurrentPattern([Windows.Automation.InvokePattern]::Pattern,[ref]$pattern)) { $pattern.Invoke(); return }
 Click-Control $Element
}
$icon = Find-Icon
$overflowOpened = $false
if (-not $icon) {
 $expand = @(foreach ($w in Find-ShellRoots) {
  foreach($id in @('SystemTrayIcon','OverflowButton')) {
   $c = [Windows.Automation.PropertyCondition]::new([Windows.Automation.AutomationElement]::AutomationIdProperty,$id)
   foreach($e in $w.FindAll([Windows.Automation.TreeScope]::Descendants,$c)) {
    # SystemTrayIcon is shared with volume, clock and desktop controls.
    # Restrict it to the observed chevron semantic name, never emit names.
    $hiddenChinese = [string][char]0x663E + [char]0x793A + [char]0x9690 + [char]0x85CF + [char]0x7684 + [char]0x56FE + [char]0x6807
    $overflowName = $e.Current.Name -in @('Show hidden icons',$hiddenChinese)
    if (-not $e.Current.IsOffscreen -and $e.Current.IsEnabled -and
        $e.Current.ControlType -eq [Windows.Automation.ControlType]::Button -and
        (($id -eq 'SystemTrayIcon' -and $e.Current.ClassName -eq 'SystemTray.NormalButton' -and $overflowName) -or
         ($id -eq 'OverflowButton' -and $overflowName))) { $e }
   }
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
# UIA Invoke on a notification icon need not deliver a left-button event.
if ($Action -eq 'restore') { Click-Control $icon }
if ($Action -eq 'exit') {
 $icon = Find-Icon
 if (-not $icon) { throw 'Entwurf notification icon disappeared before context click.' }
 Click-Control $icon $true
 # Read the actual displayed Win32 menu; its UIA provider may expose no items.
 $deadline = [DateTime]::UtcNow.AddSeconds(5)
 do {
  Start-Sleep -Milliseconds 100
  Assert-Owner
  $point = [EntwurfTrayInput]::ExitPoint([uint32]$AppProcessId,$ExitLabel)
 } while ($null -eq $point -and [DateTime]::UtcNow -lt $deadline)
 if ($null -eq $point) { throw 'Owned native popup did not appear.' }
 Assert-Owner
 # Re-read the label, position and owning window immediately before input.
 $point = [EntwurfTrayInput]::ExitPoint([uint32]$AppProcessId,$ExitLabel)
 if ($null -eq $point) { throw 'Owned native popup disappeared before input.' }
 [EntwurfTrayInput]::Click($point[0],$point[1],$false)

}
[pscustomobject]@{ action=$Action; icon='Entwurf'; overflow_opened=$overflowOpened; real_shell=$true } | ConvertTo-Json -Compress
