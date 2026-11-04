using Entwurf.Bridge;
using Xunit;

namespace Entwurf.Bridge.Tests;

public sealed class NativeImageFrameTests
{
    [Theory]
    [InlineData(false)]
    [InlineData(true)]
    public void Preparation_uses_only_controls_owned_by_the_outer_document_regardless_of_enumeration_order(bool reverse)
    {
        var nodes = Chat().Concat(new NativeImageNode[]
        {
            new("embedded", "main", "Document", ""), new("embedded-child", "embedded", "Document", ""),
            new("nested-edit", "embedded-child", "Edit", ""), new("nested-new", "embedded", "Button", "新聊天"),
            new("nested-images", "embedded", "Button", "图像"), new("nested-login", "embedded", "Button", "登录"),
            new("nested-turn", "embedded", "Text", "你说：", Speaker: "user")
        });
        var frame = NativeImageFrame.Capture((reverse ? nodes.Reverse() : nodes).ToArray());
        Assert.True(NativeImagePreparation.HasMode(frame.Nodes));
        Assert.Equal("images-nav", NativeImagePreparation.Button(frame.Nodes, "图像", "Images")?.Id);
        Assert.Single(frame.Nodes, x => x.Role == "Edit");
        Assert.DoesNotContain(frame.Nodes, x => x.Id.StartsWith("nested") || x.Id.StartsWith("embedded"));
        Assert.Equal("doc", frame.DocumentId);
    }

    [Theory]
    [InlineData("independent-document")]
    [InlineData("missing-parent")]
    [InlineData("cycle")]
    [InlineData("duplicate")]
    public void Ambiguous_or_malformed_capture_cannot_authorize_navigation(string condition)
    {
        var nodes = Chat().ToList();
        if (condition == "independent-document") nodes.Add(new("other", "window", "Document", ""));
        if (condition == "missing-parent") nodes.Add(new("orphan", "missing", "Group", ""));
        if (condition == "cycle") { nodes.Add(new("a", "b", "Group", "")); nodes.Add(new("b", "a", "Group", "")); }
        if (condition == "duplicate") nodes.Add(nodes[0]);
        Assert.Throws<ImageAutomationException>(() => NativeImageFrame.Capture(nodes));
    }

    static NativeImageNode[] Chat() =>
    [
        new("window", null, "Window", ""), new("doc", "window", "Document", ""),
        new("root", "doc", "Group", ""), new("shell", "root", "Group", ""),
        new("rail", "shell", "Group", ""), new("new-chat", "rail", "Button", "新聊天"),
        new("images-nav", "rail", "Button", "图像"), new("mode", "rail", "Button", "切换模式，当前模式：ChatGPT"),
        new("main", "shell", "Group", ""), new("composer-wrap", "main", "Group", ""),
        new("composer-row", "composer-wrap", "Group", ""), new("composer", "composer-row", "Edit", ""),
        new("model", "composer-row", "Button", "选择 ChatGPT 模型")
    ];
}
