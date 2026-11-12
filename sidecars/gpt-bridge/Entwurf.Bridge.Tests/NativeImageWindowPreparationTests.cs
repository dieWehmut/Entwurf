using Entwurf.Bridge;
using Xunit;

namespace Entwurf.Bridge.Tests;

public sealed class NativeImageWindowPreparationTests
{
    static readonly NativeImageTarget Source = new(10, 7, DateTime.UnixEpoch);
    static readonly NativeImageTarget Created = new(20, 7, DateTime.UnixEpoch);

    [Fact]
    public void Missing_new_window_command_prevents_all_navigation_and_input()
    {
        var preparation = new NativeImagePreparation();
        Assert.Equal("independent_window_unavailable", Assert.Throws<ImageAutomationException>(() =>
            preparation.Begin(Source, [Source], _ => Frame(), _ => throw new ImageAutomationException("independent_window_unavailable"), default)).Reason);
        Assert.Null(preparation.Target);
    }

    [Fact]
    public void Only_the_new_window_receives_images_navigation_and_composer_binding()
    {
        var preparation = new NativeImagePreparation(); var actions = new List<string>();
        preparation.Begin(Source, [Source], _ => Frame(), target => actions.Add($"new-window:{target.Handle}"), default);
        Assert.Null(preparation.Advance([Source], _ => throw new Exception("Must wait for a new HWND"), (_, _) => throw new Exception("Do not read original draft"), (_, _) => throw new Exception("No old-window actions"), default));
        Assert.Null(preparation.Advance([Source, Created], target => { Assert.Equal(Created, target); return Frame(); }, ReadEmpty, (target, id) => actions.Add($"{id}:{target.Handle}"), default));
        var edit = preparation.Advance([Source, Created], _ => Frame(images: true), ReadEmpty, (_, _) => throw new Exception("Images must not replay"), default);
        Assert.Equal("composer", edit?.Id);
        Assert.Equal(Created, preparation.Target);
        Assert.Equal(new[] { "new-window:10", "images:20" }, actions);
    }

    [Fact]
    public void A_source_without_the_mode_button_requires_the_same_captured_verified_image_home()
    {
        var preparation = new NativeImagePreparation(); var actions = new List<string>();
        NativeImageFrame? captured = null;
        preparation.Begin(Source, [Source], target =>
        {
            Assert.Equal(Source, target);
            return captured = Frame(images: true, mode: false);
        }, target => actions.Add($"new-window:{target.Handle}"), default, target =>
        {
            Assert.Equal(Source, target);
            Assert.NotNull(captured);
            return NativeImagePage.IsHome(captured.Nodes, "composer");
        });
        Assert.Null(preparation.Target);
        Assert.Equal(new[] { "new-window:10" }, actions);
    }

    [Theory]
    [InlineData(false)]
    [InlineData(true)]
    public void An_unverified_mode_less_source_cannot_create_a_window(bool imageHome)
    {
        var preparation = new NativeImagePreparation();
        Assert.Equal("chatgpt_mode_required", Assert.Throws<ImageAutomationException>(() => preparation.Begin(Source, [Source],
            _ => Frame(images: imageHome, mode: false), _ => throw new Exception("No unverified creation"), default)).Reason);
        Assert.Null(preparation.Target);
    }

    [Fact]
    public void A_failed_image_home_verification_prevents_source_window_creation()
    {
        var preparation = new NativeImagePreparation();
        Assert.Equal("chatgpt_mode_required", Assert.Throws<ImageAutomationException>(() => preparation.Begin(Source, [Source],
            _ => Frame(images: true, mode: false), _ => throw new Exception("No unverified creation"), default, _ => false)).Reason);
        Assert.Null(preparation.Target);
    }

    [Fact]
    public void A_login_source_cannot_use_the_verified_image_home_fallback()
    {
        var preparation = new NativeImagePreparation();
        var frame = NativeImageFrame.Capture(Frame(images: true, mode: false).Nodes.Append(new NativeImageNode("login", "rail", "Button", "Log in")).ToArray());
        Assert.Equal("chatgpt_mode_required", Assert.Throws<ImageAutomationException>(() => preparation.Begin(Source, [Source], _ => frame,
            _ => throw new Exception("No logged-out creation"), default, _ => throw new Exception("Login must precede source fallback"))).Reason);
        Assert.Null(preparation.Target);
    }

