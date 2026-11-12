using Entwurf.Bridge;
using Xunit;

namespace Entwurf.Bridge.Tests;

public sealed class NativeComposerValueTests
{
    const string Hint = "描述新图像";

    [Theory]
    [InlineData("描述新图像")]
    [InlineData("随心输入")]
    [InlineData("Describe a new image")]
    public void A_single_empty_paragraph_excludes_its_generated_hint_and_trailing_break(string hint)
    {
        Assert.Equal("", NativeComposerValue.Read(hint + "\n", hint, Empty(hint)));
    }

    [Fact]
    public void A_real_draft_equal_to_the_hint_is_preserved_without_the_empty_paragraph_marker()
    {
        var typed = Empty().Select(x => x.Id == "paragraph" ? x with { ClassName = "" } : x).ToArray();
        Assert.Equal(Hint + "\n", NativeComposerValue.Read(Hint + "\n", Hint, typed));
    }

    [Theory]
    [InlineData(" ")]
    [InlineData("\n")]
    [InlineData(" \n")]
    [InlineData("描述新图像")]
    [InlineData("描述新图像\n\n")]
    [InlineData("user draft\n")]
    public void Whitespace_and_user_text_are_never_trimmed_or_removed(string draft)
    {
        Assert.Equal(draft, NativeComposerValue.Read(draft, Hint, Empty()));
    }

    [Theory]
    [InlineData("Text")]
    [InlineData("Image")]
    public void An_empty_paragraph_does_not_hide_other_text_or_an_attachment(string role)
    {
        var extra = Empty().Append(new NativeComposerNode("extra", "editor", role, "", "")).ToArray();
        Assert.Equal(Hint + "\n", NativeComposerValue.Read(Hint + "\n", Hint, extra));
    }

    [Theory]
    [InlineData("missing-marker")]
    [InlineData("unrelated-hint")]
    [InlineData("duplicate-id")]
    [InlineData("wrong-editor")]
    public void Missing_or_unrelated_raw_evidence_never_erases_a_possible_draft(string problem)
    {
        var nodes = Empty().Select(x => problem switch
        {
            "missing-marker" when x.Id == "paragraph" => x with { ClassName = "not-placeholder" },
            "unrelated-hint" when x.Id == "hint" => x with { ParentId = "editor" },
            "duplicate-id" when x.Id == "hint" => x with { Id = "paragraph" },
            "wrong-editor" when x.Id == "editor" => x with { Name = "other editor" },
            _ => x
        }).ToArray();
        Assert.Equal(Hint + "\n", NativeComposerValue.Read(Hint + "\n", Hint, nodes));
    }

    [Fact]
    public void Actual_class_tokens_accept_focus_styling_without_accepting_similar_class_names()
    {
        var styled = Empty().Select(x => x.Id == "editor" ? x with { ClassName = "ProseMirror ProseMirror-focused" } : x).ToArray();
        Assert.Equal("", NativeComposerValue.Read(Hint + "\n", Hint, styled));
        var impostor = styled.Select(x => x.Id == "editor" ? x with { ClassName = "not-ProseMirror" } : x).ToArray();
        Assert.Equal(Hint + "\n", NativeComposerValue.Read(Hint + "\n", Hint, impostor));
    }

    static NativeComposerNode[] Empty(string hint = Hint) =>
    [
        new("editor", null, "Edit", hint, "ProseMirror", "textbox"),
        new("paragraph", "editor", "Group", "", "placeholder", "group"),
        new("wrapper", "paragraph", "Group", "", "", "group"),
        new("break", "paragraph", "Text", "\n", "ProseMirror-trailingBreak", "description"),
        new("hint", "wrapper", "Text", hint, "", "description")
    ];
}
