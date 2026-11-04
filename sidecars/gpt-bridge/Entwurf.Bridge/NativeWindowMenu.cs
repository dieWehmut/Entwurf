using System.Diagnostics;

namespace Entwurf.Bridge;

internal sealed record NativeMenuNode(NativeImageNode Node, bool CanExpand = false, bool CanInvoke = false, string AutomationId = "", string AriaRole = "");

internal static class NativeWindowMenu
{
    internal static void CreateWindow(Func<IReadOnlyList<NativeMenuNode>> capture, Action<string> expand,
        Action<string> invoke, Action<string> collapse, CancellationToken cancellation, Action<CancellationToken>? wait = null)
    {
        cancellation.ThrowIfCancellationRequested();
        var initial = OwnedNodes(capture());
        var map = initial.ToDictionary(x => x.Node.Id, x => x.Node, StringComparer.Ordinal);
        var bars = initial.Where(x => x.Node.Role == "MenuBar" && x.AriaRole == "menubar" && x.Node.Visible && NearDocument(x.Node, map)
            && new[] { "file", "edit", "view", "help" }.All(menu => initial.Count(child => child.Node.ParentId == x.Node.Id
                && child.Node.Role == "MenuItem" && child.AutomationId == $"application-menu-trigger-{menu}-menu") == 1)).ToArray();
        if (bars.Length != 1) throw Unavailable();
        var files = initial.Where(x => x.Node.ParentId == bars[0].Node.Id && x.Node.Role == "MenuItem" && x.Node.Visible && x.Node.Enabled
            && x.AutomationId == "application-menu-trigger-file-menu" && x.Node.Name is "文件" or "File").ToArray();
        if (files.Length != 1 || !files[0].CanExpand) throw Unavailable();
        var file = files[0].Node.Id;
        cancellation.ThrowIfCancellationRequested();
        try
        {
            expand(file);
            // The renderer clears its menu snapshot before refreshing it asynchronously on open.
            var deadline = Stopwatch.StartNew();
            for (var attempt = 0; attempt < 30 && deadline.Elapsed < TimeSpan.FromSeconds(3); attempt++)
            {
                cancellation.ThrowIfCancellationRequested();
                var expanded = OwnedNodes(capture());
                var expandedMap = expanded.ToDictionary(x => x.Node.Id, x => x.Node, StringComparer.Ordinal);
                var menus = expanded.Where(x => x.Node.Role == "Menu" && x.Node.Visible && x.Node.Enabled
                    && x.AutomationId == "application-menu-content" && x.Node.Name == files[0].Node.Name).ToArray();
                if (menus.Length > 1) throw Unavailable();
                if (menus.Length == 1)
                {
                    var commands = expanded.Where(x => x.Node.Role == "MenuItem" && x.Node.Visible && x.Node.Enabled
                        && x.Node.Name is "新建窗口" or "New Window"
                        && NativeImageEvidence.IsWithin(x.Node.Id, menus[0].Node.Id, expandedMap)).ToArray();
                    if (commands.Length > 1 || commands.Length == 1 && !commands[0].CanInvoke) throw Unavailable();
                    if (commands.Length == 1)
                    {
                        cancellation.ThrowIfCancellationRequested();
                        invoke(commands[0].Node.Id);
                        return;
                    }
                }
                if (wait is not null) wait(cancellation);
                else if (cancellation.WaitHandle.WaitOne(100)) cancellation.ThrowIfCancellationRequested();
            }
            throw Unavailable();
        }
        finally { collapse(file); }
    }

    static IReadOnlyList<NativeMenuNode> OwnedNodes(IReadOnlyList<NativeMenuNode> nodes)
    {
        var frame = NativeImageFrame.Capture(nodes.Select(x => x.Node).ToArray());
        var owned = frame.Nodes.Select(x => x.Id).ToHashSet(StringComparer.Ordinal);
        return nodes.Where(x => owned.Contains(x.Node.Id)).ToArray();
    }
    static bool NearDocument(NativeImageNode bar, IReadOnlyDictionary<string, NativeImageNode> map)
    {
        // Observed application chrome: outer Document -> Group -> Group -> ARIA menubar.
        return bar.ParentId is { } parent && map.TryGetValue(parent, out var shell) && shell.Role == "Group"
            && shell.ParentId is { } shellParent && map.TryGetValue(shellParent, out var root) && root.Role == "Group"
            && root.ParentId is { } rootParent && map.TryGetValue(rootParent, out var document) && document.Role == "Document";
    }
    static ImageAutomationException Unavailable() => new("independent_window_unavailable");
}
