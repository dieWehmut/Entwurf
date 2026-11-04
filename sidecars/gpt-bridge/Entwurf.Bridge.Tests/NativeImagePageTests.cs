using Entwurf.Bridge;
using Xunit;

namespace Entwurf.Bridge.Tests;

public sealed class NativeImagePageTests
{
    [Fact]
    public void The_observed_images_page_has_a_direct_page_heading_and_a_model_picker_in_its_composer()
    {
        Assert.True(NativeImagePage.IsHome(Home(), "composer"));
    }

    [Fact]
    public void An_old_chat_message_named_images_cannot_authorize_input_while_navigation_is_still_pending()
    {
        var oldChat = Home().Where(x => x.Id != "heading").Concat(new NativeImageNode[]
        {
            new("transcript", "main", "Group", ""), new("turn", "transcript", "Group", ""),
            new("old-user", "turn", "Group", ""), new("old-text", "old-user", "Text", "图像")
        }).ToArray();
        Assert.False(NativeImagePage.IsHome(oldChat, "composer"));
    }

    [Fact]
    public void A_model_picker_elsewhere_in_the_window_does_not_validate_this_composer()
    {
        var unrelatedPicker = Home().Select(x => x.Id == "model" ? x with { ParentId = "rail" } : x).ToArray();
        Assert.False(NativeImagePage.IsHome(unrelatedPicker, "composer"));
    }

    [Fact]
    public void An_images_page_with_a_reappearing_old_transcript_cannot_authorize_prompt_input()
    {
        var changed = Home().Append(new NativeImageNode("old-turn", "main", "Text", "你说：", Speaker: "user")).ToArray();
        Assert.False(NativeImagePage.IsHome(changed, "composer"));
    }

    [Theory]
    [InlineData("OpenAI.Codex_26.1027.3384.0_x64__2p2nqsd0c76g0", true)]
    [InlineData("OpenAI.ChatGPT_1.2.3.4_x64__2p2nqsd0c76g0", true)]
    [InlineData("OpenAI.Codex_26.1027.3384.0_x64__otherpublisher", false)]
    [InlineData("Untrusted.ChatGPT_1.2.3.4_x64__2p2nqsd0c76g0", false)]
    public void Package_discovery_accepts_only_the_known_publisher_and_expected_products(string package, bool qualifies)
    {
        Assert.Equal(qualifies, WindowsImageSurface.QualifyingPackageVersion(package) is not null);
    }

    static NativeImageNode[] Home() =>
    [
        new("doc", null, "Document", ""), new("root", "doc", "Group", ""), new("shell", "root", "Group", ""),
        new("rail", "shell", "Group", ""), new("images-nav", "rail", "Button", "图像"),
        new("main", "shell", "Group", ""), new("heading", "main", "Text", "图像"),
        new("composer-wrap", "main", "Group", ""), new("composer-row", "composer-wrap", "Group", ""),
        new("composer", "composer-row", "Edit", ""), new("model", "composer-row", "Button", "选择 ChatGPT 模型")
    ];
}