    [Fact]
    public void A_fresh_empty_window_without_a_mode_button_navigates_once_and_waits_for_genuine_image_home()
    {
        var preparation = Started(); var actions = new List<string>();
        Assert.Null(preparation.Advance([Source, Created], _ => Frame(mode: false), ReadEmpty,
            (target, id) => actions.Add($"{id}:{target.Handle}"), default));
        Assert.Null(preparation.Advance([Source, Created], _ => Frame(mode: false), ReadEmpty,
            (_, _) => throw new Exception("Images must not replay"), default));
        var edit = preparation.Advance([Source, Created], _ => Frame(images: true, mode: false), ReadEmpty,
            (_, _) => throw new Exception("Images must not replay"), default);
        Assert.Equal("composer", edit?.Id);
        Assert.Equal(new[] { "images:20" }, actions);
    }

    [Theory]
    [InlineData("missing")]
    [InlineData("hidden")]
    [InlineData("disabled")]
    [InlineData("duplicate")]
    public void A_mode_less_window_requires_unique_actionable_images_navigation_before_composer_reads(string condition)
    {
        var nodes = Frame(mode: false, imagesNavigation: condition != "missing").Nodes;
        if (condition == "hidden") nodes = nodes.Select(x => x.Id == "images" ? x with { Visible = false } : x).ToArray();
        if (condition == "disabled") nodes = nodes.Select(x => x.Id == "images" ? x with { Enabled = false } : x).ToArray();
        if (condition == "duplicate") nodes = nodes.Append(new NativeImageNode("other-images", "rail", "Button", "Images")).ToArray();
        var preparation = Started();
        Assert.Null(preparation.Advance([Source, Created], _ => NativeImageFrame.Capture(nodes),
            (_, _) => throw new Exception("No reads before unique actionable Images navigation"),
            (_, _) => throw new Exception("No unverified navigation"), default));
    }

    [Theory]
    [InlineData("readonly")]
    [InlineData("duplicate")]
    [InlineData("whitespace-draft")]
    public void A_mode_less_window_requires_one_truly_empty_writable_edit_before_navigation(string condition)
    {
        var nodes = Frame(mode: false).Nodes;
        if (condition == "duplicate") nodes = nodes.Append(new NativeImageNode("other-edit", "row", "Edit", "")).ToArray();
        var preparation = Started();
        var error = Record.Exception(() => preparation.Advance([Source, Created], _ => NativeImageFrame.Capture(nodes),
            (_, node) => node.Role != "Edit" || condition == "readonly" ? null : condition == "whitespace-draft" ? " \n" : "",
            (_, _) => throw new Exception("No unverified navigation"), default));
        Assert.True(error is null or ImageAutomationException);
        if (condition == "whitespace-draft") Assert.Equal("user_changed_composer", Assert.IsType<ImageAutomationException>(error).Reason);
    }

    [Fact]
    public void A_draft_appearing_after_the_writable_probe_prevents_mode_less_navigation()
    {
        var preparation = Started(); var reads = 0;
        Assert.Equal("user_changed_composer", Assert.Throws<ImageAutomationException>(() => preparation.Advance([Source, Created],
            _ => Frame(mode: false), (_, _) => ++reads == 1 ? "" : "new draft",
            (_, _) => throw new Exception("No navigation after a changed composer"), default)).Reason);
        Assert.Equal(2, reads);
    }

    [Fact]
    public void Generation_in_a_mode_less_new_window_prevents_composer_reads_and_navigation()
    {
        var preparation = Started();
        var frame = NativeImageFrame.Capture(Frame(mode: false).Nodes.Append(new NativeImageNode("generation", "main", "Text", "Creating image")).ToArray());
        Assert.Equal("conversation_changed", Assert.Throws<ImageAutomationException>(() => preparation.Advance([Source, Created], _ => frame,
            (_, _) => throw new Exception("No generation composer reads"), (_, _) => throw new Exception("No generation navigation"), default)).Reason);
    }

    [Theory]
    [InlineData("missing-heading")]
    [InlineData("unrelated-model")]
    public void Mode_less_navigation_never_binds_an_edit_without_genuine_image_home(string condition)
    {
        var preparation = Started();
        preparation.Advance([Source, Created], _ => Frame(mode: false), ReadEmpty, (_, _) => { }, default);
        var nodes = Frame(images: true, mode: false).Nodes;
        nodes = condition == "missing-heading" ? nodes.Where(x => x.Id != "heading").ToArray()
            : nodes.Select(x => x.Id == "model" ? x with { ParentId = "rail" } : x).ToArray();
        Assert.Null(preparation.Advance([Source, Created], _ => NativeImageFrame.Capture(nodes), ReadEmpty,
            (_, _) => throw new Exception("Images must not replay"), default));
    }

