[CmdletBinding(DefaultParameterSetName = 'Select')]
param(
    [Parameter(Mandatory = $true)][ValidateRange(1, 2147483647)][int]$AppProcessId,
    [Parameter(Mandatory = $true, ParameterSetName = 'Select')][string]$SelectedPath,
    [Parameter(ParameterSetName = 'Select')][ValidateSet('file', 'directory')][string]$Kind = 'file',
    [Parameter(Mandatory = $true, ParameterSetName = 'Cancel')][switch]$Cancel
)
# Acceptance-only UI Automation of the file dialog belonging to our app.
# No clipboard, coordinates, other application windows, or production test hooks.
$ErrorActionPreference = 'Stop'
[Console]::OutputEncoding = [Text.UTF8Encoding]::new()
if ($PSCmdlet.ParameterSetName -eq 'Cancel' -and -not $Cancel) { throw 'Cancel mode requires -Cancel.' }
if (-not $Cancel) {
    # Selection is a literal absolute local path. File mode also permits a new
    # filename for Save As, but its containing directory must already exist.
    if ($SelectedPath -notmatch '^[A-Za-z]:[\\/]' -or $SelectedPath -match '[\x00-\x1F"<>|*?]') {
        throw 'Native dialog selection requires an absolute local path without control characters or wildcards.'
    }
    $SelectedPath = [IO.Path]::GetFullPath($SelectedPath)
    if ($Kind -eq 'directory') {
        if (-not (Test-Path -LiteralPath $SelectedPath -PathType Container)) { throw 'Selected directory does not exist.' }
    } elseif (-not (Test-Path -LiteralPath ([IO.Path]::GetDirectoryName($SelectedPath)) -PathType Container)) {
        throw 'Selected file parent directory does not exist.'
    }
}
Add-Type -AssemblyName UIAutomationClient
Add-Type -AssemblyName UIAutomationTypes
Add-Type -TypeDefinition @'
using System;
using System.Text;
using System.Runtime.InteropServices;
public static class EntwurfDialogControls {
    delegate bool EnumProc(IntPtr handle, IntPtr parameter);
    [DllImport("user32.dll")] static extern bool EnumChildWindows(IntPtr parent, EnumProc callback, IntPtr parameter);
    [DllImport("user32.dll")] static extern int GetDlgCtrlID(IntPtr handle);
    [DllImport("user32.dll", CharSet=CharSet.Unicode)] static extern int GetClassName(IntPtr handle, StringBuilder name, int count);
    [DllImport("user32.dll")] public static extern IntPtr GetDlgItem(IntPtr dialog, int id);
    [DllImport("user32.dll")] public static extern bool IsWindowEnabled(IntPtr handle);
    [DllImport("user32.dll", CharSet=CharSet.Unicode, EntryPoint="SendMessageTimeoutW")]
    static extern IntPtr SetText(IntPtr handle, uint message, UIntPtr w, string text, uint flags, uint timeout, out UIntPtr result);
    [DllImport("user32.dll", CharSet=CharSet.Unicode, EntryPoint="SendMessageTimeoutW")]
    static extern IntPtr Click(IntPtr handle, uint message, UIntPtr w, IntPtr l, uint flags, uint timeout, out UIntPtr result);
    public static IntPtr FindFilenameEdit(IntPtr dialog) {
        var found = GetDlgItem(dialog, 1152);
        if (found != IntPtr.Zero) return found;
        EnumChildWindows(dialog, (handle, parameter) => {
            var id = GetDlgCtrlID(handle);
            var name = new StringBuilder(64);
            GetClassName(handle, name, 64);
            if ((id == 1001 || id == 1148) && name.ToString() == "Edit") { found = handle; return false; }
            return true;
        }, IntPtr.Zero);
        return found;
    }
    public static bool Select(IntPtr edit, IntPtr button, string path) {
        UIntPtr result;
        if (SetText(edit, 0x000C, UIntPtr.Zero, path, 2, 2000, out result) == IntPtr.Zero || result == UIntPtr.Zero) return false;
        return Click(button, 0x00F5, UIntPtr.Zero, IntPtr.Zero, 2, 2000, out result) != IntPtr.Zero;
    }
    public static bool Cancel(IntPtr button) {
        UIntPtr result;
        return Click(button, 0x00F5, UIntPtr.Zero, IntPtr.Zero, 2, 2000, out result) != IntPtr.Zero;
    }
}
'@
$condition = [System.Windows.Automation.PropertyCondition]::new(
    [System.Windows.Automation.AutomationElement]::ProcessIdProperty, $AppProcessId)
