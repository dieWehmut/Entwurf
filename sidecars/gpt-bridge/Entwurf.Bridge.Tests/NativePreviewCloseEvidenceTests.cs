using Entwurf.Bridge;
using Xunit;

namespace Entwurf.Bridge.Tests;

public sealed class NativePreviewCloseEvidenceTests
{
    [Theory]
    [InlineData("关闭查看器")]
    [InlineData("Close viewer")]
    [InlineData("关闭图像预览")]
    [InlineData("关闭图片预览")]
    [InlineData("Close image preview")]
    [InlineData("关闭")]
    [InlineData("Close")]
    public void A_verified_preview_can_retain_and_select_its_unique_close_button(string label)
    {
        Assert.True(WindowsImageSurface.KnownLabel(label));
        var nodes = Preview(label).Append(new NativeImageNode("outside-close", "doc", "Button", label)).ToArray();
        Assert.Equal("close", NativeImageEvidence.FindPreviewClose(nodes, "preview").Id);
    }

    [Theory]
    [InlineData("outside")]
    [InlineData("other_preview")]
    [InlineData("hidden")]
    [InlineData("disabled")]
    [InlineData("text")]
    [InlineData("unknown_label")]
    public void An_unowned_or_nonactionable_viewer_close_does_not_authorize_dismissal(string change)
    {
        var nodes = Preview("Close viewer").Select(x => x.Id != "close" ? x : change switch
        {
            "outside" => x with { ParentId = "doc" },
            "other_preview" => x with { ParentId = "other-preview" },
            "hidden" => x with { Visible = false },
            "disabled" => x with { Enabled = false },
            "text" => x with { Role = "Text" },
            "unknown_label" => x with { Name = "" },
            _ => x
        }).Append(new NativeImageNode("other-preview", "doc", "Group", "Image preview")).ToArray();
        AssertUnavailable(nodes, "preview");
    }

    [Theory]
    [InlineData("Close viewer (Esc)")]
    [InlineData("Close viewer private content")]
    [InlineData("quoted Close viewer")]
    [InlineData("关闭查看器并发送")]
    public void Additional_text_does_not_turn_a_label_into_viewer_close_evidence(string label)
    {
        Assert.False(WindowsImageSurface.KnownLabel(label));
        AssertUnavailable(Preview(label), "preview");
    }

    [Fact]
    public void Multiple_close_buttons_inside_the_bound_preview_remain_ambiguous()
    {
        var nodes = Preview("Close viewer").Append(new NativeImageNode("second-close", "preview", "Button", "关闭查看器")).ToArray();
        AssertUnavailable(nodes, "preview");
    }

    [Fact]
    public void A_different_preview_identity_cannot_redirect_the_close_action()
        => AssertUnavailable(Preview("关闭查看器"), "replacement-preview");

    static NativeImageNode[] Preview(string label) =>
    [
        new("doc", null, "Document", ""),
        new("preview", "doc", "Group", "Image preview"),
        new("close-wrapper", "preview", "Group", ""),
        new("close", "close-wrapper", "Button", label)
    ];

    static void AssertUnavailable(IReadOnlyList<NativeImageNode> nodes, string previewRootId)
    {
        var error = Assert.Throws<ImageAutomationException>(() => NativeImageEvidence.FindPreviewClose(nodes, previewRootId));
        Assert.Equal("download_unavailable", error.Reason);
    }
}
