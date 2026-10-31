using Entwurf.Bridge;
using Xunit;

namespace Entwurf.Bridge.Tests;

public sealed class NativeImageEvidenceTests
{
    [Theory]
    [InlineData("ChatGPT 说：", true, "heading", "assistant")]
    [InlineData("You said:", true, "heading", "user")]
    [InlineData("ChatGPT 说：", false, "heading", null)]
    [InlineData("ChatGPT 说：", true, "paragraph", null)]
    [InlineData("quoted ChatGPT said:", true, "heading", null)]
    public void Speaker_markers_require_the_observed_heading_level_role_and_exact_label(string label, bool levelFour, string ariaRole, string? speaker)
    {
        Assert.Equal(speaker, NativeImageEvidence.SpeakerFromHeading(label, levelFour, ariaRole));
    }
    [Fact]
    public void An_attachment_in_the_next_user_turn_is_not_an_assistant_response()
    {
        var tree = Tree().Where(x => x.Id is not "assistant" and not "assistant-role" and not "new-image" and not "new-download")
            .Concat(new NativeImageNode[]
            {
                new("next-user", "conversation", "Group", ""),
                new("next-user-role", "next-user", "Text", "你说：", Speaker: "user"),
                new("next-prompt", "next-user", "Text", "another prompt"),
                new("next-attachment", "next-user", "Image", "", Width: 512, Height: 512),
                new("next-download", "next-user", "Button", "下载图像")
            }).ToArray();
        Assert.Null(NativeImageEvidence.Find(tree, new HashSet<string>(), "owned prompt", null));
    }

    [Fact]
    public void A_later_assistant_image_after_another_user_message_cannot_replace_our_text_only_answer()
    {
        var tree = Tree().Where(x => x.Id is not "new-image" and not "new-download")
            .Concat(new NativeImageNode[]
            {
                new("our-text-answer", "assistant", "Text", "An answer without an image"),
                new("next-user", "conversation", "Group", ""), new("next-user-role", "next-user", "Text", "你说：", Speaker: "user"),
                new("next-prompt", "next-user", "Text", "different image prompt"),
                new("next-assistant", "conversation", "Group", ""), new("next-assistant-role", "next-assistant", "Text", "ChatGPT 说：", Speaker: "assistant"),
                new("next-image", "next-assistant", "Image", "", Width: 512, Height: 512), new("next-download", "next-assistant", "Button", "下载图像")
            }).ToArray();
        Assert.Null(NativeImageEvidence.Find(tree, new HashSet<string>(), "owned prompt", null));
    }

    [Fact]
    public void Role_words_in_ordinary_message_text_do_not_supply_assistant_semantics()
    {
        Assert.Null(NativeImageEvidence.Find(Tree().Select(x => x with { Speaker = null }).ToArray(), new HashSet<string>(), "owned prompt", null));
    }
    [Fact]
    public void Only_a_new_image_in_the_response_after_this_runs_exact_user_turn_qualifies()
    {
        var tree = Tree();
        var result = NativeImageEvidence.Find(tree, new HashSet<string> { "old-image", "old-download" }, "owned prompt", null);
        Assert.NotNull(result);
        Assert.Equal("new-image", result.ImageId);
        Assert.Equal("owned-user-text", result.UserId);
        Assert.Equal("assistant", result.ResponseId);
        Assert.Equal("new-download", result.ActionId);
    }

    [Fact]
    public void A_user_attachment_is_not_the_generated_assistant_image()
    {
        var tree = Tree().Where(x => x.Id is not "new-image" and not "new-download").ToArray();
        Assert.Null(NativeImageEvidence.Find(tree, new HashSet<string>(), "owned prompt", null));
    }

    [Fact]
    public void Changing_conversation_after_the_user_turn_is_bound_pauses_instead_of_accepting_an_old_image()
    {
        var tree = Tree().Select(x => x.Id == "owned-user-text" ? x with { Id = "other-user-text" } : x).ToArray();
        var error = Assert.Throws<ImageAutomationException>(() => NativeImageEvidence.Find(tree, new HashSet<string>(), "owned prompt", "owned-user-text"));
        Assert.Equal("conversation_changed", error.Reason);
    }

    [Fact]
    public void Existing_image_and_download_runtime_ids_cannot_be_reused_as_new_output()
    {
        Assert.Null(NativeImageEvidence.Find(Tree(), new HashSet<string> { "new-image", "new-download", "old-image", "old-download" }, "owned prompt", null));
    }

    [Fact]
    public void More_than_one_matching_image_is_ambiguous_and_never_guessed()
    {
        var tree = Tree().Concat(new[] { new NativeImageNode("second-image", "assistant", "Image", "", Width: 512, Height: 512) }).ToArray();
        var error = Assert.Throws<ImageAutomationException>(() => NativeImageEvidence.Find(tree, new HashSet<string>(), "owned prompt", null));
        Assert.Equal("ambiguous_images", error.Reason);
    }

    static NativeImageNode[] Tree() =>
    [
        new("doc", null, "Document", ""),
        new("conversation", "doc", "Group", ""),
        new("old-assistant", "conversation", "Group", ""),
        new("old-role", "old-assistant", "Text", "ChatGPT 说：", Speaker: "assistant"),
        new("old-image", "old-assistant", "Image", "", Width: 512, Height: 512),
        new("old-download", "old-assistant", "Button", "下载图像"),
        new("user", "conversation", "Group", ""),
        new("user-role", "user", "Text", "你说：", Speaker: "user"),
        new("owned-user-text", "user", "Text", "owned prompt"),
        new("attachment", "user", "Image", "", Width: 512, Height: 512),
        new("attachment-download", "user", "Button", "下载图像"),
        new("assistant", "conversation", "Group", ""),
        new("assistant-role", "assistant", "Text", "ChatGPT 说：", Speaker: "assistant"),
        new("new-image", "assistant", "Image", "", Width: 512, Height: 512),
        new("new-download", "assistant", "Button", "下载图像")
    ];
}
