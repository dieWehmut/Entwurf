using System.Runtime.InteropServices;
using Entwurf.Bridge;
using Xunit;

namespace Entwurf.Bridge.Tests;

public sealed class NativeImagePreviewDismissalTests
{
    const string Prompt = "public generated image test";
    static readonly string PromptHash = System.Convert.ToHexStringLower(
        System.Security.Cryptography.SHA256.HashData(System.Text.Encoding.UTF8.GetBytes(Prompt)));
    static readonly HashSet<string> Before = new(StringComparer.Ordinal);

    [Fact]
    public async Task Delayed_dismissal_waits_for_the_original_turn_to_reappear_with_its_same_heading()
    {
        var frames = new Queue<NativeImageFrame>([
            Frame(preview: true),
            Frame(preview: true, change: "disabled_close"),
            Frame(),
            Frame(heading: true),
            Frame(heading: true, text: true, change: "rerendered_text")
        ]);
        var reads = 0;
        var own = await Dismissal().WaitAsync(_ => { reads++; return frames.Dequeue(); }, "close", default,
            TimeSpan.FromSeconds(1), TimeSpan.FromMilliseconds(5));
        Assert.Equal(new NativeUserTurn("settled-user-text", "user-heading"), own);
        Assert.Equal(5, reads);
    }

    [Fact]
    public async Task A_turn_that_is_already_present_does_not_acknowledge_a_still_visible_preview()
    {
        var reads = 0;
        var own = await Dismissal().WaitAsync(_ => Frame(preview: ++reads == 1, heading: true, text: true),
            "close", default, TimeSpan.FromSeconds(1), TimeSpan.FromMilliseconds(5));
        Assert.Equal(new NativeUserTurn("user-text", "user-heading"), own);
        Assert.Equal(2, reads);
    }

    [Theory]
    [InlineData(true)]
    [InlineData(false)]
    public async Task A_visible_preview_or_missing_turn_times_out_without_authorizing_restoration(bool visible)
    {
        var reads = 0;
        var error = await Assert.ThrowsAsync<ImageAutomationException>(() => Dismissal().WaitAsync(_ =>
        {
            reads++;
            return Frame(preview: visible);
        }, "close", default, TimeSpan.FromMilliseconds(30), TimeSpan.FromMilliseconds(5)));
        Assert.Equal("submission_uncertain", error.Reason);
        Assert.True(reads >= 1);
    }

    [Fact]
    public async Task An_already_absent_owned_preview_needs_only_the_original_turn()
    {
        var reads = 0;
        var own = await Dismissal().WaitAsync(_ => Frame(heading: ++reads > 1, text: reads > 1),
            null, default, TimeSpan.FromSeconds(1), TimeSpan.FromMilliseconds(5));
        Assert.Equal("user-heading", own.HeadingId);
        Assert.Equal(2, reads);
    }

    [Fact]
    public async Task The_exact_original_text_can_reappear_before_its_original_heading()
    {
        var reads = 0;
        var own = await Dismissal().WaitAsync(_ => Frame(heading: ++reads > 1, text: true),
            "close", default, TimeSpan.FromSeconds(1), TimeSpan.FromMilliseconds(5));
        Assert.Equal(new NativeUserTurn("user-text", "user-heading"), own);
        Assert.Equal(2, reads);
    }

    [Theory]
    [InlineData("new_preview")]
    [InlineData("wrong_preview_name")]
    [InlineData("wrong_preview_role")]
    [InlineData("document")]
    public void Changed_preview_or_document_evidence_is_rejected_before_a_close_dispatch(string change)
    {
        var error = Assert.Throws<ImageAutomationException>(() => Dismissal().ReadPreview(Frame(preview: true, change: change)));
        Assert.Equal("conversation_changed", error.Reason);
    }

    [Theory]
    [InlineData("edited_prompt")]
    [InlineData("heading_speaker")]
    [InlineData("replacement_heading")]
    [InlineData("later_same_prompt")]
    [InlineData("duplicate_prompt")]
    public void An_already_changed_user_binding_is_rejected_before_a_close_dispatch(string change)
    {
        var error = Assert.Throws<ImageAutomationException>(() => Dismissal().ReadPreview(
            Frame(preview: true, heading: true, text: true, change: change)));
        Assert.Equal("conversation_changed", error.Reason);
    }

    [Theory]
    [InlineData("new_preview")]
    [InlineData("duplicate_close")]
    [InlineData("replacement_close")]
    [InlineData("document")]
    public async Task A_changed_preview_close_or_document_stops_after_the_first_read(string change)
    {
        var reads = 0;
        var error = await Assert.ThrowsAsync<ImageAutomationException>(() => Dismissal().WaitAsync(_ =>
        {
            reads++;
            return Frame(preview: true, change: change);
        }, "close", default));
        Assert.Equal("conversation_changed", error.Reason);
        Assert.Equal(1, reads);
    }