    [Theory]
    [InlineData("two-new")]
    [InlineData("reused-hwnd")]
    [InlineData("other-process")]
    [InlineData("other-start")]
    public void Ambiguous_or_unrelated_windows_never_authorize_capture_or_input(string condition)
    {
        var preparation = Started();
        NativeImageTarget[] targets = condition switch
        {
            "two-new" => [Source, Created, Created with { Handle = 30 }],
            "reused-hwnd" => [Source with { Started = DateTime.UnixEpoch.AddSeconds(1) }],
            "other-process" => [Source, Created with { ProcessId = 8 }],
            _ => [Source, Created with { Started = DateTime.UnixEpoch.AddSeconds(1) }]
        };
        Assert.Throws<ImageAutomationException>(() => preparation.Advance(targets, _ => throw new Exception("Wrong capture"), ReadEmpty, (_, _) => throw new Exception("Wrong action"), default));
    }

    [Theory]
    [InlineData("draft")]
    [InlineData("transcript")]
    [InlineData("no-images-navigation")]
    [InlineData("old-images")]
    public void A_new_HWND_does_not_authorize_images_without_a_fresh_empty_chat(string condition)
    {
        var preparation = Started(); var actions = new List<string>();
        var error = Record.Exception(() => preparation.Advance([Source, Created], _ => Frame(images: condition == "old-images", transcript: condition == "transcript", mode: false, imagesNavigation: condition != "no-images-navigation"),
            (_, node) => node.Role == "Edit" ? condition == "draft" ? "carried draft" : "" : null, (_, id) => actions.Add(id), default));
        Assert.True(error is null or ImageAutomationException);
        Assert.Empty(actions);
    }

    [Theory]
    [InlineData(false)]
    [InlineData(true)]
    public void Replacing_the_bound_window_or_document_never_redirects_input(bool document)
    {
        var preparation = Started();
        preparation.Advance([Source, Created], _ => Frame(mode: false), ReadEmpty, (_, _) => { }, default);
        Assert.Equal("conversation_changed", Assert.Throws<ImageAutomationException>(() => preparation.Advance(
            document ? [Source, Created] : [Source, Created with { Handle = 30 }], _ => Frame(images: true, mode: false, document: document ? "replacement" : "doc"),
            ReadEmpty, (_, _) => throw new Exception("Must not navigate"), default)).Reason);
    }

    [Fact]
    public void A_nonempty_image_composer_never_becomes_owned()
    {
        var preparation = Started(); preparation.Advance([Source, Created], _ => Frame(mode: false), ReadEmpty, (_, _) => { }, default);
        Assert.Equal("user_changed_composer", Assert.Throws<ImageAutomationException>(() => preparation.Advance([Source, Created], _ => Frame(images: true, mode: false),
            (_, node) => node.Role == "Edit" ? "other draft" : null, (_, _) => throw new Exception("Must not navigate"), default)).Reason);
    }

    [Fact]
    public void Cancellation_and_repeated_begin_cannot_replay_window_creation()
    {
        using var cancellation = new CancellationTokenSource();
        var preparation = new NativeImagePreparation(); var creations = 0;
        preparation.Begin(Source, [Source], _ => Frame(), _ => { creations++; cancellation.Cancel(); }, cancellation.Token);
        Assert.ThrowsAny<OperationCanceledException>(() => preparation.Advance([Source, Created], _ => Frame(), ReadEmpty, (_, _) => throw new Exception("Cancelled"), cancellation.Token));
        Assert.Throws<ImageAutomationException>(() => preparation.Begin(Source, [Source], _ => Frame(), _ => creations++, default));
        Assert.Equal(1, creations);
    }

    [Fact]
    public void An_uncertain_creation_is_consumed_before_the_native_call()
    {
        var preparation = new NativeImagePreparation(); var creations = 0;
        Assert.Throws<InvalidOperationException>(() => preparation.Begin(Source, [Source], _ => Frame(), _ => { creations++; throw new InvalidOperationException(); }, default));
        Assert.Throws<ImageAutomationException>(() => preparation.Begin(Source, [Source], _ => Frame(), _ => creations++, default));
        Assert.Equal(1, creations);
    }

    [Fact]
    public void A_new_window_can_finish_loading_its_document_without_repeating_creation()
    {
        var preparation = Started(); var actions = new List<string>();
        Assert.Null(preparation.Advance([Source, Created], _ => throw new ImageAutomationException("image_surface_unavailable"), ReadEmpty, (_, id) => actions.Add(id), default));
        Assert.Null(preparation.Advance([Source, Created], _ => Frame(), ReadEmpty, (_, id) => actions.Add(id), default));
        Assert.Equal(new[] { "images" }, actions);
    }

