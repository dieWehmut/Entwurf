using Entwurf.Bridge;
using Xunit;

namespace Entwurf.Bridge.Tests;

public sealed class NativeWindowMenuTests
{
    [Theory]
    [InlineData("文件", "新建窗口")]
    [InlineData("File", "New Window")]
    public void Native_file_menu_expansion_precedes_one_scoped_new_window_action(string file, string command)
    {
        var actions = new List<string>(); var expanded = false;
        NativeWindowMenu.CreateWindow(() => Nodes(expanded, file, command), id => { actions.Add("expand:" + id); expanded = true; },
            id => actions.Add("invoke:" + id), id => actions.Add("collapse:" + id), default);
        Assert.Equal(new[] { "expand:file", "invoke:create", "collapse:file" }, actions);
    }

    [Theory]
    [InlineData("absent")]
    [InlineData("duplicate")]
    [InlineData("hidden")]
    [InlineData("disabled")]
    [InlineData("unsupported")]
    [InlineData("outside-menu")]
    public void Missing_or_ambiguous_native_command_never_invokes_a_window_action(string condition)
    {
        var invocations = new List<string>(); var expanded = false;
        IReadOnlyList<NativeMenuNode> Capture()
        {
            var nodes = Nodes(expanded).ToList();
            if (!expanded) return nodes;
            var i = nodes.FindIndex(x => x.Node.Id == "create");
            if (condition == "absent") nodes.RemoveAt(i);
            if (condition == "duplicate") nodes.Add(nodes[i] with { Node = nodes[i].Node with { Id = "duplicate" } });
            if (condition == "hidden") nodes[i] = nodes[i] with { Node = nodes[i].Node with { Visible = false } };
            if (condition == "disabled") nodes[i] = nodes[i] with { Node = nodes[i].Node with { Enabled = false } };
            if (condition == "unsupported") nodes[i] = nodes[i] with { CanInvoke = false };
            if (condition == "outside-menu") nodes[i] = nodes[i] with { Node = nodes[i].Node with { ParentId = "root" } };
            return nodes;
        }
        Assert.Equal("independent_window_unavailable", Assert.Throws<ImageAutomationException>(() => NativeWindowMenu.CreateWindow(Capture,
            _ => expanded = true, invocations.Add, _ => { }, default, _ => { })).Reason);
        Assert.Empty(invocations);
    }

    [Fact]
    public void Cancellation_after_expansion_does_not_invoke_new_window()
    {
        using var cancellation = new CancellationTokenSource(); var invocations = new List<string>();
        Assert.ThrowsAny<OperationCanceledException>(() => NativeWindowMenu.CreateWindow(() => Nodes(true), _ => cancellation.Cancel(), invocations.Add, _ => { }, cancellation.Token));
        Assert.Empty(invocations);
    }

    [Theory]
    [InlineData("file-id")]
    [InlineData("popup-id")]
    [InlineData("missing-sibling")]
    [InlineData("deep-transcript")]
    public void File_labels_without_the_observed_application_chrome_cannot_create_windows(string condition)
    {
        var invoked = new List<string>();
        var nodes = Nodes(true).ToList();
        var file = nodes.FindIndex(x => x.Node.Id == "file");
        var menu = nodes.FindIndex(x => x.Node.Id == "menu");
        var bar = nodes.FindIndex(x => x.Node.Id == "bar");
        if (condition == "file-id") nodes[file] = nodes[file] with { AutomationId = "message-file" };
        if (condition == "popup-id") nodes[menu] = nodes[menu] with { AutomationId = "message-menu" };
        if (condition == "missing-sibling") nodes.RemoveAll(x => x.Node.Id == "help");
        if (condition == "deep-transcript") { nodes.Add(new(new("turn", "shell", "Group", ""))); nodes[bar] = nodes[bar] with { Node = nodes[bar].Node with { ParentId = "turn" } }; }
        Assert.Throws<ImageAutomationException>(() => NativeWindowMenu.CreateWindow(() => nodes, _ => { }, invoked.Add, _ => { }, default, _ => { }));
        Assert.Empty(invoked);
    }

    [Fact]
    public void Embedded_documents_cannot_supply_duplicate_application_menus()
    {
        var actions = new List<string>();
        var nodes = Nodes(true).Concat(new NativeMenuNode[]
        {
            new(new("embedded", "shell", "Document", "")),
            new(new("other-file", "embedded", "MenuItem", "文件"), true, false, "application-menu-trigger-file-menu"),
            new(new("other-menu", "embedded", "Menu", "文件"), false, false, "application-menu-content")
        }).ToArray();
        NativeWindowMenu.CreateWindow(() => nodes, _ => { }, actions.Add, _ => { }, default, _ => { });
        Assert.Equal(new[] { "create" }, actions);
    }

    [Fact]
    public void Delayed_menu_items_are_polled_after_one_expand_without_replaying_the_action()
    {
        var reads = 0; var actions = new List<string>();
        NativeWindowMenu.CreateWindow(() => ++reads < 3 ? Nodes(false) : Nodes(true), id => actions.Add("expand:" + id),
            id => actions.Add("invoke:" + id), _ => { }, default, _ => { });
        Assert.Equal(new[] { "expand:file", "invoke:create" }, actions);
    }

    static NativeMenuNode[] Nodes(bool expanded, string file = "文件", string command = "新建窗口")
    {
        var nodes = new List<NativeMenuNode>
        {
            new(new("doc", null, "Document", "")), new(new("root", "doc", "Group", "")), new(new("shell", "root", "Group", "")),
            new(new("bar", "shell", "MenuBar", ""), AriaRole: "menubar"),
            new(new("file", "bar", "MenuItem", file), CanExpand: true, AutomationId: "application-menu-trigger-file-menu"),
            new(new("edit", "bar", "MenuItem", ""), AutomationId: "application-menu-trigger-edit-menu"),
            new(new("view", "bar", "MenuItem", ""), AutomationId: "application-menu-trigger-view-menu"),
            new(new("help", "bar", "MenuItem", ""), AutomationId: "application-menu-trigger-help-menu")
        };
        if (expanded) { nodes.Add(new(new("menu", "shell", "Menu", file), AutomationId: "application-menu-content")); nodes.Add(new(new("create", "menu", "MenuItem", command), CanInvoke: true)); }
        return nodes.ToArray();
    }
}
