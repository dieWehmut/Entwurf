using Entwurf.Bridge;
using Xunit;

namespace Entwurf.Bridge.Tests;

public sealed class NativePreviewDownloadLinkTests
{
    [Theory]
    [InlineData("下载")]
    [InlineData("下载图像")]
    [InlineData("下载图片")]
    [InlineData("Download")]
    [InlineData("Download image")]
    public void A_new_named_preview_selects_its_unique_download_link(string label)
    {
        Assert.True(WindowsImageSurface.KnownHyperlinkLabel(label));
        Assert.Equal("download", Find(Preview(label))!.Id);
    }

    [Theory]
    [InlineData("outside")]
    [InlineData("other_preview")]
    [InlineData("old")]
    [InlineData("hidden")]
    [InlineData("disabled")]
    [InlineData("text")]
    [InlineData("unknown_label")]
    [InlineData("suffixed_label")]
    public void An_unowned_or_nonactionable_link_cannot_supply_the_preview_download(string change)
    {
        var nodes = Preview().Select(x => x.Id != "download" ? x : change switch
        {
            "outside" => x with { ParentId = "doc" },
            "other_preview" => x with { ParentId = "other-preview" },
            "hidden" => x with { Visible = false },
            "disabled" => x with { Enabled = false },
            "text" => x with { Role = "Text" },
            "unknown_label" => x with { Name = "Open image" },
            "suffixed_label" => x with { Name = "Download private content" },
            _ => x
        }).Append(new NativeImageNode("other-preview", "doc", "Group", "Image preview")).ToArray();
        Assert.Null(Find(nodes, change == "old" ? new HashSet<string> { "download" } : []));
    }

    [Theory]
    [InlineData("missing")]
    [InlineData("old")]
    [InlineData("hidden")]
    [InlineData("unnamed")]
    [InlineData("wrong_role")]
    [InlineData("wrong_id")]
    public void A_download_link_does_not_bypass_the_bound_preview_root(string change)
    {
        var nodes = Preview().Select(x => x.Id != "preview" ? x : change switch
        {
            "hidden" => x with { Visible = false },
            "unnamed" => x with { Name = "" },
            "wrong_role" => x with { Role = "Pane" },
            _ => x
        }).ToArray();
        if (change == "missing") nodes = nodes.Where(x => x.Id != "preview").ToArray();
        var error = Assert.Throws<ImageAutomationException>(() => NativeImageEvidence.FindPreviewDownload(nodes,
            change == "old" ? new HashSet<string> { "preview" } : [], change == "wrong_id" ? "different-preview" : "preview"));
        Assert.Equal("download_unavailable", error.Reason);
    }

    [Theory]
    [InlineData("Button", "Hyperlink")]
    [InlineData("Hyperlink", "Button")]
    [InlineData("Hyperlink", "Hyperlink")]
    public void Multiple_download_controls_in_the_preview_are_ambiguous_across_roles(string firstRole, string secondRole)
    {
        var nodes = Preview().Select(x => x.Id == "download" ? x with { Role = firstRole } : x)
            .Append(new NativeImageNode("second-download", "preview", secondRole, "Download image")).ToArray();
        var error = Assert.Throws<ImageAutomationException>(() => Find(nodes));
        Assert.Equal("download_unavailable", error.Reason);
    }

    [Fact]
    public void An_outside_link_does_not_make_the_owned_download_link_ambiguous()
    {
        var nodes = Preview().Append(new NativeImageNode("outside-download", "doc", "Hyperlink", "Download image")).ToArray();
        Assert.Equal("download", Find(nodes)!.Id);
    }

    [Fact]
    public void A_download_link_in_the_transcript_is_not_a_new_direct_image_action()
    {
        var nodes = NativeImageTurnBindingTests.Transcript().Select(x => x.Id == "preview"
            ? x with { Role = "Hyperlink", Name = "Download image" } : x).ToArray();
        Assert.Null(NativeImageEvidence.Find(nodes, new HashSet<string>(), "public generated image test", null));
    }

    [Theory]
    [InlineData("Close image preview")]
    [InlineData("关闭查看器")]
    [InlineData("Stop generating")]
    [InlineData("正在生成图像")]
    public void A_known_link_label_does_not_authorize_a_download_unless_it_is_a_download(string label)
    {
        Assert.True(WindowsImageSurface.KnownHyperlinkLabel(label));
        Assert.Null(Find(Preview(label)));
    }

    [Theory]
    [InlineData("")]
    [InlineData("Images")]
    [InlineData("Log in")]
    [InlineData("Send")]
    [InlineData("You said:")]
    [InlineData("Image preview")]
    [InlineData("Open image preview")]
    [InlineData("已生成图像 1")]
    [InlineData("Download GIF")]
    [InlineData("Download private content")]
    [InlineData("private conversation title")]
    [InlineData("https://example.invalid/private-file")]
    public void Arbitrary_link_names_and_other_ui_labels_are_not_retained(string label)
        => Assert.False(WindowsImageSurface.KnownHyperlinkLabel(label));

    static NativeImageNode[] Preview(string label = "Download") =>
    [
        new("doc", null, "Document", ""),
        new("preview", "doc", "Group", "Image preview"),
        new("download-wrapper", "preview", "Group", ""),
        new("download", "download-wrapper", "Hyperlink", label)
    ];

    static NativeImageNode? Find(NativeImageNode[] nodes, HashSet<string>? before = null)
        => NativeImageEvidence.FindPreviewDownload(nodes, before ?? [], "preview");
}
