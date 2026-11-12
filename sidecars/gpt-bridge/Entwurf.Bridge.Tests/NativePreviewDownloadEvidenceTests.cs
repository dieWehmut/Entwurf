using Entwurf.Bridge;
using Xunit;

namespace Entwurf.Bridge.Tests;

public sealed class NativePreviewDownloadEvidenceTests
{
    [Theory]
    [InlineData("Group", "图像预览", "下载")]
    [InlineData("Group", "图片预览", "下载图片")]
    [InlineData("Window", "Image preview", "Download")]
    [InlineData("Window", "Image preview", "Download image")]
    public void A_new_named_preview_supplies_its_unique_visible_enabled_download(string role, string previewLabel, string downloadLabel)
    {
        var tree = Preview().Select(x => x.Id switch
        {
            "preview" => x with { Role = role, Name = previewLabel },
            "download" => x with { Name = downloadLabel },
            _ => x
        }).ToArray();
        Assert.Equal("download", Find(tree)!.Id);
    }

    [Theory]
    [InlineData("outside")]
    [InlineData("other_preview")]
    [InlineData("old")]
    [InlineData("hidden")]
    [InlineData("disabled")]
    [InlineData("text")]
    [InlineData("quoted_label")]
    [InlineData("suffixed_label")]
    public void An_unowned_or_nonactionable_download_does_not_qualify(string change)
    {
        var tree = Preview().Select(x => x.Id != "download" ? x : change switch
        {
            "outside" => x with { ParentId = "doc" },
            "other_preview" => x with { ParentId = "other-preview" },
            "hidden" => x with { Visible = false },
            "disabled" => x with { Enabled = false },
            "text" => x with { Role = "Text" },
            "quoted_label" => x with { Name = "quoted 下载" },
            "suffixed_label" => x with { Name = "Download private content" },
            _ => x
        }).ToArray();
        if (change == "other_preview") tree = tree.Concat(new[] { new NativeImageNode("other-preview", "doc", "Group", "图像预览") }).ToArray();
        var before = new HashSet<string>();
        if (change == "old") before.Add("download");
        Assert.Null(Find(tree, before));
    }

    [Theory]
    [InlineData("missing")]
    [InlineData("old")]
    [InlineData("hidden")]
    [InlineData("unnamed")]
    [InlineData("wrong_name")]
    [InlineData("wrong_role")]
    [InlineData("wrong_id")]
    public void An_unverified_preview_root_cannot_authorize_a_generic_download(string change)
    {
        var tree = Preview().Select(x => x.Id != "preview" ? x : change switch
        {
            "hidden" => x with { Visible = false },
            "unnamed" => x with { Name = "" },
            "wrong_name" => x with { Name = "public image browser" },
            "wrong_role" => x with { Role = "Pane" },
            _ => x
        }).ToArray();
        if (change == "missing") tree = tree.Where(x => x.Id != "preview").ToArray();
        var before = new HashSet<string>();
        if (change == "old") before.Add("preview");
        var error = Assert.Throws<ImageAutomationException>(() => NativeImageEvidence.FindPreviewDownload(tree, before,
            change == "wrong_id" ? "different-preview" : "preview"));
        Assert.Equal("download_unavailable", error.Reason);
    }

    [Fact]
    public void Multiple_download_buttons_inside_the_bound_preview_are_ambiguous()
    {
        var tree = Preview().Concat(new[] { new NativeImageNode("second-download", "preview", "Button", "Download image") }).ToArray();
        var error = Assert.Throws<ImageAutomationException>(() => Find(tree));
        Assert.Equal("download_unavailable", error.Reason);
    }

    [Fact]
    public void An_unrelated_download_outside_the_preview_does_not_make_its_owned_download_ambiguous()
    {
        var tree = Preview().Concat(new[] { new NativeImageNode("outside-download", "doc", "Button", "下载") }).ToArray();
        Assert.Equal("download", Find(tree)!.Id);
    }

    [Theory]
    [InlineData("下载")]
    [InlineData("Download")]
    public void A_generic_preview_label_does_not_authorize_a_transcript_download(string label)
    {
        Assert.True(NativeImageEvidence.IsPreviewDownload(label));
        Assert.False(NativeImageEvidence.IsDownload(label));
        var tree = NativeImageTurnBindingTests.Transcript().Select(x => x.Id switch
        {
            "preview" => x with { Name = label },
            "image" => x with { Name = "" },
            _ => x
        }).ToArray();
        Assert.Null(NativeImageEvidence.Find(tree, new HashSet<string>(), "public generated image test", null));
    }

    static NativeImageNode[] Preview() =>
    [
        new("doc", null, "Document", ""),
        new("preview", "doc", "Group", "图像预览"),
        new("download-wrapper", "preview", "Group", ""),
        new("download", "download-wrapper", "Button", "下载")
    ];

    static NativeImageNode? Find(NativeImageNode[] nodes, HashSet<string>? before = null)
        => NativeImageEvidence.FindPreviewDownload(nodes, before ?? new HashSet<string>(), "preview");
}