$editCondition = [System.Windows.Automation.PropertyCondition]::new(
    [System.Windows.Automation.AutomationElement]::AutomationIdProperty, '1148')
$acceptCondition = [System.Windows.Automation.PropertyCondition]::new(
    [System.Windows.Automation.AutomationElement]::AutomationIdProperty, '1')
$deadline = [DateTime]::UtcNow.AddSeconds(30)
while ([DateTime]::UtcNow -lt $deadline) {
    if (-not (Get-Process -Id $AppProcessId -ErrorAction SilentlyContinue)) { throw 'Entwurf exited before its file dialog opened.' }
    $windows = [System.Windows.Automation.AutomationElement]::RootElement.FindAll(
        [System.Windows.Automation.TreeScope]::Children, $condition)
    foreach ($window in $windows) {
        if ($window.Current.ClassName -ne '#32770') { continue }
        # Windows 11's native folder picker exposes filename edit 1152 through
        # its HWND even when those legacy controls are absent from the UIA tree.
        # Resolve semantic control IDs inside our process's dialog, never points.
        $handle = [IntPtr]$window.Current.NativeWindowHandle
        $nativeEdit = [EntwurfDialogControls]::FindFilenameEdit($handle)
        if ($Cancel) {
            $nativeCancel = [EntwurfDialogControls]::GetDlgItem($handle, 2)
            # Require filename controls too, so another app-owned message box
            # cannot be mistaken for the file dialog we intend to cancel.
            if ($nativeEdit -ne [IntPtr]::Zero -and $nativeCancel -ne [IntPtr]::Zero -and [EntwurfDialogControls]::IsWindowEnabled($nativeCancel)) {
                if (-not [EntwurfDialogControls]::Cancel($nativeCancel)) { throw 'Native dialog cancellation failed or timed out.' }
                Write-Output 'Cancelled the native Entwurf file dialog.'
                exit 0
            }
            continue
        }
        $nativeAccept = [EntwurfDialogControls]::GetDlgItem($handle, 1)
        if ($nativeEdit -ne [IntPtr]::Zero -and $nativeAccept -ne [IntPtr]::Zero -and [EntwurfDialogControls]::IsWindowEnabled($nativeAccept)) {
            if (-not [EntwurfDialogControls]::Select($nativeEdit, $nativeAccept, $SelectedPath)) { throw 'Native dialog selection failed or timed out.' }
            Write-Output "Selected $Kind through the native Entwurf dialog."
            exit 0
        }
        $edit = $window.FindFirst([System.Windows.Automation.TreeScope]::Descendants, $editCondition)
        $accept = $window.FindFirst([System.Windows.Automation.TreeScope]::Descendants, $acceptCondition)
        if ($edit -and $accept -and $edit.Current.IsEnabled -and $accept.Current.IsEnabled) {
            $value = $edit.GetCurrentPattern([System.Windows.Automation.ValuePattern]::Pattern)
            $value.SetValue($SelectedPath)
            $invoke = $accept.GetCurrentPattern([System.Windows.Automation.InvokePattern]::Pattern)
            $invoke.Invoke()
            Write-Output "Selected $Kind through the native Entwurf dialog."
            exit 0
        }
    }
    Start-Sleep -Milliseconds 150
}
throw 'The Entwurf file dialog did not expose the expected filename and requested action controls.'
