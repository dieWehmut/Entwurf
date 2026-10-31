using System.Diagnostics;
using System.Runtime.InteropServices;
using System.Text.RegularExpressions;
using System.Text;
using FlaUI.Core.Definitions;
using FlaUI.UIA3;

namespace Entwurf.Bridge;

/// <summary>Read-only discovery. No unvalidated ChatGPT selector or coordinate automation is shipped.</summary>
public sealed class WindowsAutomation : IWindowsAutomation
{
    nint target;
    int processId;
    string? version;
    public CapabilityReport Detect()
    {
        var windows = new List<(nint Handle, int Id, string? Version)>();
        foreach (var process in Process.GetProcessesByName("ChatGPT"))
        {
            using (process)
            {
                try
                {
                    if (process.MainWindowHandle != 0 && IsWindowVisible(process.MainWindowHandle))
                    {
                        uint length = 0;
                        if (GetPackageFullName(process.Handle, ref length, null) != 122 || length > 1024) continue;
                        var package = new StringBuilder((int)length);
                        if (GetPackageFullName(process.Handle, ref length, package) != 0) continue;
                        var appVersion = PackageVersion(package.ToString());
                        if (appVersion is null) continue;
                        windows.Add((process.MainWindowHandle, process.Id, appVersion));
                    }
                }
                catch (Exception e) when (e is System.ComponentModel.Win32Exception or InvalidOperationException) { }
            }
        }
        int controls = 0, documents = 0, writable = 0, invokable = 0;
        string detail;
        target = 0;
        if (windows.Count == 1)
        {
            target = windows[0].Handle;
            processId = windows[0].Id;
            version = windows[0].Version;
            try
            {
                using var uia = new UIA3Automation { ConnectionTimeout = TimeSpan.FromSeconds(2), TransactionTimeout = TimeSpan.FromSeconds(2) };
                var root = uia.FromHandle(target);
                var walker = uia.TreeWalkerFactory.GetControlViewWalker();
                var queue = new Queue<FlaUI.Core.AutomationElements.AutomationElement>();
                queue.Enqueue(root);
                while (queue.Count > 0 && controls < 1024)
                {
                    var parent = queue.Dequeue();
                    var child = walker.GetFirstChild(parent);
                    while (child is not null && controls < 1024)
                    {
                        controls++;
                        if (child.ControlType == ControlType.Document) documents++;
                        if (child.ControlType == ControlType.Edit && child.Patterns.Value.IsSupported) writable++;
                        if (child.ControlType == ControlType.Button && child.Patterns.Invoke.IsSupported) invokable++;
                        queue.Enqueue(child);
                        child = walker.GetNextSibling(child);
                    }
                }
                detail = $"Read-only UIA: {documents} document(s), {writable} Value-pattern edit(s), {invokable} Invoke-pattern button(s). No input, submit, output text, or account data was read. Other gates remain unverified.";
            }
            catch (Exception e) when (e is COMException or InvalidOperationException or TimeoutException)
            { detail = "UIA discovery failed or timed out; use manual handoff."; controls = 0; }
        }
        else detail = windows.Count == 0 ? "No visible ChatGPT window found; open ChatGPT manually." : "Multiple ChatGPT windows are visible; target selection is ambiguous.";
        var discovered = windows.Count == 1 && controls > 0;
        return new(windows.Count > 0, documents > 0, target != 0 && GetForegroundWindow() == target, null,
            windows.Count == 1 ? version : null, windows.Count, controls, detail,
            new()
            {
                ["discover_window"] = new(discovered, discovered ? "One visible ChatGPT process window exposes UIA controls in this session." : "Window discovery is unavailable or ambiguous."),
                ["reliable_input"] = new(false, "No authorized foreground input acceptance run has been performed for this app version."),
                ["reliable_completion"] = new(false, "Completion controls, stable ResultEnvelope JSON, and artifact confirmation are unverified."),
                ["reliable_capture"] = new(false, "Task-bound accessible result and image recovery are unverified.")
            });
    }

    public static string? PackageVersion(string fullName)
    {
        // Codex currently also uses ChatGPT.exe. Only the ChatGPT Store package identities qualify.
        var match = Regex.Match(fullName, @"^OpenAI\.ChatGPT(?:-Desktop)?_([0-9]+\.[0-9]+\.[0-9]+\.[0-9]+)_(?:x64|arm64|x86|neutral)__2p2nqsd0c76g0$", RegexOptions.CultureInvariant);
        return match.Success ? match.Groups[1].Value : null;
    }

    [DllImport("kernel32.dll", CharSet = CharSet.Unicode)]
    static extern int GetPackageFullName(nint process, ref uint length, StringBuilder? fullName);

    public SafetyState CheckSafety()
    {
        bool unchanged = target != 0 && IsWindow(target);
        if (unchanged)
        {
            GetWindowThreadProcessId(target, out var currentId);
            unchanged = currentId == processId;
        }
        return new(target != 0 && GetForegroundWindow() == target, IsInputDesktopUnlocked(), unchanged);
    }
    static bool IsInputDesktopUnlocked()
    {
        var desktop = OpenInputDesktop(0, false, 0x0001); // DESKTOP_READOBJECTS; never switches desktops.
        if (desktop == 0) return false;
        try
        {
            var name = new StringBuilder(256);
            return GetUserObjectInformation(desktop, 2, name, name.Capacity * 2, out _) && name.ToString() == "Default";
        }
        finally { CloseDesktop(desktop); }
    }
    static BridgeException Unsupported() => new("automation_unavailable", "Target-version input, completion, and capture gates are unverified; use manual handoff.");
    public Observation Observe() => new(false, "manual_required", "No validated completion adapter is installed.");
    public string CaptureText() => throw Unsupported();
    public void Paste(string text) => throw Unsupported();
    public void Submit() => throw Unsupported();
    public void OpenChatGpt() => throw Unsupported();
    [DllImport("user32.dll")] static extern nint GetForegroundWindow();
    [DllImport("user32.dll")] [return: MarshalAs(UnmanagedType.Bool)] static extern bool IsWindow(nint window);
    [DllImport("user32.dll")] [return: MarshalAs(UnmanagedType.Bool)] static extern bool IsWindowVisible(nint window);
    [DllImport("user32.dll")] static extern uint GetWindowThreadProcessId(nint window, out int process);
    [DllImport("user32.dll", SetLastError = true)] static extern nint OpenInputDesktop(uint flags, bool inherit, uint access);
    [DllImport("user32.dll", CharSet = CharSet.Unicode, SetLastError = true)] [return: MarshalAs(UnmanagedType.Bool)] static extern bool GetUserObjectInformation(nint handle, int index, StringBuilder information, int length, out int needed);
    [DllImport("user32.dll")] [return: MarshalAs(UnmanagedType.Bool)] static extern bool CloseDesktop(nint desktop);
}
