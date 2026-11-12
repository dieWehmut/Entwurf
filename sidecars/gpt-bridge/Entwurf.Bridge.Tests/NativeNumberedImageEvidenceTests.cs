using Entwurf.Bridge;
using Xunit;

namespace Entwurf.Bridge.Tests;

public sealed class NativeNumberedImageEvidenceTests
{
    [Theory]
    [InlineData("已生成图像 1", true)]
    [InlineData("已生成图像 9", true)]
    [InlineData("已生成图像 64", true)]
    [InlineData("已生成图像 0", false)]
    [InlineData("已生成图像 01", false)]
    [InlineData("已生成图像 65", false)]
    [InlineData("已生成图像 1 ", false)]
    [InlineData("已生成图像 +1", false)]
    [InlineData("已生成图像 １", false)]
    [InlineData("已生成图像\n1", false)]
    [InlineData("quoted 已生成图像 1", false)]
    [InlineData("已生成图像 1 private caption", false)]
    public void Numbered_preview_labels_have_a_fixed_bounded_format(string label, bool expected)
    {
        Assert.Equal(expected, NativeImageEvidence.IsGeneratedImageLabel(label));
    }

    [Fact]
    public void The_flat_transcript_numbered_button_opens_only_its_immediate_image_child()
    {
        var result = Find(NativeImageTurnBindingTests.Transcript());
        Assert.NotNull(result);
        Assert.Equal("user-heading", result.UserHeadingId);
        Assert.Equal("assistant-heading", result.AssistantId);
        Assert.Equal("preview", result.ResponseId);
        Assert.Equal("preview", result.ActionId);
        Assert.Equal("image", result.ImageId);
        Assert.True(result.OpensPreview);
    }

    [Theory]
    [InlineData("mismatched_labels")]
    [InlineData("unlabelled_image")]
    [InlineData("sibling_image")]
    [InlineData("nested_image")]
    [InlineData("hidden_button")]
    [InlineData("disabled_button")]
    [InlineData("hidden_image")]
    [InlineData("small_image")]
    [InlineData("old_button")]
    [InlineData("old_image")]
    [InlineData("user_attachment")]
    public void Similar_or_unowned_numbered_images_do_not_supply_a_download_action(string change)
    {
        var tree = NativeImageTurnBindingTests.Transcript();
        var before = new HashSet<string>(StringComparer.Ordinal);
        tree = tree.Select(x => change switch
        {
            "mismatched_labels" when x.Id == "preview" => x with { Name = "已生成图像 2" },
            "unlabelled_image" when x.Id == "image" => x with { Name = "" },
            "sibling_image" when x.Id == "image" => x with { ParentId = "transcript" },
            "nested_image" when x.Id == "image" => x with { ParentId = "image-wrapper" },
            "hidden_button" when x.Id == "preview" => x with { Visible = false },
            "disabled_button" when x.Id == "preview" => x with { Enabled = false },
            "hidden_image" when x.Id == "image" => x with { Visible = false },
            "small_image" when x.Id == "image" => x with { Width = 32 },
            "user_attachment" when x.Id == "assistant-heading" => x with { Speaker = "user", Name = "You said:" },
            _ => x
        }).ToArray();
        if (change == "nested_image") tree = tree.Concat(new[] { new NativeImageNode("image-wrapper", "preview", "Group", "") }).ToArray();
        if (change == "old_button") before.Add("preview");
        if (change == "old_image") before.Add("image");
        Assert.Null(Find(tree, before));
    }

    [Fact]
    public void Multiple_numbered_images_in_the_same_turn_remain_ambiguous()
    {
        var tree = NativeImageTurnBindingTests.Transcript().Concat(new NativeImageNode[]
        {
            new("second-preview", "transcript", "Button", "已生成图像 2"),
            new("second-image", "second-preview", "Image", "已生成图像 2", Width: 512, Height: 512)
        }).ToArray();
        var error = Assert.Throws<ImageAutomationException>(() => Find(tree));
        Assert.Equal("ambiguous_images", error.Reason);
    }

    [Fact]
    public void A_numbered_image_after_the_next_user_heading_is_not_our_response()
    {
        var tree = NativeImageTurnBindingTests.Transcript().Where(x => x.Id is not "preview" and not "image").Concat(new NativeImageNode[]
        {
            new("next-user-heading", "transcript", "Text", "You said:", Speaker: "user"),
            new("next-user-text", "transcript", "Text", "different prompt"),
            new("next-assistant-heading", "transcript", "Text", "ChatGPT said:", Speaker: "assistant"),
            new("preview", "transcript", "Button", "已生成图像 1"),
            new("image", "preview", "Image", "已生成图像 1", Width: 512, Height: 512)
        }).ToArray();
        Assert.Null(Find(tree));
    }

    static NativeGeneratedImage? Find(NativeImageNode[] nodes, HashSet<string>? before = null)
        => NativeImageEvidence.Find(nodes, before ?? new HashSet<string>(), "public generated image test", null);
}