    [Theory]
    [InlineData(false)]
    [InlineData(true)]
    public void A_partial_document_waits_for_images_navigation_before_reading_any_composer(bool editPresent)
    {
        var preparation = Started(); var actions = new List<string>();
        var partial = Frame(mode: false, imagesNavigation: false);
        if (!editPresent) partial = NativeImageFrame.Capture(partial.Nodes.Where(x => x.Id != "composer").ToArray());
        Assert.Null(preparation.Advance([Source, Created], _ => partial, (_, _) => throw new Exception("Images navigation must precede Value reads"), (_, id) => actions.Add(id), default));
        Assert.Empty(actions);
        Assert.Null(preparation.Advance([Source, Created], _ => Frame(mode: false), ReadEmpty, (_, id) => actions.Add(id), default));
        Assert.Equal(new[] { "images" }, actions);
    }

    [Fact]
    public void A_provisional_loading_document_does_not_bind_the_later_verified_chat_page()
    {
        var preparation = Started(); var actions = new List<string>();
        Assert.Null(preparation.Advance([Source, Created], _ => Frame(mode: false, document: "loading", imagesNavigation: false), (_, _) => throw new Exception("No reads before Images navigation"), (_, id) => actions.Add(id), default));
        Assert.Null(preparation.Advance([Source, Created], _ => Frame(mode: false, document: "ready"), ReadEmpty, (_, id) => actions.Add(id), default));
        Assert.NotNull(preparation.Advance([Source, Created], _ => Frame(images: true, mode: false, document: "ready"), ReadEmpty, (_, id) => actions.Add(id), default));
        Assert.Equal(new[] { "images" }, actions);
    }

    [Fact]
    public void A_product_verified_probe_preserves_the_selected_source_when_focus_returns_to_Entwurf()
    {
        using var surface = new WindowsImageSurface();
        Assert.True(surface.ProbeSource([Source, Created], Created.Handle, target => { Assert.Equal(Created, target); return new(true, "ready"); }).Available);
        Assert.Equal(Created, surface.DiscoverSource([Source, Created], 99));
    }

    [Fact]
    public void An_unavailable_product_probe_never_caches_the_candidate_window()
    {
        using var surface = new WindowsImageSurface();
        Assert.False(surface.ProbeSource([Source, Created], Created.Handle, _ => new(false, "chatgpt_mode_required")).Available);
        Assert.Throws<ImageAutomationException>(() => surface.DiscoverSource([Source, Created], 99));
    }

    [Fact]
    public void A_failed_creation_cannot_be_followed_by_actions_in_a_later_unrelated_window()
    {
        var preparation = new NativeImagePreparation();
        Assert.Throws<InvalidOperationException>(() => preparation.Begin(Source, [Source], _ => Frame(), _ => throw new InvalidOperationException(), default));
        Assert.Throws<ImageAutomationException>(() => preparation.Advance([Source, Created], _ => Frame(), ReadEmpty, (_, _) => throw new Exception("Must not navigate"), default));
    }

    [Fact]
    public void Source_selection_uses_the_cached_identity_or_explicit_foreground_after_restart()
    {
        Assert.Equal(Source, NativeImagePreparation.SelectSource([Created, Source], Source, Created.Handle));
        Assert.Equal(Created, NativeImagePreparation.SelectSource([Source, Created], null, Created.Handle));
        Assert.Throws<ImageAutomationException>(() => NativeImagePreparation.SelectSource([Source, Created], null, 99));
        Assert.Throws<ImageAutomationException>(() => NativeImagePreparation.SelectSource([Created], Source, Created.Handle));
    }

    static NativeImagePreparation Started()
    {
        var preparation = new NativeImagePreparation();
        preparation.Begin(Source, [Source], _ => Frame(), _ => { }, default);
        return preparation;
    }
    static string? ReadEmpty(NativeImageTarget target, NativeImageNode node) => node.Role == "Edit" ? "" : null;
    static NativeImageFrame Frame(bool images = false, bool transcript = false, bool mode = true, string document = "doc", bool imagesNavigation = true)
    {
        var nodes = new List<NativeImageNode>
        {
            new(document, null, "Document", ""), new("root", document, "Group", ""), new("shell", "root", "Group", ""),
            new("rail", "shell", "Group", ""),
            new("main", "shell", "Group", ""), new("row", "main", "Group", ""),
            new("composer", "row", "Edit", ""), new("model", "row", "Button", "选择 ChatGPT 模型")
        };
        if (imagesNavigation) nodes.Add(new("images", "rail", "Button", "图像"));
        if (mode) nodes.Add(new("mode", "rail", "Button", "切换模式，当前模式：ChatGPT"));
        if (images) nodes.Add(new("heading", "main", "Text", "图像"));
        if (transcript) nodes.Add(new("old-turn", "main", "Text", "你说：", Speaker: "user"));
        return NativeImageFrame.Capture(nodes);
    }
}
