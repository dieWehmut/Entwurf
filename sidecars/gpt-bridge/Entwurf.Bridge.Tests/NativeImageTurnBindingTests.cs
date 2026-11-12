using System.Security.Cryptography;
using System.Text;
using Entwurf.Bridge;
using Xunit;

namespace Entwurf.Bridge.Tests;

public sealed class NativeImageTurnBindingTests
{
    const string Prompt = "public generated image test";
    static readonly string PromptHash = Convert.ToHexStringLower(SHA256.HashData(Encoding.UTF8.GetBytes(Prompt)));
    static readonly HashSet<string> Before = new(StringComparer.Ordinal);

    [Fact]
    public void A_transient_composer_copy_does_not_become_the_submitted_user_turn()
    {
        var pending = Transcript().Where(x => x.Id is not "user-heading" and not "user-text" and not "assistant-heading" and not "preview" and not "image")
            .Concat(new NativeImageNode[]
            {
                new("edit", "transcript", "Edit", ""),
                new("composer-text", "edit", "Text", Prompt)
            }).ToArray();
        Assert.Null(NativeImageEvidence.FindUserTurnByHash(pending, Before, PromptHash, null));
        Assert.Null(NativeImageEvidence.FindByHash(pending, Before, PromptHash, null));
        var submitted = Transcript().Concat(pending.Where(x => x.Id is "edit" or "composer-text")).ToArray();
        var turn = NativeImageEvidence.FindUserTurnByHash(submitted, Before, PromptHash, null);
        Assert.Equal(new NativeUserTurn("user-text", "user-heading"), turn);
        Assert.Equal("image", NativeImageEvidence.FindByHash(submitted, Before, PromptHash, turn!.TextId, turn.HeadingId)!.ImageId);
    }

    [Fact]
    public void Rerendered_prompt_text_remains_bound_to_its_same_user_heading_and_response()
    {
        var beforeRender = Transcript();
        var turn = NativeImageEvidence.FindUserTurnByHash(beforeRender, Before, PromptHash, null)!;
        var expected = NativeImageEvidence.FindByHash(beforeRender, Before, PromptHash, turn.TextId, turn.HeadingId)!;
        var afterRender = beforeRender.Select(x => x.Id == "user-text" ? x with { Id = "settled-user-text" } : x).ToArray();
        var rebound = NativeImageEvidence.FindUserTurnByHash(afterRender, Before, PromptHash, turn.TextId, turn.HeadingId);
        Assert.Equal(new NativeUserTurn("settled-user-text", "user-heading"), rebound);
        var current = NativeImageEvidence.FindByHash(afterRender, Before, PromptHash, turn.TextId, turn.HeadingId);
        Assert.Equal("settled-user-text", current!.UserId);
        Assert.True(NativeImageEvidence.SameResponse(current, expected));
    }

    [Fact]
    public void A_later_identical_prompt_cannot_replace_the_bound_user_heading()
    {
        var tree = Transcript().Where(x => x.Id != "user-text").Concat(new NativeImageNode[]
        {
            new("later-user-heading", "transcript", "Text", "You said:", Speaker: "user"),
            new("later-user-text", "transcript", "Text", Prompt),
            new("later-assistant-heading", "transcript", "Text", "ChatGPT said:", Speaker: "assistant"),
            new("later-preview", "transcript", "Button", "已生成图像 1"),
            new("later-image", "later-preview", "Image", "已生成图像 1", Width: 512, Height: 512)
        }).ToArray();
        var error = Assert.Throws<ImageAutomationException>(() => NativeImageEvidence.FindByHash(tree, Before, PromptHash, "user-text", "user-heading"));
        Assert.Equal("conversation_changed", error.Reason);
    }

    [Theory]
    [InlineData("removed")]
    [InlineData("old")]
    [InlineData("assistant")]
    [InlineData("edited_prompt")]
    [InlineData("duplicate_prompt")]
    public void Changed_or_ambiguous_bound_user_evidence_is_never_rebound(string change)
    {
        var tree = Transcript();
        var before = new HashSet<string>(Before, StringComparer.Ordinal);
        if (change == "removed") tree = tree.Where(x => x.Id != "user-heading").ToArray();
        if (change == "old") before.Add("user-heading");
        if (change == "assistant") tree = tree.Select(x => x.Id == "user-heading" ? x with { Speaker = "assistant" } : x).ToArray();
        if (change == "edited_prompt") tree = tree.Select(x => x.Id == "user-text" ? x with { Name = Prompt + " edited" } : x).ToArray();
        if (change == "duplicate_prompt") tree = tree.SelectMany(x => x.Id == "user-text" ? new[] { x, x with { Id = "duplicate" } } : new[] { x }).ToArray();
        var error = Assert.Throws<ImageAutomationException>(() => NativeImageEvidence.FindUserTurnByHash(tree, before, PromptHash, "user-text", "user-heading"));
        Assert.Equal("conversation_changed", error.Reason);
    }

    [Theory]
    [InlineData("user")]
    [InlineData("response")]
    [InlineData("image")]
    [InlineData("action")]
    [InlineData("assistant")]
    [InlineData("preview")]
    public void A_heading_binding_does_not_authorize_a_different_response(string change)
    {
        var expected = NativeImageEvidence.FindByHash(Transcript(), Before, PromptHash, null)!;
        var changed = change switch
        {
            "user" => expected with { UserHeadingId = "different-user-heading" },
            "response" => expected with { ResponseId = "different-response" },
            "image" => expected with { ImageId = "different-image" },
            "action" => expected with { ActionId = "different-action" },
            "assistant" => expected with { AssistantId = "different-assistant" },
            _ => expected with { OpensPreview = !expected.OpensPreview }
        };
        Assert.False(NativeImageEvidence.SameResponse(changed, expected));
        Assert.False(NativeImageEvidence.SameResponse(null, expected));
    }

    [Fact]
    public void A_legacy_text_binding_does_not_accept_a_changed_text_id()
    {
        var expected = NativeImageEvidence.FindByHash(Transcript(), Before, PromptHash, null)! with { UserHeadingId = null };
        Assert.False(NativeImageEvidence.SameResponse(expected with { UserId = "settled-user-text" }, expected));
    }

    internal static NativeImageNode[] Transcript() =>
    [
        new("doc", null, "Document", ""),
        new("transcript", "doc", "Group", ""),
        new("user-heading", "transcript", "Text", "You said:", Speaker: "user"),
        new("user-text", "transcript", "Text", Prompt),
        new("assistant-heading", "transcript", "Text", "ChatGPT said:", Speaker: "assistant"),
        new("preview", "transcript", "Button", "已生成图像 1"),
        new("image", "preview", "Image", "已生成图像 1", Width: 512, Height: 512)
    ];
}