    [Theory]
    [InlineData("disabled_close")]
    [InlineData("hidden_close")]
    [InlineData("missing_close")]
    public async Task The_original_close_can_become_nonactionable_during_dismissal(string change)
    {
        var reads = 0;
        var own = await Dismissal().WaitAsync(_ => ++reads == 1 ? Frame(preview: true, change: change) : Frame(heading: true, text: true),
            "close", default, TimeSpan.FromSeconds(1), TimeSpan.FromMilliseconds(5));
        Assert.Equal("user-text", own.TextId);
        Assert.Equal(2, reads);
    }

    [Theory]
    [InlineData(false)]
    [InlineData(true)]
    public async Task A_preview_cannot_appear_after_its_disappearance_has_been_observed(bool initiallyAbsent)
    {
        var reads = 0;
        var error = await Assert.ThrowsAsync<ImageAutomationException>(() => Dismissal().WaitAsync(_ =>
            Frame(preview: initiallyAbsent || ++reads > 1), initiallyAbsent ? null : "close", default,
            TimeSpan.FromSeconds(1), TimeSpan.FromMilliseconds(5)));
        Assert.Equal("conversation_changed", error.Reason);
    }

    [Theory]
    [InlineData("edited_prompt")]
    [InlineData("heading_speaker")]
    [InlineData("replacement_heading")]
    [InlineData("duplicate_prompt")]
    [InlineData("later_same_prompt")]
    [InlineData("later_different_prompt")]
    public async Task Changed_or_ambiguous_user_bindings_are_rejected_without_waiting(string change)
    {
        var reads = 0;
        var error = await Assert.ThrowsAsync<ImageAutomationException>(() => Dismissal().WaitAsync(_ =>
        {
            reads++;
            return Frame(heading: true, text: true, change: change);
        }, "close", default));
        Assert.Equal("conversation_changed", error.Reason);
        Assert.Equal(1, reads);
    }

    [Fact]
    public async Task A_replaced_user_heading_is_rejected_even_while_the_owned_preview_is_visible()
    {
        var error = await Assert.ThrowsAsync<ImageAutomationException>(() => Dismissal().WaitAsync(_ =>
            Frame(preview: true, change: "later_same_prompt"), "close", default));
        Assert.Equal("conversation_changed", error.Reason);
    }

    [Fact]
    public async Task A_legacy_text_binding_does_not_accept_a_rerendered_text_id()
    {
        var error = await Assert.ThrowsAsync<ImageAutomationException>(() => Dismissal(heading: null).WaitAsync(_ =>
            Frame(heading: true, text: true, change: "rerendered_text"), "close", default));
        Assert.Equal("conversation_changed", error.Reason);
    }

    [Fact]
    public void A_changed_legacy_text_id_is_rejected_before_close()
    {
        var error = Assert.Throws<ImageAutomationException>(() => Dismissal(heading: null).ReadPreview(
            Frame(preview: true, heading: true, text: true, change: "rerendered_text")));
        Assert.Equal("conversation_changed", error.Reason);
    }

    [Fact]
    public async Task A_legacy_text_binding_can_wait_for_its_exact_original_text_id()
    {
        var reads = 0;
        var own = await Dismissal(heading: null).WaitAsync(_ => Frame(heading: true, text: ++reads > 1), "close", default,
            TimeSpan.FromSeconds(1), TimeSpan.FromMilliseconds(5));
        Assert.Equal(new NativeUserTurn("user-text", "user-heading"), own);
        Assert.Equal(2, reads);
    }

    [Fact]
    public void An_unbound_saved_preview_cannot_start_dismissal_recovery()
    {
        var error = Assert.Throws<ImageAutomationException>(() => new NativeImagePreviewDismissal("doc", "owned-preview", Before,
            PromptHash, null, null).ReadPreview(Frame(preview: true)));
        Assert.Equal("submission_uncertain", error.Reason);
    }

    [Fact]
    public async Task Cancellation_before_polling_does_not_read_the_provider()
    {
        using var cancellation = new CancellationTokenSource(); cancellation.Cancel();
        var reads = 0;
        await Assert.ThrowsAnyAsync<OperationCanceledException>(() => Dismissal().WaitAsync(_ =>
        {
            reads++;
            return Frame(heading: true, text: true);
        }, "close", cancellation.Token));
        Assert.Equal(0, reads);
    }

    [Fact]
    public async Task Cancellation_during_a_ready_read_cannot_report_restoration()
    {
        using var cancellation = new CancellationTokenSource();
        await Assert.ThrowsAnyAsync<OperationCanceledException>(() => Dismissal().WaitAsync(_ =>
        {
            cancellation.Cancel();
            return Frame(heading: true, text: true);
        }, "close", cancellation.Token));
    }

    [Fact]
    public async Task Cancellation_interrupts_the_poll_delay_before_another_read()
    {
        using var cancellation = new CancellationTokenSource();
        var firstRead = new TaskCompletionSource(TaskCreationOptions.RunContinuationsAsynchronously);
        var reads = 0;
        var wait = Dismissal().WaitAsync(_ =>
        {
            reads++;
            firstRead.TrySetResult();
            return Frame(preview: true);
        }, "close", cancellation.Token, pollInterval: TimeSpan.FromSeconds(1));
        await firstRead.Task;
        cancellation.Cancel();
        await Assert.ThrowsAnyAsync<OperationCanceledException>(() => wait);
        Assert.Equal(1, reads);
    }

    [Theory]
    [InlineData("window")]
    [InlineData("process")]
    [InlineData("lifetime")]
    public async Task Every_poll_keeps_the_original_window_and_process_lifetime_bound(string change)
    {
        var target = new NativeImageTarget(10, 7, DateTime.UnixEpoch);
        var current = new NativeWindowIdentity(10, 7, DateTime.UnixEpoch, true);
        var access = new NativeImageWindowAccess(_ => current);
        var reads = 0;
        var error = await Assert.ThrowsAsync<ImageAutomationException>(() => Dismissal().WaitAsync(_ =>
        {
            access.Require(target, default);
            if (++reads == 2) current = change switch
            {
                "window" => current with { Handle = 11 },
                "process" => current with { ProcessId = 8 },
                _ => current with { Started = DateTime.UnixEpoch.AddSeconds(1) }
            };
            var frame = Frame(preview: reads == 1, heading: reads > 1, text: reads > 1);
            access.Require(target, default);
            return frame;
        }, "close", default, TimeSpan.FromSeconds(1), TimeSpan.FromMilliseconds(5)));
        Assert.Equal("conversation_changed", error.Reason);
        Assert.Equal(2, reads);
    }

    [Fact]
    public async Task Remaining_wait_budget_is_passed_to_existing_complete_frame_stale_recovery()
    {
        var traversals = 0;
        var budgets = new List<TimeSpan>();
        var own = await Dismissal().WaitAsync(remaining =>
        {
            budgets.Add(remaining);
            return NativeImageFrameReadiness.Read(() =>
            {
                if (++traversals == 1) throw new COMException("fixture stale preview", unchecked((int)0x80040201));
                return Frame(preview: traversals == 2, heading: traversals > 2, text: traversals > 2);
            }, default, timeout: remaining);
        }, "close", default, TimeSpan.FromSeconds(1), TimeSpan.FromMilliseconds(5));
        Assert.Equal("user-heading", own.HeadingId);
        Assert.Equal(3, traversals);
        Assert.Equal(2, budgets.Count);
        Assert.True(budgets[1] < budgets[0]);
    }

    static NativeImagePreviewDismissal Dismissal(string? heading = "user-heading")
        => new("doc", "owned-preview", Before, PromptHash, "user-text", heading);

    static NativeImageFrame Frame(bool preview = false, bool heading = false, bool text = false, string? change = null)
    {
        var nodes = new List<NativeImageNode>
        {
            new("doc", null, "Document", ""),
            new("transcript", "doc", "Group", "")
        };
        if (heading) nodes.Add(new("user-heading", "transcript", "Text", "You said:", Speaker: "user"));
        if (text) nodes.Add(new("user-text", "transcript", "Text", Prompt));
        if (preview)
        {
            nodes.Add(new("owned-preview", "doc", "Group", "Image preview"));
            nodes.Add(new("close", "owned-preview", "Button", "Close viewer"));
        }
        if (change == "new_preview") nodes.Add(new("new-preview", "doc", "Group", "Image preview"));
        if (change == "duplicate_close") nodes.Add(new("other-close", "owned-preview", "Button", "关闭查看器"));
        if (change == "duplicate_prompt") nodes.Add(new("other-text", "transcript", "Text", Prompt));
        if (change is "later_same_prompt" or "later_different_prompt")
        {
            nodes.Add(new("later-heading", "transcript", "Text", "You said:", Speaker: "user"));
            nodes.Add(new("later-text", "transcript", "Text", change == "later_same_prompt" ? Prompt : ""));
        }
        nodes = nodes.Select(x => change switch
        {
            "disabled_close" when x.Id == "close" => x with { Enabled = false },
            "hidden_close" when x.Id == "close" => x with { Visible = false },
            "replacement_close" when x.Id == "close" => x with { Id = "replacement-close" },
            "wrong_preview_name" when x.Id == "owned-preview" => x with { Name = "" },
            "wrong_preview_role" when x.Id == "owned-preview" => x with { Role = "Pane" },
            "rerendered_text" when x.Id == "user-text" => x with { Id = "settled-user-text" },
            "edited_prompt" when x.Id == "user-text" => x with { Name = "" },
            "heading_speaker" when x.Id == "user-heading" => x with { Speaker = "assistant" },
            "replacement_heading" when x.Id == "user-heading" => x with { Id = "replacement-heading" },
            _ => x
        }).ToList();
        if (change == "missing_close") nodes.RemoveAll(x => x.Id == "close");
        var frame = NativeImageFrame.Capture(nodes);
        return change == "document" ? frame with { DocumentId = "replacement-document" } : frame;
    }
}
